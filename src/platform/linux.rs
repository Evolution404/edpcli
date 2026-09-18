use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;
use std::process::Command;

use super::{ExtDisk, HardwareProbe, InquiryInfo, NativeTransport, PlatformKind};
use crate::common::SECTOR;
use crate::sysinfo::CmdRunner;

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Linux
}

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

pub(super) fn unmount_disk(_runner: &dyn CmdRunner, _disk: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Linux 写盘卸载 backend 尚未启用；为避免写挂载磁盘已拒绝操作",
    ))
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
