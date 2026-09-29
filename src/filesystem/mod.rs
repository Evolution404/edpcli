pub mod analysis;
mod driver;
mod error;
mod exfat;
mod fat12;
mod fat16;
mod fat32;
mod format;
mod image;
mod io;
mod kind;
mod metadata;
mod ntfs;
pub mod registry;

pub use driver::{DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver};
pub use error::{FilesystemError, FilesystemErrorKind};
pub(crate) use exfat::analysis_layout as exfat_analysis_layout;
pub(crate) use exfat::{
    exfat_boot_checksum, exfat_geometry, exfat_upcase_table, fat_chain, put_stream, put_u16,
    put_u32, put_u64, upcase_mapping,
};
pub use exfat::{ExFatDriver, EXFAT_DRIVER};
pub use fat12::{Fat12Driver, FAT12_DRIVER};
pub use fat16::{Fat16Driver, FAT16_DRIVER};
pub use fat32::{Fat32Driver, FAT32_DRIVER};
pub use format::{
    FilesystemGeometry, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
};
pub use image::{
    build_empty_exfat, build_empty_fat16, build_empty_filesystem, validate_volume_label,
    SparseFilesystemImage,
};
pub use io::{BootSectorReader, FilesystemReader};
pub use kind::FilesystemKind;
pub use metadata::FilesystemMetadata;
pub use ntfs::{NtfsDriver, NTFS_DRIVER};
pub use registry::{default_registry, detect_boot_sector, DetectedFilesystem, DriverRegistry};
