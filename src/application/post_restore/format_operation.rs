//! Independently authorized post-restore formatting of one plaintext partition.

use super::*;
use crate::application::media_identity::MediaIdentityResumePin;
use crate::application::progress::FormatStep;
use crate::application::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::application::Prompter;
use crate::common::{EdpCliError, EXIT_TARGET};
use crate::sysinfo::{self, CmdRunner};
use std::time::Duration;

pub(super) const REOPEN_WAIT: Duration = Duration::from_secs(10);

pub fn format_partition_after_restore_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    prompt: &mut dyn Prompter,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    volume_label: &str,
) -> PostRestoreFormatResult {
    format_partition_after_restore_on_disk_assessed(
        runner,
        disk,
        prompt,
        outcome,
        request,
        volume_label,
    )
    .format
}

pub fn format_partition_after_restore_on_disk_assessed(
    runner: &dyn CmdRunner,
    disk: u32,
    prompt: &mut dyn Prompter,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    volume_label: &str,
) -> AssessedPostRestoreFormatResult {
    format_progress::emit(prompt, format_progress::stage(FormatStep::VerifyTarget));
    let volume_serial = crate::diskio::Clock::now_epoch(&crate::diskio::SystemClock) as u32;
    let mut assessment = None;
    let result = (|| -> Result<(), PostRestoreFormatError> {
        let expected = outcome
            .format_target_pin
            .as_ref()
            .ok_or(PostRestoreFormatError::TargetIdentityMissing)?;
        let mut dev = crate::application::device::open_readonly_usb_disk(runner, disk)
            .map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
        format_partition_on_disk_with_key(
            runner,
            disk,
            &mut dev,
            prompt,
            expected,
            outcome,
            request,
            volume_label,
            volume_serial,
            None,
            &mut assessment,
        )
        .result
    })();
    AssessedPostRestoreFormatResult {
        format: PostRestoreFormatResult {
            partition_index: request.partition_index,
            filesystem: request.filesystem,
            result,
        },
        assessment,
    }
}

fn verified_original_file_key<'a>(
    dev: &mut dyn SectorDev,
    outcome: &MetadataRestoreOutcome,
    partition_index: u32,
    password: Option<&'a [u8]>,
) -> Result<(&'a [u8], [u8; 16]), ExistingFileKeyError> {
    let protocol =
        read_protocol_image(dev).map_err(|_| ExistingFileKeyError::MalformedKeyRecord)?;
    let image = ProvisionImage::from_bytes(protocol)
        .map_err(|_| ExistingFileKeyError::MalformedKeyRecord)?;
    let parsed = parse_existing_provision(&image, &outcome.device_id, outcome.total_sectors)
        .map_err(|_| ExistingFileKeyError::MalformedKeyRecord)?
        .ok_or(ExistingFileKeyError::MalformedKeyRecord)?;
    let index = partition_index
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or(ExistingFileKeyError::MalformedKeyRecord)?;
    let record = parsed
        .records
        .get(index)
        .ok_or(ExistingFileKeyError::MalformedKeyRecord)?;
    if record.lba12.need_encrypt == 0 {
        return Err(ExistingFileKeyError::MalformedKeyRecord);
    }
    if record.lba12.encrypt_mode != FileKeyWrapMode::Sm4.raw() {
        return Err(ExistingFileKeyError::UnsupportedEncryptMode);
    }
    let default = crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD;
    match record.verified_file_key(Some(default)) {
        Ok(key) => Ok((default, key)),
        Err(ExistingFileKeyError::PasswordMismatch) => {
            let password = password
                .filter(|password| !password.is_empty())
                .ok_or(ExistingFileKeyError::PasswordRequired)?;
            record
                .verified_file_key(Some(password))
                .map(|key| (password, key))
        }
        Err(error) => Err(error),
    }
}

/// Format an encrypted partition with its verified original password and FileKey.
/// No protocol key record is rewritten.
#[allow(clippy::too_many_arguments)]
pub fn format_encrypted_partition_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    password: Option<&[u8]>,
    volume_label: &str,
    volume_serial: u32,
) -> EncryptedPostRestoreFormatResult {
    format_progress::emit(prompt, format_progress::stage(FormatStep::VerifyTarget));
    let result = match verified_original_file_key(dev, outcome, request.partition_index, password) {
        Ok((original_password, key)) => format_partition_on_disk_with_key(
            runner,
            disk,
            dev,
            prompt,
            expected,
            outcome,
            request,
            volume_label,
            volume_serial,
            Some((original_password, &key)),
            &mut None,
        )
        .result
        .map_err(EncryptedPostRestoreError::Operation),
        Err(error) => Err(EncryptedPostRestoreError::FileKey(error)),
    };
    EncryptedPostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
    }
}

