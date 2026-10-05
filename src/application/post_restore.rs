//! Typed, UI-neutral post-restore assessment and follow-up requests.
//!
//! Metadata restore success is independent from filesystem usability. This
//! Assessment is read-only; explicit follow-up format operations use the separate
//! write authorization chain in `format_operation`.

use std::path::PathBuf;

use crate::common::SECTOR;
use crate::diskio::SectorDev;
use crate::edpb::ManifestPartition;
use crate::filesystem::{build_empty_filesystem, FilesystemKind, SparseFilesystemImage};
use crate::partition_transform::{decrypt_mode2, EdpSm4Transform};
use crate::provision::{
    parse_existing_provision, ExistingFileKeyError, FileKeyWrapMode, ProvisionImage, SecretBytes,
};

mod format_operation;
mod layout_projection;
mod reinitialize;
pub use format_operation::format_partition_after_restore_on_disk;
pub use format_operation::format_partition_on_disk;
pub use format_operation::{
    format_encrypted_partition_after_restore_on_disk, format_encrypted_partition_on_disk,
};
pub use layout_projection::project_restored_layout_readonly;
pub use reinitialize::{
    reinitialize_encrypted_partition_after_restore_on_disk,
    reinitialize_encrypted_partition_on_disk, EncryptedPartitionReinitializeResult,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataBackupReport {
    pub path: PathBuf,
    pub partition_count: usize,
    pub edp_protocol_saved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRestoreReport {
    pub metadata_restored: bool,
    pub readback_verified: bool,
    pub restored_artifact_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRestoreOutcome {
    pub report: MetadataRestoreReport,
    pub assessment: PostRestoreAssessment,
    pub partitions: Vec<ManifestPartition>,
    pub device_state: String,
    pub device_id: String,
    pub total_sectors: u64,
    pub layout: Result<crate::application::disk_layout::DiskLayoutModel, String>,
    pub format_target_pin: Option<crate::media_identity::MediaIdentityResumePin>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostRestorePartitionState {
    Usable,
    NeedsFormat,
    PasswordRequired,
    CryptoMetadataInvalid,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostRestorePartition {
    pub index: u32,
    pub role: Option<String>,
    pub start_lba: u64,
    pub sector_count: u64,
    pub filesystem_hint: Option<String>,
    pub detected_filesystem: Option<FilesystemKind>,
    pub requires_original_key: bool,
    pub state: PostRestorePartitionState,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PostRestoreAssessment {
    pub partitions: Vec<PostRestorePartition>,
    pub issues: Vec<String>,
}

impl PostRestoreAssessment {
    pub fn unsupported(partitions: &[ManifestPartition], message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            partitions: partitions
                .iter()
                .map(|partition| PostRestorePartition {
                    index: partition.index,
                    role: partition.role.clone(),
                    start_lba: partition.start_lba,
                    sector_count: partition.sector_count,
                    filesystem_hint: partition.filesystem_hint.clone(),
                    detected_filesystem: None,
                    requires_original_key: false,
                    state: PostRestorePartitionState::Unsupported,
                    detail: message.clone(),
                })
                .collect(),
            issues: vec![message],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionFormatRequest {
    pub partition_index: u32,
    pub filesystem: FilesystemKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostRestoreFormatResult {
    pub partition_index: u32,
    pub filesystem: FilesystemKind,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncryptedPostRestoreError {
    FileKey(ExistingFileKeyError),
    Operation(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedPostRestoreFormatResult {
    pub partition_index: u32,
    pub filesystem: FilesystemKind,
    pub result: Result<(), EncryptedPostRestoreError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedPartitionReinitializeRequest {
    pub partition_index: u32,
    new_password: SecretBytes,
}

pub(crate) fn build_empty_partition_image(
    partition: &ManifestPartition,
    filesystem: FilesystemKind,
    volume_label: &str,
    volume_serial: u32,
) -> Result<SparseFilesystemImage, String> {
    build_empty_filesystem(
        filesystem,
        partition.start_lba,
        partition.sector_count,
        volume_serial,
        Some(volume_label),
    )
}

impl EncryptedPartitionReinitializeRequest {
    pub fn new(
        partition_index: u32,
        new_password: impl AsRef<[u8]>,
        confirmation: impl AsRef<[u8]>,
    ) -> Result<Self, String> {
        let new_password = new_password.as_ref();
        let confirmation = confirmation.as_ref();
        if new_password.is_empty() {
            return Err("新密码不能为空".into());
        }
        if new_password != confirmation {
            return Err("两次输入的新密码不一致".into());
        }
        Ok(Self {
            partition_index,
            new_password: SecretBytes::new(new_password),
        })
    }

    pub fn password(&self) -> &[u8] {
        self.new_password.as_bytes()
    }
}

pub(crate) fn format_partition_after_restore(
    dev: &mut dyn SectorDev,
    assessment: &PostRestoreAssessment,
    partition: &ManifestPartition,
    request: &PartitionFormatRequest,
    volume_label: &str,
    volume_serial: u32,
    file_key: Option<&[u8; 16]>,
) -> PostRestoreFormatResult {
    let result = (|| {
        if request.partition_index != partition.index {
            return Err("格式化请求与目标分区索引不一致".to_string());
        }
        let state = assessment
            .partitions
            .iter()
            .find(|candidate| candidate.index == partition.index)
            .ok_or_else(|| "恢复后评估中找不到目标分区".to_string())?
            .state;
        if state != PostRestorePartitionState::NeedsFormat {
            return Err(format!(
                "分区当前状态为 {state:?}，只有 NeedsFormat 可进入明文格式化"
            ));
        }
        let assessed = assessment
            .partitions
            .iter()
            .find(|candidate| candidate.index == partition.index)
            .expect("state lookup succeeded");
        if assessed.start_lba != partition.start_lba
            || assessed.sector_count != partition.sector_count
        {
            return Err("恢复后评估分区几何与格式化目标不一致".into());
        }
        let plain_image =
            build_empty_partition_image(partition, request.filesystem, volume_label, volume_serial)
                .map_err(|error| format!("格式化镜像生成失败: {error}"))?;
        if plain_image
            .sectors()
            .keys()
            .any(|relative| *relative >= partition.sector_count)
        {
            return Err("格式化镜像写入范围超出所选分区".into());
        }
        let image = if let Some(file_key) = file_key {
            plain_image.transformed(&EdpSm4Transform::new(*file_key))
        } else {
            plain_image
        };
        super::filesystem_format::write_sparse_filesystem_image(
            dev,
            partition.start_lba,
            &image,
            &mut |_| {},
        )
        .map_err(|error| error.msg)?;
        let boot = read_sector(dev, partition.start_lba)?;
        let plain_boot = if let Some(file_key) = file_key {
            decrypt_mode2(&boot, file_key)?
        } else {
            boot
        };
        let detected = crate::filesystem::detect_boot_sector(partition.sector_count, &plain_boot)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "格式化后文件系统 boot sector 未通过严格校验".to_string())?;
        if detected != request.filesystem {
            return Err(format!(
                "格式化后文件系统类型不一致: expected {}, got {}",
                request.filesystem.config_token(),
                detected.label()
            ));
        }
        Ok(())
    })();
    PostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
    }
}

fn read_sector(dev: &mut dyn SectorDev, lba: u64) -> Result<Vec<u8>, String> {
    let lba =
        u32::try_from(lba).map_err(|_| format!("LBA{lba} exceeds current SectorDev range"))?;
    let raw = dev
        .read_sector(lba)
        .map_err(|error| format!("read LBA{lba} failed: {error}"))?;
    if raw.len() != SECTOR {
        return Err(format!(
            "read LBA{lba} returned {}B, expected {SECTOR}B",
            raw.len()
        ));
    }
    Ok(raw)
}

fn read_protocol_image(dev: &mut dyn SectorDev) -> Result<Vec<u8>, String> {
    let mut image = Vec::with_capacity(13 * SECTOR);
    for lba in 0..13u64 {
        image.extend_from_slice(&read_sector(dev, lba)?);
    }
    Ok(image)
}

fn plain_partition(
    dev: &mut dyn SectorDev,
    partition: &ManifestPartition,
    total_sectors: u64,
) -> PostRestorePartition {
    let end = partition
        .start_lba
        .checked_add(partition.sector_count)
        .filter(|end| *end <= total_sectors);
    if partition.sector_count == 0 || end.is_none() {
        return PostRestorePartition {
            index: partition.index,
            role: partition.role.clone(),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem_hint: partition.filesystem_hint.clone(),
            detected_filesystem: None,
            requires_original_key: false,
            state: PostRestorePartitionState::Unsupported,
            detail: "分区几何超出目标介质范围".into(),
        };
    }

    match read_sector(dev, partition.start_lba) {
        Ok(boot) => {
            let detected = crate::filesystem::detect_boot_sector(partition.sector_count, &boot)
                .ok()
                .flatten();
            PostRestorePartition {
                index: partition.index,
                role: partition.role.clone(),
                start_lba: partition.start_lba,
                sector_count: partition.sector_count,
                filesystem_hint: partition.filesystem_hint.clone(),
                detected_filesystem: detected,
                requires_original_key: false,
                state: if detected.is_some() {
                    PostRestorePartitionState::Usable
                } else {
                    PostRestorePartitionState::NeedsFormat
                },
                detail: if detected.is_some() {
                    "当前文件系统 boot sector 通过严格校验".into()
                } else {
                    "元数据已恢复，但当前分区没有可验证文件系统".into()
                },
            }
        }
        Err(error) => PostRestorePartition {
            index: partition.index,
            role: partition.role.clone(),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem_hint: partition.filesystem_hint.clone(),
            detected_filesystem: None,
            requires_original_key: false,
            state: PostRestorePartitionState::Unsupported,
            detail: error,
        },
    }
}

fn compatibility_reserve_partition(partition: &ManifestPartition) -> Option<PostRestorePartition> {
    (partition.role.as_deref() == Some("compatibility_reserve")).then(|| PostRestorePartition {
        index: partition.index,
        role: partition.role.clone(),
        start_lba: partition.start_lba,
        sector_count: partition.sector_count,
        filesystem_hint: None,
        detected_filesystem: None,
        requires_original_key: false,
        state: PostRestorePartitionState::Usable,
        detail: "模式2兼容保留区不承载文件系统；无需格式化。".into(),
    })
}

fn edp_crypto_state(
    record: crate::provision::ExistingPartitionRecord,
    physically_encrypted: bool,
    boot: &[u8],
    sector_count: u64,
    password: Option<&[u8]>,
) -> (PostRestorePartitionState, Option<FilesystemKind>, String) {
    let password_source = if password.is_some() {
        "原密码"
    } else {
        "默认密码"
    };
    if !physically_encrypted {
        let detected = crate::filesystem::detect_boot_sector(sector_count, boot)
            .ok()
            .flatten();
        return if let Some(filesystem) = detected {
            (
                PostRestorePartitionState::Usable,
                Some(filesystem),
                "模式语义=物理明文；文件系统 boot sector 通过严格校验".into(),
            )
        } else {
            (
                PostRestorePartitionState::NeedsFormat,
                None,
                format!(
                    "模式语义=物理明文；NeedEncrypt={} 仅是协议/密钥域字段；当前没有可验证文件系统",
                    record.lba12.need_encrypt
                ),
            )
        };
    }
    if record.lba12.need_encrypt == 0 {
        return (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            "模式语义要求物理密文，但协议 NeedEncrypt=0".into(),
        );
    }

    match record.verified_file_key(Some(
        password.unwrap_or(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD),
    )) {
        Ok(file_key) if record.lba12.encrypt_mode == FileKeyWrapMode::Sm4.raw() => {
            match decrypt_mode2(boot, &file_key) {
                Ok(plain_boot) => {
                    let detected = crate::filesystem::detect_boot_sector(sector_count, &plain_boot)
                        .ok()
                        .flatten();
                    if let Some(filesystem) = detected {
                        (
                            PostRestorePartitionState::Usable,
                            Some(filesystem),
                            format!(
                                "{password_source}与 FileKeyCRC 均验证通过，解密后文件系统可用"
                            ),
                        )
                    } else {
                        (
                            PostRestorePartitionState::NeedsFormat,
                            None,
                            "原密钥域验证通过，但解密后没有可验证文件系统".into(),
                        )
                    }
                }
                Err(error) => (
                    PostRestorePartitionState::CryptoMetadataInvalid,
                    None,
                    format!("已验证 FileKey 但解密 boot sector 失败: {error}"),
                ),
            }
        }
        Ok(_) => (
            PostRestorePartitionState::Unsupported,
            None,
            "原 FileKey 已验证，但当前版本不能验证此 EncryptMode 的文件系统 boot".into(),
        ),
        Err(ExistingFileKeyError::PasswordMismatch | ExistingFileKeyError::PasswordRequired) => (
            PostRestorePartitionState::PasswordRequired,
            None,
            format!("{password_source}不匹配；需要原密码"),
        ),
        Err(ExistingFileKeyError::FileKeyCrcMismatch) => (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            format!("{password_source}路径 unwrap 后 FileKeyCRC 不匹配"),
        ),
        Err(
            ExistingFileKeyError::UnsupportedEncryptMode | ExistingFileKeyError::MalformedKeyRecord,
        ) => (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            "加密元数据异常".into(),
        ),
    }
}

pub fn assess_partitions_readonly(
    dev: &mut dyn SectorDev,
    device_state: &str,
    device_id: &str,
    total_sectors: u64,
    partitions: &[ManifestPartition],
) -> Result<PostRestoreAssessment, String> {
    assess_partitions_impl(
        dev,
        device_state,
        device_id,
        total_sectors,
        partitions,
        None,
    )
}

pub fn assess_partitions_with_password_readonly(
    dev: &mut dyn SectorDev,
    device_state: &str,
    device_id: &str,
    total_sectors: u64,
    partitions: &[ManifestPartition],
    partition_index: u32,
    password: &[u8],
) -> Result<PostRestoreAssessment, String> {
    assess_partitions_impl(
        dev,
        device_state,
        device_id,
        total_sectors,
        partitions,
        Some((partition_index, password)),
    )
}

fn assess_partitions_impl(
    dev: &mut dyn SectorDev,
    device_state: &str,
    device_id: &str,
    total_sectors: u64,
    partitions: &[ManifestPartition],
    selected_password: Option<(u32, &[u8])>,
) -> Result<PostRestoreAssessment, String> {
    if device_state.eq_ignore_ascii_case("plain") {
        return Ok(PostRestoreAssessment {
            partitions: partitions
                .iter()
                .map(|partition| plain_partition(dev, partition, total_sectors))
                .collect(),
            issues: Vec::new(),
        });
    }

    if device_id.is_empty() {
        return Ok(PostRestoreAssessment {
            partitions: partitions
                .iter()
                .map(|partition| PostRestorePartition {
                    index: partition.index,
                    role: partition.role.clone(),
                    start_lba: partition.start_lba,
                    sector_count: partition.sector_count,
                    filesystem_hint: partition.filesystem_hint.clone(),
                    detected_filesystem: None,
                    requires_original_key: false,
                    state: PostRestorePartitionState::Unsupported,
                    detail: "缺少 device_id，不能可靠解码 EDP 元数据".into(),
                })
                .collect(),
            issues: vec!["缺少 device_id，EDP 恢复后状态只能标记 Unsupported".into()],
        });
    }

    let raw_protocol = read_protocol_image(dev)?;
    let image = ProvisionImage::from_bytes(raw_protocol.clone())?;
    let parsed = match parse_existing_provision(&image, device_id, total_sectors) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            return Ok(PostRestoreAssessment {
                partitions: partitions
                    .iter()
                    .map(|partition| PostRestorePartition {
                        index: partition.index,
                        role: partition.role.clone(),
                        start_lba: partition.start_lba,
                        sector_count: partition.sector_count,
                        filesystem_hint: partition.filesystem_hint.clone(),
                        detected_filesystem: None,
                        requires_original_key: false,
                        state: PostRestorePartitionState::CryptoMetadataInvalid,
                        detail: "EDP backup 恢复后未发现成对有效的 LBA7/LBA12 EDPF".into(),
                    })
                    .collect(),
                issues: vec!["EDP protocol registration missing after metadata restore".into()],
            });
        }
        Err(error) => {
            return Ok(PostRestoreAssessment {
                partitions: partitions
                    .iter()
                    .map(|partition| PostRestorePartition {
                        index: partition.index,
                        role: partition.role.clone(),
                        start_lba: partition.start_lba,
                        sector_count: partition.sector_count,
                        filesystem_hint: partition.filesystem_hint.clone(),
                        detected_filesystem: None,
                        requires_original_key: false,
                        state: PostRestorePartitionState::CryptoMetadataInvalid,
                        detail: error.clone(),
                    })
                    .collect(),
                issues: vec![error],
            });
        }
    };

    let mut assessment = PostRestoreAssessment::default();
    for (index, (partition, record)) in parsed
        .profile
        .partitions
        .iter()
        .zip(parsed.records.iter().copied())
        .enumerate()
    {
        let manifest = partitions
            .iter()
            .find(|candidate| candidate.start_lba == partition.start_lba);
        if let Some(reserved) = manifest.and_then(compatibility_reserve_partition) {
            assessment.partitions.push(reserved);
            continue;
        }
        let boot = match read_sector(dev, partition.start_lba) {
            Ok(boot) => boot,
            Err(error) => {
                assessment.partitions.push(PostRestorePartition {
                    index: manifest.map_or((index + 1) as u32, |value| value.index),
                    role: manifest.and_then(|value| value.role.clone()),
                    start_lba: partition.start_lba,
                    sector_count: partition.sector_count,
                    filesystem_hint: manifest.and_then(|value| value.filesystem_hint.clone()),
                    detected_filesystem: None,
                    requires_original_key: partition.physically_encrypted,
                    state: PostRestorePartitionState::Unsupported,
                    detail: error,
                });
                continue;
            }
        };
        let partition_index = manifest.map_or((index + 1) as u32, |value| value.index);
        let password = selected_password
            .filter(|(selected, _)| *selected == partition_index)
            .map(|(_, password)| password);
        let (state, detected_filesystem, detail) = edp_crypto_state(
            record,
            partition.physically_encrypted,
            &boot,
            partition.sector_count,
            password,
        );
        assessment.partitions.push(PostRestorePartition {
            index: manifest.map_or((index + 1) as u32, |value| value.index),
            role: manifest.and_then(|value| value.role.clone()),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem_hint: manifest.and_then(|value| value.filesystem_hint.clone()),
            detected_filesystem,
            requires_original_key: partition.physically_encrypted,
            state,
            detail,
        });
    }
    Ok(assessment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode2_compatibility_reserve_never_enters_filesystem_format_flow() {
        let partition = ManifestPartition {
            index: 1,
            role: Some("compatibility_reserve".into()),
            partition_type: Some("edp:1".into()),
            start_lba: 63,
            sector_count: 63,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: Some("stale-label".into()),
        };

        let assessed = compatibility_reserve_partition(&partition).expect("compatibility reserve");
        assert_eq!(assessed.state, PostRestorePartitionState::Usable);
        assert_eq!(assessed.start_lba, 63);
        assert_eq!(assessed.sector_count, 63);
        assert_eq!(assessed.filesystem_hint, None);
        assert_eq!(assessed.detected_filesystem, None);
        assert!(!assessed.requires_original_key);
        assert!(assessed.detail.contains("无需格式化"));
    }
}
