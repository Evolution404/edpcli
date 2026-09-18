use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use super::{ExtDisk, HardwareProbe, InquiryInfo, NativeTransport, PlatformKind};
use crate::common::SECTOR;
use crate::plist;
use crate::sysinfo::CmdRunner;

const DISKUTIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const IOREG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::MacOS
}

pub(super) struct WriteGuard;

pub(super) fn raw_disk_path(disk: u32) -> String {
    format!("/dev/rdisk{disk}")
}

pub(super) fn parse_disk_selector(value: &str) -> Result<u32, String> {
    let n = value
        .strip_prefix("/dev/rdisk")
        .or_else(|| value.strip_prefix("/dev/disk"))
        .unwrap_or(value);
    if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
        n.parse::<u32>()
            .map_err(|_| format!("磁盘编号超出范围: {value}"))
    } else {
        Err(format!("无法解析 macOS 磁盘选择器: {value}"))
    }
}

pub(super) const fn disk_selector_syntax() -> &'static str {
    "--disk <N|/dev/diskN|/dev/rdiskN>"
}

pub(super) fn disk_selector_value(disk: u32) -> String {
    format!("/dev/disk{disk}")
}

pub(super) fn is_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn sudo_user() -> Option<String> {
    let user = std::env::var("SUDO_USER").ok()?;
    let valid = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    valid.then_some(user)
}

fn user_home(name: &str) -> Option<PathBuf> {
    let name = CString::new(name).ok()?;
    let mut pwd = MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buf = vec![0u8; 4096];
    loop {
        let rc = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                pwd.as_mut_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut result,
            )
        };
        if rc == 0 {
            if result.is_null() {
                return None;
            }
            let pwd = unsafe { pwd.assume_init() };
            if pwd.pw_dir.is_null() {
                return None;
            }
            let bytes = unsafe { CStr::from_ptr(pwd.pw_dir) }.to_bytes();
            return Some(PathBuf::from(OsStr::from_bytes(bytes)));
        }
        if rc != libc::ERANGE || buf.len() >= 1024 * 1024 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    if let Some(user) = sudo_user() {
        user_home(&user)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

pub(super) fn has_elevation_origin() -> bool {
    sudo_user().is_some()
}

pub(super) fn probe_command_cacheable(cmd: &[&str]) -> bool {
    matches!(cmd, ["ioreg", ..])
        || matches!(cmd, ["diskutil", "list", ..] | ["diskutil", "info", ..])
}

pub(super) fn is_raw_device_path(path: &str) -> bool {
    path.starts_with("/dev/rdisk")
}

pub(super) fn raw_busy_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(16)
}

pub(super) fn sync_raw_device(file: &File) -> io::Result<()> {
    use std::os::raw::c_ulong;
    extern "C" {
        fn ioctl(fd: i32, request: c_ulong, ...) -> i32;
    }
    const DKIOCSYNCHRONIZECACHE: c_ulong = 0x2000_6416;
    let rc = unsafe { ioctl(file.as_raw_fd(), DKIOCSYNCHRONIZECACHE) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

pub(super) fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    super::macos_native::probe_disk(disk)
}

fn split_class_blocks<'a>(out: &'a str, class: &str) -> Vec<&'a str> {
    let marker_with_fields = format!("<class {class},");
    let marker_bare = format!("<class {class}>");
    let mut starts = Vec::new();
    let mut offset = 0usize;
    for line in out.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("+-o")
            && (line.contains(&marker_with_fields) || line.contains(&marker_bare))
        {
            starts.push(offset);
        }
        offset += line.len() + 1;
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

fn block_str_field(block: &str, key: &str) -> Option<String> {
    let quoted_key = format!("\"{key}\"");
    let bytes = block.as_bytes();
    let mut from = 0usize;
    while let Some(position) = block[from..].find(&quoted_key) {
        let mut index = from + position + quoted_key.len();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            from += position + 1;
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'"' {
            from += position + 1;
            continue;
        }
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != b'"' {
            index += 1;
        }
        if index < bytes.len() {
            return Some(block[value_start..index].to_string());
        }
        from += position + 1;
    }
    None
}

