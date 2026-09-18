use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
#[cfg(feature = "ci-virtual-disk")]
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

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

pub(super) fn parse_disk_selector(value: &str) -> Result<u32, String> {
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value
            .parse::<u32>()
            .map_err(|_| format!("磁盘编号超出范围: {value}"));
    }
    let name = value.strip_prefix("/dev/").unwrap_or(value);
    linux_block_names()
        .iter()
        .position(|candidate| candidate == name)
        .map(|index| index as u32 + 2)
        .ok_or_else(|| format!("无法解析 Linux 整盘选择器: {value}"))
}

pub(super) const fn disk_selector_syntax() -> &'static str {
    "--disk <N|/dev/sdX|/dev/nvmeXnY>"
}

pub(super) fn disk_selector_value(disk: u32) -> String {
    raw_disk_path(disk)
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
            return Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)));
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

pub(super) fn probe_command_cacheable(_cmd: &[&str]) -> bool {
    false
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

fn linux_block_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
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
    })
}

fn block_name(disk: u32) -> Option<String> {
    linux_block_names()
        .get(disk.checked_sub(2)? as usize)
        .cloned()
}

fn disk_device_numbers(name: &str) -> io::Result<HashSet<String>> {
    let parent = std::fs::canonicalize(format!("/sys/class/block/{name}"))?;
    let mut devices = HashSet::new();
    let mut related_names = HashSet::new();
    for entry in std::fs::read_dir("/sys/class/block")? {
        let entry = entry?;
        let entry_name = entry.file_name();
        let Some(entry_name) = entry_name.to_str() else {
            continue;
        };
        let entry_path = format!("/sys/class/block/{entry_name}");
        let Ok(real) = std::fs::canonicalize(&entry_path) else {
            continue;
        };
        let is_parent = entry_name == name;
        let is_partition = std::path::Path::new(&entry_path).join("partition").exists()
            && real.starts_with(&parent);
        if is_parent || is_partition {
            related_names.insert(entry_name.to_string());
        }
    }

    // 根文件系统可能位于 dm-crypt/LVM/md 等 holder 上。递归纳入 holder，
    // 避免底层物理盘被误判成“非系统盘”。
    let mut pending: Vec<String> = related_names.iter().cloned().collect();
    while let Some(current) = pending.pop() {
        let holders = std::path::Path::new("/sys/class/block")
            .join(&current)
            .join("holders");
        let Ok(entries) = std::fs::read_dir(&holders) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let Some(holder) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if related_names.insert(holder.clone()) {
                pending.push(holder);
            }
        }
    }

    for related in related_names {
        let dev_path = std::path::Path::new("/sys/class/block")
            .join(&related)
            .join("dev");
        let dev = std::fs::read_to_string(&dev_path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("无法读取 {}: {error}", dev_path.display()),
            )
        })?;
        let dev = dev.trim();
        if dev.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} 的 major:minor 为空", related),
            ));
        }
        devices.insert(dev.to_string());
    }
    Ok(devices)
}

