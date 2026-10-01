use crate::filesystem::{default_registry, FilesystemKind};

pub const PROVISION_FILESYSTEMS: [FilesystemKind; 3] = [
    FilesystemKind::Fat16,
    FilesystemKind::Fat32,
    FilesystemKind::ExFat,
];

pub fn is_provision_filesystem_supported(filesystem: FilesystemKind) -> bool {
    PROVISION_FILESYSTEMS.contains(&filesystem)
}

pub fn validate_provision_filesystem(filesystem: FilesystemKind) -> Result<(), String> {
    if !is_provision_filesystem_supported(filesystem) {
        return Err(format!(
            "制盘格式仅支持 FAT16/FAT32/exFAT，当前为 {}",
            filesystem.display_name()
        ));
    }
    let registry = default_registry();
    let driver = registry
        .driver(filesystem)
        .ok_or_else(|| format!("{} 文件系统没有已注册驱动", filesystem.display_name()))?;
    let capabilities = driver.capabilities();
    if !capabilities.format || !capabilities.verify_format {
        return Err(format!(
            "{} 文件系统尚无可验证的格式化实现",
            filesystem.display_name()
        ));
    }
    Ok(())
}

pub fn shift_provision_filesystem(filesystem: FilesystemKind, reverse: bool) -> FilesystemKind {
    let current = PROVISION_FILESYSTEMS
        .iter()
        .position(|candidate| *candidate == filesystem);
    let index = match (current, reverse) {
        (Some(index), false) => (index + 1) % PROVISION_FILESYSTEMS.len(),
        (Some(0), true) => PROVISION_FILESYSTEMS.len() - 1,
        (Some(index), true) => index - 1,
        (None, false) => 0,
        (None, true) => PROVISION_FILESYSTEMS.len() - 1,
    };
    PROVISION_FILESYSTEMS[index]
}
