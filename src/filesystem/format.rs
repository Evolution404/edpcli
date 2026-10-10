use super::{FilesystemKind, FilesystemMetadata};

/// FAT/exFAT BPB bytes-per-sector must be a power of two in the 512..=4096
/// standard range. This is an FS capability predicate, NOT a native device
/// geometry restriction: other 512*n values remain valid for raw/protocol I/O.
pub(crate) fn native_fat_sector_bytes_supported(bytes: u32) -> bool {
    crate::domain::hardware::native_sector_capability(bytes)
        .is_some_and(|capability| capability.fat_exfat_format)
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

// FAT/exFAT native formatters and their readback parser share the capability
// registry; do not confuse a BPB-valid offline plan with OS HIL certification.
#[cfg(test)]
mod multi_native_format_tests {
    use super::*;
    use crate::filesystem::{
        detect_native_boot_sector, EXFAT_DRIVER, FAT12_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
    };

    #[test]
    fn each_standard_logical_sector_formats_and_roundtrips_fat_and_exfat_boot() {
        for sector in [512u32, 1024, 2048, 4096] {
            for (fs, mib) in [
                (FilesystemKind::Fat12, 4u64),
                (FilesystemKind::Fat16, 64),
                (FilesystemKind::Fat32, 512),
                (FilesystemKind::ExFat, 128),
            ] {
                let sectors = mib * 1_048_576 / u64::from(sector);
                let geometry = FilesystemGeometry::new(2048, sectors, sector);
                let request = FormatRequest {
                    filesystem: fs,
                    volume_label: Some("NATIVE".into()),
                    volume_serial: Some(0x1203_4567),
                };
                let plan = match fs {
                    FilesystemKind::Fat12 => {
                        FAT12_DRIVER.build_native_format_plan(geometry, &request)
                    }
                    FilesystemKind::Fat16 => {
                        FAT16_DRIVER.build_native_format_plan(geometry, &request)
                    }
                    FilesystemKind::Fat32 => {
                        FAT32_DRIVER.build_native_format_plan(geometry, &request)
                    }
                    FilesystemKind::ExFat => {
                        EXFAT_DRIVER.build_native_format_plan(geometry, &request)
                    }
                    _ => unreachable!(),
                }
                .unwrap_or_else(|err| panic!("{sector}B {fs:?} format: {err}"));
                assert!(!plan.writes.is_empty());
                assert!(plan.writes.iter().all(|w| w.data.len() == sector as usize));
                let boot = plan.writes.iter().find(|w| w.relative_lba == 0).unwrap();
                let detected = detect_native_boot_sector(&boot.data, sectors, sector)
                    .unwrap_or_else(|err| panic!("{sector}B {fs:?} detect: {err}"));
                assert_eq!(detected, Some(fs), "{sector}B {fs:?} native boot");
            }
        }
        for sector in [1536u32, 2560, 3072, 8192] {
            assert!(!native_fat_sector_bytes_supported(sector));
        }
    }
}
