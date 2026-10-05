use super::{default_registry, FilesystemError, FilesystemErrorKind, FilesystemKind};

/// Filesystems with a first-party formatter and readback verifier.
///
/// This is the single ordered product capability list used by Provision and
/// post-restore formatting. Detection-only filesystems must not be added here.
pub const WRITABLE_FILESYSTEMS: [FilesystemKind; 3] = [
    FilesystemKind::Fat16,
    FilesystemKind::Fat32,
    FilesystemKind::ExFat,
];

pub fn is_writable_filesystem(filesystem: FilesystemKind) -> bool {
    WRITABLE_FILESYSTEMS.contains(&filesystem)
}

pub fn validate_writable_filesystem(filesystem: FilesystemKind) -> Result<(), FilesystemError> {
    if !is_writable_filesystem(filesystem) {
        return Err(FilesystemError::for_filesystem(
            filesystem,
            FilesystemErrorKind::FormatUnsupported,
            format!(
                "写入格式仅支持 FAT16/FAT32/exFAT，当前为 {}",
                filesystem.display_name()
            ),
        ));
    }

    let registry = default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        FilesystemError::for_filesystem(
            filesystem,
            FilesystemErrorKind::Unsupported,
            format!("{} 文件系统没有已注册驱动", filesystem.display_name()),
        )
    })?;
    let capabilities = driver.capabilities();
    if !capabilities.format || !capabilities.verify_format {
        return Err(FilesystemError::for_filesystem(
            filesystem,
            FilesystemErrorKind::FormatUnsupported,
            format!(
                "{} 文件系统尚无可验证的格式化实现",
                filesystem.display_name()
            ),
        ));
    }
    Ok(())
}

pub fn shift_writable_filesystem(filesystem: FilesystemKind, reverse: bool) -> FilesystemKind {
    let current = WRITABLE_FILESYSTEMS
        .iter()
        .position(|candidate| *candidate == filesystem);
    let index = match (current, reverse) {
        (Some(index), false) => (index + 1) % WRITABLE_FILESYSTEMS.len(),
        (Some(0), true) => WRITABLE_FILESYSTEMS.len() - 1,
        (Some(index), true) => index - 1,
        (None, false) => 0,
        (None, true) => WRITABLE_FILESYSTEMS.len() - 1,
    };
    WRITABLE_FILESYSTEMS[index]
}
