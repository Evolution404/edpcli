use super::*;

impl AppState {
    pub fn result_partition_table_view(&self) -> Option<crate::tui::table_layout::TableViewData> {
        use crate::tui::table_layout::{table_column_schema, TableKind, TableViewData};

        let rows = if self
            .shell
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            let wizard = self.shell.wizard.as_ref()?;
            let outcome = wizard.restore_outcome.as_ref()?;
            outcome
                .assessment
                .partitions
                .iter()
                .map(|partition| {
                    let status = match partition.state {
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
                    };
                    let filesystem = partition
                        .detected_filesystem
                        .map(|value| value.label().to_string())
                        .or_else(|| partition.filesystem_hint.clone())
                        .unwrap_or_else(|| "—".into());
                    let key_state = if partition.requires_original_key {
                        "需要原密钥域"
                    } else {
                        "无需原密钥"
                    };
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
                        .map(|kind| kind.config_token().to_string())
                        .unwrap_or_else(|| "—".into());
                    let disposition = |value: crate::provision::RegionDisposition| match value {
                        crate::provision::RegionDisposition::PreserveOpaque => "原样保留",
                        crate::provision::RegionDisposition::PreserveVerified => "验证保留",
                        crate::provision::RegionDisposition::RewrapVerified => "密钥已更新",
                        crate::provision::RegionDisposition::Migrate => "数据已迁移",
                        crate::provision::RegionDisposition::Rebuild => "已重建",
                        crate::provision::RegionDisposition::Drop => "已移除",
                    };
                    let action = if partition.selected_for_format {
                        "格式化"
                    } else {
                        partition.disposition.map(disposition).unwrap_or("写入")
                    };
                    let final_status = if plan.target == crate::provision::ProvisionTarget::Plain {
                        if self.provision.result_outcome.is_some() {
                            "已写入 · 读回通过".to_string()
                        } else {
                            "未确认".to_string()
                        }
                    } else if partition.selected_for_format {
                        let format_status = partition.role.and_then(|role| {
                            self.provision.result_outcome.as_ref().and_then(|outcome| {
                                match &outcome.commit {
                                    crate::application::provision::ProvisionCommitOutcome::Official(report) => report
                                        .formats
                                        .iter()
                                        .find(|item| item.role == role)
                                        .map(|format| {
                                            if format.result.is_ok() {
                                                "已格式化 · 读回通过"
                                            } else {
                                                "格式化失败"
                                            }
                                        }),
                                    crate::application::provision::ProvisionCommitOutcome::Plain { .. } => None,
                                }
                            })
                        });
                        format_status.unwrap_or("已写入").to_string()
                    } else {
                        partition
                            .disposition
                            .map(disposition)
                            .unwrap_or("未格式化")
                            .to_string()
                    };
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
            .shell
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            self.shell
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
