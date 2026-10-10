use super::review_region_projection::{
    disposition_label, merge_all_regions, partition_reason, password_disposition_label,
};
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionConfirmationAction {
    Fixed,
    Preserve,
    Free,
    Passthrough,
    Rewrap,
    FormatRebuild,
    New,
    Delete,
}

impl ProvisionConfirmationAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fixed => "● 固定",
            Self::Preserve => "● 保留",
            Self::Free => "○ 空闲",
            Self::Passthrough => "✓ 透传",
            Self::Rewrap => "✓ 改密",
            Self::FormatRebuild => "⚠ 格式化重建",
            Self::New => "+ 新建",
            Self::Delete => "✗ 删除",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionConfirmationDataEffect {
    Preserve,
    Clear,
    None,
}

impl ProvisionConfirmationDataEffect {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Preserve => "✓ 保留",
            Self::Clear => "⚠ 清空",
            Self::None => "— 不涉及",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionConfirmationPasswordEffect {
    None,
    Preserve,
    Rewrap,
    InitializeNew,
    Rebuild,
}

impl ProvisionConfirmationPasswordEffect {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "— 不涉及",
            Self::Preserve => "✓ 保留原密码域",
            Self::Rewrap => "↻ 使用目标密码，FileKey 保持",
            Self::InitializeNew => "+ 新建密码域",
            Self::Rebuild => "⚠ 重建密码域，生成新 FileKey",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionConfirmationFilesystemEffect {
    Keep,
    Format(crate::filesystem::FilesystemKind),
    Create(crate::filesystem::FilesystemKind),
    None,
}

impl ProvisionConfirmationFilesystemEffect {
    pub fn label(self) -> String {
        match self {
            Self::Keep => "✓ 保持".into(),
            Self::Format(filesystem) => {
                format!("⚠ 格式化 {}", filesystem.display_name())
            }
            Self::Create(filesystem) => format!("+ 新建 {}", filesystem.display_name()),
            Self::None => "— 不涉及".into(),
        }
    }
}

/// Project the native write planner's decision; never infer data loss
/// from the presence of native protocol writes.
fn native_partition_review(
    part: &crate::application::provision::native_flow::NativePreviewPartition,
) -> Result<
    (
        ProvisionConfirmationAction,
        ProvisionConfirmationDataEffect,
        ProvisionConfirmationPasswordEffect,
        ProvisionConfirmationFilesystemEffect,
        String,
    ),
    String,
> {
    use crate::provision::{PartitionRole, PasswordDisposition, RegionDisposition};
    if part.formatted {
        if part.role.is_some() && part.disposition != Some(RegionDisposition::Rebuild) {
            return Err("原生格式化勾选状态与底层分区处理计划不一致".into());
        }
        return Ok((
            ProvisionConfirmationAction::FormatRebuild,
            ProvisionConfirmationDataEffect::Clear,
            if part.physically_encrypted {
                ProvisionConfirmationPasswordEffect::Rebuild
            } else {
                ProvisionConfirmationPasswordEffect::None
            },
            part.filesystem
                .map(ProvisionConfirmationFilesystemEffect::Format)
                .unwrap_or(ProvisionConfirmationFilesystemEffect::None),
            "已明确选择格式化：重新创建文件系统，原分区数据将清空。".into(),
        ));
    }
    if part.role == Some(PartitionRole::CompatibilityReserve) {
        return Ok((
            ProvisionConfirmationAction::Preserve,
            ProvisionConfirmationDataEffect::None,
            ProvisionConfirmationPasswordEffect::None,
            ProvisionConfirmationFilesystemEffect::None,
            "官方兼容保留范围，不格式化。".into(),
        ));
    }
    part.validate_preservation()?;
    let (action, reason) = match part.disposition {
        Some(RegionDisposition::PreserveOpaque | RegionDisposition::PreserveVerified) => (
            if part.physically_encrypted {
                ProvisionConfirmationAction::Passthrough
            } else {
                ProvisionConfirmationAction::Preserve
            },
            "来源分区原样保留；原始数据、文件系统和 FileKey 不变。",
        ),
        Some(RegionDisposition::RewrapVerified) => (
            ProvisionConfirmationAction::Rewrap,
            "仅修改密码封装；原 FileKey、文件系统与密文数据保持不变。",
        ),
        _ => return Err("原生非格式化区域没有有效的保留/改密处置".into()),
    };
    let password_effect = match part.password_disposition {
        Some(PasswordDisposition::Passthrough(_)) => ProvisionConfirmationPasswordEffect::Preserve,
        Some(PasswordDisposition::Rewrap) => ProvisionConfirmationPasswordEffect::Rewrap,
        Some(PasswordDisposition::Rebuild | PasswordDisposition::Blocked) => {
            return Err("未格式化区域不能重建密码域或处于密码不可执行状态".into());
        }
        None => ProvisionConfirmationPasswordEffect::None,
    };
    Ok((
        action,
        ProvisionConfirmationDataEffect::Preserve,
        password_effect,
        ProvisionConfirmationFilesystemEffect::Keep,
        reason.into(),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationTarget {
    pub disk: u32,
    pub total_sectors: u64,
    pub device_id: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub onlyid: Option<String>,
    pub target: crate::provision::ProvisionTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationRegion {
    pub label: String,
    pub role: Option<crate::provision::PartitionRole>,
    pub selection: crate::tui::disk_layout::DiskCapacitySelection,
    pub sector_count: u64,
    pub action: ProvisionConfirmationAction,
    pub data_effect: ProvisionConfirmationDataEffect,
    pub password_effect: ProvisionConfirmationPasswordEffect,
    pub filesystem_effect: ProvisionConfirmationFilesystemEffect,
    pub reason_summary: String,
    pub technical_basis: Vec<String>,
}

impl ProvisionConfirmationRegion {
    pub(crate) fn result_summary(&self) -> String {
        match self.filesystem_effect {
            ProvisionConfirmationFilesystemEffect::Format(filesystem)
            | ProvisionConfirmationFilesystemEffect::Create(filesystem) => {
                return format!("创建新的空 {} 文件系统。", filesystem.display_name());
            }
            ProvisionConfirmationFilesystemEffect::Keep
            | ProvisionConfirmationFilesystemEffect::None => {}
        }

        if self.action == ProvisionConfirmationAction::Delete {
            return "该来源区域不会保留在最终布局中。".into();
        }
        if self.action == ProvisionConfirmationAction::Free {
            return "该范围保持未分配，不创建文件系统。".into();
        }
        if self.action == ProvisionConfirmationAction::Rewrap {
            return "仅更新密码封装；FileKey、数据范围和文件系统保持不变。".into();
        }
        if self.password_effect == ProvisionConfirmationPasswordEffect::Preserve
            && self.data_effect == ProvisionConfirmationDataEffect::Preserve
        {
            return "原 FileKey、数据范围和文件系统保持不变。".into();
        }
        if self.data_effect == ProvisionConfirmationDataEffect::Preserve {
            return "原数据范围和文件系统保持不变。".into();
        }
        match self.action {
            ProvisionConfirmationAction::Fixed => "按最终计划生成固定协议结构。".into(),
            ProvisionConfirmationAction::New => "按最终计划新建该区域。".into(),
            ProvisionConfirmationAction::Preserve => "该区域按最终布局保留。".into(),
            ProvisionConfirmationAction::FormatRebuild => "按最终几何重建该区域。".into(),
            ProvisionConfirmationAction::Passthrough => "原数据范围和密钥材料保持不变。".into(),
            ProvisionConfirmationAction::Rewrap
            | ProvisionConfirmationAction::Free
            | ProvisionConfirmationAction::Delete => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationOverall {
    /// Source partitions affected, deduplicated by their original identity.
    pub source_discarded: Vec<String>,
    pub source_retained: Vec<String>,
    /// Separate target/key operations: these are NOT source data-loss counts.
    pub target_formatted: Vec<String>,
    pub key_changed: Vec<String>,
}

/// Source data is counted against the source layout, never against the number
/// of targets created by splitting or merging it.
fn classify_source_partition_impact(
    sources: &[(String, u64, u64)],
    targets: &[crate::application::provision::native_flow::NativePreviewPartition],
) -> (Vec<String>, Vec<String>) {
    use crate::provision::RegionDisposition;
    let mut discarded = Vec::new();
    let mut retained = Vec::new();
    for (label, start, sectors) in sources {
        let unchanged = targets.iter().any(|target| {
            !target.formatted
                && target.start_lba == *start
                && target.sector_count == *sectors
                && target
                    .role
                    .is_some_and(|role| role.label() == label.as_str())
                && matches!(
                    target.disposition,
                    Some(
                        RegionDisposition::PreserveOpaque
                            | RegionDisposition::PreserveVerified
                            | RegionDisposition::RewrapVerified
                    )
                )
        });
        let names = if unchanged {
            &mut retained
        } else {
            &mut discarded
        };
        if !names.contains(label) {
            names.push(label.clone());
        }
    }
    (discarded, retained)
}

fn native_source_partitions(
    native: &crate::application::provision::native_flow::NativePreparedProvision,
) -> Result<Vec<(String, u64, u64)>, String> {
    use crate::provision::{DiskProvisionKind, PartitionRole};
    if native.source != DiskProvisionKind::Plain {
        let protocol = crate::protocol::image::NativeProtocolImage::from_native_bytes(
            native.plan.sector_bytes,
            native
                .source_native_prefix
                .iter()
                .flat_map(|block| block.iter().copied())
                .collect(),
        )
        .map_err(|e| format!("确认页来源协议解析失败: {e}"))?;
        let original = crate::provision::parse_existing_provision_native(
            &protocol,
            &native.device_id,
            native.plan.total_sectors,
        )?
        .ok_or("确认页无法确认来源 EDP 分区，禁止显示未经证实的数据保留状态")?;
        return Ok(original
            .profile
            .partitions
            .iter()
            .filter(|p| p.role != PartitionRole::CompatibilityReserve)
            .map(|p| (p.role.label().to_string(), p.start_lba, p.sector_count))
            .collect());
    }

    // Plain MBR: derive source partition names from the captured original,
    // not from a newly generated target. Blank/unpartitioned sources stay empty.
    let mbr = native
        .source_native_prefix
        .first()
        .ok_or("来源 MBR 快照缺失")?;
    if mbr.len() < 512 || mbr[510..512] != [0x55, 0xaa] {
        return Ok(Vec::new());
    }
    let mut parts = Vec::new();
    for index in 0..4 {
        let offset = 446 + index * 16;
        if mbr[offset + 4] == 0 {
            continue;
        }
        let start = u32::from_le_bytes(
            mbr[offset + 8..offset + 12]
                .try_into()
                .map_err(|_| "来源 MBR 起点无效")?,
        ) as u64;
        let count = u32::from_le_bytes(
            mbr[offset + 12..offset + 16]
                .try_into()
                .map_err(|_| "来源 MBR 容量无效")?,
        ) as u64;
        if count == 0 {
            continue;
        }
        if start
            .checked_add(count)
            .is_none_or(|end| end > native.plan.total_sectors)
        {
            return Err("来源普通盘分区超出设备容量，不能报告数据影响".into());
        }
        parts.push((format!("普通分区P{}", index + 1), start, count));
    }
    Ok(parts)
}

fn old_mode_source_roles(kind: crate::provision::DiskProvisionKind) -> Vec<String> {
    use crate::provision::{DiskProvisionKind, PartitionRole};
    let roles: &[PartitionRole] = match kind {
        DiskProvisionKind::Plain => &[],
        DiskProvisionKind::Mode0 => &[
            PartitionRole::Boot,
            PartitionRole::Share,
            PartitionRole::Encrypt,
        ],
        DiskProvisionKind::Mode1 => &[PartitionRole::BootShareCombined, PartitionRole::Encrypt],
        DiskProvisionKind::Mode2 => &[PartitionRole::Encrypt],
        DiskProvisionKind::Mode3 => &[PartitionRole::Boot, PartitionRole::Share],
    };
    roles.iter().map(|role| role.label().to_string()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationViewModel {
    pub target: ProvisionConfirmationTarget,
    pub layout: crate::tui::disk_layout::DiskLayoutModel,
    pub overall: ProvisionConfirmationOverall,
    pub algorithm: Option<crate::provision::OfficialLabelAlgorithm>,
    pub geometry_note: Option<String>,
    pub regions: Vec<ProvisionConfirmationRegion>,
}

impl ProvisionConfirmationViewModel {
    pub(crate) fn from_prepared(
        prepared: &crate::application::provision::PreparedProvision,
    ) -> Result<Self, String> {
        use crate::tui::disk_layout::{
            DiskCapacitySelection, DiskLayoutModel, DiskLayoutSegment, DiskRegionKind,
        };

        let probe = prepared.hardware_probe();
        let target = ProvisionConfirmationTarget {
            disk: prepared.disk(),
            total_sectors: prepared.total_sectors(),
            device_id: prepared.device_id().to_string(),
            vid: probe.vid,
            pid: probe.pid,
            onlyid: prepared.expected_onlyid().map(str::to_string),
            target: prepared.target(),
        };

        let (layout, regions, geometry_note) = match prepared {
            crate::application::provision::PreparedProvision::Native(native) => {
                let partition_segments = native
                    .partitions
                    .iter()
                    .enumerate()
                    .map(|(i, part)| {
                        let label = part
                            .role
                            .map(|role| role.label().to_owned())
                            .unwrap_or_else(|| format!("普通分区[{}]", i + 1));
                        DiskLayoutSegment {
                            label,
                            start_lba: part.start_lba,
                            sector_count: part.sector_count,
                            kind: part
                                .role
                                .map(DiskRegionKind::from_partition_role)
                                .unwrap_or(DiskRegionKind::Plain),
                        }
                    })
                    .collect::<Vec<_>>();
                let layout = if let Some((lce, count)) = native.lce_extent {
                    DiskLayoutModel::canonical_edp(
                        native.plan.total_sectors,
                        partition_segments,
                        lce,
                        count,
                    )
                } else {
                    DiskLayoutModel::canonical_plain_plan(
                        native.plan.total_sectors,
                        partition_segments,
                    )
                }
                .map_err(|error| format!("原生制盘最终布局生成失败: {error}"))?
                .with_logical_sector_bytes(native.plan.sector_bytes)
                .map_err(|error| format!("原生制盘逻辑块大小无效: {error}"))?;
                layout
                    .validate_complete()
                    .map_err(|error| format!("原生制盘最终布局不完整: {error}"))?;
                let mut regions = Vec::with_capacity(native.partitions.len());
                for (i, part) in native.partitions.iter().enumerate() {
                    let end_exclusive = part
                        .start_lba
                        .checked_add(part.sector_count)
                        .ok_or("原生制盘分区范围溢出")?;
                    let kind = part
                        .role
                        .map(DiskRegionKind::from_partition_role)
                        .unwrap_or(DiskRegionKind::Plain);
                    let selection = DiskCapacitySelection {
                        start_lba: part.start_lba,
                        end_exclusive,
                        kind,
                    };
                    let (action, data_effect, password_effect, filesystem_effect, reason_summary) =
                        native_partition_review(part)?;
                    regions.push(ProvisionConfirmationRegion {
                        label: part
                            .role
                            .map(|role| role.label().to_string())
                            .unwrap_or_else(|| format!("P{}", i + 1)),
                        role: part.role,
                        selection,
                        sector_count: part.sector_count,
                        action,
                        data_effect,
                        password_effect,
                        filesystem_effect,
                        reason_summary,
                        technical_basis: vec![
                            format!("逻辑块大小  {}B", native.plan.sector_bytes),
                            format!("LBA 范围    {}–{}", part.start_lba, end_exclusive - 1),
                            format!("来源模式    {}", native.source.short_name()),
                            format!("分区处理    {:?}", part.disposition),
                            format!("是否格式化  {}", part.formatted),
                            format!("事务写集    {} 块（WAL保存原块）", native.plan.writes.len()),
                        ],
                    });
                }
                let regions = merge_all_regions(&layout, regions)?;
                (layout, regions, None)
            }
            crate::application::provision::PreparedProvision::Official(official) => {
                let target_plan = official
                    .target_plan
                    .as_ref()
                    .ok_or("计划确认失败：正式制盘计划缺少最终分区计划")?;
                let (lce_start_lba, lce_sector_count) = prepared
                    .lce_extent()
                    .ok_or("计划确认失败：正式制盘计划缺少 LCE 范围")?;
                let partition_segments = target_plan
                    .partitions
                    .iter()
                    .map(|part| DiskLayoutSegment {
                        label: part.geometry.role.label().into(),
                        start_lba: part.geometry.start_lba,
                        sector_count: part.geometry.sector_count,
                        kind: DiskRegionKind::from_partition_role(part.geometry.role),
                    })
                    .collect::<Vec<_>>();
                let layout = DiskLayoutModel::canonical_edp(
                    prepared.total_sectors(),
                    partition_segments,
                    lce_start_lba,
                    lce_sector_count,
                )
                .map_err(|_| "计划确认失败：无法生成完整的 EDP 最终布局".to_string())?;
                layout
                    .validate_complete()
                    .map_err(|_| "计划确认失败：EDP 最终磁盘布局不完整".to_string())?;
                if layout
                    .segments
                    .iter()
                    .any(|segment| segment.kind == DiskRegionKind::Conflict)
                {
                    return Err("计划确认失败：最终磁盘布局存在区域冲突".into());
                }

                let reviewed = crate::application::provision::assess_official_review(
                    target_plan,
                    &official.format_targets,
                    official.source_kind,
                )?;
                let mut regions = Vec::with_capacity(target_plan.partitions.len());
                for (part, facts) in target_plan.partitions.iter().zip(reviewed) {
                    let format_selected = facts.format_selected;

                    let kind = DiskRegionKind::from_partition_role(part.geometry.role);
                    let end_exclusive = part
                        .geometry
                        .start_lba
                        .checked_add(part.geometry.sector_count)
                        .ok_or("计划确认失败：目标分区 LBA 范围溢出")?;
                    let selection = DiskCapacitySelection {
                        start_lba: part.geometry.start_lba,
                        end_exclusive,
                        kind,
                    };
                    let matching_segments = layout
                        .segments
                        .iter()
                        .filter(|segment| {
                            segment.start_lba == selection.start_lba
                                && segment.kind == selection.kind
                                && segment.end_exclusive().ok() == Some(selection.end_exclusive)
                        })
                        .count();
                    if matching_segments != 1 {
                        return Err(format!(
                            "计划确认失败：{} 的 LBA 范围无法唯一对应到最终布局",
                            part.geometry.role.label()
                        ));
                    }

                    let (action, data_effect) = if format_selected {
                        (
                            ProvisionConfirmationAction::FormatRebuild,
                            ProvisionConfirmationDataEffect::Clear,
                        )
                    } else {
                        match part.password_disposition {
                            Some(crate::provision::PasswordDisposition::Passthrough(_)) => (
                                ProvisionConfirmationAction::Passthrough,
                                ProvisionConfirmationDataEffect::Preserve,
                            ),
                            Some(crate::provision::PasswordDisposition::Rewrap) => (
                                ProvisionConfirmationAction::Rewrap,
                                ProvisionConfirmationDataEffect::Preserve,
                            ),
                            Some(crate::provision::PasswordDisposition::Rebuild) => {
                                return Err(format!(
                                    "计划确认失败：{} 的密码域重建状态尚未收敛",
                                    part.geometry.role.label()
                                ))
                            }
                            Some(crate::provision::PasswordDisposition::Blocked) => unreachable!(),
                            None => match part.disposition {
                                crate::provision::RegionDisposition::PreserveOpaque
                                | crate::provision::RegionDisposition::PreserveVerified
                                | crate::provision::RegionDisposition::RewrapVerified => (
                                    ProvisionConfirmationAction::Preserve,
                                    ProvisionConfirmationDataEffect::Preserve,
                                ),
                                crate::provision::RegionDisposition::Rebuild
                                    if part.geometry.role
                                        == crate::provision::PartitionRole::CompatibilityReserve =>
                                {
                                    (
                                        ProvisionConfirmationAction::New,
                                        ProvisionConfirmationDataEffect::None,
                                    )
                                }
                                crate::provision::RegionDisposition::Rebuild => {
                                    return Err(format!(
                                        "计划确认失败：{} 的重建状态尚未收敛",
                                        part.geometry.role.label()
                                    ))
                                }
                                crate::provision::RegionDisposition::Drop => (
                                    ProvisionConfirmationAction::Delete,
                                    ProvisionConfirmationDataEffect::Clear,
                                ),
                            },
                        }
                    };

                    let password_effect = match part.password_disposition {
                        Some(crate::provision::PasswordDisposition::Passthrough(_)) => {
                            ProvisionConfirmationPasswordEffect::Preserve
                        }
                        Some(crate::provision::PasswordDisposition::Rewrap) => {
                            ProvisionConfirmationPasswordEffect::Rewrap
                        }
                        Some(crate::provision::PasswordDisposition::Rebuild) => {
                            let source_has_domain = facts.source_has_password_domain;
                            if source_has_domain {
                                ProvisionConfirmationPasswordEffect::Rebuild
                            } else {
                                ProvisionConfirmationPasswordEffect::InitializeNew
                            }
                        }
                        Some(crate::provision::PasswordDisposition::Blocked) => unreachable!(),
                        None => ProvisionConfirmationPasswordEffect::None,
                    };
                    let filesystem_effect = if format_selected {
                        ProvisionConfirmationFilesystemEffect::Format(
                            facts
                                .selected_filesystem
                                .expect("application review validated format filesystem"),
                        )
                    } else if part.geometry.role
                        == crate::provision::PartitionRole::CompatibilityReserve
                    {
                        ProvisionConfirmationFilesystemEffect::None
                    } else {
                        ProvisionConfirmationFilesystemEffect::Keep
                    };
                    let mut technical_basis = vec![format!(
                        "区域处理    {}",
                        disposition_label(part.disposition)
                    )];
                    if let Some(password) = part.password_disposition {
                        technical_basis.push(format!(
                            "密码处理    {}",
                            password_disposition_label(password)
                        ));
                    }
                    technical_basis.push(format!(
                        "LBA 范围    {}–{}",
                        selection.start_lba,
                        selection.end_exclusive.saturating_sub(1)
                    ));
                    regions.push(ProvisionConfirmationRegion {
                        label: part.geometry.role.label().into(),
                        role: Some(part.geometry.role),
                        selection,
                        sector_count: part.geometry.sector_count,
                        action,
                        data_effect,
                        password_effect,
                        filesystem_effect,
                        reason_summary: partition_reason(
                            part.disposition,
                            part.password_disposition,
                            format_selected,
                        ),
                        technical_basis,
                    });
                }
                let encrypt_geometry_preserved = target_plan.partitions.iter().any(|part| {
                    part.geometry.role == crate::provision::PartitionRole::Encrypt
                        && matches!(
                            part.disposition,
                            crate::provision::RegionDisposition::PreserveOpaque
                                | crate::provision::RegionDisposition::PreserveVerified
                                | crate::provision::RegionDisposition::RewrapVerified
                        )
                });
                let geometry_note = super::mode2_geometry_note::note_for(
                    target_plan.mode,
                    target_plan.unallocated_sectors,
                    encrypt_geometry_preserved,
                );
                let regions = merge_all_regions(&layout, regions)?;
                (layout, regions, geometry_note)
            }
            crate::application::provision::PreparedProvision::Plain(plain) => {
                let partition_segments = plain
                    .plan
                    .partitions
                    .iter()
                    .enumerate()
                    .map(|(index, part)| DiskLayoutSegment {
                        label: format!("普通分区[{}]", index + 1),
                        start_lba: part.start_lba,
                        sector_count: part.sector_count,
                        kind: DiskRegionKind::Plain,
                    })
                    .collect::<Vec<_>>();
                let layout = DiskLayoutModel::canonical_plain_plan(
                    plain.plan.total_sectors,
                    partition_segments,
                )
                .map_err(|_| "计划确认失败：无法生成完整的普通盘最终布局".to_string())?;
                layout
                    .validate_complete()
                    .map_err(|_| "计划确认失败：普通盘最终磁盘布局不完整".to_string())?;
                let mut regions = Vec::with_capacity(plain.plan.partitions.len());
                for (index, part) in plain.plan.partitions.iter().enumerate() {
                    let end_exclusive = part.end_exclusive().map_err(|_| {
                        format!("计划确认失败：普通分区 P{} 的 LBA 范围无效", index + 1)
                    })?;
                    let selection = DiskCapacitySelection {
                        start_lba: part.start_lba,
                        end_exclusive,
                        kind: DiskRegionKind::Plain,
                    };
                    let matching_segments = layout
                        .segments
                        .iter()
                        .filter(|segment| {
                            segment.start_lba == selection.start_lba
                                && segment.kind == selection.kind
                                && segment.end_exclusive().ok() == Some(selection.end_exclusive)
                        })
                        .count();
                    if matching_segments != 1 {
                        return Err(format!(
                            "计划确认失败：普通分区 P{} 的 LBA 范围无法唯一对应到最终布局",
                            index + 1
                        ));
                    }
                    let from_edp = plain.source_kind != crate::provision::DiskProvisionKind::Plain;
                    regions.push(ProvisionConfirmationRegion {
                        label: format!("P{}", index + 1),
                        role: None,
                        selection,
                        sector_count: part.sector_count,
                        action: ProvisionConfirmationAction::New,
                        data_effect: ProvisionConfirmationDataEffect::Clear,
                        password_effect: ProvisionConfirmationPasswordEffect::None,
                        filesystem_effect: ProvisionConfirmationFilesystemEffect::Create(part.filesystem),
                        reason_summary: if from_edp {
                            "当前 EDP 盘将重新初始化为普通盘；不会读取或迁移来源文件，目标普通分区创建新的空文件系统。".into()
                        } else {
                            "目标普通分区将创建新的空文件系统，原文件不保留。".into()
                        },
                        technical_basis: vec![
                            "文件处理    不读取、不迁移来源文件".into(),
                            format!(
                                "LBA 范围    {}–{}",
                                part.start_lba,
                                end_exclusive.saturating_sub(1)
                            ),
                        ],
                    });
                }
                let regions = merge_all_regions(&layout, regions)?;
                (layout, regions, None)
            }
        };

        let (source_discarded, source_retained) = match prepared {
            crate::application::provision::PreparedProvision::Native(native) => {
                classify_source_partition_impact(
                    &native_source_partitions(native)?,
                    &native.partitions,
                )
            }
            crate::application::provision::PreparedProvision::Official(official) => {
                let sources = old_mode_source_roles(official.source_kind);
                let retained = official
                    .target_plan
                    .as_ref()
                    .map(|plan| {
                        plan.partitions
                            .iter()
                            .filter(|p| p.disposition.preserves_extent())
                            .map(|p| p.geometry.role.label().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let discarded = sources
                    .iter()
                    .filter(|name| !retained.contains(name))
                    .cloned()
                    .collect();
                (discarded, retained)
            }
            crate::application::provision::PreparedProvision::Plain(plain) => {
                // The legacy Plain path recreates filesystems; it cannot claim
                // preservation without captured source extents.
                (old_mode_source_roles(plain.source_kind), Vec::new())
            }
        };
        let overall = ProvisionConfirmationOverall {
            source_discarded,
            source_retained,
            target_formatted: regions
                .iter()
                .filter(|region| {
                    matches!(
                        region.filesystem_effect,
                        ProvisionConfirmationFilesystemEffect::Format(_)
                            | ProvisionConfirmationFilesystemEffect::Create(_)
                    )
                })
                .map(|region| region.label.clone())
                .collect(),
            key_changed: regions
                .iter()
                .filter(|region| {
                    matches!(
                        region.password_effect,
                        ProvisionConfirmationPasswordEffect::Rewrap
                            | ProvisionConfirmationPasswordEffect::InitializeNew
                            | ProvisionConfirmationPasswordEffect::Rebuild
                    )
                })
                .map(|region| region.label.clone())
                .collect(),
        };
        Ok(Self {
            target,
            layout,
            overall,
            algorithm: match prepared {
                crate::application::provision::PreparedProvision::Official(official) => {
                    Some(official.algorithm)
                }
                crate::application::provision::PreparedProvision::Plain(_) => None,
                crate::application::provision::PreparedProvision::Native(_) => None,
            },
            geometry_note,
            regions,
        })
    }
}

impl AppState {
    pub(crate) fn provision_confirmation_view_model(
        &self,
    ) -> Result<ProvisionConfirmationViewModel, String> {
        if let Some(projection) = self.provision.review_projection.as_ref() {
            return Ok(projection.clone());
        }
        let prepared = self.provision.prepared.as_ref().ok_or("计划对象尚未准备")?;
        ProvisionConfirmationViewModel::from_prepared(prepared)
    }

    pub fn provision_review_region_count(&self) -> usize {
        self.provision_confirmation_view_model()
            .map(|view| view.regions.len())
            .unwrap_or(0)
    }

    pub fn provision_review_selected_region(&self) -> usize {
        let count = self.provision_review_region_count();
        self.provision
            .review_region_selected
            .min(count.saturating_sub(1))
    }

    pub fn provision_review_move_region(&mut self, delta: isize) {
        let count = self.provision_review_region_count();
        if count == 0 {
            self.provision.review_region_selected = 0;
            return;
        }
        let current = self.provision_review_selected_region();
        self.provision.review_region_selected = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(count - 1)
        };
    }

    pub fn provision_review_toggle_details(&mut self) {
        self.provision.review_details_expanded = !self.provision.review_details_expanded;
    }
}

#[cfg(test)]
mod native_preservation_projection_tests {
    use super::*;

    #[test]
    fn source_impact_reports_mode1_combined_once_when_split_into_two_mode0_targets() {
        use crate::application::provision::native_flow::NativePreviewPartition;
        use crate::provision::{PartitionRole, RegionDisposition};
        let sources = vec![
            ("二合一区".to_string(), 63, 13_627_329),
            ("保密区".to_string(), 13_627_392, 2_097_152),
        ];
        let targets = vec![
            NativePreviewPartition {
                role: Some(PartitionRole::Boot),
                start_lba: 63,
                sector_count: 20_417,
                filesystem: None,
                formatted: true,
                physically_encrypted: false,
                disposition: Some(RegionDisposition::Rebuild),
                password_disposition: None,
            },
            NativePreviewPartition {
                role: Some(PartitionRole::Share),
                start_lba: 20_480,
                sector_count: 13_606_912,
                filesystem: None,
                formatted: true,
                physically_encrypted: true,
                disposition: Some(RegionDisposition::Rebuild),
                password_disposition: None,
            },
            NativePreviewPartition {
                role: Some(PartitionRole::Encrypt),
                start_lba: 13_627_392,
                sector_count: 2_097_152,
                filesystem: None,
                formatted: false,
                physically_encrypted: true,
                disposition: Some(RegionDisposition::PreserveVerified),
                password_disposition: None,
            },
        ];
        let (discarded, retained) = classify_source_partition_impact(&sources, &targets);
        assert_eq!(discarded, ["二合一区"]);
        assert_eq!(retained, ["保密区"]);

        let mut resized = targets.clone();
        resized[2].sector_count -= 1;
        let (discarded, retained) = classify_source_partition_impact(&sources, &resized);
        assert_eq!(discarded, ["二合一区", "保密区"]);
        assert!(retained.is_empty());
    }

    use crate::application::provision::native_flow::NativePreviewPartition;
    use crate::filesystem::FilesystemKind;
    use crate::provision::{
        PartitionRole, PassthroughBasis, PasswordDisposition, RegionDisposition,
    };

    #[test]
    fn passthrough_rewrap_and_rebuild_render_from_actual_native_plan_for_all_sector_sizes() {
        for sector in [512u32, 1024, 2048, 4096] {
            let mut part = NativePreviewPartition {
                role: Some(PartitionRole::Encrypt),
                start_lba: 13627392 / (u64::from(sector) / 512),
                sector_count: 2097152,
                filesystem: Some(FilesystemKind::ExFat),
                physically_encrypted: true,
                formatted: false,
                disposition: Some(RegionDisposition::PreserveVerified),
                password_disposition: Some(PasswordDisposition::Passthrough(
                    PassthroughBasis::Verified,
                )),
            };
            let (action, data, password, fs, reason) = native_partition_review(&part).unwrap();
            assert_eq!(action, ProvisionConfirmationAction::Passthrough, "{sector}");
            assert_eq!(data, ProvisionConfirmationDataEffect::Preserve);
            assert_eq!(password, ProvisionConfirmationPasswordEffect::Preserve);
            assert_eq!(fs, ProvisionConfirmationFilesystemEffect::Keep);
            assert!(!reason.contains("清空"));

            part.disposition = Some(RegionDisposition::RewrapVerified);
            part.password_disposition = Some(PasswordDisposition::Rewrap);
            let (action, data, password, fs, _) = native_partition_review(&part).unwrap();
            assert_eq!(action, ProvisionConfirmationAction::Rewrap);
            assert_eq!(data, ProvisionConfirmationDataEffect::Preserve);
            assert_eq!(password, ProvisionConfirmationPasswordEffect::Rewrap);
            assert_eq!(fs, ProvisionConfirmationFilesystemEffect::Keep);

            part.formatted = true;
            part.disposition = Some(RegionDisposition::Rebuild);
            part.password_disposition = Some(PasswordDisposition::Rebuild);
            let (action, data, _, fs, _) = native_partition_review(&part).unwrap();
            assert_eq!(action, ProvisionConfirmationAction::FormatRebuild);
            assert_eq!(data, ProvisionConfirmationDataEffect::Clear);
            assert_eq!(
                fs,
                ProvisionConfirmationFilesystemEffect::Format(FilesystemKind::ExFat)
            );

            part.formatted = false;
            assert!(native_partition_review(&part).is_err());
            part.disposition = None;
            part.password_disposition = None;
            assert!(native_partition_review(&part).is_err());
        }
    }
}
