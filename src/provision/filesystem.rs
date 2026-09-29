//! Pure filesystem construction for first-party provisioning profiles.
//!
//! The current first-party Windows writer reads the GLOBAL/fType setting from
//! usbtoolCfg.ini, defaults it to exfat, normalizes ntfs/exfat/fat32, and
//! passes that format name to fmifs FormatEx. This module models that axis
//! separately from EDP partition type and provides the portable exFAT builder
//! used by the default profile.

use std::collections::BTreeMap;

mod migration;
pub use migration::build_migrated_filesystem;

use crate::partition_transform::PartitionTransform;

const SECTOR_SIZE: usize = 512;

/// 迁移期兼容别名。正式文件系统类型的唯一事实源是 `FilesystemKind`。
use crate::filesystem::FilesystemKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseFilesystemImage {
    volume_sectors: u64,
    sectors: BTreeMap<u64, [u8; SECTOR_SIZE]>,
}

impl SparseFilesystemImage {
    pub fn volume_sectors(&self) -> u64 {
        self.volume_sectors
    }

    pub fn sectors(&self) -> &BTreeMap<u64, [u8; SECTOR_SIZE]> {
        &self.sectors
    }

    pub fn sector_or_zero(&self, relative_lba: u64) -> Option<[u8; SECTOR_SIZE]> {
        if relative_lba >= self.volume_sectors {
            return None;
        }
        Some(
            self.sectors
                .get(&relative_lba)
                .copied()
                .unwrap_or([0; SECTOR_SIZE]),
        )
    }

    pub fn metadata_bytes(&self) -> usize {
        self.sectors.len() * SECTOR_SIZE
    }

    pub fn transformed<T: PartitionTransform>(&self, transform: &T) -> Self {
        Self {
            volume_sectors: self.volume_sectors,
            sectors: self
                .sectors
                .iter()
                .map(|(&lba, sector)| (lba, transform.transform_sector(lba, sector)))
                .collect(),
        }
    }
}

pub fn validate_volume_label(filesystem: FilesystemKind, label: &str) -> Result<(), String> {
    match filesystem {
        FilesystemKind::Fat16 => {
            let request = crate::filesystem::FormatRequest {
                filesystem: crate::filesystem::FilesystemKind::Fat16,
                volume_label: (!label.is_empty()).then(|| label.to_string()),
                volume_serial: None,
            };
            crate::filesystem::FilesystemDriver::validate_format_request(
                &crate::filesystem::FAT16_DRIVER,
                &request,
            )
            .map_err(|error| error.to_string())
        }
        FilesystemKind::ExFat => {
            let request = crate::filesystem::FormatRequest {
                filesystem: crate::filesystem::FilesystemKind::ExFat,
                volume_label: (!label.is_empty()).then(|| label.to_string()),
                volume_serial: None,
            };
            crate::filesystem::FilesystemDriver::validate_format_request(
                &crate::filesystem::EXFAT_DRIVER,
                &request,
            )
            .map_err(|error| error.to_string())
        }
        FilesystemKind::Fat12 | FilesystemKind::Fat32 | FilesystemKind::Ntfs => Err(format!(
            "portable filesystem writer does not yet implement {}",
            filesystem.config_token()
        )),
    }
}

/// 迁移期兼容入口。FAT16 的具体磁盘结构由 Fat16Driver 单一实现。
pub fn build_empty_fat16(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
) -> Result<SparseFilesystemImage, String> {
    let request = crate::filesystem::FormatRequest {
        filesystem: crate::filesystem::FilesystemKind::Fat16,
        volume_label: (!volume_label.is_empty()).then(|| volume_label.to_string()),
        volume_serial: Some(volume_serial),
    };
    let geometry = crate::filesystem::FilesystemGeometry::new(
        partition_offset,
        volume_sectors,
        SECTOR_SIZE as u32,
    );
    let plan = crate::filesystem::FilesystemDriver::build_format_plan(
        &crate::filesystem::FAT16_DRIVER,
        geometry,
        &request,
    )
    .map_err(|error| error.to_string())?;
    Ok(SparseFilesystemImage {
        volume_sectors,
        sectors: plan
            .writes
            .into_iter()
            .map(|write| (write.relative_lba, write.data))
            .collect(),
    })
}

/// 迁移期兼容入口。exFAT 的具体磁盘结构由 ExFatDriver 单一实现。
pub fn build_empty_exfat(
    partition_offset_lba: u64,
    volume_sectors: u64,
    volume_serial: u32,
    label: &str,
) -> Result<SparseFilesystemImage, String> {
    let request = crate::filesystem::FormatRequest {
        filesystem: crate::filesystem::FilesystemKind::ExFat,
        volume_label: (!label.is_empty()).then(|| label.to_string()),
        volume_serial: Some(volume_serial),
    };
    let geometry = crate::filesystem::FilesystemGeometry::new(
        partition_offset_lba,
        volume_sectors,
        SECTOR_SIZE as u32,
    );
    let plan = crate::filesystem::FilesystemDriver::build_format_plan(
        &crate::filesystem::EXFAT_DRIVER,
        geometry,
        &request,
    )
    .map_err(|error| error.to_string())?;
    Ok(SparseFilesystemImage {
        volume_sectors,
        sectors: plan
            .writes
            .into_iter()
            .map(|write| (write.relative_lba, write.data))
            .collect(),
    })
}
