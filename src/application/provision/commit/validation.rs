//! Pure validation for prepared provision commits.

use super::*;

pub(in crate::application::provision) fn validate_preserve_source_snapshot(
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

pub(in crate::application::provision) fn validate_key_disposition_plan(
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
        let key_domain = KeyDomainRole::from_partition_role(part.geometry.role);
        match (key_domain, part.password_disposition) {
            (None, None) => {}
            (None, Some(_)) => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}不是密码域，却携带密码动作",
                        part.geometry.role.label()
                    ),
                ));
            }
            (Some(_), None) => {
                return Err(err(
                    EXIT_TARGET,
                    format!("错误: {}缺少统一密码动作", part.geometry.role.label()),
                ));
            }
            (Some(_), Some(crate::provision::PasswordDisposition::Blocked)) => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}密码域仍为“需重建”，拒绝进入写盘阶段",
                        part.geometry.role.label()
                    ),
                ));
            }
            (
                Some(_),
                Some(crate::provision::PasswordDisposition::Passthrough(
                    crate::provision::PassthroughBasis::OpaqueCompatible,
                )),
            ) if part.disposition != RegionDisposition::PreserveOpaque => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}透传依据为 OpaqueCompatible，但区域动作不是 PreserveOpaque",
                        part.geometry.role.label()
                    ),
                ));
            }
            (
                Some(_),
                Some(crate::provision::PasswordDisposition::Passthrough(
                    crate::provision::PassthroughBasis::Verified,
                )),
            ) if part.disposition != RegionDisposition::PreserveVerified => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}透传依据为 Verified，但区域动作不是 PreserveVerified",
                        part.geometry.role.label()
                    ),
                ));
            }
            (Some(_), Some(crate::provision::PasswordDisposition::Rewrap))
                if part.disposition != RegionDisposition::RewrapVerified =>
            {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}密码动作为 Rewrap，但区域动作不是 RewrapVerified",
                        part.geometry.role.label()
                    ),
                ));
            }
            (Some(_), Some(crate::provision::PasswordDisposition::Rebuild))
                if part.disposition != RegionDisposition::Rebuild =>
            {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}密码动作为 Rebuild，但区域动作不允许重建密钥域",
                        part.geometry.role.label()
                    ),
                ));
            }
            _ => {}
        }
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
                if let Some(record) = part.preserved_record {
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
                } else if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {} PreserveVerified 密码域缺少来源记录",
                            part.geometry.role.label()
                        ),
                    ));
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

pub(in crate::application::provision) fn validate_target_write_set(
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
        part.action == PartitionAction::PreserveExact
            && KeyDomainRole::from_partition_role(part.geometry.role).is_some()
            && part.preserved_record.is_none()
    }) {
        return Err(err(EXIT_TARGET, "错误: 保留密码域缺少原 key material"));
    }
    Ok(())
}
