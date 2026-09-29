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

use crate::crypto::crc32_bare;
use crate::partition_transform::{EdpSm4Transform, PartitionTransform};

use super::{
    layout::{OfficialPartitionGeometry, OfficialProvisionPlan, PartitionFormatTarget},
    FileKeyWrapMode,
};

const SECTOR_SIZE: usize = 512;

/// 迁移期兼容别名。正式文件系统类型的唯一事实源是 `FilesystemKind`。
use crate::filesystem::FilesystemKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseFilesystemImage {
    volume_sectors: u64,
    sectors: BTreeMap<u64, [u8; SECTOR_SIZE]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionFilesystemImage {
    pub geometry: OfficialPartitionGeometry,
    pub physically_encrypted: bool,
    pub image: SparseFilesystemImage,
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

pub fn encrypt_sparse_mode2(
    image: &SparseFilesystemImage,
    file_key: &[u8; 16],
) -> SparseFilesystemImage {
    let transform = EdpSm4Transform::new(*file_key);
    let sectors = image
        .sectors
        .iter()
        .map(|(&lba, sector)| (lba, transform.transform_sector(lba, sector)))
        .collect();
    SparseFilesystemImage {
        volume_sectors: image.volume_sectors,
        sectors,
    }
}

pub fn build_official_exfat_partition(
    plan: &OfficialProvisionPlan,
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serial: u32,
) -> Result<PartitionFilesystemImage, String> {
    if plan.filesystem_format != FilesystemKind::ExFat {
        return Err(format!(
            "portable filesystem writer does not yet implement {}",
            plan.filesystem_format.config_token()
        ));
    }
    build_official_partition_filesystem_with_format(
        plan,
        target,
        file_key,
        volume_label,
        volume_serial,
        FilesystemKind::ExFat,
    )
}

pub fn build_official_partition_filesystem(
    plan: &OfficialProvisionPlan,
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serial: u32,
) -> Result<PartitionFilesystemImage, String> {
    let format = target
        .filesystem
        .ok_or("compatibility reserve is not a filesystem")?;
    build_official_partition_filesystem_with_format(
        plan,
        target,
        file_key,
        volume_label,
        volume_serial,
        format,
    )
}

fn build_official_partition_filesystem_with_format(
    plan: &OfficialProvisionPlan,
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serial: u32,
    format: FilesystemKind,
) -> Result<PartitionFilesystemImage, String> {
    let targets = plan.format_targets()?;
    let Some(index) = targets.iter().position(|candidate| candidate == target) else {
        return Err("partition is not a format-capable target in this plan".into());
    };
    if !target.format_capable {
        return Err("partition is not a format-capable target in this plan".into());
    }
    if target.physically_encrypted {
        let material = plan.partition_lba12_material[index].unwrap_or(plan.lba12_key_material);
        if crc32_bare(file_key) != material.file_key_crc {
            return Err("filesystem file key does not match LBA12 FileKeyCRC".into());
        }
        if material.encrypt_mode != FileKeyWrapMode::Sm4 {
            return Err(
                "portable encrypted filesystem writer is validated only for current mode2 SM4"
                    .into(),
            );
        }
    }
    let plain = match format {
        FilesystemKind::Fat16 => build_empty_fat16(
            target.geometry.start_sector,
            target.geometry.sector_count(),
            volume_serial,
            volume_label,
        )?,
        FilesystemKind::ExFat => build_empty_exfat(
            target.geometry.start_sector,
            target.geometry.sector_count(),
            volume_serial,
            volume_label,
        )?,
        FilesystemKind::Fat12 | FilesystemKind::Fat32 | FilesystemKind::Ntfs => {
            return Err(format!(
                "portable filesystem writer does not yet implement {}",
                format.config_token()
            ))
        }
    };
    let image = if target.physically_encrypted {
        encrypt_sparse_mode2(&plain, file_key)
    } else {
        plain
    };
    Ok(PartitionFilesystemImage {
        geometry: target.geometry,
        physically_encrypted: target.physically_encrypted,
        image,
    })
}

/// Construct the explicit legacy all-exFAT filesystem stage for all logical
/// partitions that carry a filesystem. New provisioning uses each target's
/// configured filesystem through `build_official_partition_filesystem`.
///
/// The whole-disk-encrypted mode's 0x7E00 type1 compatibility entry is not a
/// filesystem. The mode1 front type2 is physically plaintext because it is
/// exposed directly by the MBR; all other type2/type4 filesystem images use
/// the verified current mode2 SM4 sector transform.
pub fn build_official_exfat_partitions(
    plan: &OfficialProvisionPlan,
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serials: &[u32],
) -> Result<Vec<PartitionFilesystemImage>, String> {
    if plan.filesystem_format != FilesystemKind::ExFat {
        return Err(format!(
            "portable filesystem writer does not yet implement {}",
            plan.filesystem_format.config_token()
        ));
    }
    let targets = plan.format_targets()?;
    if volume_serials.len() != targets.len() {
        return Err(format!(
            "filesystem volume serial count mismatch: got {}, need {}",
            volume_serials.len(),
            targets.len()
        ));
    }
    let mut out = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().enumerate() {
        if !target.format_capable {
            continue;
        }
        out.push(build_official_exfat_partition(
            plan,
            target,
            file_key,
            volume_label,
            volume_serials[index],
        )?);
    }
    Ok(out)
}
