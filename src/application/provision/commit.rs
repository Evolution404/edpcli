use super::*;

pub fn commit_plain_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedPlainProvision,
) -> EdpCliResult<()> {
    let session = TargetSession::<ReadOnly>::open_usb(runner, prepared.disk)?;
    let session = session.prepare_write().map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    let _session = session
        .reopen_and_verify(dev, OPEN_WAIT, |dev| {
            verify_reopened_snapshot(dev, &prepared.source_metadata)
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
    diskio::execute_write_transaction(dev, &transaction)?;

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
        != crate::provision::DiskProvisionKind::Plain
    {
        return Err(err(EXIT_IO, "错误: Plain 写后重新识别仍为 EDP 模式"));
    }
    Ok(())
}

pub fn commit_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedProvision,
) -> EdpCliResult<ProvisionCommitOutcome> {
    match prepared {
        PreparedProvision::Official(prepared) => {
            commit_new_provision(runner, dev, prepared).map(ProvisionCommitOutcome::Official)
        }
        PreparedProvision::Plain(prepared) => {
            let partition_count = prepared.plan.partitions.len();
            commit_plain_provision(runner, dev, prepared)
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
            Ok(())
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
    if let Some(serial) = &prepared.expected_serial {
        if runner.hardware_serial(prepared.disk).as_ref() != Some(serial) {
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
    diskio::atomic_write_official_provision_sectors(
        dev,
        &prepared.write_image.patch,
        prepared.write_image.total_sectors,
    )?;
    verify_protocol_readback(dev, prepared)?;
    let mut report = ProvisionCommitReport {
        provision_succeeded: true,
        formats: Vec::new(),
    };
    for choice in prepared
        .format_targets
        .iter()
        .filter(|choice| choice.selected)
    {
        let result = format_partition(runner, dev, prepared, choice);
        report.formats.push(PartitionFormatResult {
            role: choice.target.role,
            result: result.map_err(|error| error.msg),
        });
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
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {} Migrate 当前 unsupported，拒绝 commit",
                        part.geometry.role.label()
                    ),
                ));
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
        prepared.expected_serial.as_deref(),
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
    expected_serial: Option<&str>,
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
    if expected_serial.is_none_or(|serial| Some(serial) != fresh_serial) {
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

fn format_partition(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
    choice: &PlannedPartitionFormat,
) -> EdpCliResult<()> {
    verify_format_identity(runner, dev, prepared)?;
    execute_partition_format(dev, choice)?;
    verify_format_identity(runner, dev, prepared)?;
    Ok(())
}

/// Format one verified official partition. The caller owns device identity and
/// protocol verification; this operation never writes the protocol region.
pub(super) fn execute_partition_format(
    dev: &mut dyn SectorDev,
    choice: &PlannedPartitionFormat,
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
    for (&relative, sector) in built.image.sectors() {
        let absolute = choice
            .target
            .geometry
            .start_sector
            .checked_add(relative)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| err(EXIT_TARGET, "错误: 格式化写入 LBA 溢出"))?;
        dev.write_sector(absolute, sector).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 写入失败: {error}"),
            )
        })?;
    }
    dev.sync()
        .map_err(|error| err(EXIT_IO, format!("错误: 格式化同步失败: {error}")))?;
    for (&relative, expected) in built.image.sectors() {
        let absolute = u32::try_from(choice.target.geometry.start_sector + relative)
            .map_err(|_| err(EXIT_TARGET, "错误: 格式化读回 LBA 溢出"))?;
        let actual = dev.read_sector(absolute).map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回失败: {error}"),
            )
        })?;
        if actual.as_slice() != expected {
            return Err(err(
                EXIT_IO,
                format!("错误: 格式化 LBA{absolute} 读回不一致"),
            ));
        }
    }
    let raw_boot = dev
        .read_sector(
            u32::try_from(choice.target.geometry.start_sector)
                .map_err(|_| err(EXIT_TARGET, "错误: 分区起点 LBA 溢出"))?,
        )
        .map_err(|error| err(EXIT_IO, format!("错误: 读取文件系统引导扇区失败: {error}")))?;
    if choice.target.physically_encrypted
        && (raw_boot.get(3..11) == Some(b"EXFAT   ") || raw_boot.get(54..62) == Some(b"FAT16   "))
    {
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
    let mut reader = PreparedImageReader {
        image: verification_image,
    };
    let boot = reader
        .read_sector(0)
        .map_err(|error| err(EXIT_IO, error.to_string()))?;
    let geometry_ok = match filesystem {
        OfficialFilesystemFormat::ExFat => {
            boot.get(3..11) == Some(b"EXFAT   ")
                && u64::from_le_bytes(boot[64..72].try_into().unwrap())
                    == choice.target.geometry.start_sector
                && u64::from_le_bytes(boot[72..80].try_into().unwrap())
                    == choice.target.geometry.sector_count()
                && u32::from_le_bytes(boot[100..104].try_into().unwrap()) == choice.volume_serial
        }
        OfficialFilesystemFormat::Fat16 => {
            let total16 = u16::from_le_bytes(boot[19..21].try_into().unwrap()) as u64;
            let total = if total16 != 0 {
                total16
            } else {
                u32::from_le_bytes(boot[32..36].try_into().unwrap()) as u64
            };
            boot.get(54..62) == Some(b"FAT16   ")
                && u32::from_le_bytes(boot[28..32].try_into().unwrap()) as u64
                    == choice.target.geometry.start_sector
                && total == choice.target.geometry.sector_count()
                && u32::from_le_bytes(boot[39..43].try_into().unwrap()) == choice.volume_serial
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => false,
    };
    if !geometry_ok {
        return Err(err(EXIT_IO, "错误: 文件系统签名、几何或卷序列号读回不一致"));
    }
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
    let root_lba = match filesystem {
        OfficialFilesystemFormat::ExFat => {
            let root_cluster = u32::from_le_bytes(boot[96..100].try_into().unwrap());
            let heap_offset = u32::from_le_bytes(boot[88..92].try_into().unwrap()) as u64;
            let cluster_sectors = 1u64 << boot[109];
            heap_offset + (root_cluster as u64 - 2) * cluster_sectors
        }
        OfficialFilesystemFormat::Fat16 => {
            u16::from_le_bytes(boot[14..16].try_into().unwrap()) as u64
                + boot[16] as u64 * u16::from_le_bytes(boot[22..24].try_into().unwrap()) as u64
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => unreachable!(),
    };
    let root = reader
        .read_sector(root_lba)
        .map_err(|error| err(EXIT_IO, error.to_string()))?;
    let actual_label = match filesystem {
        OfficialFilesystemFormat::ExFat => root
            .as_chunks::<32>()
            .0
            .iter()
            .find(|entry| entry[0] == 0x83)
            .and_then(|entry| {
                let count = entry[1] as usize;
                (count <= 11).then(|| {
                    (0..count)
                        .map(|index| {
                            u16::from_le_bytes([entry[2 + index * 2], entry[3 + index * 2]])
                        })
                        .collect::<Vec<_>>()
                })
            })
            .and_then(|units| String::from_utf16(&units).ok())
            .unwrap_or_default(),
        OfficialFilesystemFormat::Fat16 => {
            if root[11] != 0x08 || boot[43..54] != root[..11] {
                return Err(err(EXIT_IO, "错误: FAT16 卷标目录项读回不一致"));
            }
            let (decoded, _, had_errors) = GBK.decode(&root[..11]);
            if had_errors {
                return Err(err(EXIT_IO, "错误: FAT16 卷标无法按 GBK 解码"));
            }
            decoded.trim_end_matches(' ').to_string()
        }
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => unreachable!(),
    };
    let expected_label = if filesystem == OfficialFilesystemFormat::Fat16 {
        choice.volume_label.to_uppercase()
    } else {
        choice.volume_label.clone()
    };
    if actual_label != expected_label {
        return Err(err(EXIT_IO, "错误: 文件系统卷标读回不一致"));
    }
    Ok(())
}
