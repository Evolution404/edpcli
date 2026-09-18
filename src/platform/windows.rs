use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;

use super::{HardwareProbe, PlatformKind};

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Windows
}

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

pub(super) fn hardware_probe(_disk: u32) -> Option<HardwareProbe> {
    None
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