fn decode_mount_field(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(|b| matches!(b, b'0'..=b'7'))
        {
            let value =
                (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mounted_points_for_devices(devices: &HashSet<String>) -> io::Result<Vec<String>> {
    let text = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mut points = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 5 && devices.contains(fields[2]) {
            points.push(decode_mount_field(fields[4]));
        }
    }
    points.sort_by_key(|path| std::cmp::Reverse(path.len()));
    points.dedup();
    Ok(points)
}

pub(super) fn is_system_disk(_runner: &dyn CmdRunner, disk: u32) -> bool {
    let Some(name) = block_name(disk) else {
        return true;
    };
    let Ok(devices) = disk_device_numbers(&name) else {
        return true;
    };
    let Ok(points) = mounted_points_for_devices(&devices) else {
        return true;
    };
    points.iter().any(|point| point == "/")
}

fn read_trim(path: impl AsRef<std::path::Path>) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
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

fn usb_transport(name: &str) -> NativeTransport {
    let Ok(mut path) = std::fs::canonicalize(format!("/sys/class/block/{name}/device")) else {
        return NativeTransport::Unknown;
    };
    loop {
        if let Ok(driver) = std::fs::canonicalize(path.join("driver")) {
            if let Some(driver) = driver
                .file_name()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
            {
                if driver == "uas" || driver.contains("uas") {
                    return NativeTransport::Uas;
                }
                if driver == "usb-storage" || driver.contains("usb_storage") {
                    return NativeTransport::Bot;
                }
            }
        }
        if !path.pop() {
            return NativeTransport::Unknown;
        }
    }
}

pub(super) fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    let name = block_name(disk)?;
    let device = std::path::Path::new("/sys/class/block")
        .join(&name)
        .join("device");
    let vendor = read_trim(device.join("vendor")).unwrap_or_default();
    let product = read_trim(device.join("model")).unwrap_or_default();
    let revision = read_trim(device.join("rev")).unwrap_or_default();
    let usb = usb_ancestor(&name);
    let parse_hex = |value: Option<String>| u16::from_str_radix(value?.trim(), 16).ok();
    let vid = parse_hex(usb.as_ref().and_then(|p| read_trim(p.join("idVendor"))));
    let pid = parse_hex(usb.as_ref().and_then(|p| read_trim(p.join("idProduct"))));
    let transport = if usb.is_some() {
        usb_transport(&name)
    } else {
        NativeTransport::Unknown
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

pub(super) fn fallback_hardware_probe(
    _runner: &dyn CmdRunner,
    _disk: u32,
) -> Option<HardwareProbe> {
    None
}

pub(super) fn disk_total_sectors(_runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    let name = block_name(disk)?;
    read_trim(format!("/sys/class/block/{name}/size"))?
        .parse()
        .ok()
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
        .iter()
        .cloned()
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
                proto: if usb {
                    "USB".into()
                } else {
                    "Removable".into()
                },
            })
        })
        .collect()
}

fn prepare_write_for_block_name(name: &str) -> io::Result<WriteGuard> {
    let devices = disk_device_numbers(name)?;
    if devices.is_empty() {
        return Err(io::Error::other(
            "无法确认 Linux 块设备 major:minor，拒绝写盘",
        ));
    }
    let mounted = mounted_points_for_devices(&devices)?;
    if mounted.iter().any(|point| point == "/") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "目标磁盘承载根文件系统，拒绝卸载",
        ));
    }
    for mountpoint in mounted {
        let path = CString::new(mountpoint.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "挂载点包含 NUL"))?;
        let rc = unsafe { libc::umount2(path.as_ptr(), 0) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let remaining = mounted_points_for_devices(&devices)?;
    if !remaining.is_empty() {
        return Err(io::Error::other(format!(
            "卸载后仍有挂载点: {}",
            remaining.join(", ")
        )));
    }
    Ok(WriteGuard)
}

pub(super) fn prepare_write(_runner: &dyn CmdRunner, disk: u32) -> io::Result<WriteGuard> {
    let name = block_name(disk)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Linux 块设备不存在"))?;
    prepare_write_for_block_name(&name)
}

#[cfg(feature = "ci-virtual-disk")]
pub(super) fn ci_prepare_virtual_write(path: &str) -> io::Result<WriteGuard> {
    let name = path.strip_prefix("/dev/").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "CI 虚拟磁盘只允许 /dev/loopN",
        )
    })?;
    let Some(index) = name.strip_prefix("loop") else {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "CI 虚拟磁盘只允许 /dev/loopN",
        ));
    };
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "CI 虚拟磁盘只允许 /dev/loopN 整盘",
        ));
    }
    let metadata = std::fs::metadata(path)?;
    if !metadata.file_type().is_block_device() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "CI 虚拟磁盘目标不是块设备",
        ));
    }
    let class = std::path::Path::new("/sys/class/block").join(name);
    if class.join("partition").exists() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "CI 虚拟磁盘必须选择 loop 整盘而不是分区",
        ));
    }
    prepare_write_for_block_name(name)
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
        struct NoopRunner;
        impl CmdRunner for NoopRunner {
            fn check_output(
                &self,
                _cmd: &[&str],
                _timeout: std::time::Duration,
            ) -> io::Result<String> {
                Err(io::Error::other("unused"))
            }
        }
        assert!(is_system_disk(&NoopRunner, u32::MAX));
    }
}
