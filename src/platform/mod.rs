//! 操作系统依赖层。
//!
//! 业务模块只依赖这里暴露的跨平台数据模型与能力入口。macOS / Linux / Windows
//! 的设备路径、权限、原生硬件探测等细节分别放在子模块中。

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    MacOS,
    Linux,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTransport {
    Uas,
    Bot,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InquiryInfo {
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProbe {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub transport: NativeTransport,
    pub inquiry: Option<InquiryInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtDisk {
    pub n: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
}

pub struct WriteGuard {
    _inner: imp::WriteGuard,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_native;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "windows")]
use windows as imp;

pub fn kind() -> PlatformKind {
    imp::kind()
}

pub fn raw_disk_path(disk: u32) -> String {
    imp::raw_disk_path(disk)
}

pub fn parse_disk_selector(value: &str) -> Result<u32, String> {
    imp::parse_disk_selector(value)
}

pub fn disk_selector_syntax() -> &'static str {
    imp::disk_selector_syntax()
}

/// 重新提权/重执行时使用平台原生选择器，避免把 Linux 当前枚举序号跨进程固化。
pub fn disk_selector_value(disk: u32) -> String {
    imp::disk_selector_value(disk)
}

pub fn is_elevated() -> bool {
    imp::is_elevated()
}

pub fn invoking_user_home() -> Option<PathBuf> {
    imp::invoking_user_home()
}

pub fn has_elevation_origin() -> bool {
    imp::has_elevation_origin()
}

pub fn probe_command_cacheable(cmd: &[&str]) -> bool {
    imp::probe_command_cacheable(cmd)
}

pub fn is_raw_device_path(path: &str) -> bool {
    imp::is_raw_device_path(path)
}

pub fn raw_busy_error(error: &io::Error) -> bool {
    imp::raw_busy_error(error)
}

pub fn sync_raw_device(file: &File) -> io::Result<()> {
    imp::sync_raw_device(file)
}

pub fn sync_directory(path: &std::path::Path) -> io::Result<()> {
    imp::sync_directory(path)
}

pub fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    imp::hardware_probe(disk)
}

/// 平台原生探测缺字段时的兼容探测。仅平台实现知道具体 OS 工具或 API；
/// 业务层只消费统一的 `HardwareProbe`。
pub fn fallback_hardware_probe(
    runner: &dyn crate::sysinfo::CmdRunner,
    disk: u32,
) -> Option<HardwareProbe> {
    imp::fallback_hardware_probe(runner, disk)
}


pub fn is_system_disk(disk: u32) -> bool {
    imp::is_system_disk(disk)
}

pub fn list_external_disks(runner: &dyn crate::sysinfo::CmdRunner) -> Vec<ExtDisk> {
    imp::list_external_disks(runner)
}

pub fn disk_total_sectors(
    runner: &dyn crate::sysinfo::CmdRunner,
    disk: u32,
) -> Option<u64> {
    imp::disk_total_sectors(runner, disk)
}

pub fn usb_vid_pid(
    runner: &dyn crate::sysinfo::CmdRunner,
    disk: u32,
) -> (String, String) {
    imp::usb_vid_pid(runner, disk)
}

pub fn prepare_write(
    runner: &dyn crate::sysinfo::CmdRunner,
    disk: u32,
) -> io::Result<WriteGuard> {
    imp::prepare_write(runner, disk).map(|inner| WriteGuard { _inner: inner })
}

pub fn elevation_label() -> &'static str {
    imp::elevation_label()
}

pub fn run_elevated(exe: &Path, argv: &[String], sentinel: &str) -> io::Result<i32> {
    imp::run_elevated(exe, argv, sentinel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_platform_matches_target() {
        #[cfg(target_os = "macos")]
        assert_eq!(kind(), PlatformKind::MacOS);
        #[cfg(target_os = "linux")]
        assert_eq!(kind(), PlatformKind::Linux);
        #[cfg(target_os = "windows")]
        assert_eq!(kind(), PlatformKind::Windows);
    }
}
