//! Read-only, frontend-neutral facts for the official provision review.
//! The submit path must independently recheck all write authorization.
use super::PlannedPartitionFormat;
use crate::filesystem::FilesystemKind;
use crate::provision::{
    DiskProvisionKind, KeyDomainRole, PartitionRole, PasswordDisposition, RegionDisposition,
    TargetProvisionPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OfficialReviewFacts {
    pub format_selected: bool,
    pub selected_filesystem: Option<FilesystemKind>,
    pub source_has_password_domain: bool,
}

pub(crate) fn assess_official_review(
    plan: &TargetProvisionPlan,
    formats: &[PlannedPartitionFormat],
    source_kind: DiskProvisionKind,
) -> Result<Vec<OfficialReviewFacts>, String> {
    plan.partitions
        .iter()
        .map(|part| {
            let role = part.geometry.role;
            let choice = formats.iter().find(|choice| {
                choice.target.role == role
                    && choice.target.geometry.start_sector == part.geometry.start_lba
                    && choice.target.geometry.sector_count() == part.geometry.sector_count
            });
            let format_selected = choice.is_some_and(|item| item.selected);
            if part.password_disposition == Some(PasswordDisposition::Blocked) {
                return Err(format!(
                    "计划确认失败：{} 的密码处理条件仍未满足",
                    role.label()
                ));
            }
            if part.disposition == RegionDisposition::Rebuild
                && role != PartitionRole::CompatibilityReserve
                && !format_selected
            {
                return Err(format!(
                    "计划确认失败：{} 需要重建，但未找到已选择的格式化目标",
                    role.label()
                ));
            }
            if part.password_disposition == Some(PasswordDisposition::Rebuild) && !format_selected {
                return Err(format!(
                    "计划确认失败：{} 的密码域需要重建，但未找到已选择的格式化目标",
                    role.label()
                ));
            }
            let selected_filesystem = if format_selected {
                Some(
                    choice
                        .and_then(|item| item.filesystem)
                        .or(part.geometry.filesystem)
                        .ok_or_else(|| {
                            format!(
                                "计划确认失败：{} 已选择格式化，但缺少文件系统类型",
                                role.label()
                            )
                        })?,
                )
            } else {
                None
            };
            Ok(OfficialReviewFacts {
                format_selected,
                selected_filesystem,
                source_has_password_domain: KeyDomainRole::from_partition_role(role)
                    .is_some_and(|domain| source_kind.has_key_domain(domain)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provision::{
        OfficialPartitionFilesystems, OfficialPartitionMode, OfficialPartitionSizes,
        TargetPartitionGeometry, TargetPartitionPlan,
    };

    #[test]
    fn review_requires_format_for_rebuild_and_rejects_blocked_password() {
        let mode = OfficialPartitionMode::DefaultThreePartition;
        let target = crate::provision::official_format_targets_with_filesystems(
            mode,
            OfficialPartitionSizes::new(32, 64, 128),
            512,
            OfficialPartitionFilesystems::defaults(),
        )
        .unwrap()
        .into_iter()
        .find(|target| target.role == PartitionRole::Share)
        .unwrap();
        let partition = TargetPartitionPlan {
            geometry: TargetPartitionGeometry {
                role: target.role,
                partition_type: target.geometry.partition_type,
                start_lba: target.geometry.start_sector,
                sector_count: target.geometry.sector_count(),
                physically_encrypted: target.physically_encrypted,
                filesystem: target.filesystem,
            },
            disposition: RegionDisposition::Rebuild,
            password_disposition: Some(PasswordDisposition::Rebuild),
            source_password_knowledge: None,
            target_password_policy: None,
            reason: String::new(),
            preserved_record: None,
        };
        let mut plan = TargetProvisionPlan {
            mode,
            partitions: vec![partition],
            unallocated_sectors: 0,
        };
        let mut formats = vec![PlannedPartitionFormat {
            target,
            selected: false,
            filesystem: Some(FilesystemKind::Fat32),
            volume_label: String::new(),
            volume_serial: 1,
            prepared_image: None,
            verification_image: None,
        }];
        assert!(
            assess_official_review(&plan, &formats, DiskProvisionKind::Plain)
                .unwrap_err()
                .contains("需要重建")
        );
        formats[0].selected = true;
        let facts = assess_official_review(&plan, &formats, DiskProvisionKind::Plain).unwrap();
        assert!(facts[0].format_selected);
        assert_eq!(facts[0].selected_filesystem, Some(FilesystemKind::Fat32));
        assert!(!facts[0].source_has_password_domain);
        plan.partitions[0].password_disposition = Some(PasswordDisposition::Blocked);
        assert!(
            assess_official_review(&plan, &formats, DiskProvisionKind::Plain)
                .unwrap_err()
                .contains("密码处理条件")
        );
    }
}
