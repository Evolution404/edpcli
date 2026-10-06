//! EDP provisioning composition for filesystem images and partition transforms.
//!
//! Filesystem bytes are produced by the filesystem domain. This module owns
//! EDP-specific FileKeyCRC / EncryptMode validation and selects the physical
//! partition transform.

use crate::filesystem::{build_empty_filesystem, FilesystemKind, SparseFilesystemImage};
use crate::partition_transform::EdpSm4Transform;
use crate::protocol::crypto::crc32_bare;

use super::{
    layout::{OfficialPartitionGeometry, OfficialProvisionPlan, PartitionFormatTarget},
    FileKeyWrapMode,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionFilesystemImage {
    pub geometry: OfficialPartitionGeometry,
    pub physically_encrypted: bool,
    pub image: SparseFilesystemImage,
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
    crate::filesystem::validate_writable_filesystem(format).map_err(|error| error.to_string())?;
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

    let plain = build_empty_filesystem(
        format,
        target.geometry.start_sector,
        target.geometry.sector_count(),
        volume_serial,
        Some(volume_label),
    )?;

    let image = if target.physically_encrypted {
        plain.transformed(&EdpSm4Transform::new(*file_key))
    } else {
        plain
    };

    Ok(PartitionFilesystemImage {
        geometry: target.geometry,
        physically_encrypted: target.physically_encrypted,
        image,
    })
}