fn block_int_field(block: &str, key: &str) -> Option<i64> {
    let quoted_key = format!("\"{key}\"");
    let bytes = block.as_bytes();
    let mut from = 0usize;
    while let Some(position) = block[from..].find(&quoted_key) {
        let mut index = from + position + quoted_key.len();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            from += position + 1;
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let digits_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index > digits_start {
            return block[digits_start..index].parse().ok();
        }
        from += position + 1;
    }
    None
}

fn ioreg_block(runner: &dyn CmdRunner, class: &str, disk: u32) -> Option<String> {
    let out = runner
        .check_output(&["ioreg", "-r", "-c", class, "-l"], IOREG_TIMEOUT)
        .ok()?;
    let marker = format!("\"BSD Name\" = \"disk{disk}\"");
    split_class_blocks(&out, class)
        .into_iter()
        .find(|block| block.contains(&marker))
        .map(str::to_string)
}

pub(super) fn fallback_hardware_probe(runner: &dyn CmdRunner, disk: u32) -> Option<HardwareProbe> {
    let usb_block = ioreg_block(runner, "IOUSBHostDevice", disk);
    let vid = usb_block
        .as_deref()
        .and_then(|block| block_int_field(block, "idVendor"))
        .and_then(|value| u16::try_from(value).ok());
    let pid = usb_block
        .as_deref()
        .and_then(|block| block_int_field(block, "idProduct"))
        .and_then(|value| u16::try_from(value).ok());

    let transport = if ioreg_block(runner, "IOUSBMassStorageUASDriver", disk).is_some() {
        NativeTransport::Uas
    } else if ioreg_block(runner, "IOUSBMassStorageInterfaceNub", disk).is_some()
        || ioreg_block(runner, "IOUSBMassStorageDriver", disk).is_some()
    {
        NativeTransport::Bot
    } else {
        NativeTransport::Unknown
    };

    let inquiry = [
        "IOSCSITargetDevice",
        "IOSCSILogicalUnitNub",
        "IOSCSIPeripheralDeviceNub",
    ]
    .into_iter()
    .find_map(|class| {
        let block = ioreg_block(runner, class, disk)?;
        let vendor = block_str_field(&block, "Vendor Identification")?;
        if vendor.is_empty() {
            return None;
        }
        Some(InquiryInfo {
            vendor,
            product: block_str_field(&block, "Product Identification").unwrap_or_default(),
            revision: block_str_field(&block, "Product Revision Level").unwrap_or_default(),
        })
    });

    (vid.is_some() || pid.is_some() || transport != NativeTransport::Unknown || inquiry.is_some())
        .then_some(HardwareProbe {
            vid,
            pid,
            transport,
            inquiry,
        })
}

