use super::review::{
    ProvisionConfirmationAction, ProvisionConfirmationDataEffect,
    ProvisionConfirmationFilesystemEffect, ProvisionConfirmationPasswordEffect,
    ProvisionConfirmationRegion,
};
use crate::provision::{PassthroughBasis, PasswordDisposition, RegionDisposition};
use crate::tui::disk_layout::{
    DiskCapacitySelection, DiskLayoutModel, DiskLayoutSegment, DiskRegionKind,
};

pub(super) fn merge_all_regions(
    layout: &DiskLayoutModel,
    mut planned_regions: Vec<ProvisionConfirmationRegion>,
) -> Result<Vec<ProvisionConfirmationRegion>, String> {
    let visible = layout.collapsed_tail_model();
    let mut regions = Vec::with_capacity(visible.segments.len());
    for segment in &visible.segments {
        let selection = DiskCapacitySelection::from_segment(segment)
            .ok_or_else(|| format!("{} 的 LBA 范围无效", segment.label))?;
        if let Some(index) = planned_regions
            .iter()
            .position(|region| region.selection == selection)
        {
            let mut region = planned_regions.remove(index);
            region.label = segment.label.clone();
            regions.push(region);
            continue;
        }
        regions.push(system_region(segment, selection)?);
    }
    if let Some(region) = planned_regions.first() {
        return Err(format!(
            "最终布局缺少已计划区域：{}（LBA {}–{}）",
            region.label,
            region.selection.start_lba,
            region.selection.end_exclusive.saturating_sub(1)
        ));
    }
    Ok(regions)
}

fn system_region(
    segment: &DiskLayoutSegment,
    selection: DiskCapacitySelection,
) -> Result<ProvisionConfirmationRegion, String> {
    let (action, reason) = match segment.kind {
        DiskRegionKind::Protocol => (
            ProvisionConfirmationAction::Fixed,
            "EDP 主协议区位置固定，按最终计划写入协议元数据。",
        ),
        DiskRegionKind::Metadata => (
            ProvisionConfirmationAction::Fixed,
            "分区表元数据位置固定，按最终计划生成。",
        ),
        DiskRegionKind::Reserved => (
            ProvisionConfirmationAction::Preserve,
            "官方保留范围不参与数据分区格式化，保持为保留区域。",
        ),
        DiskRegionKind::Free => (
            ProvisionConfirmationAction::Free,
            "最终计划未分配该 LBA 范围，保持为空闲区域。",
        ),
        DiskRegionKind::Lce | DiskRegionKind::BackupMirror | DiskRegionKind::RestoreNode => (
            ProvisionConfirmationAction::Fixed,
            "该区域属于 EDP 盘尾协议结构，位置由协议固定。",
        ),
        DiskRegionKind::Tail => (
            ProvisionConfirmationAction::Fixed,
            "尾部区域包含 LCE 与盘尾恢复元数据，整体位置由 EDP 协议固定。",
        ),
        DiskRegionKind::Boot
        | DiskRegionKind::Share
        | DiskRegionKind::Combined
        | DiskRegionKind::Encrypt
        | DiskRegionKind::Compatibility
        | DiskRegionKind::Plain => {
            return Err(format!(
                "{} 未找到对应的最终执行计划（LBA {}–{}）",
                segment.label,
                selection.start_lba,
                selection.end_exclusive.saturating_sub(1)
            ));
        }
        DiskRegionKind::Unknown => {
            return Err(format!("最终布局仍包含未识别区域：{}", segment.label));
        }
        DiskRegionKind::Conflict => {
            return Err(format!("最终布局存在区域冲突：{}", segment.label));
        }
    };
    Ok(ProvisionConfirmationRegion {
        label: segment.label.clone(),
        role: None,
        selection,
        sector_count: segment.sector_count,
        action,
        data_effect: ProvisionConfirmationDataEffect::None,
        password_effect: ProvisionConfirmationPasswordEffect::None,
        filesystem_effect: ProvisionConfirmationFilesystemEffect::None,
        reason_summary: reason.into(),
        technical_basis: vec![
            format!("区域类型    {}", segment.kind.label()),
            format!(
                "LBA 范围    {}–{}",
                segment.start_lba,
                segment
                    .end_exclusive()
                    .unwrap_or(segment.start_lba.saturating_add(1))
                    .saturating_sub(1)
            ),
        ],
    })
}

pub(super) const fn disposition_label(disposition: RegionDisposition) -> &'static str {
    match disposition {
        RegionDisposition::PreserveOpaque => "原样保留",
        RegionDisposition::PreserveVerified => "验证后保留",
        RegionDisposition::RewrapVerified => "验证后改密",
        RegionDisposition::Migrate => "数据迁移",
        RegionDisposition::Rebuild => "重建",
        RegionDisposition::Drop => "删除",
    }
}

pub(super) const fn password_disposition_label(disposition: PasswordDisposition) -> &'static str {
    match disposition {
        PasswordDisposition::Passthrough(PassthroughBasis::Verified) => "密码域透传（已验证）",
        PasswordDisposition::Passthrough(PassthroughBasis::OpaqueCompatible) => {
            "密码域透传（满足原样保留条件）"
        }
        PasswordDisposition::Rewrap => "密码域改密",
        PasswordDisposition::Rebuild => "密码域重建",
        PasswordDisposition::Blocked => "密码处理不可执行",
    }
}

