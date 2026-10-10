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
mod native_boot;
mod native_virtual;
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
    NativeFilesystemWrite, NativeFormatPlan,
};
pub use image::{
    build_empty_exfat, build_empty_fat16, build_empty_fat32, build_empty_filesystem,
    build_empty_filesystem_typed, validate_volume_label, validate_volume_label_typed,
    SparseFilesystemImage,
};
pub use io::{BootSectorReader, FilesystemReader};
pub use kind::FilesystemKind;
pub use metadata::FilesystemMetadata;
pub use native_boot::detect_native_boot_sector;
pub use native_virtual::NativeVirtualDiskPlan;
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

/// OEM Windows `FormatEx(..., L"FAT", ...)` selects a FAT variant from
/// its actual native volume geometry, not from whether the device is 4Kn.
/// Use the already-validated native FAT solvers; never silently resize the
/// partition to force FAT16 or upgrade the legacy 512B FAT12 write registry.
pub fn select_native_oem_boot_fat(geometry: FilesystemGeometry) -> Result<FilesystemKind, String> {
    if !format::native_fat_sector_bytes_supported(geometry.sector_size)
        || geometry.sector_count == 0
        || geometry.sector_count > u64::from(u32::MAX)
        || geometry.partition_offset > u64::from(u32::MAX)
        || geometry
            .partition_offset
            .checked_add(geometry.sector_count)
            .is_none()
    {
        return Err("启动区的原生扇区几何不支持 FAT 格式化".into());
    }
    // FAT16 must be preferred over FAT12 when the same volume can be made
    // FAT12 only by artificially increasing the cluster size. This matches
    // the two OEM samples: 512B/20417 -> FAT16, 4Kn/2497 -> FAT12.
    if fat16::choose_format_geometry_for_sector_size(geometry.sector_count, geometry.sector_size)
        .is_ok()
    {
        return Ok(FilesystemKind::Fat16);
    }
    if fat12::choose_native_fat12_geometry(geometry.sector_count, geometry.sector_size).is_some() {
        return Ok(FilesystemKind::Fat12);
    }
    Err("启动区实际簇数无法使用已验证 FAT12/FAT16 几何表示".into())
}

pub(crate) mod probe;

#[cfg(test)]
mod oem_boot_fat_tests {
    use super::*;

    #[test]
    fn oem_generic_fat_uses_native_cluster_geometry_not_a_4kn_switch() {
        let choose = |sectors, bytes| {
            select_native_oem_boot_fat(FilesystemGeometry::new(63, sectors, bytes)).unwrap()
        };
        // The original 512B and 4Kn OEM native samples.
        assert_eq!(choose(20_417, 512), FilesystemKind::Fat16);
        assert_eq!(choose(2_497, 4_096), FilesystemKind::Fat12);
        // FAT types follow capacity and the actual cluster solver, even
        // when the *same* block size crosses the FAT12/FAT16 threshold.
        assert_eq!(choose(2_497, 512), FilesystemKind::Fat12);
        assert_eq!(choose(8_192, 4_096), FilesystemKind::Fat16);
        assert_eq!(choose(20_417, 4_096), FilesystemKind::Fat16);
        for (sectors, bytes) in [(0, 512), (1, 4_096), (2_497, 8_192)] {
            assert!(
                select_native_oem_boot_fat(FilesystemGeometry::new(63, sectors, bytes)).is_err()
            );
        }
    }
}
