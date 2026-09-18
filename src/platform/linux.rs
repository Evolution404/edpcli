use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;
use std::process::Command;

use super::{HardwareProbe, PlatformKind};

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Linux
}

pub(super) fn raw_disk_path(disk: u32) -> String {
    // 兼容层第一阶段保留数字 selector；真实 Linux 设备枚举会在平台 backend
    // 中把编号映射到 /dev/sdX / /dev/nvmeXnY，而不是让业务层拼路径。
    format!("/dev/disk{disk}")
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

pub(super) fn hardware_probe(_disk: u32) -> Option<HardwareProbe> {
    None
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