pub fn format_encrypted_partition_after_restore_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    prompt: &mut dyn Prompter,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    password: Option<&[u8]>,
    volume_label: &str,
) -> EncryptedPostRestoreFormatResult {
    format_progress::emit(prompt, format_progress::stage(FormatStep::VerifyTarget));
    let result = (|| -> Result<(), EncryptedPostRestoreError> {
        let expected =
            outcome
                .format_target_pin
                .as_ref()
                .ok_or(EncryptedPostRestoreError::Operation(
                    PostRestoreFormatError::TargetIdentityMissing,
                ))?;
        let mut dev =
            crate::application::device::open_readonly_usb_disk(runner, disk).map_err(|error| {
                EncryptedPostRestoreError::Operation(PostRestoreFormatError::Operation(
                    error.into(),
                ))
            })?;
        format_encrypted_partition_on_disk(
            runner,
            disk,
            &mut dev,
            prompt,
            expected,
            outcome,
            request,
            password,
            volume_label,
            crate::diskio::Clock::now_epoch(&crate::diskio::SystemClock) as u32,
        )
        .result
    })();
    EncryptedPostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
    }
}

pub(super) fn failure(message: impl Into<String>) -> EdpCliError {
    EdpCliError::new(EXIT_TARGET, message)
}

pub(super) enum KeyCheck<'a> {
    Plain,
    Existing(&'a [u8; 16]),
    Reinitialize,
}

pub(super) fn verify_current_target(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
    expected: &MediaIdentityResumePin,
    outcome: &MetadataRestoreOutcome,
    partition: &ManifestPartition,
    key_check: KeyCheck<'_>,
) -> Result<(), PostRestoreFormatError> {
    let total_sectors = outcome.total_sectors;
    let observed = crate::application::media_identity_observer::observe_media_identity_readonly(
        runner, disk, dev,
    )
    .map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
    expected
        .verify(&observed.snapshot, &observed.protocol_image)
        .map_err(PostRestoreFormatError::TargetIdentity)?;
    if sysinfo::disk_total_sectors(runner, disk) != Some(total_sectors)
        || observed.snapshot.hardware.total_sectors != Some(total_sectors)
        || observed.snapshot.hardware.logical_sector_size != Some(SECTOR as u32)
    {
        return Err(PostRestoreFormatError::TargetGeometryChanged);
    }
    if partition.sector_count == 0
        || partition
            .start_lba
            .checked_add(partition.sector_count)
            .is_none_or(|end| end > total_sectors)
    {
        return Err(PostRestoreFormatError::GeometryMismatch {
            index: partition.index,
        });
    }

    if outcome.device_state.eq_ignore_ascii_case("plain") {
        if !matches!(key_check, KeyCheck::Plain) {
            return Err(PostRestoreFormatError::Operation(
                "Plain 分区不能使用加密密钥格式化".into(),
            ));
        }
        let table = crate::partition_table::read_partition_table(total_sectors, |lba| {
            read_sector(dev, lba)
        })
        .map_err(|message| {
            PostRestoreFormatError::Operation((format!("重读分区表失败: {message}")).into())
        })?;
        if !table.partitions.iter().any(|current| {
            current.index == partition.index as usize
                && current.start_lba == partition.start_lba
                && current.sector_count == partition.sector_count
        }) {
            return Err(PostRestoreFormatError::GeometryMismatch {
                index: partition.index,
            });
        }
    } else {
        let image = ProvisionImage::from_bytes(observed.protocol_image).map_err(|message| {
            PostRestoreFormatError::Operation((format!("重读 EDP 协议失败: {message}")).into())
        })?;
        let parsed = parse_existing_provision(&image, &outcome.device_id, total_sectors)
            .map_err(|message| {
                PostRestoreFormatError::Operation((format!("重读 EDP 分区失败: {message}")).into())
            })?
            .ok_or_else(|| {
                PostRestoreFormatError::Operation("当前盘缺少有效 EDP 分区记录".into())
            })?;
        let index = partition
            .index
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or(PostRestoreFormatError::PartitionNotFound {
                index: partition.index,
            })?;
        let current = parsed.profile.partitions.get(index).ok_or(
            PostRestoreFormatError::PartitionNotFound {
                index: partition.index,
            },
        )?;
        let record = parsed
            .records
            .get(index)
            .ok_or_else(|| PostRestoreFormatError::Operation("EDP 分区密钥记录不存在".into()))?;
        if current.start_lba != partition.start_lba
            || current.sector_count != partition.sector_count
        {
            return Err(PostRestoreFormatError::GeometryMismatch {
                index: partition.index,
            });
        }
        match key_check {
            KeyCheck::Reinitialize => {
                if record.lba12.need_encrypt == 0 || !current.physically_encrypted {
                    return Err(PostRestoreFormatError::Operation(
                        "所选目标不是 EDP 加密数据分区".into(),
                    ));
                }
            }
            KeyCheck::Existing(key) => {
                if !current.physically_encrypted
                    || record.lba12.need_encrypt == 0
                    || record.lba12.encrypt_mode != FileKeyWrapMode::Sm4.raw()
                    || crate::crypto::crc32_bare(key) != record.lba12.file_key_crc
                {
                    return Err(PostRestoreFormatError::Operation(
                        "当前密钥域与已验证原 FileKey 不一致".into(),
                    ));
                }
            }
            KeyCheck::Plain if current.physically_encrypted => {
                return Err(PostRestoreFormatError::Operation(
                    "物理加密分区禁止明文格式化".into(),
                ));
            }
            KeyCheck::Plain => {}
        }
    }
    Ok(())
}

