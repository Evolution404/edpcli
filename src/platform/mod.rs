//! 操作系统依赖层。
//!
//! 业务模块只依赖这里暴露的跨平台数据模型与能力入口。macOS / Linux / Windows
//! 的设备路径、权限、原生硬件探测等细节分别放在子模块中。

use std::fs::File;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub use crate::domain::hardware::{
    ExtDisk, HardwareProbe, InquiryInfo, NativeTransport, ObservedDeviceGeometry, PlatformKind,
};

/// 稳定平台门面下的设备标识推导。
pub mod identity {
    pub use crate::identify::*;
}

/// 稳定平台门面下的只读系统探测与命令执行抽象。
pub mod system;

pub struct WriteGuard {
    _inner: imp::WriteGuard,
}

#[cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]
pub(crate) struct NativeCommandRunner;
#[cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]
impl crate::ports::CmdRunner for NativeCommandRunner {
    fn check_output(&self, cmd: &[&str], timeout: std::time::Duration) -> io::Result<String> {
        crate::infrastructure::process::check_output(cmd, timeout)
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_native;
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod test_support;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix_support;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

pub(crate) fn own_invoking_user_file(file: &File) -> io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        unix_support::own_invoking_user_file(file)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = file;
        Ok(())
    }
}

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

/// Compare the *open handles* rather than names: a symlink or re-enumerated
/// target can resolve to a different object when write access is reopened.
#[cfg(unix)]
pub(crate) fn same_open_file_identity(a: &File, b: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let a = a.metadata()?;
    let b = b.metadata()?;
    Ok(a.dev() == b.dev() && a.ino() == b.ino() && a.rdev() == b.rdev())
}

/// One Win32 file-identity provider for backup-cache fingerprints and disk
/// handle reopen checks. Returning an error avoids mistaking an unavailable
/// file index for a verified stable target.
#[cfg(windows)]
pub(crate) fn filesystem_handle_identity(file: &File) -> io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the owned File handle is live and the record has the exact API size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(windows)]
pub(crate) fn same_open_file_identity(a: &File, b: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::{
        IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    #[derive(PartialEq, Eq)]
    enum OpenIdentity {
        FilesystemFile(u64, u64),
        StorageDevice(u32, u32, u32),
    }
    fn fingerprint(file: &File) -> io::Result<OpenIdentity> {
        if let Ok((volume, index)) = filesystem_handle_identity(file) {
            return Ok(OpenIdentity::FilesystemFile(volume, index));
        }
        // Raw physical drive handles can reject GetFileInformationByHandle.
        // Query the underlying storage device identity instead of rejecting
        // every legitimate Windows write-reopen or silently trusting its name.
        let mut number = STORAGE_DEVICE_NUMBER::default();
        let mut returned = 0u32;
        if unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                std::ptr::null(),
                0,
                (&mut number as *mut STORAGE_DEVICE_NUMBER).cast(),
                std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(OpenIdentity::StorageDevice(
            number.DeviceType,
            number.DeviceNumber,
            number.PartitionNumber,
        ))
    }
    Ok(fingerprint(a)? == fingerprint(b)?)
}

pub fn raw_busy_error(error: &io::Error) -> bool {
    imp::raw_busy_error(error)
}

pub fn sync_raw_device(file: &File) -> io::Result<()> {
    imp::sync_raw_device(file)
}

pub fn sync_directory(path: &std::path::Path) -> io::Result<DirectoryDurability> {
    #[cfg(unix)]
    {
        imp::sync_directory(path)?;
        Ok(DirectoryDurability::Synced)
    }
    #[cfg(windows)]
    {
        let _ = path;
        Ok(DirectoryDurability::WriteThroughPublicationRequired)
    }
}

pub fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    imp::hardware_probe(disk)
}

pub fn hardware_serial(disk: u32) -> Option<String> {
    imp::hardware_serial(disk).filter(|serial| !serial.trim().is_empty())
}

/// 平台原生探测缺字段时的兼容探测。仅平台实现知道具体 OS 工具或 API；
/// 业务层只消费统一的 `HardwareProbe`。
pub fn fallback_hardware_probe(
    runner: &dyn crate::ports::CmdRunner,
    disk: u32,
) -> Option<HardwareProbe> {
    imp::fallback_hardware_probe(runner, disk)
}

pub fn is_system_disk(runner: &dyn crate::ports::CmdRunner, disk: u32) -> bool {
    imp::is_system_disk(runner, disk)
}

pub fn list_external_disks(runner: &dyn crate::ports::CmdRunner) -> Vec<ExtDisk> {
    imp::list_external_disks(runner)
}

pub fn disk_total_sectors(runner: &dyn crate::ports::CmdRunner, disk: u32) -> Option<u64> {
    imp::disk_total_sectors(runner, disk)
}

pub fn device_geometry(
    runner: &dyn crate::ports::CmdRunner,
    disk: u32,
) -> Option<ObservedDeviceGeometry> {
    imp::device_geometry(runner, disk)
}

pub(crate) fn fallback_device_geometry(
    runner: &dyn crate::ports::CmdRunner,
    disk: u32,
) -> Option<ObservedDeviceGeometry> {
    #[cfg(target_os = "macos")]
    {
        imp::device_geometry(runner, disk)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (runner, disk);
        None
    }
}

pub fn usb_vid_pid(runner: &dyn crate::ports::CmdRunner, disk: u32) -> (String, String) {
    imp::usb_vid_pid(runner, disk)
}

pub fn prepare_write(runner: &dyn crate::ports::CmdRunner, disk: u32) -> io::Result<WriteGuard> {
    imp::prepare_write(runner, disk).map(|inner| WriteGuard { _inner: inner })
}

/// CI 专用虚拟磁盘写前准备。
///
/// 该入口只在显式启用 `ci-virtual-disk` feature 时存在；各平台都必须二次证明
/// 目标确实是 disposable virtual disk：Linux=/dev/loopN，Windows=Virtual/VHD，
/// macOS=WholeDisk + Virtual + BusProtocol=Disk Image。
#[cfg(all(
    feature = "ci-virtual-disk",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
))]
pub fn ci_prepare_virtual_write(path: &str) -> io::Result<WriteGuard> {
    imp::ci_prepare_virtual_write(path).map(|inner| WriteGuard { _inner: inner })
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

/// New private files/directories inherit only owner, system and administrator access on Windows.
pub(crate) fn protect_private_path(path: &std::path::Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::protect_private_path(path)
    }
    #[cfg(unix)]
    {
        let _ = path;
        Ok(())
    }
}

/// The caller must distinguish Unix directory fsync from Windows write-through publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryDurability {
    Synced,
    WriteThroughPublicationRequired,
}
#[cfg(windows)]
pub(crate) fn publish_file(
    source: &std::path::Path,
    target: &std::path::Path,
    replace: bool,
) -> io::Result<()> {
    windows::publish_file(source, target, replace)
}
