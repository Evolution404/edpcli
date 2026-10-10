//! One result-state projection shared by drawing and copied table values.
use crate::tui::state::{ProvisionResultPartition, ProvisionState};

pub(crate) fn partition_action_label(partition: &ProvisionResultPartition) -> &'static str {
    use crate::provision::RegionDisposition as D;
    if partition.selected_for_format {
        return "格式化";
    }
    match partition.disposition {
        Some(D::PreserveOpaque) => "原样保留",
        Some(D::PreserveVerified) => "验证保留",
        Some(D::RewrapVerified) => "更新密钥",
        Some(D::Rebuild) => "重建",
        Some(D::Drop) => "移除",
        None => "写入",
    }
}

/// A native commit records the full filesystem initialization in its WAL,
/// then verifies every written native block. The legacy per-partition format
/// reports are intentionally absent, not failures or unknown statuses.
fn native_write_set_verified(provision: &ProvisionState) -> bool {
    use crate::application::provision::{ProvisionCommitOutcome, ProvisionExecutionStatus};
    provision.result_status == Some(ProvisionExecutionStatus::Success)
        && provision.result_outcome.as_ref().is_some_and(|outcome| {
            outcome.backup.path.extension().is_some_and(|ext| ext == "wal")
                && matches!(
                    &outcome.commit,
                    ProvisionCommitOutcome::Official(report) if report.provision_succeeded && report.formats.is_empty()
                )
        })
}

pub(crate) fn partition_filesystem_label(
    provision: &ProvisionState,
    plan: &crate::tui::state::ProvisionResultSnapshot,
    partition: &ProvisionResultPartition,
) -> String {
    use crate::application::provision::ProvisionCommitOutcome;
    let Some(filesystem) = partition.filesystem else {
        return "—".into();
    };
    let confirmed = match provision
        .result_outcome
        .as_ref()
        .map(|outcome| &outcome.commit)
    {
        Some(ProvisionCommitOutcome::Plain { partition_count })
            if plan.target == crate::provision::ProvisionTarget::Plain =>
        {
            *partition_count == plan.partitions.len()
        }
        Some(ProvisionCommitOutcome::Official(report))
            if report.provision_succeeded
                && plan.target != crate::provision::ProvisionTarget::Plain =>
        {
            !partition.selected_for_format
                || native_write_set_verified(provision)
                || report
                    .formats
                    .iter()
                    .any(|format| Some(format.role) == partition.role && format.result.is_ok())
        }
        _ => false,
    };
    if confirmed {
        filesystem.display_name().into()
    } else {
        let origin = if partition.selected_for_format {
            "目标"
        } else {
            "来源"
        };
        format!("{origin} {}", filesystem.display_name())
    }
}

pub(crate) fn disposition_label(disposition: crate::provision::RegionDisposition) -> &'static str {
    use crate::provision::RegionDisposition as D;
    match disposition {
        D::PreserveOpaque => "原样保留",
        D::PreserveVerified => "验证保留",
        D::RewrapVerified => "密钥已更新",
        D::Rebuild => "已重建",
        D::Drop => "已移除",
    }
}

