//! Typed, UI-neutral post-restore assessment and follow-up requests.
//!
//! Metadata restore success is independent from filesystem usability. This
//! module performs read-only assessment only; it never formats or writes media.

use std::path::PathBuf;

use crate::backup_deep::keys::{decrypt_mode2, DefaultFileKeyError};
use crate::common::SECTOR;
use crate::diskio::SectorDev;
use crate::edpb::ManifestPartition;
use crate::inspect_target::{detect_plain_filesystem, FilesystemBootKind};
use crate::provision::{
    build_empty_exfat, build_empty_fat16, parse_existing_provision, FileKeyWrapMode,
    OfficialFilesystemFormat, ProvisionImage, SecretBytes,
};

mod format_operation;
pub use format_operation::format_partition_after_restore_on_disk;
pub use format_operation::format_partition_on_disk;

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
    pub detected_filesystem: Option<FilesystemBootKind>,
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
    pub filesystem: OfficialFilesystemFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostRestoreFormatResult {
    pub partition_index: u32,
    pub filesystem: OfficialFilesystemFormat,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedPartitionReinitializeRequest {
    pub partition_index: u32,
    new_password: SecretBytes,
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
        let image = match request.filesystem {
            OfficialFilesystemFormat::Fat16 => build_empty_fat16(
                partition.start_lba,
                partition.sector_count,
                volume_serial,
                volume_label,
            ),
            OfficialFilesystemFormat::ExFat => build_empty_exfat(
                partition.start_lba,
                partition.sector_count,
                volume_serial,
                volume_label,
            ),
            OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => Err(format!(
                "portable filesystem writer does not yet implement {}",
                request.filesystem.config_token()
            )),
        }
        .map_err(|error| format!("格式化镜像生成失败: {error}"))?;
        if image
            .sectors()
            .keys()
            .any(|relative| *relative >= partition.sector_count)
        {
            return Err("格式化镜像写入范围超出所选分区".into());
        }
        super::filesystem_format::write_sparse_filesystem_image(
            dev,
            partition.start_lba,
            &image,
            &mut |_| {},
        )
        .map_err(|error| error.msg)?;
        let boot = read_sector(dev, partition.start_lba)?;
        let detected = detect_plain_filesystem(partition.sector_count, &boot)
            .ok_or_else(|| "格式化后文件系统 boot sector 未通过严格校验".to_string())?;
        let expected = match request.filesystem {
            OfficialFilesystemFormat::Fat16 => FilesystemBootKind::Fat16,
            OfficialFilesystemFormat::ExFat => FilesystemBootKind::Exfat,
            OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => unreachable!(),
        };
        if detected != expected {
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
            state: PostRestorePartitionState::Unsupported,
            detail: "分区几何超出目标介质范围".into(),
        };
    }

    match read_sector(dev, partition.start_lba) {
        Ok(boot) => {
            let detected = detect_plain_filesystem(partition.sector_count, &boot);
            PostRestorePartition {
                index: partition.index,
                role: partition.role.clone(),
                start_lba: partition.start_lba,
                sector_count: partition.sector_count,
                filesystem_hint: partition.filesystem_hint.clone(),
                detected_filesystem: detected,
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
            state: PostRestorePartitionState::Unsupported,
            detail: error,
        },
    }
}

fn edp_crypto_state(
    protocol_image: &[u8],
    device_id: &str,
    record: crate::provision::ExistingPartitionRecord,
    index: usize,
    boot: &[u8],
    sector_count: u64,
) -> (
    PostRestorePartitionState,
    Option<FilesystemBootKind>,
    String,
) {
    if record.lba12.need_encrypt == 0 {
        let detected = detect_plain_filesystem(sector_count, boot);
        return if let Some(filesystem) = detected {
            (
                PostRestorePartitionState::Usable,
                Some(filesystem),
                "明文文件系统 boot sector 通过严格校验".into(),
            )
        } else {
            (
                PostRestorePartitionState::NeedsFormat,
                None,
                "明文分区没有可验证文件系统".into(),
            )
        };
    }

    if FileKeyWrapMode::from_raw(record.lba12.encrypt_mode).is_none() {
        return (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            format!("未知 EncryptMode={}", record.lba12.encrypt_mode),
        );
    }

    if record.lba12.encrypt_mode != FileKeyWrapMode::Sm4.raw() {
        return (
            PostRestorePartitionState::PasswordRequired,
            None,
            "加密域存在；需要原密码验证并 unwrap 原 FileKey".into(),
        );
    }

    match crate::backup_deep::keys::default_file_key_checked(protocol_image, device_id, index) {
        Ok(file_key) => match decrypt_mode2(boot, &file_key) {
            Ok(plain_boot) => {
                let detected = detect_plain_filesystem(sector_count, &plain_boot);
                if let Some(filesystem) = detected {
                    (
                        PostRestorePartitionState::Usable,
                        Some(filesystem),
                        "默认密码与 FileKeyCRC 均验证通过，解密后文件系统可用".into(),
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
        },
        Err(DefaultFileKeyError::NotDefaultPassword) => (
            PostRestorePartitionState::PasswordRequired,
            None,
            "默认密码不匹配；需要原密码".into(),
        ),
        Err(DefaultFileKeyError::FileKeyCrcMismatch) => (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            "默认密码路径 unwrap 后 FileKeyCRC 不匹配".into(),
        ),
        Err(DefaultFileKeyError::NotEncryptedMode2) => (
            PostRestorePartitionState::PasswordRequired,
            None,
            "加密域存在；需要原密码验证".into(),
        ),
        Err(error) => (
            PostRestorePartitionState::CryptoMetadataInvalid,
            None,
            format!("加密元数据异常: {error}"),
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
                    state: PostRestorePartitionState::Unsupported,
                    detail: error,
                });
                continue;
            }
        };
        let (state, detected_filesystem, detail) = edp_crypto_state(
            &raw_protocol,
            device_id,
            record,
            index,
            &boot,
            partition.sector_count,
        );
        assessment.partitions.push(PostRestorePartition {
            index: manifest.map_or((index + 1) as u32, |value| value.index),
            role: manifest.and_then(|value| value.role.clone()),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem_hint: manifest.and_then(|value| value.filesystem_hint.clone()),
            detected_filesystem,
            state,
            detail,
        });
    }
    Ok(assessment)
}
