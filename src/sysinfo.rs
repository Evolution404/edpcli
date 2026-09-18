//! 跨平台系统探测门面与可注入命令执行器。
//! 操作系统细节由 `platform` 实现；业务层只依赖这里的统一接口。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::platform::HardwareProbe;
pub use crate::platform::ExtDisk;

/// 子进程执行抽象: 成功返回 stdout 文本(非零退出/超时/启动失败均为 Err)。
/// 等价 Python subprocess.check_output(text=True, errors='ignore', timeout=…)。
pub trait CmdRunner {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String>;

    /// 可选的原生硬件探测。测试 runner 默认没有 native backend；
    /// production `SysRunner` 由当前平台实现提供。
    fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
        None
    }
}

pub struct SysRunner;

impl CmdRunner for SysRunner {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String> {
        let mut child = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("无法启动 {}: {}", cmd[0], e)))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other(format!("{} 未提供 stdout 管道", cmd[0])))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
        });
        let start = Instant::now();
        let output = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(s) => break s,
                Err(RecvTimeoutError::Timeout) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(io::Error::new(
                            ErrorKind::TimedOut,
                            format!("{} 超时({}s)", cmd[0], timeout.as_secs()),
                        ));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("读取输出失败"))
                }
            }
        };
        let status = child.wait()?;
        if status.success() {
            Ok(output)
        } else {
            Err(io::Error::other(format!("{} 退出码 {:?}", cmd[0], status.code())))
        }
    }

    fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe> {
        crate::platform::hardware_probe(disk)
    }
}

#[derive(Clone)]
enum CachedOutput {
    Ok(String),
    Err(ErrorKind, String),
}

impl CachedOutput {
    fn from_result(result: &io::Result<String>) -> Self {
        match result {
            Ok(value) => Self::Ok(value.clone()),
            Err(error) => Self::Err(error.kind(), error.to_string()),
        }
    }

    fn into_result(self) -> io::Result<String> {
        match self {
            Self::Ok(value) => Ok(value),
            Self::Err(kind, message) => Err(io::Error::new(kind, message)),
        }
    }
}

/// 单次只读命令会话内的系统探测缓存。
///
/// 只缓存由当前平台明确标记为纯查询的命令；有副作用命令永远直通。
/// 该类型只用于 list/meta/inspect/completion；apply/restore 的安全终验继续使用
/// fresh `SysRunner`，避免缓存掩盖换盘或设备状态变化。
pub struct ReadProbeCache<'a> {
    inner: &'a dyn CmdRunner,
    cache: RefCell<HashMap<String, CachedOutput>>,
    hardware: RefCell<HashMap<u32, Option<HardwareProbe>>>,
    hits: Cell<usize>,
    misses: Cell<usize>,
}

impl<'a> ReadProbeCache<'a> {
    pub fn new(inner: &'a dyn CmdRunner) -> Self {
        Self {
            inner,
            cache: RefCell::new(HashMap::new()),
            hardware: RefCell::new(HashMap::new()),
            hits: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    fn cacheable(cmd: &[&str]) -> bool {
        crate::platform::probe_command_cacheable(cmd)
    }

    fn key(cmd: &[&str], timeout: Duration) -> String {
        format!("{}\0{}", cmd.join("\0"), timeout.as_millis())
    }

    #[cfg(all(test, target_os = "macos"))]
    fn hits(&self) -> usize {
        self.hits.get()
    }

    #[cfg(all(test, target_os = "macos"))]
    fn misses(&self) -> usize {
        self.misses.get()
    }
}

impl CmdRunner for ReadProbeCache<'_> {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String> {
        if !Self::cacheable(cmd) {
            return self.inner.check_output(cmd, timeout);
        }
        let key = Self::key(cmd, timeout);
        if let Some(value) = self.cache.borrow().get(&key).cloned() {
            self.hits.set(self.hits.get() + 1);
            return value.into_result();
        }
        self.misses.set(self.misses.get() + 1);
        let result = self.inner.check_output(cmd, timeout);
        self.cache
            .borrow_mut()
            .insert(key, CachedOutput::from_result(&result));
        result
    }

    fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe> {
        if let Some(value) = self.hardware.borrow().get(&disk).cloned() {
            return value;
        }
        let value = self.inner.hardware_probe(disk);
        self.hardware.borrow_mut().insert(disk, value.clone());
        value
    }
}

