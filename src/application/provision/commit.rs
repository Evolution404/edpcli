use super::*;
#[path = "commit/partition_format.rs"]
mod partition_format;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) use partition_format::execute_partition_format;
use partition_format::format_partition_with_progress;
#[path = "commit/validation.rs"]
mod validation;
pub(super) use validation::{
    validate_key_disposition_plan, validate_preserve_source_snapshot, validate_target_write_set,
};

pub(super) fn commit_plain_provision_with_progress(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedPlainProvision,
    progress: &mut dyn FnMut(
        crate::application::progress::Phase,
        crate::application::progress::Step,
        Option<diskio::TransactionActivity>,
    ),
) -> EdpCliResult<()> {
    use crate::application::progress::{Phase, Step};
    progress(Phase::Identity, Step::LockAndReopen, None);
    let session = TargetSession::<ReadOnly>::open_usb(runner, prepared.disk)?;
    let session = session.prepare_write().map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    let mut locked_session = session
        .reopen_and_verify(dev, OPEN_WAIT, |dev| {
            verify_reopened_snapshot(dev, &prepared.source_metadata)?;
            let fresh = super::super::media_identity_observer::observe_media_identity_readonly(
                runner,
                prepared.disk,
                dev,
            )?;
            prepared
                .before_pin
                .verify(&fresh.snapshot, &fresh.protocol_image)
                .map_err(|conflict| {
                    err(
                        EXIT_TARGET,
                        format!("错误: Plain 重开后介质身份 pin 不一致: {conflict:?}"),
                    )
                })
        })
        .map_err(|error| match error {
            ReopenAndVerifyError::Reopen(error) => {
                err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}"))
            }
            ReopenAndVerifyError::Verify(error) => error,
            ReopenAndVerifyError::Geometry(error) => error,
        })?;

    let dev = locked_session.device();
    let fresh_probe = runner
        .hardware_probe(prepared.disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, prepared.disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标硬件身份"))?;
    let fresh_total = sysinfo::disk_total_sectors(runner, prepared.disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标容量"))?;
    if fresh_probe != prepared.expected_probe || fresh_total != prepared.plan.total_sectors {
        return Err(err(
            EXIT_TARGET,
            "错误: Plain 确认/卸载期间目标硬件身份或容量发生变化，疑似换盘，拒绝写入",
        ));
    }
    let transaction = diskio::WriteTransactionPlan::from_plain_provision(&prepared.write_plan)
        .map_err(|message| err(EXIT_TARGET, format!("错误: Plain 事务计划无效: {message}")))?;
    diskio::execute_write_transaction_observed(dev, &transaction, &mut |activity| {
        progress(Phase::Transaction, Step::ProtocolWrite, Some(activity));
    })?;
    progress(Phase::Transaction, Step::ProtocolWrite, None);

    let mbr = dev
        .read_sector(0)
        .map_err(|error| err(EXIT_IO, format!("错误: Plain 写后读取 MBR 失败: {error}")))?;
    if mbr.as_slice() != prepared.write_plan.mbr {
        return Err(err(EXIT_IO, "错误: Plain 写后 MBR 与计划不一致"));
    }
    let verify_lba3 = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: Plain 写后读取 LBA3 失败: {error}")))?;
    if verify_lba3.as_slice() != &prepared.source_metadata[3 * SECTOR..4 * SECTOR] {
        return Err(err(
            EXIT_IO,
            "错误: Plain 写后 LBA3 未保持 byte-for-byte 一致",
        ));
    }
    let lba7 = dev
        .read_sector(7)
        .map_err(|error| err(EXIT_IO, format!("错误: Plain 写后读取 LBA7 失败: {error}")))?;
    let lba12 = dev
        .read_sector(12)
        .map_err(|error| err(EXIT_IO, format!("错误: Plain 写后读取 LBA12 失败: {error}")))?;
    if crate::provision::DiskProvisionKind::from_sectors(&lba7, &lba12, &prepared.device_id)
        .is_some()
    {
        return Err(err(EXIT_IO, "错误: Plain 写后仍检测到合法 EDP 模式"));
    }
    progress(Phase::Readback, Step::ProtocolReadback, None);
    Ok(())
}

pub(super) fn commit_provision_with_progress(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedProvision,
    progress: &mut dyn FnMut(
        crate::application::progress::Phase,
        crate::application::progress::Step,
        Option<diskio::TransactionActivity>,
    ),
) -> EdpCliResult<ProvisionCommitOutcome> {
    match prepared {
        PreparedProvision::Official(prepared) => {
            commit_new_provision_with_progress(runner, dev, prepared, progress)
                .map(ProvisionCommitOutcome::Official)
        }
        PreparedProvision::Plain(prepared) => {
            let partition_count = prepared.plan.partitions.len();
            commit_plain_provision_with_progress(runner, dev, prepared, progress)
                .map(|()| ProvisionCommitOutcome::Plain { partition_count })
        }
    }
}

pub fn capture_manufacturer_lba3(
    dev: &mut dyn SectorDev,
    prepared: &mut PreparedNewProvision,
) -> EdpCliResult<()> {
    let raw = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 读取目标 LBA3 失败: {error}")))?;
    let lba3: [u8; SECTOR] = raw.try_into().map_err(|raw: Vec<u8>| {
        err(
            EXIT_IO,
            format!("错误: 目标 LBA3 长度为 {}B，预期 {SECTOR}B", raw.len()),
        )
    })?;
    prepared.write_image.patch.insert(3, lba3.to_vec());
    prepared.expected_lba3 = Some(lba3);
    Ok(())
}

pub(super) fn commit_new_provision_with_progress(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
    progress: &mut dyn FnMut(
        crate::application::progress::Phase,
        crate::application::progress::Step,
        Option<diskio::TransactionActivity>,
    ),
) -> EdpCliResult<ProvisionCommitReport> {
    use crate::application::progress::{Phase, Step};
    let target_plan = prepared
        .target_plan
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 缺少统一目标制盘计划，拒绝写盘"))?;
    validate_target_write_set(
        target_plan,
        &prepared.write_image.patch,
        &prepared.format_targets,
    )?;
    validate_key_disposition_plan(target_plan, &prepared.plan, &prepared.format_targets)?;
    validate_preserve_source_snapshot(
        target_plan.has_preserved_partitions(),
        prepared.source_metadata.as_deref(),
    )?;
    progress(Phase::Identity, Step::LockAndReopen, None);
    let session = TargetSession::<ReadOnly>::open_usb(runner, prepared.disk)?;
    let session = session.prepare_write().map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    let mut locked_session = session
        .reopen_and_verify(dev, OPEN_WAIT, |dev| {
            if let Some(source_metadata) = &prepared.source_metadata {
                verify_reopened_snapshot(dev, source_metadata)?;
            }
            let fresh = super::super::media_identity_observer::observe_media_identity_readonly(
                runner,
                prepared.disk,
                dev,
            )?;
            prepared
                .before_pin
                .verify(&fresh.snapshot, &fresh.protocol_image)
                .map_err(|conflict| {
                    err(
                        EXIT_TARGET,
                        format!("错误: 制盘重开后介质身份 pin 不一致: {conflict:?}"),
                    )
                })
        })
        .map_err(|error| match error {
            ReopenAndVerifyError::Reopen(error) => {
                err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}"))
            }
            ReopenAndVerifyError::Verify(error) => error,
            ReopenAndVerifyError::Geometry(error) => error,
        })?;

    let dev = locked_session.device();
    let fresh_probe = runner
        .hardware_probe(prepared.disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, prepared.disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标硬件身份"))?;
    let fresh_total = sysinfo::disk_total_sectors(runner, prepared.disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标容量"))?;
    if fresh_probe != prepared.expected_probe || fresh_total != prepared.write_image.total_sectors {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间目标硬件身份或容量发生变化，疑似换盘，拒绝写入",
        ));
    }
    if let Some(digest) = &prepared.expected_serial_digest {
        let fresh = super::super::media_identity::serial_digest_evidence(
            runner.hardware_serial(prepared.disk).as_deref(),
        );
        if fresh.sha256.as_ref() != Some(digest) {
            return Err(err(
                EXIT_TARGET,
                "错误: 制盘确认期间 USB 硬件序列号发生变化",
            ));
        }
    }
    let expected_lba3 = prepared.expected_lba3.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 制盘写入前未捕获制造商 LBA3，拒绝覆盖不透明制造商元数据",
        )
    })?;
    let fresh_lba3 = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 重开后复核 LBA3 失败: {error}")))?;
    if fresh_lba3.as_slice() != expected_lba3 {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间制造商 LBA3 发生变化，拒绝写入",
        ));
    }
    diskio::atomic_write_official_provision_sectors_observed(
        dev,
        &prepared.write_image.patch,
        prepared.write_image.total_sectors,
        &mut |activity| progress(Phase::Transaction, Step::ProtocolWrite, Some(activity)),
    )?;
    progress(Phase::Transaction, Step::ProtocolWrite, None);
    verify_protocol_readback(dev, prepared)?;
    verify_lce_readback(dev, prepared)?;
    progress(Phase::Readback, Step::ProtocolReadback, None);
    let mut report = ProvisionCommitReport {
        provision_succeeded: true,
        formats: Vec::new(),
    };
    report.formats = execute_selected_formats(&prepared.format_targets, |choice| {
        let result =
            format_partition_with_progress(runner, dev, prepared, choice, &mut |activity| {
                progress(
                    Phase::Format,
                    Step::PartitionFormat(choice.target.role),
                    Some(activity),
                );
            });
        progress(
            Phase::Format,
            Step::PartitionFormat(choice.target.role),
            None,
        );
        result
    });
    Ok(report)
}

