mod driver;
mod error;
mod exfat;
mod fat16;
mod format;
mod io;
mod kind;
mod metadata;
pub mod registry;

pub use driver::{DetectionConfidence, DetectionResult, FilesystemCapabilities, FilesystemDriver};
pub use error::{FilesystemError, FilesystemErrorKind};
pub(crate) use exfat::analysis_layout as exfat_analysis_layout;
pub(crate) use exfat::{
    exfat_boot_checksum, exfat_geometry, exfat_upcase_table, fat_chain, put_stream, put_u16,
    put_u32, put_u64, upcase_mapping,
};
pub use exfat::{ExFatDriver, EXFAT_DRIVER};
pub use fat16::{Fat16Driver, FAT16_DRIVER};
pub use format::{
    FilesystemGeometry, FilesystemWrite, FormatPlan, FormatRequest, FormatVerification,
};
pub use io::{BootSectorReader, FilesystemReader};
pub use kind::FilesystemKind;
pub use metadata::FilesystemMetadata;
pub use registry::{DetectedFilesystem, DriverRegistry};
