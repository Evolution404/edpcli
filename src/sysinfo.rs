//! macOS 系统信息: diskutil(-plist) / ioreg 查询。
//! 全部子进程调用收在 CmdRunner 之后 — 这是测试注入罐头输出的缝
//! (Python 版以 mock.patch(subprocess.check_output) 达成同一目的)。

use std::io::{self, ErrorKind};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::plist;

const DISKUTIL_TIMEOUT: Duration = Duration::from_secs(10);
const IOREG_TIMEOUT: Duration = Duration::from_secs(15);

/// 外接整盘信息: (盘号, 字节数, vid, pid, 总线协议)。
#[derive(Debug, Clone)]
pub struct ExtDisk {
    pub n: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
}

/// 子进程执行抽象: 成功返回 stdout 文本(非零退出/超时/启动失败均为 Err)。
/// 等价 Python subprocess.check_output(text=True, errors='ignore', timeout=…)。
pub trait CmdRunner {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String>;
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
        let mut stdout = child.stdout.take().unwrap();
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
                    return Err(io::Error::new(ErrorKind::Other, "读取输出失败"))
                }
            }
        };
        let status = child.wait()?;
        if status.success() {
            Ok(output)
        } else {
            Err(io::Error::new(
                ErrorKind::Other,
                format!("{} 退出码 {:?}", cmd[0], status.code()),
            ))
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// ioreg 文本解析(无 regex; ioreg 输出行结构化, 逐行扫描)
// ══════════════════════════════════════════════════════════════════
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

fn bsd_name_marker(disk: u32) -> String {
    format!("\"BSD Name\" = \"disk{}\"", disk)
}

/// 盘总扇区数(diskutil DiskSize/512); 失败/缺失返回 None(显示为 unknown)。
pub fn disk_total_sectors(runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    let out = runner
        .check_output(&["diskutil", "info", "-plist", &format!("disk{}", disk)], DISKUTIL_TIMEOUT)
        .ok()?;
    let p = plist::parse(&out).ok()?;
    // Python: info.get('DiskSize') or info.get('TotalSize') or 0; if ds: — 0 视同缺失
    let ds = ["DiskSize", "TotalSize"]
        .iter()
        .find_map(|k| p.get(k).and_then(|v| v.as_int()).filter(|&v| v != 0))?;
    Some(ds as u64 / crate::common::SECTOR as u64)
}

/// USB VID/PID(hex4); 失败返回 ("xxxx","xxxx")。
pub fn usb_vid_pid(runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    let out = match runner.check_output(&["ioreg", "-r", "-c", "IOUSBHostDevice", "-l"], IOREG_TIMEOUT) {
        Ok(o) => o,
        Err(_) => return ("xxxx".into(), "xxxx".into()),
    };
    let want = bsd_name_marker(disk);
    for b in split_class_blocks(&out, "IOUSBHostDevice") {
        if !b.contains(&want) {
            continue;
        }
        if let (Some(v), Some(p)) = (
            block_int_field(b, "idVendor"),
            block_int_field(b, "idProduct"),
        ) {
            return (format!("{:04x}", v), format!("{:04x}", p));
        }
    }
    ("xxxx".into(), "xxxx".into())
}

// ══════════════════════════════════════════════════════════════════
// 外接盘枚举
// ══════════════════════════════════════════════════════════════════
/// `disk<纯数字>` 解析为盘号(排除 disk4s1 等分区)。
fn whole_disk_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("disk")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// 枚举全部外接整盘(disk≥2)。系统盘(disk<2)、分区、内部盘与虚拟盘(DMG)不进入。
pub fn list_external_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    let out = match runner.check_output(&["diskutil", "list", "-plist"], DISKUTIL_TIMEOUT) {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let p = match plist::parse(&out) {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let empty: Vec<plist::Plist> = Vec::new();
    let all = p.get("AllDisks").and_then(|a| a.as_arr()).unwrap_or(&empty);
    let mut disks = Vec::new();
    for name in all.iter().filter_map(|d| d.as_str()) {
        let Some(n) = whole_disk_number(name) else { continue };
        if n < 2 {
            continue; // 系统盘防护
        }
        let Ok(info_out) = runner.check_output(&["diskutil", "info", "-plist", name], DISKUTIL_TIMEOUT)
        else {
            continue;
        };
        let Ok(info) = plist::parse(&info_out) else { continue };
        let whole = info.get("WholeDisk").and_then(|v| v.as_bool()).unwrap_or(false);
        let internal = info.get("Internal").and_then(|v| v.as_bool()).unwrap_or(false);
        if !whole || internal {
            continue;
        }
        if info.get("VirtualOrPhysical").and_then(|v| v.as_str()) == Some("Virtual") {
            continue; // DMG 等虚拟盘, 非物理介质
        }
        let proto = info
            .get("BusProtocol")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        // Python: TotalSize or DiskSize or Size or 0 (0 视同缺失, 逐级回退)
        let size = ["TotalSize", "DiskSize", "Size"]
            .iter()
            .find_map(|k| info.get(k).and_then(|v| v.as_int()).filter(|&v| v != 0))
            .unwrap_or(0) as u64;
        let (vid, pid) = if proto == "USB" {
            usb_vid_pid(runner, n)
        } else {
            ("xxxx".into(), "xxxx".into())
        };
        disks.push(ExtDisk { n, size, vid, pid, proto });
    }
    disks
}

/// 本工具可操作的外接 USB 整盘子集(供自动选盘)。
pub fn list_usb_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    list_external_disks(runner)
        .into_iter()
        .filter(|d| d.proto == "USB")
        .collect()
}

/// 强制卸载整盘(写入前; 结果忽略 — 卸不掉时写回校验会兜底)。
pub fn unmount_disk(runner: &dyn CmdRunner, disk: u32) {
    let _ = runner.check_output(
        &["diskutil", "unmountDisk", "force", &format!("disk{}", disk)],
        Duration::from_secs(60),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 罐头 CmdRunner: (子命令前缀) → 预置输出。
    struct FakeRunner {
        outputs: HashMap<String, String>,
    }
    impl CmdRunner for FakeRunner {
        fn check_output(&self, cmd: &[&str], _t: Duration) -> io::Result<String> {
            self.outputs
                .get(&cmd.join(" "))
                .cloned()
                .ok_or_else(|| io::Error::new(ErrorKind::Other, format!("无罐头: {}", cmd.join(" "))))
        }
    }

    // 与 Python test_list.py 相同的 8 盘矩阵: disk4=USB(未识别), disk6=USB(cems),
    // disk7=Thunderbolt, disk0/1=系统盘, disk8=DMG虚拟盘
    fn plist_str(s: &str) -> String {
        format!("<string>{}</string>", s)
    }
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
}