/// Stop on the first failed format, including a successful rollback. The protocol
/// transaction is already committed; later partition writes need a fresh assessment.
pub(super) fn execute_selected_formats(
    choices: &[PlannedPartitionFormat],
    mut execute: impl FnMut(
        &PlannedPartitionFormat,
    ) -> Result<(), crate::application::error::OperationError>,
) -> Vec<PartitionFormatResult> {
    let mut stopped_after = None;
    choices
        .iter()
        .filter(|choice| choice.selected)
        .map(|choice| {
            let result = match stopped_after {
                Some(after) => Err(PartitionFormatError::Skipped { after }),
                None => execute(choice).map_err(|error| {
                    stopped_after = Some(choice.target.role);
                    PartitionFormatError::Failed(error)
                }),
            };
            PartitionFormatResult {
                role: choice.target.role,
                result,
            }
        })
        .collect()
}

fn verify_format_identity(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    let session = TargetSession::<ReadOnly>::open_usb(runner, prepared.disk)?;
    let fresh_probe = session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前无法复核硬件身份"))?;
    let fresh_total = session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前无法复核容量"))?;
    let fresh_serial = session.hardware_serial();
    verify_format_hardware(
        &prepared.expected_probe,
        prepared.write_image.total_sectors,
        &prepared.device_id,
        prepared.expected_serial_digest.as_deref(),
        &fresh_probe,
        fresh_total,
        fresh_serial.as_deref(),
    )?;
    verify_protocol_readback(dev, prepared)
}

