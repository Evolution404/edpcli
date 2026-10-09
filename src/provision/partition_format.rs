//! EDP provisioning composition for filesystem images and partition transforms.
//!
//! Filesystem bytes are produced by the filesystem domain. This module owns
//! EDP-specific FileKeyCRC / EncryptMode validation and selects the physical
//! partition transform.

use crate::filesystem::{build_empty_filesystem, FilesystemKind, SparseFilesystemImage};
use crate::partition_transform::{
    transform_native_sector_offline, EdpSm4Transform, NativeCipherDirection,
    NativePartitionDataCipher,
};
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
    let cipher = validate_official_format_target(plan, target, file_key)?;
    let plain = build_empty_filesystem(
        format,
        target.geometry.start_sector,
        target.geometry.sector_count(),
        volume_serial,
        Some(volume_label),
    )?;
    prepared_from_plain(target, file_key, &plain, cipher)
}

fn validate_official_format_target(
    plan: &OfficialProvisionPlan,
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
) -> Result<Option<FileKeyWrapMode>, String> {
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
        // Logical ciphertext production is independently tested for all three
        // EncryptMode values. This does not relax the physical-write capability
        // gate enforced by the application commit path.
        return Ok(Some(material.encrypt_mode));
    }
    Ok(None)
}

/// Planning already built a fully validated plain image. Reuse it directly
/// rather than independently materializing the same sparse filesystem again.
/// All FileKeyCRC, role and encryption-mode checks remain mandatory.
pub(crate) fn build_official_partition_filesystem_from_plain(
    plan: &OfficialProvisionPlan,
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
    plain: &SparseFilesystemImage,
) -> Result<PartitionFilesystemImage, String> {
    let format = target
        .filesystem
        .ok_or("compatibility reserve is not a filesystem")?;
    crate::filesystem::validate_writable_filesystem(format).map_err(|error| error.to_string())?;
    let cipher = validate_official_format_target(plan, target, file_key)?;
    if plain.volume_sectors() != target.geometry.sector_count() {
        return Err("plain filesystem image size does not match target geometry".into());
    }
    prepared_from_plain(target, file_key, plain, cipher)
}

fn prepared_from_plain(
    target: &PartitionFormatTarget,
    file_key: &[u8; 16],
    plain: &SparseFilesystemImage,
    cipher: Option<FileKeyWrapMode>,
) -> Result<PartitionFilesystemImage, String> {
    let image = match cipher {
        None => plain.clone(),
        Some(FileKeyWrapMode::Sm4) => plain.transformed(&EdpSm4Transform::new(*file_key)),
        Some(mode) => {
            let data_cipher = NativePartitionDataCipher::from_encrypt_mode(mode.raw())?;
            plain.try_transformed(|relative_lba, block| {
                let absolute_lba = target
                    .geometry
                    .start_sector
                    .checked_add(relative_lba)
                    .ok_or("分区数据绝对LBA溢出")?;
                let output = transform_native_sector_offline(
                    data_cipher,
                    NativeCipherDirection::Encrypt,
                    block,
                    file_key,
                    absolute_lba,
                    512,
                )?;
                output
                    .try_into()
                    .map_err(|_| "原生512B分区加密输出长度不符".to_string())
            })?
        }
    };
    Ok(PartitionFilesystemImage {
        geometry: target.geometry,
        physically_encrypted: target.physically_encrypted,
        image,
    })
}
