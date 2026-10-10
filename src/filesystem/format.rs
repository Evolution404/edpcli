use super::{FilesystemKind, FilesystemMetadata};

/// FAT/exFAT BPB bytes-per-sector must be a power of two in the 512..=4096
/// standard range. This is an FS capability predicate, NOT a native device
/// geometry restriction: other 512*n values remain valid for raw/protocol I/O.
pub(crate) fn native_fat_sector_bytes_supported(bytes: u32) -> bool {
    (512..=4096).contains(&bytes) && bytes.is_power_of_two()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemGeometry {
    pub partition_offset: u64,
    pub sector_count: u64,
    pub sector_size: u32,
}

impl FilesystemGeometry {
    pub const fn new(partition_offset: u64, sector_count: u64, sector_size: u32) -> Self {
        Self {
            partition_offset,
            sector_count,
            sector_size,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatRequest {
    pub filesystem: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}

impl FormatRequest {
    pub fn new(filesystem: FilesystemKind) -> Self {
        Self {
            filesystem,
            volume_label: None,
            volume_serial: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemWrite {
    pub relative_lba: u64,
    pub data: [u8; 512],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatPlan {
    pub filesystem: FilesystemKind,
    pub geometry: FilesystemGeometry,
    pub writes: Vec<FilesystemWrite>,
    pub expected_metadata: FilesystemMetadata,
}

/// A complete native block emitted only to a virtual formatting plan.
/// This type does not grant permission to write physical disks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFilesystemWrite {
    pub relative_lba: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFormatPlan {
    pub filesystem: FilesystemKind,
    pub geometry: FilesystemGeometry,
    pub writes: Vec<NativeFilesystemWrite>,
    pub expected_metadata: FilesystemMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatVerification {
    pub metadata: FilesystemMetadata,
}
