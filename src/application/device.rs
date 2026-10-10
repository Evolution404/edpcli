//! Shared device safety policy used before read and write application flows.

use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET};
use crate::diskio::{raw_path, FileDev};
use crate::platform::system;
use crate::ports::CmdRunner;

pub fn guard_usb_disk(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<()> {
    guard_observed_usb_disk(&system::SystemDeviceAccess { runner }, disk)
}

pub fn guard_observed_usb_disk(
    observer: &dyn crate::ports::DeviceObserver,
    disk: u32,
) -> EdpCliResult<()> {
    if observer.is_system_disk(disk) {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!("错误: 拒绝系统盘 disk{disk}"),
        ));
    }
    if observer.is_external_usb_whole(disk) {
        return Ok(());
    }
    Err(EdpCliError::new(
        EXIT_TARGET,
        format!(
            "错误: disk{} 当前不是已确认的外接 USB 整盘或显式启用的虚拟 Disk Image 整盘",
            disk
        ),
    ))
}

pub(crate) fn open_readonly_usb_disk(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<FileDev> {
    guard_usb_disk(runner, disk)?;
    let path = raw_path(disk);
    FileDev::open_rdonly(&path)
        .map_err(|error| EdpCliError::new(EXIT_IO, format!("错误: 无法只读打开 {path}: {error}")))
}
