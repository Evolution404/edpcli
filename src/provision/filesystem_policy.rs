use crate::filesystem::{
    is_writable_filesystem, shift_writable_filesystem, validate_writable_filesystem,
    FilesystemKind, WRITABLE_FILESYSTEMS,
};

pub const PROVISION_FILESYSTEMS: [FilesystemKind; 3] = WRITABLE_FILESYSTEMS;

pub fn is_provision_filesystem_supported(filesystem: FilesystemKind) -> bool {
    is_writable_filesystem(filesystem)
}

pub fn validate_provision_filesystem(filesystem: FilesystemKind) -> Result<(), String> {
    validate_writable_filesystem(filesystem).map_err(|error| error.to_string())
}

pub fn shift_provision_filesystem(filesystem: FilesystemKind, reverse: bool) -> FilesystemKind {
    shift_writable_filesystem(filesystem, reverse)
}
