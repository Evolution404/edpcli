use super::*;

pub fn commit_plain_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedPlainProvision,
) -> EdpCliResult<()> {
    commit_plain_provision_with_progress(runner, dev, prepared, &mut |_, _, _| {})
}

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
    let _session = session
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
        })?;

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

pub fn commit_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedProvision,
) -> EdpCliResult<ProvisionCommitOutcome> {
    commit_provision_with_progress(runner, dev, prepared, &mut |_, _, _| {})
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

pub fn commit_new_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<ProvisionCommitReport> {
    commit_new_provision_with_progress(runner, dev, prepared, &mut |_, _, _| {})
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
    let _session = session
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
        })?;
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
    progress(Phase::Readback, Step::ProtocolReadback, None);
    let mut report = ProvisionCommitReport {
        provision_succeeded: true,
        formats: Vec::new(),
    };
    for choice in prepared
        .format_targets
        .iter()
        .filter(|choice| choice.selected)
    {
        let result =
            format_partition_with_progress(runner, dev, prepared, choice, &mut |activity| {
                progress(
                    Phase::Format,
                    Step::PartitionFormat(choice.target.role),
                    Some(activity),
                );
            });
        report.formats.push(PartitionFormatResult {
            role: choice.target.role,
            result: result.map_err(|error| error.msg),
        });
        progress(
            Phase::Format,
            Step::PartitionFormat(choice.target.role),
            None,
        );
    }
    Ok(report)
}

pub(super) fn validate_preserve_source_snapshot(
    has_preserved_partitions: bool,
    source_metadata: Option<&[u8]>,
) -> EdpCliResult<()> {
    if !has_preserved_partitions {
        return Ok(());
    }
    let source_metadata = source_metadata.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: PreserveExact 计划缺少准备阶段来源元数据快照，拒绝写盘",
        )
    })?;
    if source_metadata.len() != 13 * SECTOR {
        return Err(err(
            EXIT_TARGET,
            "错误: PreserveExact 计划的来源元数据快照长度异常，拒绝写盘",
        ));
    }
    Ok(())
}

