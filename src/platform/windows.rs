use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;

use super::{ExtDisk, HardwareProbe, PlatformKind};
use crate::common::SECTOR;
use crate::sysinfo::CmdRunner;
use std::time::Duration;

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Windows
}

pub(super) struct WriteGuard;

pub(super) fn raw_disk_path(disk: u32) -> String {
    format!(r"\\.\PhysicalDrive{disk}")
}

pub(super) fn is_elevated() -> bool {
    // Windows token 检查在下一阶段由 windows-sys backend 实现；当前保持
    // fail-closed，避免把普通权限误认为管理员权限。
    false
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

pub(super) fn is_raw_device_path(path: &str) -> bool {
    path.starts_with(r"\\.\PhysicalDrive")
}

pub(super) fn raw_busy_error(_error: &io::Error) -> bool {
    false
}

pub(super) fn sync_raw_device(file: &File) -> io::Result<()> {
    file.sync_all()
}

pub(super) fn sync_directory(_path: &Path) -> io::Result<()> {
    // std::fs::File 无法按 Unix 方式直接打开目录。备份文件与 md5 sidecar
    // 已各自 FlushFileBuffers(sync_all)；目录项同步由 NTFS/系统缓存负责。
    Ok(())
}

pub(super) fn hardware_probe(_disk: u32) -> Option<HardwareProbe> {
    None
}

pub(super) fn is_system_disk(_disk: u32) -> bool {
    true
}

const POWERSHELL_TIMEOUT: Duration = Duration::from_secs(15);

fn parse_disk_lines(output: &str) -> Vec<ExtDisk> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('|');
            let n: u32 = parts.next()?.trim().parse().ok()?;
            let size: u64 = parts.next()?.trim().parse().ok()?;
            let interface = parts.next()?.trim();
            let media = parts.next()?.trim();
            let pnp = parts.next().unwrap_or_default().trim();
            let upper = pnp.to_ascii_uppercase();
            let usb = interface.eq_ignore_ascii_case("USB") || upper.contains("USBSTOR\\");
            let removable = media.to_ascii_lowercase().contains("removable");
            if !usb && !removable {
                return None;
            }
            let extract = |tag: &str| -> Option<String> {
                let pos = upper.find(tag)? + tag.len();
                upper.get(pos..pos + 4).map(|s| s.to_ascii_lowercase())
            };
            Some(ExtDisk {
                n,
                size,
                vid: extract("VID_").unwrap_or_else(|| "xxxx".into()),
                pid: extract("PID_").unwrap_or_else(|| "xxxx".into()),
                proto: if usb { "USB".into() } else { "Removable".into() },
            })
        })
        .collect()
}

pub(super) fn list_external_disks(runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    const SCRIPT: &str = "Get-CimInstance Win32_DiskDrive | ForEach-Object { '{0}|{1}|{2}|{3}|{4}' -f $_.Index,$_.Size,$_.InterfaceType,$_.MediaType,$_.PNPDeviceID }";
    runner
        .check_output(
            &["powershell.exe", "-NoProfile", "-Command", SCRIPT],
            POWERSHELL_TIMEOUT,
        )
        .map(|out| parse_disk_lines(&out))
        .unwrap_or_default()
}

pub(super) fn disk_total_sectors(runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    list_external_disks(runner)
        .into_iter()
        .find(|item| item.n == disk)
        .map(|item| item.size / SECTOR as u64)
}

pub(super) fn usb_vid_pid(runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    list_external_disks(runner)
        .into_iter()
        .find(|item| item.n == disk)
        .map(|item| (item.vid, item.pid))
        .unwrap_or_else(|| ("xxxx".into(), "xxxx".into()))
}

pub(super) fn prepare_write(_runner: &dyn CmdRunner, _disk: u32) -> io::Result<WriteGuard> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows 写盘卸载 backend 尚未启用；为避免写挂载磁盘已拒绝操作",
    ))
}

pub(super) const fn elevation_label() -> &'static str {
    "管理员权限"
}

pub(super) fn run_elevated(_exe: &Path, _argv: &[String], _sentinel: &str) -> io::Result<i32> {
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Windows 请从‘以管理员身份运行’的终端启动 edpcli",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_diskdrive_lines() {
        let rows = parse_disk_lines(
            "2|62914560000|USB|Removable Media|USBSTOR\\DISK&VEN_NETAC&PROD_ONLYDISK\\VID_0DD8&PID_2005\n0|1000000|SCSI|Fixed hard disk media|PCI\\X\n",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].n, 2);
        assert_eq!(rows[0].vid, "0dd8");
        assert_eq!(rows[0].pid, "2005");
    }
}