/// Execute a separately selected and confirmed format operation on one partition.
/// The caller's metadata-restore report is never mutated by this operation.
#[allow(clippy::too_many_arguments)]
pub fn format_partition_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    volume_label: &str,
    volume_serial: u32,
) -> PostRestoreFormatResult {
    format_partition_on_disk_with_key(
        runner,
        disk,
        dev,
        prompt,
        expected,
        outcome,
        request,
        volume_label,
        volume_serial,
        None,
        &mut None,
    )
}

#[allow(clippy::too_many_arguments)]
fn format_partition_on_disk_with_key(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
    outcome: &MetadataRestoreOutcome,
    request: &PartitionFormatRequest,
    volume_label: &str,
    volume_serial: u32,
    original_key: Option<(&[u8], &[u8; 16])>,
    assessment_sink: &mut Option<PostRestoreAssessment>,
) -> PostRestoreFormatResult {
    format_progress::emit(prompt, format_progress::stage(FormatStep::VerifyTarget));
    let mut format_committed = false;
    let result = (|| -> Result<(), PostRestoreFormatError> {
        if !outcome.report.metadata_restored || !outcome.report.readback_verified {
            return Err(PostRestoreFormatError::Operation(
                "元数据恢复尚未成功且读回验证，禁止后续格式化".into(),
            ));
        }
        validate_writable_filesystem(request.filesystem)?;
        expected
            .validate()
            .map_err(|message| PostRestoreFormatError::TargetPinInvalid(message.to_string()))?;
        let partition = outcome
            .partitions
            .iter()
            .find(|partition| partition.index == request.partition_index)
            .ok_or(PostRestoreFormatError::PartitionNotFound {
                index: request.partition_index,
            })?;
        let assessed = outcome
            .assessment
            .partitions
            .iter()
            .find(|candidate| candidate.index == request.partition_index)
            .ok_or(PostRestoreFormatError::PartitionNotFound {
                index: request.partition_index,
            })?;
        if assessed.start_lba != partition.start_lba
            || assessed.sector_count != partition.sector_count
        {
            return Err(PostRestoreFormatError::GeometryMismatch {
                index: request.partition_index,
            });
        }
        let eligible = match original_key {
            Some(_) => matches!(
                assessed.state,
                PostRestorePartitionState::NeedsFormat
                    | PostRestorePartitionState::PasswordRequired
            ),
            None => assessed.state == PostRestorePartitionState::NeedsFormat,
        };
        if !eligible {
            return Err(PostRestoreFormatError::PartitionState {
                index: request.partition_index,
                state: assessed.state,
            });
        }
        let session = TargetSession::<ReadOnly>::open_usb(runner, disk)
            .map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
        verify_current_target(
            runner,
            disk,
            dev,
            expected,
            outcome,
            partition,
            original_key.map_or(KeyCheck::Plain, |(_, key)| KeyCheck::Existing(key)),
        )?;
        let assess = |dev: &mut dyn SectorDev| match original_key {
            Some((password, _)) => assess_partitions_with_password_readonly(
                dev,
                &outcome.device_state,
                &outcome.device_id,
                outcome.total_sectors,
                &outcome.partitions,
                request.partition_index,
                password,
            ),
            None => assess_partitions_readonly(
                dev,
                &outcome.device_state,
                &outcome.device_id,
                outcome.total_sectors,
                &outcome.partitions,
            ),
        };
        let fresh_assessment =
            assess(dev).map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
        let fresh = fresh_assessment
            .partitions
            .iter()
            .find(|candidate| candidate.index == request.partition_index)
            .ok_or(PostRestoreFormatError::PartitionNotFound {
                index: request.partition_index,
            })?;
        if fresh.start_lba != partition.start_lba || fresh.sector_count != partition.sector_count {
            return Err(PostRestoreFormatError::GeometryMismatch {
                index: request.partition_index,
            });
        }
        if fresh.state != PostRestorePartitionState::NeedsFormat {
            return Err(PostRestoreFormatError::PartitionState {
                index: request.partition_index,
                state: fresh.state,
            });
        }
        if !prompt.confirm_post_restore_format_yes(&format!(
            "单独确认格式化 disk{disk} 分区 {} (LBA{} + {} sectors) 为 {}；输入 YES: ",
            partition.index,
            partition.start_lba,
            partition.sector_count,
            request.filesystem.config_token()
        )) {
            return Err(PostRestoreFormatError::Cancelled);
        }
        format_progress::emit(prompt, format_progress::stage(FormatStep::LockAndReopen));
        let session = session.prepare_write().map_err(|error| {
            PostRestoreFormatError::Operation((format!("无法卸载/锁定 disk{disk}: {error}")).into())
        })?;
        let mut locked_session = session
            .reopen_and_verify(dev, REOPEN_WAIT, |dev| {
                verify_current_target(
                    runner,
                    disk,
                    dev,
                    expected,
                    outcome,
                    partition,
                    original_key.map_or(KeyCheck::Plain, |(_, key)| KeyCheck::Existing(key)),
                )?;
                let reopened =
                    assess(dev).map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
                let candidate = reopened
                    .partitions
                    .iter()
                    .find(|candidate| candidate.index == request.partition_index)
                    .ok_or(PostRestoreFormatError::PartitionNotFound {
                        index: request.partition_index,
                    })?;
                if candidate.start_lba != partition.start_lba
                    || candidate.sector_count != partition.sector_count
                {
                    return Err(PostRestoreFormatError::GeometryMismatch {
                        index: request.partition_index,
                    });
                }
                if candidate.state != PostRestorePartitionState::NeedsFormat {
                    return Err(PostRestoreFormatError::PartitionState {
                        index: request.partition_index,
                        state: candidate.state,
                    });
                }
                Ok(())
            })
            .map_err(|error| match error {
                ReopenAndVerifyError::Reopen(error) => PostRestoreFormatError::Operation(
                    (format!("重开 disk{disk} 失败: {error}")).into(),
                ),
                ReopenAndVerifyError::Verify(error) => error,
                ReopenAndVerifyError::Geometry(error) => {
                    PostRestoreFormatError::Operation(error.into())
                }
            })?;

        let dev = locked_session.device();
        let format = format_partition_after_restore(
            dev,
            &fresh_assessment,
            partition,
            request,
            volume_label,
            volume_serial,
            original_key.map(|(_, key)| key),
            &mut |event| prompt.operation_progress(event),
        );
        format.result?;
        format_committed = true;
        format_progress::emit(prompt, format_progress::stage(FormatStep::Reassess));
        let after = assess(dev).map_err(|error| PostRestoreFormatError::Operation(error.into()))?;
        let candidate = after
            .partitions
            .iter()
            .find(|candidate| candidate.index == request.partition_index)
            .ok_or(PostRestoreFormatError::PartitionNotFound {
                index: request.partition_index,
            })?;
        if candidate.start_lba != partition.start_lba
            || candidate.sector_count != partition.sector_count
        {
            return Err(PostRestoreFormatError::GeometryMismatch {
                index: request.partition_index,
            });
        }
        if candidate.state != PostRestorePartitionState::Usable {
            return Err(PostRestoreFormatError::PartitionState {
                index: request.partition_index,
                state: candidate.state,
            });
        }
        *assessment_sink = Some(after);
        format_progress::emit(prompt, format_progress::completed());
        Ok(())
    })()
    .map_err(|error| {
        if format_committed && error.media_state().is_none() {
            PostRestoreFormatError::UnverifiedAfterWrite(Box::new(error))
        } else {
            error
        }
    });
    PostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
    }
}
