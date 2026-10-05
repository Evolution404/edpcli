use super::*;

fn post_restore_key_state(
    partition: &crate::application::post_restore::PostRestorePartition,
) -> &'static str {
    use crate::application::post_restore::PostRestorePartitionState;

    if partition.role.as_deref() == Some("compatibility_reserve") {
        return "无需处理";
    }
    if !partition.requires_original_key {
        return "无需原密钥";
    }
    match partition.state {
        PostRestorePartitionState::Usable => "沿用密钥域",
        PostRestorePartitionState::NeedsFormat => "密钥已验证",
        PostRestorePartitionState::PasswordRequired => "需要原密码",
        PostRestorePartitionState::CryptoMetadataInvalid => "需重建密钥",
        PostRestorePartitionState::Unsupported => "密钥域未知",
    }
}

fn filesystem_display_label(value: &str) -> String {
    crate::filesystem::FilesystemKind::from_config_token(value)
        .map(|kind| kind.display_name().to_string())
        .unwrap_or_else(|| crate::ui::sanitize_terminal_text(value))
}

impl AppState {
    pub fn result_partition_table_view(&self) -> Option<crate::tui::table_layout::TableViewData> {
        use crate::tui::table_layout::{table_column_schema, TableKind, TableViewData};

        let rows = if self
            .restore
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            let wizard = self.restore.wizard.as_ref()?;
            let outcome = wizard.restore_outcome.as_ref()?;
            outcome
                .assessment
                .partitions
                .iter()
                .map(|partition| {
                    let compatibility_reserve =
                        partition.role.as_deref() == Some("compatibility_reserve");
                    let status = if compatibility_reserve {
                        "兼容保留"
                    } else {
                        match partition.state {
                        crate::application::post_restore::PostRestorePartitionState::Usable => {
                            "可用"
                        }
                        crate::application::post_restore::PostRestorePartitionState::NeedsFormat => {
                            "需要格式化"
                        }
                        crate::application::post_restore::PostRestorePartitionState::PasswordRequired => {
                            "需要原密码"
                        }
                        crate::application::post_restore::PostRestorePartitionState::CryptoMetadataInvalid => {
                            "加密元数据异常"
                        }
                        crate::application::post_restore::PostRestorePartitionState::Unsupported => {
                            "暂不支持"
                        }
                    }
                    };
                    let filesystem = if compatibility_reserve {
                        "—".into()
                    } else {
                        partition
                            .detected_filesystem
                            .map(|value| value.label().to_string())
                            .or_else(|| partition.filesystem_hint.as_deref().map(filesystem_display_label))
                            .unwrap_or_else(|| "—".into())
                    };
                    let key_state = post_restore_key_state(partition);
                    vec![
                        format!("P{}", partition.index),
                        status.into(),
                        filesystem,
                        crate::common::fmt_capacity(
                            partition
                                .sector_count
                                .saturating_mul(crate::common::SECTOR as u64),
                        ),
                        key_state.into(),
                        crate::ui::sanitize_terminal_text(&partition.detail),
                    ]
                })
                .collect::<Vec<_>>()
        } else if self.shell.workspace == Workspace::Provision
            && self.provision.stage == ProvisionStage::Result
        {
            let plan = self.provision.result_plan.as_ref()?;
            plan.partitions
                .iter()
                .enumerate()
                .map(|(index, partition)| {
                    let filesystem = partition
                        .filesystem
                        .map(|kind| kind.display_name().to_string())
                        .unwrap_or_else(|| "—".into());
                    let disposition =
                        super::super::provision_result_presentation::disposition_label;
                    let action = if partition.selected_for_format {
                        "格式化"
                    } else {
                        partition.disposition.map(disposition).unwrap_or("写入")
                    };
                    let (final_status, _) =
                        super::super::provision_result_presentation::partition_final_status(
                            &self.provision,
                            plan,
                            partition,
                        );
                    vec![
                        format!("P{}", index + 1),
                        partition
                            .role
                            .map(crate::provision::PartitionRole::label)
                            .unwrap_or("普通分区")
                            .into(),
                        filesystem,
                        crate::common::fmt_capacity(partition.size_bytes),
                        action.into(),
                        final_status,
                    ]
                })
                .collect::<Vec<_>>()
        } else {
            return None;
        };

        Some(TableViewData::from_rows(
            0,
            &table_column_schema(TableKind::ResultPartitions)
                .expect("result partition table schema"),
            rows,
        ))
    }

    pub fn visible_result_partition_indices(&self) -> Vec<usize> {
        let Some(view) = self.result_partition_table_view() else {
            return Vec::new();
        };
        view.sorted_indices(
            (0..view.rows.len()).collect(),
            self.table_interaction(crate::tui::table_layout::TableKind::ResultPartitions),
        )
    }

    pub fn result_partition_selected_source_index(&self) -> Option<usize> {
        if self
            .restore
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            self.restore
                .wizard
                .as_ref()
                .and_then(|wizard| wizard.post_restore_workbench.selected_partition)
        } else if self.shell.workspace == Workspace::Provision
            && self.provision.stage == ProvisionStage::Result
        {
            self.provision.result_workbench.selected_partition
        } else {
            None
        }
    }

    pub fn result_partition_moved_source(
        &self,
        current_source: usize,
        delta: isize,
    ) -> Option<usize> {
        let order = self.visible_result_partition_indices();
        let current = order
            .iter()
            .position(|source| *source == current_source)
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(order.len().saturating_sub(1));
        order.get(next).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::post_restore::{PostRestorePartition, PostRestorePartitionState};

    fn partition(
        state: PostRestorePartitionState,
        requires_original_key: bool,
        role: Option<&str>,
    ) -> PostRestorePartition {
        PostRestorePartition {
            index: 1,
            role: role.map(str::to_string),
            start_lba: 126,
            sector_count: 2_048,
            filesystem_hint: None,
            detected_filesystem: None,
            requires_original_key,
            state,
            detail: String::new(),
        }
    }

    #[test]
    fn filesystem_labels_use_canonical_user_facing_names() {
        for (raw, expected) in [
            ("fat16", "FAT16"),
            ("FAT32", "FAT32"),
            ("exfat", "exFAT"),
            ("NTFS", "NTFS"),
        ] {
            assert_eq!(filesystem_display_label(raw), expected);
        }
    }

    #[test]
    fn post_restore_key_state_describes_action_instead_of_implying_work_for_usable_partition() {
        assert_eq!(
            post_restore_key_state(&partition(PostRestorePartitionState::Usable, true, None)),
            "沿用密钥域"
        );
        assert_eq!(
            post_restore_key_state(&partition(
                PostRestorePartitionState::PasswordRequired,
                true,
                None,
            )),
            "需要原密码"
        );
        assert_eq!(
            post_restore_key_state(&partition(
                PostRestorePartitionState::NeedsFormat,
                true,
                None
            )),
            "密钥已验证"
        );
        assert_eq!(
            post_restore_key_state(&partition(
                PostRestorePartitionState::Usable,
                false,
                Some("compatibility_reserve"),
            )),
            "无需处理"
        );
    }
}
