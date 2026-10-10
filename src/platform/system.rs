//! 跨平台系统探测门面与可注入命令执行器。
//! 操作系统细节由 `platform` 实现；业务层只依赖这里的统一接口。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::time::Duration;

use crate::platform::ExtDisk;
use crate::platform::HardwareProbe;

use crate::ports::CmdRunner;

pub struct SysRunner;

impl crate::ports::CommandExecutor for SysRunner {
    fn run_command(
        &self,
        cmd: &[&str],
        timeout: Duration,
    ) -> io::Result<crate::ports::CommandOutcome> {
        crate::infrastructure::process::run_command(cmd, timeout)
    }
}

/// Host adapter for the narrow observation and lease ports. It never caches
/// the facts used by TargetSession before unmount or after reopen.
pub struct SystemDeviceAccess<'a> {
    pub runner: &'a dyn CmdRunner,
}

impl crate::ports::DeviceObserver for SystemDeviceAccess<'_> {
    fn is_system_disk(&self, disk: u32) -> bool {
        crate::platform::is_system_disk(self.runner, disk)
    }
    fn is_external_usb_whole(&self, disk: u32) -> bool {
        usb_disk(self.runner, disk).is_some()
            || (crate::platform::include_virtual()
                && crate::platform::confirmed_virtual_disk_image(self.runner, disk))
    }
    fn device_geometry(
        &self,
        disk: u32,
    ) -> Option<crate::domain::hardware::ObservedDeviceGeometry> {
        device_geometry(self.runner, disk)
    }
    fn total_sectors(&self, disk: u32) -> Option<u64> {
        disk_total_sectors(self.runner, disk)
    }
    fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe> {
        self.runner
            .hardware_probe(disk)
            .or_else(|| crate::platform::fallback_hardware_probe(self.runner, disk))
    }
    fn hardware_serial(&self, disk: u32) -> Option<String> {
        self.runner.hardware_serial(disk)
    }
}

static WRITE_LEASES: std::sync::Mutex<Option<std::collections::HashSet<u32>>> =
    std::sync::Mutex::new(None);

struct SystemWriteLease {
    disk: u32,
    guard: Option<crate::platform::WriteGuard>,
}
impl crate::ports::WriteLease for SystemWriteLease {}
impl SystemWriteLease {
    fn reserve(disk: u32) -> io::Result<Self> {
        let mut leases = WRITE_LEASES
            .lock()
            .map_err(|_| io::Error::other("写租约状态不可用"))?;
        if !leases.get_or_insert_with(Default::default).insert(disk) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("disk{disk} 已持有写租约"),
            ));
        }
        Ok(Self { disk, guard: None })
    }
}
impl Drop for SystemWriteLease {
    fn drop(&mut self) {
        // Platform locks must be released before another in-process writer enters.
        self.guard = None;
        let mut leases = WRITE_LEASES
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(leases) = leases.as_mut() {
            leases.remove(&self.disk);
        }
    }
}

impl crate::ports::WriteLeaseProvider for SystemDeviceAccess<'_> {
    fn acquire_write_lease(&self, disk: u32) -> io::Result<Box<dyn crate::ports::WriteLease>> {
        let mut lease = SystemWriteLease::reserve(disk)?;
        lease.guard = Some(prepare_write(self.runner, disk)?);
        Ok(Box::new(lease))
    }
}

impl CmdRunner for SysRunner {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String> {
        crate::infrastructure::process::check_output(cmd, timeout)
    }

    fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe> {
        crate::platform::hardware_probe(disk)
    }

    fn hardware_serial(&self, disk: u32) -> Option<String> {
        crate::platform::hardware_serial(disk)
    }