pub(super) fn validate_key_disposition_plan(
    target_plan: &TargetProvisionPlan,
    plan: &OfficialProvisionPlan,
    formats: &[PlannedPartitionFormat],
) -> EdpCliResult<()> {
    if target_plan.partitions.len() != plan.mode.partition_types().len() {
        return Err(err(
            EXIT_TARGET,
            "错误: disposition 计划与目标协议分区数量不一致",
        ));
    }
    for (index, part) in target_plan.partitions.iter().enumerate() {
        let selected_format = formats
            .iter()
            .find(|choice| choice.target.role == part.geometry.role)
            .is_some_and(|choice| choice.selected && choice.prepared_image.is_some());
        match part.disposition {
            RegionDisposition::PreserveOpaque => {
                if part.source_password_knowledge != Some(SourcePasswordKnowledge::Unknown)
                    || part.target_password_policy != Some(TargetPasswordPolicy::PreserveOpaque)
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveOpaque 的密码状态/目标策略不一致",
                            part.geometry.role.label()
                        ),
                    ));
                }
                let record = part.preserved_record.ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveOpaque 缺少来源 key material",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                if record.lba12.need_encrypt == 0 {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} 非加密记录不能标记 PreserveOpaque",
                            part.geometry.role.label()
                        ),
                    ));
                }
                let expected_lba12 = record
                    .lba12_key_material()
                    .map_err(|message| err(EXIT_TARGET, message))?;
                if plan.partition_lba7_material[index] != Some(record.lba7_key_material())
                    || plan.partition_lba12_material[index] != Some(expected_lba12)
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveOpaque 未逐字段复用来源 key material",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if selected_format {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveOpaque 禁止格式化/data extent 写入",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::PreserveVerified => {
                let record = part.preserved_record.ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveVerified 缺少来源记录",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                if record.lba12.need_encrypt != 0 {
                    let expected_lba12 = record
                        .lba12_key_material()
                        .map_err(|message| err(EXIT_TARGET, message))?;
                    if plan.partition_lba7_material[index] != Some(record.lba7_key_material())
                        || plan.partition_lba12_material[index] != Some(expected_lba12)
                    {
                        return Err(err(
                            EXIT_TARGET,
                            format!(
                                "错误: {} PreserveVerified 改变了来源 key material",
                                part.geometry.role.label()
                            ),
                        ));
                    }
                }
                if selected_format {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveVerified 禁止格式化/data extent 写入",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::RewrapVerified => {
                if matches!(
                    part.source_password_knowledge,
                    None | Some(SourcePasswordKnowledge::Unknown)
                ) || part.target_password_policy != Some(TargetPasswordPolicy::ReplaceVerified)
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 缺少已验证来源密码状态",
                            part.geometry.role.label()
                        ),
                    ));
                }
                let record = part.preserved_record.ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 缺少来源 key record",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                let source_lba12 = record
                    .lba12_key_material()
                    .map_err(|message| err(EXIT_TARGET, message))?;
                let target_lba12 = plan.partition_lba12_material[index].ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 缺少目标 LBA12 key material",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                let target_lba7 = plan.partition_lba7_material[index].ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 缺少目标 LBA7 key material",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                if source_lba12.file_key_crc != target_lba12.file_key_crc
                    || record.lba7.file_key_crc != target_lba7.file_key_crc
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 改变了 raw FileKey",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if selected_format {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} RewrapVerified 禁止格式化/data extent 写入",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::Rebuild => {
                if part.geometry.role != PartitionRole::CompatibilityReserve && !selected_format {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} Rebuild 缺少完整 filesystem initialization image",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if KeyDomainRole::from_partition_role(part.geometry.role).is_some()
                    && plan.partition_lba12_material[index].is_none()
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} Rebuild 缺少新 FileKey material",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::Migrate => {
                if part.migration_sources.is_empty() {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} Migrate 缺少 typed migration source",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if selected_format {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} Migrate 不能再执行独立格式化阶段",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if KeyDomainRole::from_partition_role(part.geometry.role).is_some()
                    && plan.partition_lba12_material[index].is_none()
                {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} Migrate 缺少新的目标 FileKey material",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::Drop => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 目标分区 {}不能使用 Drop disposition",
                        part.geometry.role.label()
                    ),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_target_write_set(
    target_plan: &TargetProvisionPlan,
    patch: &BTreeMap<u32, Vec<u8>>,
    formats: &[PlannedPartitionFormat],
) -> EdpCliResult<()> {
    for (start, count) in target_plan.preserved_extents() {
        let end = start
            .checked_add(count)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 保留分区 LBA 溢出"))?;
        if patch
            .keys()
            .any(|lba| (start..end).contains(&u64::from(*lba)))
        {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 协议写集合触碰保留分区 LBA{start}..{}", end - 1),
            ));
        }
        if formats.iter().any(|choice| {
            choice.selected
                && choice.target.geometry.start_sector < end
                && start < choice.target.geometry.end_sector_exclusive()
        }) {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 格式化计划触碰保留分区 LBA{start}..{}", end - 1),
            ));
        }
    }
    if target_plan.partitions.iter().any(|part| {
        part.action == PartitionAction::PreserveExact && part.preserved_record.is_none()
    }) {
        return Err(err(EXIT_TARGET, "错误: 保留分区缺少原 key material"));
    }
    for part in target_plan
        .partitions
        .iter()
        .filter(|part| part.disposition == RegionDisposition::Migrate)
    {
        let start = u32::try_from(part.geometry.start_lba)
            .map_err(|_| err(EXIT_TARGET, "错误: K6 目标起点 LBA 溢出"))?;
        if !patch.contains_key(&start) {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: {} Migrate 写集合缺少目标文件系统引导扇区",
                    part.geometry.role.label()
                ),
            ));
        }
        if formats
            .iter()
            .any(|choice| choice.target.role == part.geometry.role && choice.selected)
        {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: {} Migrate 与格式化写集合冲突",
                    part.geometry.role.label()
                ),
            ));
        }
    }
    Ok(())
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

struct PreparedImageReader<'a> {
    image: &'a SparseFilesystemImage,
}

impl PartitionReader for PreparedImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        self.image
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| std::io::Error::other("format read outside partition"))
    }
}

impl crate::filesystem::FilesystemReader for PreparedImageReader<'_> {
    fn sector_size(&self) -> u32 {
        SECTOR as u32
    }

    fn sector_count(&self) -> u64 {
        self.image.volume_sectors()
    }

    fn read_sector(
        &mut self,
        relative_lba: u64,
    ) -> Result<[u8; SECTOR], crate::filesystem::FilesystemError> {
        self.image.sector_or_zero(relative_lba).ok_or_else(|| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                "格式化验证读取超出分区范围",
            )
        })
    }
}

fn format_partition_with_progress(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
    choice: &PlannedPartitionFormat,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    verify_format_identity(runner, dev, prepared)?;
    execute_partition_format_observed(dev, choice, observer)?;
    verify_format_identity(runner, dev, prepared)?;
    Ok(())
}