fn whole_disk_number(identifier: &str) -> Option<u32> {
    let rest = identifier.strip_prefix("disk")?;
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn system_disk_numbers(runner: &dyn CmdRunner) -> Option<Vec<u32>> {
    let out = runner
        .check_output(&["diskutil", "info", "-plist", "/"], DISKUTIL_TIMEOUT)
        .ok()?;
    let info = plist::parse(&out).ok()?;

    if let Some(stores) = info
        .get("APFSPhysicalStores")
        .and_then(|value| value.as_arr())
    {
        if stores.is_empty() {
            return None;
        }
        let mut disks = Vec::new();
        for store in stores {
            let identifier = store.get("APFSPhysicalStore")?.as_str()?;
            let disk = whole_disk_number(identifier)?;
            if !disks.contains(&disk) {
                disks.push(disk);
            }
        }
        return (!disks.is_empty()).then_some(disks);
    }

    let filesystem = info
        .get("FilesystemType")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if filesystem.eq_ignore_ascii_case("apfs") {
        // APFS 根卷却无法得到 PhysicalStores 时，无法确认实际物理承载盘。
        return None;
    }

    let identifier = info
        .get("ParentWholeDisk")
        .and_then(|value| value.as_str())
        .or_else(|| {
            info.get("DeviceIdentifier")
                .and_then(|value| value.as_str())
        })?;
    Some(vec![whole_disk_number(identifier)?])
}

pub(super) fn is_system_disk(runner: &dyn CmdRunner, disk: u32) -> bool {
    let Some(system_disks) = system_disk_numbers(runner) else {
        // 无法确认根文件系统实际落在哪块物理盘时，一律按系统盘处理。
        return true;
    };
    system_disks.contains(&disk)
}

fn disk_info(runner: &dyn CmdRunner, disk: u32) -> Option<plist::Plist> {
    let name = format!("disk{disk}");
    let out = runner
        .check_output(&["diskutil", "info", "-plist", &name], DISKUTIL_TIMEOUT)
        .ok()?;
    plist::parse(&out).ok()
}

pub(super) fn disk_total_sectors(runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    let info = disk_info(runner, disk)?;
    let bytes = ["DiskSize", "TotalSize", "Size"]
        .iter()
        .find_map(|key| info.get(key).and_then(|v| v.as_int()).filter(|&v| v > 0))?;
    Some(bytes as u64 / SECTOR as u64)
}

pub(super) fn usb_vid_pid(runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    if let Some(probe) = hardware_probe(disk) {
        if let (Some(vid), Some(pid)) = (probe.vid, probe.pid) {
            return (format!("{vid:04x}"), format!("{pid:04x}"));
        }
    }
    let Some(block) = ioreg_block(runner, "IOUSBHostDevice", disk) else {
        return ("xxxx".into(), "xxxx".into());
    };
    if let (Some(vid), Some(pid)) = (
        block_int_field(&block, "idVendor"),
        block_int_field(&block, "idProduct"),
    ) {
        return (format!("{vid:04x}"), format!("{pid:04x}"));
    }
    ("xxxx".into(), "xxxx".into())
}

fn external_disk_info(runner: &dyn CmdRunner, disk: u32) -> Option<ExtDisk> {
    let info = disk_info(runner, disk)?;
    let whole = info
        .get("WholeDisk")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let internal = info
        .get("Internal")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let virtual_disk = info.get("VirtualOrPhysical").and_then(|v| v.as_str()) == Some("Virtual");
    if !whole || internal || virtual_disk {
        return None;
    }
    let proto = info
        .get("BusProtocol")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let size = ["TotalSize", "DiskSize", "Size"]
        .iter()
        .find_map(|key| info.get(key).and_then(|v| v.as_int()).filter(|&v| v > 0))
        .unwrap_or(0) as u64;
    let (vid, pid) = if proto == "USB" {
        usb_vid_pid(runner, disk)
    } else {
        ("xxxx".into(), "xxxx".into())
    };
    Some(ExtDisk {
        n: disk,
        size,
        vid,
        pid,
        proto,
    })
}

pub(super) fn list_external_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    let Ok(out) = runner.check_output(&["diskutil", "list", "-plist"], DISKUTIL_TIMEOUT) else {
        return vec![];
    };
    let Ok(list) = plist::parse(&out) else {
        return vec![];
    };
    list.get("AllDisks")
        .and_then(|v| v.as_arr())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .filter_map(|name| {
            let rest = name.strip_prefix("disk")?;
            (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
                .then(|| rest.parse::<u32>().ok())
                .flatten()
        })
        .filter_map(|disk| external_disk_info(runner, disk))
        .collect()
}

pub(super) fn prepare_write(runner: &dyn CmdRunner, disk: u32) -> io::Result<WriteGuard> {
    if is_system_disk(runner, disk) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "目标磁盘承载 macOS 根文件系统或系统盘身份不可确认，拒绝写盘",
        ));
    }
    runner
        .check_output(
            &["diskutil", "unmountDisk", "force", &format!("disk{disk}")],
            DISKUTIL_TIMEOUT,
        )
        .map(|_| WriteGuard)
}