    fn device_geometry(
        &self,
        disk: u32,
    ) -> Option<crate::domain::hardware::ObservedDeviceGeometry> {
        crate::platform::device_geometry(self, disk)
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
/// 该类型只用于 list/info/inspect/completion；restore/provision 的安全终验继续使用
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

    fn device_geometry(
        &self,
        disk: u32,
    ) -> Option<crate::domain::hardware::ObservedDeviceGeometry> {
        self.inner.device_geometry(disk)
    }

    fn hardware_serial(&self, disk: u32) -> Option<String> {
        // Keep raw serial transient; canonical observation hashes it immediately.
        self.inner.hardware_serial(disk)
    }
}

/// 当前平台整盘总扇区数；失败/缺失返回 None。
pub fn disk_total_sectors(runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    crate::platform::disk_total_sectors(runner, disk)
}

pub fn device_geometry(
    runner: &dyn CmdRunner,
    disk: u32,
) -> Option<crate::domain::hardware::ObservedDeviceGeometry> {
    runner
        .device_geometry(disk)
        .or_else(|| crate::platform::fallback_device_geometry(runner, disk))
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

    #[test]
    #[cfg(target_os = "macos")]
    fn geometry_uses_observed_block_size_without_a_512_default() {
        for (size, logical, supported) in [
            (8192, Some(512), true),
            (8192, Some(4096), false),
            (8192, None, false),
            (8193, Some(512), false),
        ] {
            let block = logical
                .map(|n| format!("<key>DeviceBlockSize</key><integer>{n}</integer>"))
                .unwrap_or_default();
            let runner = FakeRunner { outputs: HashMap::from([("diskutil info -plist disk6".into(), format!("<plist><dict><key>DiskSize</key><integer>{size}</integer>{block}<key>PhysicalBlockSize</key><integer>4096</integer></dict></plist>"))]) };
            let geometry = device_geometry(&runner, 6).unwrap();
            assert_eq!(geometry.capacity_bytes, size);
            assert_eq!(geometry.logical_sector_bytes, logical);
            assert_eq!(geometry.physical_sector_bytes, Some(4096));
            assert_eq!(geometry.writable_protocol_sectors().is_ok(), supported);
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
                "<plist version=\"1.0\"><dict><key>WholeDisk</key><{}/><key>Internal</key><{}/><key>BusProtocol</key><string>{}</string><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>{}</integer>{}</dict></plist>",
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
            info(
                "disk8",
                "Disk Image",
                false,
                "<key>VirtualOrPhysical</key><string>Virtual</string>",
                1_000_000_000,
            ),
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
        assert_eq!(
            (disks[2].vid.as_str(), disks[2].pid.as_str()),
            ("xxxx", "xxxx")
        ); // 非USB 无 VID/PID
           // USB 盘走 ioreg 查 VID/PID(罐头里没有 ioreg → 回退 xxxx)
        assert_eq!(
            (disks[0].vid.as_str(), disks[0].pid.as_str()),
            ("xxxx", "xxxx")
        );

        let usb = list_usb_disks(&runner);
        assert_eq!(usb.iter().map(|d| d.n).collect::<Vec<_>>(), vec![4, 6]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disk_total_sectors_prefers_disksize() {
        let mut m = HashMap::new();
        m.insert(
            "diskutil info -plist disk6".to_string(),
            "<plist version=\"1.0\"><dict><key>DiskSize</key><integer>62914560000</integer><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>999</integer></dict></plist>".to_string(),
        );
        let runner = FakeRunner { outputs: m };
        assert_eq!(disk_total_sectors(&runner, 6), Some(62914560000 / 512));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disk_total_sectors_prefers_physical_size_over_smaller_volume_size() {
        let mut m = HashMap::new();
        m.insert(
            "diskutil info -plist disk4".to_string(),
            "<plist version=\"1.0\"><dict><key>IOKitSize</key><integer>15502147584</integer><key>Size</key><integer>15502147584</integer><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>15502143488</integer></dict></plist>".to_string(),
        );
        let runner = FakeRunner { outputs: m };
        assert_eq!(disk_total_sectors(&runner, 4), Some(30_277_632));
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
                    windows_pnp_instance_id: None,
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
                    windows_pnp_instance_id: None,
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

    #[test]
    fn read_probe_cache_preserves_hardware_serial_for_canonical_identity() {
        struct SerialRunner {
            calls: Cell<usize>,
        }
        impl CmdRunner for SerialRunner {
            fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
                Err(io::Error::other("not used"))
            }

            fn hardware_serial(&self, disk: u32) -> Option<String> {
                self.calls.set(self.calls.get() + 1);
                Some(format!("SERIAL-{disk}"))
            }
        }

        let inner = SerialRunner {
            calls: Cell::new(0),
        };
        let cached = ReadProbeCache::new(&inner);
        assert_eq!(cached.hardware_serial(4).as_deref(), Some("SERIAL-4"));
        assert_eq!(inner.calls.get(), 1);
    }
}

#[cfg(test)]
mod lease_tests {
    use super::SystemWriteLease;
    #[test]
    fn host_lease_excludes_same_disk_and_releases_on_drop_without_device_io() {
        let first = SystemWriteLease::reserve(u32::MAX).unwrap();
        assert!(SystemWriteLease::reserve(u32::MAX).is_err());
        let different = SystemWriteLease::reserve(u32::MAX - 1).unwrap();
        drop(first);
        let reacquired = SystemWriteLease::reserve(u32::MAX).unwrap();
        drop(reacquired);
        drop(different);
    }
}