pub(super) fn partition_reason(
    disposition: RegionDisposition,
    password: Option<PasswordDisposition>,
    format_selected: bool,
) -> String {
    if format_selected {
        return "用户已明确选择格式化；该区域将按目标几何重建，原数据不会保留。".into();
    }
    if let Some(password) = password {
        return match password {
            PasswordDisposition::Passthrough(PassthroughBasis::Verified) => {
                "物理结构、语义、分区几何和文件系统均与来源一致，来源密钥已验证；原 FileKey 和数据范围保持不变。".into()
            }
            PasswordDisposition::Passthrough(PassthroughBasis::OpaqueCompatible) => {
                "分区几何与密钥配置满足原样透传条件；原密钥材料和数据范围保持不变。".into()
            }
            PasswordDisposition::Rewrap => {
                "来源 FileKey 已验证；仅更新密码封装，FileKey 和数据范围保持不变。".into()
            }
            PasswordDisposition::Rebuild => {
                "使用目标密码重建密码域并生成新 FileKey。".into()
            }
            PasswordDisposition::Blocked => "密码处理条件不满足，当前计划不可执行。".into(),
        };
    }
    match disposition {
        RegionDisposition::PreserveOpaque => "该区域满足原样保留条件，原数据范围保持不变。".into(),
        RegionDisposition::PreserveVerified => "该区域已验证可保留，原数据范围保持不变。".into(),
        RegionDisposition::RewrapVerified => {
            "该区域已验证，仅更新密码封装，原数据范围保持不变。".into()
        }
        RegionDisposition::Migrate => "来源数据将迁移到新的目标区域。".into(),
        RegionDisposition::Rebuild => "该区域将按最终目标布局重建。".into(),
        RegionDisposition::Drop => "该来源区域不会保留在最终布局中。".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned_boot() -> ProvisionConfirmationRegion {
        ProvisionConfirmationRegion {
            label: "启动区".into(),
            role: Some(crate::provision::PartitionRole::Boot),
            selection: DiskCapacitySelection {
                start_lba: 63,
                end_exclusive: 1_000,
                kind: DiskRegionKind::Boot,
            },
            sector_count: 937,
            action: ProvisionConfirmationAction::Preserve,
            data_effect: ProvisionConfirmationDataEffect::Preserve,
            password_effect: ProvisionConfirmationPasswordEffect::None,
            filesystem_effect: ProvisionConfirmationFilesystemEffect::Keep,
            reason_summary: "启动区保持不变。".into(),
            technical_basis: vec!["LBA 范围    63–999".into()],
        }
    }

    #[test]
    fn confirmation_region_plan_contains_every_visible_disk_region() {
        let layout = DiskLayoutModel::new(
            2_000,
            vec![
                DiskLayoutSegment {
                    label: "EDP 主协议区".into(),
                    start_lba: 0,
                    sector_count: 13,
                    kind: DiskRegionKind::Protocol,
                },
                DiskLayoutSegment {
                    label: "保留区域".into(),
                    start_lba: 13,
                    sector_count: 50,
                    kind: DiskRegionKind::Reserved,
                },
                DiskLayoutSegment {
                    label: "启动区".into(),
                    start_lba: 63,
                    sector_count: 937,
                    kind: DiskRegionKind::Boot,
                },
                DiskLayoutSegment {
                    label: "空闲区域".into(),
                    start_lba: 1_000,
                    sector_count: 500,
                    kind: DiskRegionKind::Free,
                },
                DiskLayoutSegment {
                    label: "LCE".into(),
                    start_lba: 1_500,
                    sector_count: 6,
                    kind: DiskRegionKind::Lce,
                },
                DiskLayoutSegment {
                    label: "盘尾恢复节点".into(),
                    start_lba: 1_506,
                    sector_count: 494,
                    kind: DiskRegionKind::RestoreNode,
                },
            ],
        );
        let regions = merge_all_regions(&layout, vec![planned_boot()]).expect("all regions");
        let labels = regions
            .iter()
            .map(|region| region.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            ["EDP 主协议区", "保留区域", "启动区", "空闲区域", "尾部区域"]
        );
        assert_eq!(regions[0].action, ProvisionConfirmationAction::Fixed);
        assert_eq!(regions[1].action, ProvisionConfirmationAction::Preserve);
        assert_eq!(regions[3].action, ProvisionConfirmationAction::Free);
        assert_eq!(regions[4].action, ProvisionConfirmationAction::Fixed);
    }

    #[test]
    fn user_facing_confirmation_text_never_exposes_internal_english_state_names() {
        let mut texts = Vec::new();
        for disposition in [
            RegionDisposition::PreserveOpaque,
            RegionDisposition::PreserveVerified,
            RegionDisposition::RewrapVerified,
            RegionDisposition::Migrate,
            RegionDisposition::Rebuild,
            RegionDisposition::Drop,
        ] {
            texts.push(disposition_label(disposition).to_string());
            texts.push(partition_reason(disposition, None, false));
        }
        for disposition in [
            PasswordDisposition::Passthrough(PassthroughBasis::Verified),
            PasswordDisposition::Passthrough(PassthroughBasis::OpaqueCompatible),
            PasswordDisposition::Rewrap,
            PasswordDisposition::Rebuild,
            PasswordDisposition::Blocked,
        ] {
            texts.push(password_disposition_label(disposition).to_string());
            texts.push(partition_reason(
                RegionDisposition::PreserveVerified,
                Some(disposition),
                false,
            ));
        }
        let joined = texts.join("\n");
        for forbidden in [
            "Preserve",
            "Passthrough",
            "Rewrap",
            "Rebuild",
            "Migrate",
            "Drop",
            "Blocked",
            "filesystem",
            "data extent",
            "extent=",
            "disposition=",
            "password=",
        ] {
            assert!(
                !joined.contains(forbidden),
                "用户可见确认文本泄露内部术语: {forbidden}\n{joined}"
            );
        }
        assert!(joined.contains("FileKey"));
    }
}