// 以下解析器仅保留给历史罐头测试；生产平台解析已收敛到 platform/macos.rs。
#[cfg(test)]
/// 按 ioreg 节点行切块: 行以 `+-o` 开头且携带 `<class <cls>,`。
/// 真实输出的节点名常是产品名(如 `+-o USB DISK@01200000  <class IOUSBHostDevice, …>`),
/// 不能按 `+-o <类名>` 前缀切 — 那样永远切不出块(Python 版因此退化为整段输出
/// 当单一块, 多 USB 设备时会拿错 idVendor; 按类标记切块同时修复了这一点)。
/// 嵌套其他类的 `+-o` 行属于父块内容 — `BSD Name` 就在子节点里。
/// 无块起点时整段输出作为单一候选块(与 Python re.split 行为一致)。
pub fn split_class_blocks<'a>(out: &'a str, cls: &str) -> Vec<&'a str> {
    let c1 = format!("<class {},", cls);
    let c2 = format!("<class {}>", cls);
    let mut starts: Vec<usize> = Vec::new();
    let mut off = 0usize;
    for line in out.lines() {
        let t = line.trim_start();
        if t.starts_with("+-o") && (line.contains(&c1) || line.contains(&c2)) {
            starts.push(off);
        }
        off += line.len() + 1;
    }
    let mut blocks = Vec::new();
    for pair in starts.windows(2) {
        blocks.push(&out[pair[0]..pair[1]]);
    }
    if let Some(&last) = starts.last() {
        blocks.push(&out[last..]);
    }
    if blocks.is_empty() && !out.trim().is_empty() {
        blocks.push(out);
    }
    blocks
}

#[cfg(test)]
/// 块内找 `"Key" = "value"` 形式的字符串字段(块内任意位置, 允许 = 两边空白)。
/// 等价 Python `re.search(r'"Key"\s*=\s*"([^"]*)"', block)` — ioreg 属性行
/// 带树形前缀(`|   "Key" = …`), 不能按行首匹配。
pub fn block_str_field(block: &str, key: &str) -> Option<String> {
    let quoted_key = format!("\"{}\"", key);
    let bytes = block.as_bytes();
    let mut from = 0usize;
    while let Some(p) = block[from..].find(&quoted_key) {
        let mut i = from + p + quoted_key.len();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            from += p + 1;
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            from += p + 1;
            continue;
        }
        i += 1;
        let vstart = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i < bytes.len() {
            return Some(block[vstart..i].to_string());
        }
        from += p + 1;
    }
    None
}

#[cfg(test)]
/// 块内找 `"Key" = 1234` 形式的无引号十进制整数字段(块内任意位置)。
pub fn block_int_field(block: &str, key: &str) -> Option<i64> {
    let quoted_key = format!("\"{}\"", key);
    let bytes = block.as_bytes();
    let mut from = 0usize;
    while let Some(p) = block[from..].find(&quoted_key) {
        let mut i = from + p + quoted_key.len();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            from += p + 1;
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let dstart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i > dstart {
            return block[dstart..i].parse().ok();
        }
        from += p + 1;
    }
    None
}

/// 当前平台整盘总扇区数；失败/缺失返回 None。
pub fn disk_total_sectors(runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    crate::platform::disk_total_sectors(runner, disk)
}

/// USB VID/PID(hex4); 失败返回 ("xxxx","xxxx")。
pub fn usb_vid_pid(runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    if let Some(probe) = runner.hardware_probe(disk) {
        if let (Some(vid), Some(pid)) = (probe.vid, probe.pid) {
            return (format!("{vid:04x}"), format!("{pid:04x}"));
        }
    }
    crate::platform::usb_vid_pid(runner, disk)
}

// ══════════════════════════════════════════════════════════════════
// 外接盘枚举
// ══════════════════════════════════════════════════════════════════
pub fn list_external_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    crate::platform::list_external_disks(runner)
}

/// 本工具可操作的外接 USB 整盘子集(供自动选盘)。
pub fn list_usb_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    list_external_disks(runner)
        .into_iter()
        .filter(|d| d.proto == "USB")
        .collect()
}

/// 直接核验一个显式盘号是否为可操作的外接 USB 整盘。
pub fn usb_disk(runner: &dyn CmdRunner, disk: u32) -> Option<ExtDisk> {
    list_external_disks(runner)
        .into_iter()
        .find(|item| item.n == disk && item.proto == "USB")
}