pub(super) const fn elevation_label() -> &'static str {
    "sudo"
}

pub(super) fn run_elevated(exe: &Path, argv: &[String], sentinel: &str) -> io::Result<i32> {
    let mut cmd = Command::new("sudo");
    cmd.arg(exe);
    for arg in argv {
        if arg != sentinel {
            cmd.arg(arg);
        }
    }
    cmd.arg(sentinel);
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(crate::common::EXIT_IO))
}

#[cfg(test)]
mod selector_tests {
    use super::*;

    struct RootInfoRunner {
        plist: String,
    }

    impl CmdRunner for RootInfoRunner {
        fn check_output(&self, cmd: &[&str], _timeout: std::time::Duration) -> io::Result<String> {
            if cmd == ["diskutil", "info", "-plist", "/"] {
                Ok(self.plist.clone())
            } else {
                Err(io::Error::other("unexpected command"))
            }
        }
    }

    #[test]
    fn parses_native_macos_disk_selectors() {
        assert_eq!(parse_disk_selector("4").unwrap(), 4);
        assert_eq!(parse_disk_selector("/dev/disk4").unwrap(), 4);
        assert_eq!(parse_disk_selector("/dev/rdisk4").unwrap(), 4);
        assert!(parse_disk_selector("/dev/disk4s1").is_err());
    }

    #[test]
    fn ioreg_parser_keeps_nested_children_inside_parent_block() {
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
        assert_eq!(block_str_field(blocks[1], "Vendor Identification"), None);
    }

    #[test]
    fn ioreg_parser_reads_decimal_fields_and_product_named_roots() {
        let out = "\
+-o Keyboard Tal@14100000  <class IOUSBHostDevice, id 0x100002a01, retain 14>\n\
  |   \"idVendor\" = 1452\n\
  |   \"idProduct\" = 610\n\
+-o USB DISK@01200000  <class IOUSBHostDevice, id 0x100002af0, retain 15>\n\
  |   \"idVendor\" = 13621\n\
  |   \"idProduct\" = 25344\n\
  |   \"USB Product Name\" = \"Mass Storage\"\n\
  +-o USB DISK Media  <class IOMedia, id 0x100002b17>\n\
    |   \"BSD Name\" = \"disk6\"\n";
        let blocks = split_class_blocks(out, "IOUSBHostDevice");
        assert_eq!(blocks.len(), 2);
        assert_eq!(block_int_field(blocks[1], "idVendor"), Some(13621));
        assert_eq!(block_int_field(blocks[1], "idProduct"), Some(25344));
        assert_eq!(
            block_str_field(blocks[1], "USB Product Name").as_deref(),
            Some("Mass Storage")
        );
        let no_root = "  |   \"idVendor\" = 1\n  |   \"BSD Name\" = \"disk6\"\n";
        assert_eq!(split_class_blocks(no_root, "IOUSBHostDevice").len(), 1);
    }

    #[test]
    fn system_disk_uses_apfs_physical_store_not_fixed_disk_number() {
        let runner = RootInfoRunner {
            plist: r#"<plist><dict>
                <key>FilesystemType</key><string>apfs</string>
                <key>ParentWholeDisk</key><string>disk9</string>
                <key>APFSPhysicalStores</key><array>
                    <dict><key>APFSPhysicalStore</key><string>disk6s2</string></dict>
                </array>
            </dict></plist>"#
                .into(),
        };
        assert!(is_system_disk(&runner, 6));
        assert!(!is_system_disk(&runner, 0));
    }

    #[test]
    fn system_disk_detection_fails_closed_when_apfs_store_is_unknown() {
        let runner = RootInfoRunner {
            plist: r#"<plist><dict>
                <key>FilesystemType</key><string>apfs</string>
                <key>ParentWholeDisk</key><string>disk9</string>
            </dict></plist>"#
                .into(),
        };
        assert!(is_system_disk(&runner, 6));
    }
}