pub(super) fn verify_format_hardware(
    expected_probe: &crate::platform::HardwareProbe,
    expected_total: u64,
    expected_device_id: &str,
    expected_serial_digest: Option<&str>,
    fresh_probe: &crate::platform::HardwareProbe,
    fresh_total: u64,
    fresh_serial: Option<&str>,
) -> EdpCliResult<()> {
    let fresh_identity =
        TargetIdentity::from_probe(fresh_probe, fresh_total).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 格式化前硬件身份无效: {message}"),
            )
        })?;
    if fresh_probe != expected_probe
        || fresh_total != expected_total
        || fresh_identity.device_id() != expected_device_id
    {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前硬件身份、容量或 device_id 已变化",
        ));
    }
    let fresh_digest = super::super::media_identity::serial_digest_evidence(fresh_serial);
    if expected_serial_digest.is_none_or(|digest| fresh_digest.sha256.as_deref() != Some(digest)) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 USB 硬件序列号缺失或已变化",
        ));
    }
    Ok(())
}

pub(super) fn verify_protocol_readback(
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    let raw = read_image(dev)?;
    if (0..13u32).any(|lba| {
        prepared.write_image.patch.get(&lba).is_none_or(|expected| {
            raw[lba as usize * SECTOR..(lba as usize + 1) * SECTOR] != expected[..]
        })
    }) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 LBA0–12 与本次制盘计划不一致",
        ));
    }
    let onlyid = diskio::lba4_label_id_from(&raw[4 * SECTOR..5 * SECTOR]);
    if onlyid.as_deref() != Some(&prepared.expected_onlyid) {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 onlyid 与本次制盘计划不一致",
        ));
    }
    let actual = crate::backup_metadata::parse_partition_geometry(
        &raw,
        &prepared.device_id,
        prepared.write_image.total_sectors,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 格式化前分区表无效: {message}")))?;
    let planned = prepared
        .plan
        .logical_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, message))?;
    if actual.len() != planned.len()
        || actual.iter().zip(planned.iter()).any(|(actual, planned)| {
            actual.partition_type != planned.partition_type.raw()
                || actual.start_sector != planned.start_sector
                || actual.sector_count != planned.sector_count()
        })
    {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前实际分区布局与制盘计划不一致",
        ));
    }
    Ok(())
}

