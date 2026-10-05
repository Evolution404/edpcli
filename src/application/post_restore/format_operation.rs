//! Independently authorized post-restore formatting of one plaintext partition.

use super::*;
use crate::application::media_identity::MediaIdentityResumePin;
use crate::application::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::application::Prompter;
use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET};
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
    let volume_serial = crate::diskio::Clock::now_epoch(&crate::diskio::SystemClock) as u32;
    let result = (|| -> EdpCliResult<()> {
        let expected = outcome
            .format_target_pin
            .as_ref()
            .ok_or_else(|| failure("元数据恢复后未能固定目标介质身份，禁止格式化"))?;
        let mut dev = crate::application::device::open_readonly_usb_disk(runner, disk)?;
        let format = format_partition_on_disk(
            runner,
            disk,
            &mut dev,
            prompt,
            expected,
            outcome,
            request,
            volume_label,
            volume_serial,
        );
        format.result.map_err(failure)
    })()
    .map_err(|error: EdpCliError| error.msg);
    PostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
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
    let result = (|| -> EdpCliResult<Result<(), EncryptedPostRestoreError>> {
        let expected = outcome
            .format_target_pin
            .as_ref()
            .ok_or_else(|| failure("元数据恢复后未能固定目标介质身份，禁止格式化"))?;
        let mut dev = crate::application::device::open_readonly_usb_disk(runner, disk)?;
        Ok(format_encrypted_partition_on_disk(
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
        .result)
    })();
    EncryptedPostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result: match result {
            Ok(result) => result,
            Err(error) => Err(EncryptedPostRestoreError::Operation(error.msg)),
        },
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
) -> EdpCliResult<()> {
    let total_sectors = outcome.total_sectors;
    let observed = crate::application::media_identity_observer::observe_media_identity_readonly(
        runner, disk, dev,
    )?;
    expected
        .verify(&observed.snapshot, &observed.protocol_image)
        .map_err(|conflict| failure(format!("格式化目标物理身份不一致: {conflict:?}")))?;
    if sysinfo::disk_total_sectors(runner, disk) != Some(total_sectors)
        || observed.snapshot.hardware.total_sectors != Some(total_sectors)
        || observed.snapshot.hardware.logical_sector_size != Some(SECTOR as u32)
    {
        return Err(failure("格式化目标总扇区数或逻辑扇区大小不一致"));
    }
    if partition.sector_count == 0
        || partition
            .start_lba
            .checked_add(partition.sector_count)
            .is_none_or(|end| end > total_sectors)
    {
        return Err(failure("格式化分区几何超出目标盘"));
    }

    if outcome.device_state.eq_ignore_ascii_case("plain") {
        if !matches!(key_check, KeyCheck::Plain) {
            return Err(failure("Plain 分区不能使用加密密钥格式化"));
        }
        let table = crate::partition_table::read_partition_table(total_sectors, |lba| {
            read_sector(dev, lba)
        })
        .map_err(|message| failure(format!("重读分区表失败: {message}")))?;
        if !table.partitions.iter().any(|current| {
            current.index == partition.index as usize
                && current.start_lba == partition.start_lba
                && current.sector_count == partition.sector_count
        }) {
            return Err(failure("目标盘当前分区表与所选分区几何不一致"));
        }
    } else {
        let image = ProvisionImage::from_bytes(observed.protocol_image)
            .map_err(|message| failure(format!("重读 EDP 协议失败: {message}")))?;
        let parsed = parse_existing_provision(&image, &outcome.device_id, total_sectors)
            .map_err(|message| failure(format!("重读 EDP 分区失败: {message}")))?
            .ok_or_else(|| failure("当前盘缺少有效 EDP 分区记录"))?;
        let index = partition
            .index
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| failure("EDP 分区索引无效"))?;
        let current = parsed
            .profile
            .partitions
            .get(index)
            .ok_or_else(|| failure("EDP 分区索引不存在"))?;
        let record = parsed
            .records
            .get(index)
            .ok_or_else(|| failure("EDP 分区密钥记录不存在"))?;
        if current.start_lba != partition.start_lba
            || current.sector_count != partition.sector_count
        {
            return Err(failure("当前 EDP 分区几何与所选目标不一致"));
        }
        match key_check {
            KeyCheck::Reinitialize => {
                if record.lba12.need_encrypt == 0 || !current.physically_encrypted {
                    return Err(failure("所选目标不是 EDP 加密数据分区"));
                }
            }
            KeyCheck::Existing(key) => {
                if !current.physically_encrypted
                    || record.lba12.need_encrypt == 0
                    || record.lba12.encrypt_mode != FileKeyWrapMode::Sm4.raw()
                    || crate::crypto::crc32_bare(key) != record.lba12.file_key_crc
                {
                    return Err(failure("当前密钥域与已验证原 FileKey 不一致"));
                }
            }
            KeyCheck::Plain if current.physically_encrypted => {
                return Err(failure("物理加密分区禁止明文格式化"));
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
) -> PostRestoreFormatResult {
    let result = (|| -> EdpCliResult<()> {
        if !outcome.report.metadata_restored || !outcome.report.readback_verified {
            return Err(failure("元数据恢复尚未成功且读回验证，禁止后续格式化"));
        }
        expected
            .validate()
            .map_err(|message| failure(format!("目标身份 pin 无效: {message}")))?;
        let partition = outcome
            .partitions
            .iter()
            .find(|partition| partition.index == request.partition_index)
            .ok_or_else(|| failure("所选分区不在恢复清单中"))?;
        let assessed = outcome
            .assessment
            .partitions
            .iter()
            .find(|candidate| candidate.index == request.partition_index)
            .ok_or_else(|| failure("所选分区没有恢复后评估"))?;
        let eligible = match original_key {
            Some(_) => matches!(
                assessed.state,
                PostRestorePartitionState::NeedsFormat
                    | PostRestorePartitionState::PasswordRequired
            ),
            None => assessed.state == PostRestorePartitionState::NeedsFormat,
        };
        if !eligible
            || assessed.start_lba != partition.start_lba
            || assessed.sector_count != partition.sector_count
        {
            return Err(failure("所选分区不是匹配的 NeedsFormat 分区"));
        }
        let session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
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
        let fresh_assessment = assess(dev).map_err(failure)?;
        if !fresh_assessment.partitions.iter().any(|candidate| {
            candidate.index == request.partition_index
                && candidate.start_lba == partition.start_lba
                && candidate.sector_count == partition.sector_count
                && candidate.state == PostRestorePartitionState::NeedsFormat
        }) {
            return Err(failure("当前分区状态已变化，不再允许格式化"));
        }
        if !prompt.confirm_post_restore_format_yes(&format!(
            "单独确认格式化 disk{disk} 分区 {} (LBA{} + {} sectors) 为 {}；输入 YES: ",
            partition.index,
            partition.start_lba,
            partition.sector_count,
            request.filesystem.config_token()
        )) {
            return Err(failure("已取消本次分区格式化"));
        }
        let session = session.prepare_write().map_err(|error| {
            EdpCliError::new(EXIT_IO, format!("无法卸载/锁定 disk{disk}: {error}"))
        })?;
        let _session = session
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
                let reopened = assess(dev).map_err(failure)?;
                if !reopened.partitions.iter().any(|candidate| {
                    candidate.index == request.partition_index
                        && candidate.start_lba == partition.start_lba
                        && candidate.sector_count == partition.sector_count
                        && candidate.state == PostRestorePartitionState::NeedsFormat
                }) {
                    return Err(failure("重开后分区状态已变化，拒绝格式化"));
                }
                Ok(())
            })
            .map_err(|error| match error {
                ReopenAndVerifyError::Reopen(error) => {
                    EdpCliError::new(EXIT_IO, format!("重开 disk{disk} 失败: {error}"))
                }
                ReopenAndVerifyError::Verify(error) => error,
            })?;
        let format = format_partition_after_restore(
            dev,
            &fresh_assessment,
            partition,
            request,
            volume_label,
            volume_serial,
            original_key.map(|(_, key)| key),
        );
        format.result.map_err(failure)?;
        let after = assess(dev).map_err(failure)?;
        if !after.partitions.iter().any(|candidate| {
            candidate.index == request.partition_index
                && candidate.start_lba == partition.start_lba
                && candidate.sector_count == partition.sector_count
                && candidate.state == PostRestorePartitionState::Usable
        }) {
            return Err(failure("格式化读回后重新评估未达到 Usable"));
        }
        Ok(())
    })()
    .map_err(|error| error.msg);
    PostRestoreFormatResult {
        partition_index: request.partition_index,
        filesystem: request.filesystem,
        result,
    }
}
