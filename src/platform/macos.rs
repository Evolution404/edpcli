use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::path::Path;
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
        n.parse::<u32>().map_err(|_| format!("磁盘编号超出范围: {value}"))
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

fn ioreg_block(
    runner: &dyn CmdRunner,
    class: &str,
    disk: u32,
) -> Option<String> {
    let out = runner
        .check_output(&["ioreg", "-r", "-c", class, "-l"], IOREG_TIMEOUT)
        .ok()?;
    let marker = format!("\"BSD Name\" = \"disk{disk}\"");
    split_class_blocks(&out, class)
        .into_iter()
        .find(|block| block.contains(&marker))
        .map(str::to_string)
}

pub(super) fn fallback_hardware_probe(
    runner: &dyn CmdRunner,
    disk: u32,
) -> Option<HardwareProbe> {
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

    (vid.is_some()
        || pid.is_some()
        || transport != NativeTransport::Unknown
        || inquiry.is_some())
    .then_some(HardwareProbe {
        vid,
        pid,
        transport,
        inquiry,
    })
}

pub(super) fn is_system_disk(disk: u32) -> bool {
    disk < 2
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
    if disk < 2 {
        return None;
    }
    let info = disk_info(runner, disk)?;
    let whole = info.get("WholeDisk").and_then(|v| v.as_bool()).unwrap_or(false);
    let internal = info.get("Internal").and_then(|v| v.as_bool()).unwrap_or(false);
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

    #[test]
    fn parses_native_macos_disk_selectors() {
        assert_eq!(parse_disk_selector("4").unwrap(), 4);
        assert_eq!(parse_disk_selector("/dev/disk4").unwrap(), 4);
        assert_eq!(parse_disk_selector("/dev/rdisk4").unwrap(), 4);
        assert!(parse_disk_selector("/dev/disk4s1").is_err());
    }
}