pub(crate) fn partition_final_status(
    provision: &ProvisionState,
    plan: &crate::tui::state::ProvisionResultSnapshot,
    partition: &ProvisionResultPartition,
) -> (String, crate::tui::ui::ResultTone) {
    let Some(outcome) = provision.result_outcome.as_ref() else {
        return ("未确认".into(), crate::tui::ui::ResultTone::Warning);
    };
    if plan.target == crate::provision::ProvisionTarget::Plain {
        return if matches!(outcome.commit, crate::application::provision::ProvisionCommitOutcome::Plain { partition_count }
            if partition_count == plan.partitions.len())
        {
            (
                "已写入 · 读回通过".into(),
                crate::tui::ui::ResultTone::Success,
            )
        } else {
            ("未确认".into(), crate::tui::ui::ResultTone::Warning)
        };
    }
    let crate::application::provision::ProvisionCommitOutcome::Official(report) = &outcome.commit
    else {
        return ("未确认".into(), crate::tui::ui::ResultTone::Warning);
    };
    if !report.provision_succeeded {
        return ("未确认".into(), crate::tui::ui::ResultTone::Warning);
    }

    if partition.selected_for_format {
        if native_write_set_verified(provision) {
            return (
                "已初始化 · 原生回读通过".into(),
                crate::tui::ui::ResultTone::Success,
            );
        }
        if let Some(role) = partition.role {
            if let Some(format) = report.formats.iter().find(|item| item.role == role) {
                return match &format.result {
                    Ok(()) => (
                        "已格式化 · 读回通过".into(),
                        crate::tui::ui::ResultTone::Success,
                    ),
                    Err(error) if error.is_skipped() => {
                        ("未执行".into(), crate::tui::ui::ResultTone::Muted)
                    }
                    Err(error)
                        if error
                            .operation_error()
                            .and_then(|error| error.media_state)
                            .is_some_and(|state| state.requires_reinspection()) =>
                    {
                        ("介质状态未确认".into(), crate::tui::ui::ResultTone::Danger)
                    }
                    Err(error)
                        if error.operation_error().and_then(|error| error.media_state)
                            == Some(crate::application::error::MediaState::RolledBack) =>
                    {
                        (
                            "格式化失败 · 已回滚".into(),
                            crate::tui::ui::ResultTone::Warning,
                        )
                    }
                    Err(_) => ("格式化失败".into(), crate::tui::ui::ResultTone::Warning),
                };
            }
        }
        return ("未确认".into(), crate::tui::ui::ResultTone::Warning);
    }

    (
        partition
            .disposition
            .map(disposition_label)
            .unwrap_or("未格式化")
            .into(),
        crate::tui::ui::ResultTone::Success,
    )
}

#[cfg(test)]
mod native_result_tests {
    use super::*;
    use crate::application::provision::{
        ProvisionCommitOutcome, ProvisionCommitReport, ProvisionExecutionStatus,
        ProvisionWriteOutcome,
    };
    use crate::provision::{OfficialPartitionMode, PartitionRole, ProvisionTarget};

    #[test]
    fn native_wal_readback_confirms_each_formatted_partition_but_not_unverified_plans() {
        let snapshot = crate::tui::state::ProvisionResultSnapshot {
            disk: 5,
            target: ProvisionTarget::Official(OfficialPartitionMode::DefaultThreePartition),
            total_bytes: 4_294_967_296,
            logical_sector_bytes: 4096,
            lce_extent: Some((1_044_001, 1)),
            partitions: vec![crate::tui::state::ProvisionResultPartition {
                role: Some(PartitionRole::Boot),
                filesystem: Some(crate::filesystem::FilesystemKind::Fat12),
                start_lba: 63,
                size_bytes: 2497 * 4096,
                selected_for_format: true,
                disposition: Some(crate::provision::RegionDisposition::Rebuild),
            }],
        };
        let partition = &snapshot.partitions[0];
        let mut state = ProvisionState::default();
        assert_eq!(
            partition_final_status(&state, &snapshot, partition).0,
            "未确认"
        );
        state.result_status = Some(ProvisionExecutionStatus::Success);
        state.result_outcome = Some(ProvisionWriteOutcome {
            backup: crate::application::post_restore::MetadataBackupReport {
                path: std::path::PathBuf::from("native-provision-disk5-test.wal"),
                partition_count: 1,
                edp_protocol_saved: true,
            },
            commit: ProvisionCommitOutcome::Official(ProvisionCommitReport {
                provision_succeeded: true,
                formats: vec![],
            }),
            warnings: vec![],
        });
        assert_eq!(
            partition_final_status(&state, &snapshot, partition).0,
            "已初始化 · 原生回读通过"
        );
        assert_eq!(
            partition_filesystem_label(&state, &snapshot, partition),
            "FAT12"
        );
        // An EDPB backup and no per-partition reports are not native WAL proof.
        state.result_outcome.as_mut().unwrap().backup.path = "old-backup.edpb".into();
        assert_eq!(
            partition_final_status(&state, &snapshot, partition).0,
            "未确认"
        );
        state.result_outcome.as_mut().unwrap().backup.path = "native-provision-test.wal".into();
        state.result_status = Some(ProvisionExecutionStatus::FatalFailure);
        assert_eq!(
            partition_final_status(&state, &snapshot, partition).0,
            "未确认"
        );
    }
}