/// 强制卸载整盘。写盘流程必须确认卸载成功后才能重新以 O_RDWR 打开设备。
pub fn prepare_write(runner: &dyn CmdRunner, disk: u32) -> io::Result<crate::platform::WriteGuard> {
    crate::platform::prepare_write(runner, disk)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::platform::{HardwareProbe, NativeTransport};
    #[cfg(target_os = "macos")]
    use std::collections::HashMap;

    /// 罐头 CmdRunner: (子命令前缀) → 预置输出。
    #[cfg(target_os = "macos")]
    struct FakeRunner {
        outputs: HashMap<String, String>,
    }
    #[cfg(target_os = "macos")]
    impl CmdRunner for FakeRunner {
        fn check_output(&self, cmd: &[&str], _t: Duration) -> io::Result<String> {
            self.outputs
                .get(&cmd.join(" "))
                .cloned()
                .ok_or_else(|| io::Error::other(format!("无罐头: {}", cmd.join(" "))))
        }
    }

    // 与 Python test_list.py 相同的 8 盘矩阵: disk4=USB(未识别), disk6=USB(cems),
    // disk7=Thunderbolt, disk0/1=系统盘, disk8=DMG虚拟盘
    #[cfg(target_os = "macos")]
    fn plist_str(s: &str) -> String {
        format!("<string>{}</string>", s)
    }
    #[cfg(target_os = "macos")]
    fn fake_diskutil() -> FakeRunner {
        let list = format!(
            "<plist version=\"1.0\"><dict><key>AllDisks</key><array>{}</array></dict></plist>",
            ["disk0", "disk1", "disk4", "disk4s1", "disk6", "disk7", "disk8"]
                .iter()
                .map(|d| plist_str(d))
                .collect::<Vec<_>>()
                .join("")
        );
        let info = |name: &str, proto: &str, internal: bool, extra: &str, size: i64| {
            format!(
                "<plist version=\"1.0\"><dict><key>WholeDisk</key><{}/><key>Internal</key><{}/><key>BusProtocol</key><string>{}</string><key>TotalSize</key><integer>{}</integer>{}</dict></plist>",
                if name.ends_with("s1") { "false" } else { "true" },
                if internal { "true" } else { "false" },
                proto,
                size,
                extra
            )
        };
        let mut m = HashMap::new();
        m.insert("diskutil list -plist".to_string(), list);
        m.insert(
            "diskutil info -plist disk0".to_string(),
            info("disk0", "Apple Fabric", true, "", 500_000_000_000),
        );
        m.insert(
            "diskutil info -plist disk1".to_string(),
            info("disk1", "Apple Fabric", true, "", 500_000_000_000),
        );
        m.insert(
            "diskutil info -plist disk4".to_string(),
            info("disk4", "USB", false, "", 64_000_000_000),
        );
        m.insert(
            "diskutil info -plist disk4s1".to_string(),
            info("disk4s1", "USB", false, "", 64_000_000_000),
        );
        m.insert(
            "diskutil info -plist disk6".to_string(),
            info("disk6", "USB", false, "", 62_914_560_000),
        );
        m.insert(
            "diskutil info -plist disk7".to_string(),
            info("disk7", "Thunderbolt", false, "", 500_107_862_016),
        );
        m.insert(
            "diskutil info -plist disk8".to_string(),
            info("disk8", "Disk Image", false, "<key>VirtualOrPhysical</key><string>Virtual</string>", 1_000_000_000),
        );
        FakeRunner { outputs: m }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn enumeration_filters_and_protocol() {
        let runner = fake_diskutil();
        let disks = list_external_disks(&runner);
        assert_eq!(disks.iter().map(|d| d.n).collect::<Vec<_>>(), vec![4, 6, 7]);
        assert_eq!(disks[0].proto, "USB");
        assert_eq!(disks[2].proto, "Thunderbolt");
        assert_eq!((disks[2].vid.as_str(), disks[2].pid.as_str()), ("xxxx", "xxxx")); // 非USB 无 VID/PID
        // USB 盘走 ioreg 查 VID/PID(罐头里没有 ioreg → 回退 xxxx)
        assert_eq!((disks[0].vid.as_str(), disks[0].pid.as_str()), ("xxxx", "xxxx"));

        let usb = list_usb_disks(&runner);
        assert_eq!(usb.iter().map(|d| d.n).collect::<Vec<_>>(), vec![4, 6]);
    }

    #[test]
    fn ioreg_block_split_keeps_nested_children() {
        // 真实树形: 竖线前缀、@0 节点名; BSD Name 在子节点里, 子节点的其他 +-o 行不是块边界
        let out = "\
+-o IOSCSITargetDevice@0  <class IOSCSITargetDevice, id 0x100002b0e, retain 8>\n\
  |   \"IOPropertyMatch\" = \"x\"\n\
  +-o IOSCSILogicalUnitNub@0  <class IOSCSILogicalUnitNub, id 0x100002b11>\n\
    |   \"Vendor Identification\" = \"Netac  \"\n\
    |   \"Product Identification\" = \"OnlyDisk\"\n\
    |   \"Product Revision Level\" = \"1.00\"\n\
    +-o Netac OnlyDisk Media  <class IOMedia, id 0x100002b17>\n\
      |   \"BSD Name\" = \"disk6\"\n\
+-o IOSCSITargetDevice@0  <class IOSCSITargetDevice, id 0x100002b9e>\n\
  |   \"Nothing here\" = \"y\"\n\
  +-o Other Media  <class IOMedia, id 0x100002b99>\n\
    |   \"BSD Name\" = \"disk9\"\n";
        let blocks = split_class_blocks(out, "IOSCSITargetDevice");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("\"BSD Name\" = \"disk6\""));
        assert_eq!(
            block_str_field(blocks[0], "Vendor Identification").as_deref(),
            Some("Netac  ")
        );
        assert_eq!(block_int_field(blocks[0], "idVendor"), None);
        // 第二块是 disk9, 无 vendor
        assert_eq!(block_str_field(blocks[1], "Vendor Identification"), None);
    }

    #[test]
    fn ioreg_int_fields_unquoted_decimal() {
        // 真实格式: 属性行带树形前缀, 字段在行中任意位置
        let block = "\
+-o IOUSBHostDevice  <class IOUSBHostDevice, id 0x100002af0, retain 15>\n\
  |   \"idVendor\" = 3352\n\
  |   \"idProduct\" = 8197\n\
  |   \"USB Product Name\" = \"Mass Storage\"\n\
  +-o Some Media  <class IOMedia>\n\
    |   \"BSD Name\" = \"disk6\"\n";
        assert_eq!(block_int_field(block, "idVendor"), Some(3352));
        assert_eq!(block_int_field(block, "idProduct"), Some(8197));
        assert_eq!(block_str_field(block, "USB Product Name").as_deref(), Some("Mass Storage"));
    }

    #[test]
    fn ioreg_product_named_usb_root_blocks() {
        // 真实 U 盘: IOUSBHostDevice 根节点名是产品名(USB DISK@…), 类只在 <class …> 里;
        // 多个 USB 设备时按类标记切块, 各取各的 idVendor
        let out = "\
+-o Keyboard Tal@14100000  <class IOUSBHostDevice, id 0x100002a01, retain 14>\n\
  |   \"idVendor\" = 1452\n\
  |   \"idProduct\" = 610\n\
  |   \"Product\" = \"Apple Internal Keyboard\"\n\
+-o USB DISK@01200000  <class IOUSBHostDevice, id 0x100002af0, retain 15>\n\
  |   \"idVendor\" = 13621\n\
  |   \"idProduct\" = 25344\n\
  +-o IOUSBMassStorageInterfaceNub  <class IOUSBMassStorageInterfaceNub, id 0x100002b04>\n\
    +-o USB DISK Media  <class IOMedia, id 0x100002b17>\n\
      |   \"BSD Name\" = \"disk6\"\n";
        let blocks = split_class_blocks(out, "IOUSBHostDevice");
        assert_eq!(blocks.len(), 2); // 键盘 + U 盘各一块
        assert!(blocks[0].contains("Keyboard"));
        assert!(blocks[1].contains("\"BSD Name\" = \"disk6\""));
        assert_eq!(block_int_field(blocks[1], "idVendor"), Some(13621)); // 0x3535
        assert_eq!(block_int_field(blocks[1], "idProduct"), Some(25344)); // 0x6300
        // 无块起点时整段输出为单一块(Python re.split 兜底行为)
        let no_root = "  |   \"idVendor\" = 1\n  |   \"BSD Name\" = \"disk6\"\n";
        assert_eq!(split_class_blocks(no_root, "IOUSBHostDevice").len(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disk_total_sectors_prefers_disksize() {
        let mut m = HashMap::new();
        m.insert(
            "diskutil info -plist disk6".to_string(),
            "<plist version=\"1.0\"><dict><key>DiskSize</key><integer>62914560000</integer><key>TotalSize</key><integer>999</integer></dict></plist>".to_string(),
        );
        let runner = FakeRunner { outputs: m };
        assert_eq!(disk_total_sectors(&runner, 6), Some(62914560000 / 512));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn read_probe_cache_reuses_ioreg_snapshot_across_disks() {
        let out = "\
+-o USB A@00100000  <class IOUSBHostDevice, id 0x1>\n\
  |   \"idVendor\" = 13621\n\
  |   \"idProduct\" = 25344\n\
  +-o A Media  <class IOMedia>\n\
    |   \"BSD Name\" = \"disk4\"\n\
+-o USB B@00200000  <class IOUSBHostDevice, id 0x2>\n\
  |   \"idVendor\" = 3352\n\
  |   \"idProduct\" = 8197\n\
  +-o B Media  <class IOMedia>\n\
    |   \"BSD Name\" = \"disk6\"\n";
        let mut outputs = HashMap::new();
        outputs.insert(
            "ioreg -r -c IOUSBHostDevice -l".to_string(),
            out.to_string(),
        );
        let runner = FakeRunner { outputs };
        let cached = ReadProbeCache::new(&runner);

        assert_eq!(usb_vid_pid(&cached, 4), ("3535".into(), "6300".into()));
        assert_eq!(usb_vid_pid(&cached, 6), ("0d18".into(), "2005".into()));
        assert_eq!(cached.misses(), 1, "同一 ioreg class 只应实际执行一次");
        assert_eq!(cached.hits(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn read_probe_cache_reuses_failed_queries() {
        let runner = FakeRunner {
            outputs: HashMap::new(),
        };
        let cached = ReadProbeCache::new(&runner);
        assert_eq!(usb_vid_pid(&cached, 4), ("xxxx".into(), "xxxx".into()));
        assert_eq!(usb_vid_pid(&cached, 6), ("xxxx".into(), "xxxx".into()));
        assert_eq!(cached.misses(), 1);
        assert_eq!(cached.hits(), 1, "失败结果也应缓存，避免重复启动同一查询");
    }

    #[test]
    fn usb_vid_pid_prefers_native_probe_without_ioreg() {
        struct NativeRunner {
            calls: Cell<usize>,
        }
        impl CmdRunner for NativeRunner {
            fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
                self.calls.set(self.calls.get() + 1);
                Err(io::Error::other("native path should not call ioreg"))
            }

            fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
                Some(HardwareProbe {
                    vid: Some(0x3535),
                    pid: Some(0x6300),
                    transport: NativeTransport::Uas,
                    inquiry: None,
                })
            }
        }

        let runner = NativeRunner {
            calls: Cell::new(0),
        };
        assert_eq!(usb_vid_pid(&runner, 6), ("3535".into(), "6300".into()));
        assert_eq!(runner.calls.get(), 0);
    }

    #[test]
    fn read_probe_cache_reuses_native_hardware_probe_per_disk() {
        struct NativeCountingRunner {
            calls: Cell<usize>,
        }
        impl CmdRunner for NativeCountingRunner {
            fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
                Err(io::Error::other("not used"))
            }

            fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe> {
                self.calls.set(self.calls.get() + 1);
                Some(HardwareProbe {
                    vid: Some(disk as u16),
                    pid: Some(0x2005),
                    transport: NativeTransport::Bot,
                    inquiry: None,
                })
            }
        }

        let inner = NativeCountingRunner {
            calls: Cell::new(0),
        };
        let cached = ReadProbeCache::new(&inner);
        assert_eq!(cached.hardware_probe(6).unwrap().vid, Some(6));
        assert_eq!(cached.hardware_probe(6).unwrap().vid, Some(6));
        assert_eq!(inner.calls.get(), 1, "同一 disk 的 IOKit 探测应只执行一次");
        assert_eq!(cached.hardware_probe(7).unwrap().vid, Some(7));
        assert_eq!(inner.calls.get(), 2, "不同 disk 必须独立探测");
    }
}
