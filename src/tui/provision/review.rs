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
                format!("⚠ 格式化 {}", filesystem.windows_format_name())
            }
            Self::Create(filesystem) => format!("+ 新建 {}", filesystem.windows_format_name()),
            Self::None => "— 不涉及".into(),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationOverall {
    pub cleared_regions: usize,
    pub reformatted_regions: usize,
    pub password_changed_regions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionConfirmationViewModel {
    pub target: ProvisionConfirmationTarget,
    pub layout: crate::tui::disk_layout::DiskLayoutModel,
    pub overall: ProvisionConfirmationOverall,
    pub regions: Vec<ProvisionConfirmationRegion>,
}

impl ProvisionConfirmationViewModel {
    pub(crate) fn from_prepared(prepared: &ProvisionPrepared) -> Result<Self, String> {
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

        let (layout, regions) = match prepared {
            ProvisionPrepared::Official(official) => {
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

                let mut regions = Vec::with_capacity(target_plan.partitions.len());
                for part in &target_plan.partitions {
                    if part.password_disposition
                        == Some(crate::provision::PasswordDisposition::Blocked)
                    {
                        return Err(format!(
                            "计划确认失败：{} 的密码处理条件仍未满足",
                            part.geometry.role.label()
                        ));
                    }
                    let format_choice = official.format_targets.iter().find(|choice| {
                        choice.target.role == part.geometry.role
                            && choice.target.geometry.start_sector == part.geometry.start_lba
                            && choice.target.geometry.sector_count() == part.geometry.sector_count
                    });
                    let format_selected = format_choice.is_some_and(|choice| choice.selected);
                    if part.disposition == crate::provision::RegionDisposition::Rebuild
                        && part.geometry.role
                            != crate::provision::PartitionRole::CompatibilityReserve
                        && !format_selected
                    {
                        return Err(format!(
                            "计划确认失败：{} 需要重建，但未找到已选择的格式化目标",
                            part.geometry.role.label()
                        ));
                    }
                    if part.password_disposition
                        == Some(crate::provision::PasswordDisposition::Rebuild)
                        && !format_selected
                    {
                        return Err(format!(
                            "计划确认失败：{} 的密码域需要重建，但未找到已选择的格式化目标",
                            part.geometry.role.label()
                        ));
                    }

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
                            let source_has_domain =
                                crate::provision::KeyDomainRole::from_partition_role(
                                    part.geometry.role,
                                )
                                .is_some_and(|domain| official.source_kind.has_key_domain(domain));
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
                        let filesystem = format_choice
                            .and_then(|choice| choice.filesystem)
                            .or(part.geometry.filesystem)
                            .ok_or_else(|| {
                                format!(
                                    "计划确认失败：{} 已选择格式化，但缺少文件系统类型",
                                    part.geometry.role.label()
                                )
                            })?;
                        ProvisionConfirmationFilesystemEffect::Format(filesystem)
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
                let regions = merge_all_regions(&layout, regions)?;
                (layout, regions)
            }
            ProvisionPrepared::Plain(plain) => {
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
                (layout, regions)
            }
        };

        let overall = ProvisionConfirmationOverall {
            cleared_regions: regions
                .iter()
                .filter(|region| region.data_effect == ProvisionConfirmationDataEffect::Clear)
                .count(),
            reformatted_regions: regions
                .iter()
                .filter(|region| {
                    matches!(
                        region.filesystem_effect,
                        ProvisionConfirmationFilesystemEffect::Format(_)
                            | ProvisionConfirmationFilesystemEffect::Create(_)
                    )
                })
                .count(),
            password_changed_regions: regions
                .iter()
                .filter(|region| {
                    matches!(
                        region.password_effect,
                        ProvisionConfirmationPasswordEffect::Rewrap
                            | ProvisionConfirmationPasswordEffect::InitializeNew
                            | ProvisionConfirmationPasswordEffect::Rebuild
                    )
                })
                .count(),
        };
        Ok(Self {
            target,
            layout,
            overall,
            regions,
        })
    }
}

impl AppState {
    pub(crate) fn provision_confirmation_view_model(
        &self,
    ) -> Result<ProvisionConfirmationViewModel, String> {
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
