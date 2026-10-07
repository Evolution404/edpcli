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
mod policy;
pub mod registry;
mod resource_budget;

pub use driver::{DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver};
pub use error::{FilesystemError, FilesystemErrorKind};
pub(crate) use exfat::analysis_layout as exfat_analysis_layout;
pub use exfat::{ExFatDriver, EXFAT_DRIVER};
pub use fat12::{Fat12Driver, FAT12_DRIVER};
pub use fat16::{Fat16Driver, FAT16_DRIVER};
pub use fat32::{Fat32Driver, FAT32_DRIVER};
pub use format::{
    FilesystemGeometry, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
};
pub use image::{
    build_empty_exfat, build_empty_fat16, build_empty_fat32, build_empty_filesystem,
    build_empty_filesystem_typed, validate_volume_label, validate_volume_label_typed,
    SparseFilesystemImage,
};
pub use io::{BootSectorReader, FilesystemReader};
pub use kind::FilesystemKind;
pub use metadata::FilesystemMetadata;
pub use ntfs::{NtfsDriver, NTFS_DRIVER};
pub use policy::{
    is_writable_filesystem, shift_writable_filesystem, validate_writable_filesystem,
    WRITABLE_FILESYSTEMS,
};
pub use registry::{
    default_registry, detect_boot_sector, detect_boot_sector_with_geometry, DetectedFilesystem,
    DriverRegistry,
};
pub use resource_budget::{
    estimate_format_resources, FormatResourceBudget, FormatResourceEstimate,
};

pub(crate) mod probe;
