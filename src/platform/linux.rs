use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;
use std::process::Command;
use std::collections::HashSet;
use std::ffi::CString;

use super::{ExtDisk, HardwareProbe, InquiryInfo, NativeTransport, PlatformKind};
use crate::common::SECTOR;
use crate::sysinfo::CmdRunner;

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Linux
}

pub(super) struct WriteGuard;

pub(super) fn raw_disk_path(disk: u32) -> String {
    linux_block_names()
        .get(disk.saturating_sub(2) as usize)
        .map(|name| format!("/dev/{name}"))
        .unwrap_or_else(|| format!("/dev/edpcli-invalid-disk{disk}"))
}

pub(super) fn is_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(super) fn is_raw_device_path(path: &str) -> bool {
    path.starts_with("/dev/")
}

pub(super) fn raw_busy_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::EBUSY)
}

pub(super) fn sync_raw_device(file: &File) -> io::Result<()> {
    file.sync_all()
}

pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn linux_block_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/block")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            !name.starts_with("loop")
                && !name.starts_with("ram")
                && !name.starts_with("zram")
                && !name.starts_with("dm-")
                && !std::path::Path::new("/sys/class/block")
                    .join(name)
                    .join("partition")
                    .exists()
        })
        .collect();
    names.sort();
    names
}

fn block_name(disk: u32) -> Option<String> {
    linux_block_names().get(disk.checked_sub(2)? as usize).cloned()
}

fn disk_device_numbers(name: &str) -> HashSet<String> {
    let Ok(parent) = std::fs::canonicalize(format!("/sys/class/block/{name}")) else {
        return HashSet::new();
    };
    let mut devices = HashSet::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/block") else {
        return devices;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(entry_name) = entry_name.to_str() else { continue };
        let entry_path = format!("/sys/class/block/{entry_name}");
        let Ok(real) = std::fs::canonicalize(&entry_path) else { continue };
        let is_parent = entry_name == name;
        let is_partition = std::path::Path::new(&entry_path).join("partition").exists()
            && real.starts_with(&parent);
        if is_parent || is_partition {
            if let Some(dev) = read_trim(std::path::Path::new(&entry_path).join("dev")) {
                devices.insert(dev);
            }
        }
    }
    devices
}

fn decode_mount_field(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(|b| matches!(b, b'0'..=b'7'))
        {
            let value = (bytes[i + 1] - b'0') * 64
                + (bytes[i + 2] - b'0') * 8
                + (bytes[i + 3] - b'0');
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mounted_points_for_devices(devices: &HashSet<String>) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return vec![];
    };
    let mut points = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 5 && devices.contains(fields[2]) {
            points.push(decode_mount_field(fields[4]));
        }
    }
    points.sort_by_key(|path| std::cmp::Reverse(path.len()));
    points.dedup();
    points
}

pub(super) fn is_system_disk(disk: u32) -> bool {
    let Some(name) = block_name(disk) else { return true };
    let devices = disk_device_numbers(&name);
    mounted_points_for_devices(&devices).iter().any(|point| point == "/")
}

fn read_trim(path: impl AsRef<std::path::Path>) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn usb_ancestor(name: &str) -> Option<std::path::PathBuf> {
    let mut path = std::fs::canonicalize(format!("/sys/class/block/{name}/device")).ok()?;
    loop {
        if path.join("idVendor").is_file() && path.join("idProduct").is_file() {
            return Some(path);
        }
        if !path.pop() {
            return None;
        }
    }
}

pub(super) fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    let name = block_name(disk)?;
    let device = std::path::Path::new("/sys/class/block").join(&name).join("device");
    let vendor = read_trim(device.join("vendor")).unwrap_or_default();
    let product = read_trim(device.join("model")).unwrap_or_default();
    let revision = read_trim(device.join("rev")).unwrap_or_default();
    let usb = usb_ancestor(&name);
    let parse_hex = |value: Option<String>| u16::from_str_radix(value?.trim(), 16).ok();
    let vid = parse_hex(usb.as_ref().and_then(|p| read_trim(p.join("idVendor"))));
    let pid = parse_hex(usb.as_ref().and_then(|p| read_trim(p.join("idProduct"))));
    let driver = std::fs::canonicalize(device.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
    let transport = match driver.as_deref() {
        Some(value) if value.contains("uas") => NativeTransport::Uas,
        Some(value) if value.contains("usb") => NativeTransport::Bot,
        _ if usb.is_some() => NativeTransport::Unknown,
        _ => NativeTransport::Unknown,
    };
    let inquiry = (!vendor.is_empty()).then_some(InquiryInfo {
        vendor,
        product,
        revision,
    });
    Some(HardwareProbe {
        vid,
        pid,
        transport,
        inquiry,
    })
}

pub(super) fn disk_total_sectors(_runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    let name = block_name(disk)?;
    read_trim(format!("/sys/class/block/{name}/size"))?.parse().ok()
}

pub(super) fn usb_vid_pid(_runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    let Some(probe) = hardware_probe(disk) else {
        return ("xxxx".into(), "xxxx".into());
    };
    match (probe.vid, probe.pid) {
        (Some(vid), Some(pid)) => (format!("{vid:04x}"), format!("{pid:04x}")),
        _ => ("xxxx".into(), "xxxx".into()),
    }
}

pub(super) fn list_external_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    linux_block_names()
        .into_iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let disk = index as u32 + 2;
            let class = std::path::Path::new("/sys/class/block").join(&name);
            let removable = read_trim(class.join("removable")).as_deref() == Some("1");
            let probe = hardware_probe(disk);
            let usb = probe.as_ref().and_then(|p| p.vid).is_some();
            if !removable && !usb {
                return None;
            }
            let sectors = disk_total_sectors(runner, disk).unwrap_or(0);
            let (vid, pid) = usb_vid_pid(runner, disk);
            Some(ExtDisk {
                n: disk,
                size: sectors.saturating_mul(SECTOR as u64),
                vid,
                pid,
                proto: if usb { "USB".into() } else { "Removable".into() },
            })
        })
        .collect()
}

pub(super) fn prepare_write(_runner: &dyn CmdRunner, disk: u32) -> io::Result<WriteGuard> {
    let name = block_name(disk)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Linux 块设备不存在"))?;
    let devices = disk_device_numbers(&name);
    if devices.is_empty() {
        return Err(io::Error::other("无法确认 Linux 块设备 major:minor，拒绝写盘"));
    }
    if mounted_points_for_devices(&devices).iter().any(|point| point == "/") {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "目标磁盘承载根文件系统，拒绝卸载"));
    }
    for mountpoint in mounted_points_for_devices(&devices) {
        let path = CString::new(mountpoint.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "挂载点包含 NUL"))?;
        let rc = unsafe { libc::umount2(path.as_ptr(), 0) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let remaining = mounted_points_for_devices(&devices);
    if !remaining.is_empty() {
        return Err(io::Error::other(format!(
            "卸载后仍有挂载点: {}",
            remaining.join(", ")
        )));
    }
    Ok(WriteGuard)
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
mod tests {
    use super::*;

    #[test]
    fn mountinfo_escapes_are_decoded() {
        assert_eq!(decode_mount_field(r"/media/My\040USB"), "/media/My USB");
        assert_eq!(decode_mount_field(r"/tmp/a\134b"), r"/tmp/a\b");
    }

    #[test]
    fn invalid_selector_is_fail_closed_for_system_disk_check() {
        assert!(is_system_disk(u32::MAX));
    }
}