/// Format one verified official partition. The caller owns device identity and
/// protocol verification; this operation never writes the protocol region.
#[cfg(test)]
pub(super) fn execute_partition_format(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
) -> EdpCliResult<()> {
    execute_partition_format_observed(dev, choice, &mut |_| {})
}

fn execute_partition_format_observed(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
    observer: &mut dyn FnMut(diskio::TransactionActivity),
) -> EdpCliResult<()> {
    let filesystem = choice
        .filesystem
        .ok_or_else(|| err(EXIT_TARGET, "错误: 兼容保留区不可格式化"))?;
    let built = choice
        .prepared_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少预生成物理镜像"))?;
    let verification_image = choice
        .verification_image
        .as_ref()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化计划缺少验证镜像"))?;
    if built.geometry != choice.target.geometry
        || built.physically_encrypted != choice.target.physically_encrypted
        || built.image.volume_sectors() != choice.target.geometry.sector_count()
        || verification_image.volume_sectors() != choice.target.geometry.sector_count()
    {
        return Err(err(EXIT_TARGET, "错误: 预生成格式化镜像与目标几何不一致"));
    }
    super::super::filesystem_format::write_sparse_filesystem_image(
        dev,
        choice.target.geometry.start_sector,
        &built.image,
        observer,
    )?;
    let raw_boot = dev
        .read_sector(
            u32::try_from(choice.target.geometry.start_sector)
                .map_err(|_| err(EXIT_TARGET, "错误: 分区起点 LBA 溢出"))?,
        )
        .map_err(|error| err(EXIT_IO, format!("错误: 读取文件系统引导扇区失败: {error}")))?;
    let raw_plain_filesystem = if choice.target.physically_encrypted {
        let registry = crate::filesystem::default_registry();
        let mut raw_reader = crate::filesystem::BootSectorReader::new(
            &raw_boot,
            choice.target.geometry.sector_count(),
        );
        registry
            .detect(&mut raw_reader)
            .map_err(|error| err(EXIT_IO, format!("错误: 文件系统首扇区检测失败: {error}")))?
            .is_some_and(|detected| {
                detected.result.confidence == crate::filesystem::DetectionConfidence::Exact
            })
    } else {
        false
    };
    if raw_plain_filesystem {
        return Err(err(EXIT_IO, "错误: 加密分区物理首扇区出现明文文件系统签名"));
    }
    let geometry = PartitionGeometry {
        index: 0,
        partition_type: choice.target.geometry.partition_type.raw(),
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: choice.target.geometry.start_sector,
        sector_size: SECTOR as u64,
        partition_size: choice.target.geometry.size_bytes,
        sector_count: choice.target.geometry.sector_count(),
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let registry = crate::filesystem::default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        err(
            EXIT_IO,
            format!("错误: {} 文件系统没有已注册驱动", filesystem.config_token()),
        )
    })?;
    if !driver.capabilities().verify_format {
        return Err(err(
            EXIT_IO,
            format!(
                "错误: 当前不支持 {} 格式化读回校验",
                filesystem.config_token()
            ),
        ));
    }
    let mut request = crate::filesystem::FormatRequest::new(filesystem);
    request.volume_label = (!choice.volume_label.is_empty()).then(|| choice.volume_label.clone());
    request.volume_serial = Some(choice.volume_serial);
    let expected = driver.expected_format_metadata(&request).map_err(|error| {
        err(
            EXIT_IO,
            format!(
                "错误: {} 格式化预期元数据无效: {error}",
                filesystem.config_token()
            ),
        )
    })?;
    let fs_geometry = crate::filesystem::FilesystemGeometry::new(
        choice.target.geometry.start_sector,
        choice.target.geometry.sector_count(),
        SECTOR as u32,
    );
    let mut reader = PreparedImageReader {
        image: verification_image,
    };
    driver
        .verify_format(&mut reader, fs_geometry, &expected)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!(
                    "错误: {} 格式化读回校验失败: {error}",
                    filesystem.config_token()
                ),
            )
        })?;

    let report = analyze_partition(&geometry, &mut reader);
    if report.status != AnalysisStatus::Parsed
        || report.filesystem.as_deref() != Some(filesystem.config_token())
        || report.file_count != Some(0)
    {
        return Err(err(
            EXIT_IO,
            format!(
                "错误: {} 深度解析失败: {}",
                filesystem.config_token(),
                report.reason
            ),
        ));
    }
    Ok(())
}
