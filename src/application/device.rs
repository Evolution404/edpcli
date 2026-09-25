//! Shared device safety policy used before read and write application flows.

use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET};
use crate::diskio::{raw_path, FileDev};
use crate::sysinfo::{self, CmdRunner};

pub fn guard_system_disk(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<()> {
    if crate::platform::is_system_disk(runner, disk) {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!("错误: 拒绝系统盘 disk{disk}"),
        ));
    }
    Ok(())
}

pub fn guard_usb_disk(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<()> {
    guard_system_disk(runner, disk)?;
    if sysinfo::usb_disk(runner, disk).is_some() {
        return Ok(());
    }
    Err(EdpCliError::new(
        EXIT_TARGET,
        format!(
            "错误: disk{} 当前不是可操作的外接 USB 整盘（要求 WholeDisk=true、Internal=false、非虚拟盘、BusProtocol=USB），拒绝裸盘操作",
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
