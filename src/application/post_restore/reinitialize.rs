//! Separately authorized replacement of one EDP encrypted key domain.

use super::*;
use crate::application::media_identity::MediaIdentityResumePin;
use crate::application::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::application::Prompter;
use crate::common::{EdpCliError, EdpCliResult, EXIT_IO};
use crate::diskio::{self, SectorDev, SectorWriteStage, WriteTransactionPlan};
use crate::provision::{rekey_existing_partition_image, wrap_file_key, wrap_legacy_lba7_file_key};
use crate::sysinfo::CmdRunner;

use super::format_operation::{failure, verify_current_target, KeyCheck, REOPEN_WAIT};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedPartitionReinitializeResult {
    pub partition_index: u32,
    pub filesystem: FilesystemKind,
    pub result: Result<(), String>,
}

#[allow(clippy::too_many_arguments)]
pub fn reinitialize_encrypted_partition_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
    outcome: &MetadataRestoreOutcome,
    request: &EncryptedPartitionReinitializeRequest,
    filesystem: FilesystemKind,
    volume_label: &str,
    volume_serial: u32,
) -> EncryptedPartitionReinitializeResult {
    let result = (|| -> EdpCliResult<()> {
        if !outcome.report.metadata_restored || !outcome.report.readback_verified {
            return Err(failure("元数据恢复尚未成功且读回验证，禁止重建密钥域"));
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
        if !assessed.requires_original_key
            || assessed.start_lba != partition.start_lba
            || assessed.sector_count != partition.sector_count
        {
            return Err(failure("所选分区不是匹配的 EDP 加密分区"));
        }
        let session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
        verify_current_target(
            runner, disk, dev, expected, outcome, partition, KeyCheck::Reinitialize,
        )
        .map_err(|error| failure(error.to_string()))?;
        let original = ProvisionImage::from_bytes(read_protocol_image(dev).map_err(failure)?)
            .map_err(failure)?;
        let parsed = parse_existing_provision(&original, &outcome.device_id, outcome.total_sectors)
            .map_err(failure)?
            .ok_or_else(|| failure("当前盘缺少 EDP 分区记录"))?;
        let index = request
            .partition_index
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| failure("EDP 分区索引无效"))?;
        let record = parsed
            .records
            .get(index)
            .ok_or_else(|| failure("EDP 分区索引不存在"))?;
        let password_crc = crate::crypto::crc32_bare(request.password());
        if record.lba12.user_key_crc == password_crc || record.lba7.user_key_crc == password_crc {
            return Err(failure("新密码不能与当前分区原密码相同"));
        }
        if !prompt.confirm_reinitialize_yes(&format!(
            "独立确认清空并重建 disk{disk} 分区 {} (LBA{} + {} sectors)：原密码和 FileKey 作废，残留数据不能再由新密钥访问；输入 YES: ",
            partition.index, partition.start_lba, partition.sector_count
        )) {
            return Err(failure("已取消清空并重建加密分区"));
        }

        let mut legacy_key = [0u8; 8];
        let mut file_key = [0u8; 16];
        getrandom::fill(&mut legacy_key)
            .map_err(|error| failure(format!("生成 LBA7 密钥失败: {error}")))?;
        getrandom::fill(&mut file_key)
            .map_err(|error| failure(format!("生成 FileKey 失败: {error}")))?;
        let legacy = wrap_legacy_lba7_file_key(request.password(), legacy_key);
        legacy_key.fill(0);
        let wrapped = wrap_file_key(request.password(), file_key, FileKeyWrapMode::Sm4);
        let updated = rekey_existing_partition_image(
            &original,
            &outcome.device_id,
            outcome.total_sectors,
            partition.index,
            legacy,
            wrapped,
        )
        .map_err(failure)?;
        let updated_record = parse_existing_provision(
            &updated,
            &outcome.device_id,
            outcome.total_sectors,
        )
        .map_err(failure)?
        .ok_or_else(|| failure("更新后 EDP 协议不可解析"))?
        .records[index];
        if updated_record.verified_file_key(Some(request.password())) != Ok(file_key) {
            return Err(failure("新密码与 FileKey 读前验证失败"));
        }

        let plain_image = build_empty_partition_image(
            partition,
            filesystem,
            volume_label,
            volume_serial,
        )
        .map_err(|error| failure(error.to_string()))?;
        if plain_image
            .sectors()
            .keys()
            .any(|relative| *relative >= partition.sector_count)
        {
            return Err(failure("格式化镜像写入范围超出所选分区"));
        }
        let encrypted_image = plain_image.transformed(&EdpSm4Transform::new(file_key));
        let mut transaction = WriteTransactionPlan::new(outcome.total_sectors);
        for (&relative, sector) in encrypted_image.sectors() {
            let absolute = partition
                .start_lba
                .checked_add(relative)
                .and_then(|lba| u32::try_from(lba).ok())
                .ok_or_else(|| failure("加密文件系统 LBA 溢出"))?;
            transaction
                .insert(absolute, sector.to_vec(), SectorWriteStage::Data, "new encrypted filesystem")
                .map_err(failure)?;
        }
        for lba in [7usize, 12] {
            transaction
                .insert(
                    lba as u32,
                    updated.as_bytes()[lba * SECTOR..(lba + 1) * SECTOR].to_vec(),
                    SectorWriteStage::Metadata,
                    "new key domain",
                )
                .map_err(failure)?;
        }
        let session = session.prepare_write().map_err(|error| {
            EdpCliError::new(EXIT_IO, format!("无法卸载/锁定 disk{disk}: {error}"))
        })?;
        let _locked = session
            .reopen_and_verify(dev, REOPEN_WAIT, |dev| {
                verify_current_target(
                    runner, disk, dev, expected, outcome, partition, KeyCheck::Reinitialize,
                )
                .map_err(|error| failure(error.to_string()))
            })
            .map_err(|error| match error {
                ReopenAndVerifyError::Reopen(error) => {
                    EdpCliError::new(EXIT_IO, format!("重开 disk{disk} 失败: {error}"))
                }
                ReopenAndVerifyError::Verify(error) => error,
            })?;
        diskio::execute_write_transaction(dev, &transaction)?;
        let after = assess_partitions_with_password_readonly(
            dev,
            &outcome.device_state,
            &outcome.device_id,
            outcome.total_sectors,
            &outcome.partitions,
            request.partition_index,
            request.password(),
        )
        .map_err(failure)?;
        if !after.partitions.iter().any(|candidate| {
            candidate.index == request.partition_index
                && candidate.start_lba == partition.start_lba
                && candidate.sector_count == partition.sector_count
                && candidate.state == PostRestorePartitionState::Usable
        }) {
            return Err(failure("新密码、FileKey 与文件系统读回链未达到 Usable"));
        }
        file_key.fill(0);
        Ok(())
    })()
    .map_err(|error| error.msg);
    EncryptedPartitionReinitializeResult {
        partition_index: request.partition_index,
        filesystem,
        result,
    }
}

pub fn reinitialize_encrypted_partition_after_restore_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    prompt: &mut dyn Prompter,
    outcome: &MetadataRestoreOutcome,
    request: &EncryptedPartitionReinitializeRequest,
    filesystem: FilesystemKind,
    volume_label: &str,
) -> EncryptedPartitionReinitializeResult {
    let result = (|| -> EdpCliResult<Result<(), String>> {
        let expected = outcome
            .format_target_pin
            .as_ref()
            .ok_or_else(|| failure("元数据恢复后未能固定目标介质身份，禁止重建密钥域"))?;
        let mut dev = crate::application::device::open_readonly_usb_disk(runner, disk)?;
        Ok(reinitialize_encrypted_partition_on_disk(
            runner,
            disk,
            &mut dev,
            prompt,
            expected,
            outcome,
            request,
            filesystem,
            volume_label,
            diskio::Clock::now_epoch(&diskio::SystemClock) as u32,
        )
        .result)
    })();
    EncryptedPartitionReinitializeResult {
        partition_index: request.partition_index,
        filesystem,
        result: match result {
            Ok(result) => result,
            Err(error) => Err(error.msg),
        },
    }
}