pub(super) fn verify_lce_readback(
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    let protocol = read_image(dev)?;
    let geometry = parse_lba7_compatibility_geometry(
        &protocol,
        &prepared.device_id,
        prepared.write_image.total_sectors,
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 格式化前无法从最终 LBA7 解析 LCE: {message}"),
        )
    })?;

    let planned = prepared.plan.lba7_compatibility_extent;
    if geometry.start_lba != prepared.lce_start_lba
        || geometry.start_lba != planned.start_lba
        || geometry.sector_count != planned.size_sectors
    {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: 格式化前 LCE 几何与制盘计划不一致(actual={}+{}, prepared={}, planned={}+{})",
                geometry.start_lba,
                geometry.sector_count,
                prepared.lce_start_lba,
                planned.start_lba,
                planned.size_sectors
            ),
        ));
    }

    let expected_sectors = crate::protocol::lba7_compat::LBA7_COMPAT_EXTENT_TOTAL_SIZE / SECTOR;
    if geometry.sector_count != expected_sectors as u64 {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: 格式化前 LCE 长度为 {} sector，预期 {expected_sectors}",
                geometry.sector_count
            ),
        ));
    }

    let logical_count = prepared
        .plan
        .logical_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, message))?
        .len();
    let expected_pointer_count = logical_count.saturating_sub(1);
    if geometry.lba7_pointer_entries.len() != expected_pointer_count {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: 格式化前 LBA7 的 3072B LCE 指针数量为 {}，预期 {}",
                geometry.lba7_pointer_entries.len(),
                expected_pointer_count
            ),
        ));
    }

    let end_lba = geometry
        .start_lba
        .checked_add(geometry.sector_count)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前 LCE LBA 范围溢出"))?;
    if end_lba > prepared.write_image.total_sectors {
        return Err(err(EXIT_TARGET, "错误: 格式化前 LCE 超出目标盘范围"));
    }

    let mut ciphertext =
        Vec::with_capacity(crate::protocol::lba7_compat::LBA7_COMPAT_EXTENT_TOTAL_SIZE);
    for offset in 0..geometry.sector_count {
        let lba = geometry
            .start_lba
            .checked_add(offset)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前 LCE LBA 溢出"))?;
        let lba32 = u32::try_from(lba)
            .map_err(|_| err(EXIT_TARGET, "错误: 格式化前 LCE LBA 超出当前读写器范围"))?;
        let sector = dev.read_sector(lba32).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化前读取 LCE LBA{lba} 失败: {error}"),
            )
        })?;
        if sector.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: 格式化前 LCE LBA{lba} 长度为 {}B，预期 {SECTOR}B",
                    sector.len()
                ),
            ));
        }
        ciphertext.extend_from_slice(&sector);
    }

    let physical_offset = geometry
        .start_lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化前 LCE 物理字节偏移溢出"))?;
    let plaintext = crate::crypto::a6b0_full_offset(&ciphertext, &[0u8; 8], physical_offset);
    if plaintext.as_slice() != crate::provision::lce_plaintext() {
        return Err(err(
            EXIT_TARGET,
            "错误: 格式化前 LCE 6 sector 解密后与官方 gold plaintext 不一致",
        ));
    }

    Ok(())
}
