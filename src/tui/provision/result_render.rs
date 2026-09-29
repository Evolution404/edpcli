use super::*;
use crate::tui::state::ProvisionState;

fn status_label(
    status: Option<crate::application::provision::ProvisionExecutionStatus>,
) -> (&'static str, crate::tui::ui::ResultTone) {
    use crate::application::provision::ProvisionExecutionStatus as Status;
    match status {
        Some(Status::Success) => ("制盘成功", crate::tui::ui::ResultTone::Success),
        Some(Status::CompletedWithWarnings) => {
            ("制盘完成，存在警告", crate::tui::ui::ResultTone::Warning)
        }
        Some(Status::PartialFormatFailure) => {
            ("部分完成：格式化失败", crate::tui::ui::ResultTone::Warning)
        }
        Some(Status::FatalFailure) | None => ("制盘失败", crate::tui::ui::ResultTone::Danger),
    }
}

fn disposition_label(disposition: crate::provision::RegionDisposition) -> &'static str {
    use crate::provision::RegionDisposition as D;
    match disposition {
        D::PreserveOpaque => "原样保留",
        D::PreserveVerified => "验证保留",
        D::RewrapVerified => "密钥已更新",
        D::Migrate => "数据已迁移",
        D::Rebuild => "已重建",
        D::Drop => "已移除",
    }
}

fn official_partition_status(
    partition: &crate::tui::state::ProvisionResultPartition,
    outcome: Option<&crate::application::provision::ProvisionWriteOutcome>,
) -> crate::tui::ui::ResultValue {
    if partition.selected_for_format {
        if let (
            Some(role),
            Some(crate::application::provision::ProvisionCommitOutcome::Official(report)),
        ) = (partition.role, outcome.map(|value| &value.commit))
        {
            if let Some(format) = report.formats.iter().find(|item| item.role == role) {
                return if format.result.is_ok() {
                    crate::tui::ui::ResultValue::success("已格式化 · 读回通过").emphasized()
                } else {
                    crate::tui::ui::ResultValue::warning("格式化失败").emphasized()
                };
            }
        }
        return crate::tui::ui::ResultValue::primary("已写入");
    }

    crate::tui::ui::ResultValue::success(
        partition
            .disposition
            .map(disposition_label)
            .unwrap_or("未格式化"),
    )
}

fn result_table(provision: &ProvisionState) -> Option<crate::tui::ui::ResultTable> {
    use ratatui::layout::Constraint;
    let plan = provision.result_plan.as_ref()?;
    let outcome = provision.result_outcome.as_ref();
    let rows = plan
        .partitions
        .iter()
        .enumerate()
        .map(|(index, partition)| {
            let filesystem = partition
                .filesystem
                .map(|kind| kind.config_token().to_string())
                .unwrap_or_else(|| "—".into());
            let status = if plan.target == crate::provision::ProvisionTarget::Plain {
                if outcome.is_some() {
                    crate::tui::ui::ResultValue::success("已写入 · 读回通过").emphasized()
                } else {
                    crate::tui::ui::ResultValue::warning("未确认")
                }
            } else {
                official_partition_status(partition, outcome)
            };
            vec![
                crate::tui::ui::ResultValue::primary(format!("P{}", index + 1)),
                crate::tui::ui::ResultValue::primary(
                    partition
                        .role
                        .map(crate::provision::PartitionRole::label)
                        .unwrap_or("普通分区"),
                ),
                crate::tui::ui::ResultValue::primary(filesystem),
                crate::tui::ui::ResultValue::muted(partition.start_lba.to_string()),
                crate::tui::ui::ResultValue::primary(crate::common::fmt_capacity(
                    partition.size_bytes,
                )),
                status,
            ]
        })
        .collect();

    Some(crate::tui::ui::ResultTable {
        title: "分区结果".into(),
        headers: vec![
            "分区".into(),
            "角色".into(),
            "文件系统".into(),
            "起始 LBA".into(),
            "大小".into(),
            "状态".into(),
        ],
        rows,
        widths: vec![
            Constraint::Length(6),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(13),
            Constraint::Length(12),
            Constraint::Min(18),
        ],
    })
}

fn validation_card(provision: &ProvisionState) -> crate::tui::ui::ResultCard {
    use crate::application::provision::ProvisionCommitOutcome;
    let mut lines = Vec::new();

    if let Some(outcome) = provision.result_outcome.as_ref() {
        lines.push(crate::tui::ui::ResultValue::success("✓ 制盘前自动备份已创建").emphasized());
        match &outcome.commit {
            ProvisionCommitOutcome::Official(report) => {
                if report.provision_succeeded {
                    lines.push(crate::tui::ui::ResultValue::success(
                        "✓ 协议与几何读回验证通过",
                    ));
                }
                for item in &report.formats {
                    if item.result.is_ok() {
                        lines.push(crate::tui::ui::ResultValue::success(format!(
                            "✓ {}格式化读回通过",
                            item.role.label()
                        )));
                    } else {
                        lines.push(crate::tui::ui::ResultValue::warning(format!(
                            "⚠ {}格式化失败",
                            item.role.label()
                        )));
                    }
                }
            }
            ProvisionCommitOutcome::Plain { partition_count } => {
                lines.push(crate::tui::ui::ResultValue::success(format!(
                    "✓ {partition_count} 个 MBR 分区写入并读回"
                )));
                lines.push(crate::tui::ui::ResultValue::success(
                    "✓ LBA3 保留且 EDP 状态已清除",
                ));
            }
        }
        lines.extend(outcome.warnings.iter().map(|warning| {
            crate::tui::ui::ResultValue::warning(format!("⚠ {}", warning.message()))
        }));
    } else {
        lines.push(crate::tui::ui::ResultValue::danger(
            provision.message.as_deref().unwrap_or("写入未完成"),
        ));
    }

    crate::tui::ui::ResultCard {
        title: "验收结果".into(),
        lines,
    }
}

fn execution_card(provision: &ProvisionState) -> crate::tui::ui::ResultCard {
    let mut lines = Vec::new();
    if let Some(outcome) = provision.result_outcome.as_ref() {
        let file = outcome
            .backup
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<无效文件名>");
        lines.push(crate::tui::ui::ResultValue::primary(format!(
            "备份文件  {file}"
        )));
        if let Some(parent) = outcome.backup.path.parent() {
            lines.push(crate::tui::ui::ResultValue::muted(format!(
                "备份目录  {}",
                parent.display()
            )));
        }
    }
    if let Some(run) = provision.run.as_ref() {
        let elapsed = run
            .last_activity_at
            .duration_since(run.started_at)
            .as_secs();
        lines.push(crate::tui::ui::ResultValue::primary(format!(
            "总耗时  {elapsed} 秒"
        )));
    }
    if lines.is_empty() {
        lines.push(crate::tui::ui::ResultValue::muted("无附加执行信息"));
    }
    crate::tui::ui::ResultCard {
        title: "执行信息".into(),
        lines,
    }
}

pub(super) fn draw_provision_result(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let provision = state.provision();
    let (status, tone) = status_label(provision.result_status);
    let result_plan = provision.result_plan.as_ref();
    let disk = result_plan
        .map(|plan| format!("disk{}", plan.disk))
        .or_else(|| {
            state
                .provision_target_disk()
                .map(|disk| format!("disk{disk}"))
        })
        .unwrap_or_else(|| "—".into());
    let target = result_plan
        .map(|plan| plan.target.full_name().to_string())
        .unwrap_or_else(|| provision.kind.title().to_string());
    let capacity = result_plan
        .map(|plan| crate::common::fmt_capacity(plan.total_bytes))
        .or_else(|| {
            state
                .selected_device()
                .map(|row| crate::common::fmt_capacity(row.size))
        })
        .unwrap_or_else(|| "—".into());
    let verification = match provision.result_status {
        Some(crate::application::provision::ProvisionExecutionStatus::Success) => {
            "全部读回验证通过"
        }
        Some(crate::application::provision::ProvisionExecutionStatus::CompletedWithWarnings) => {
            "主事务成功，存在警告"
        }
        Some(crate::application::provision::ProvisionExecutionStatus::PartialFormatFailure) => {
            "协议事务成功，部分格式化失败"
        }
        Some(crate::application::provision::ProvisionExecutionStatus::FatalFailure) | None => {
            "写入未完成"
        }
    };

    let spec = crate::tui::ui::OperationResultSpec {
        title: "制盘结果".into(),
        status: status.into(),
        tone,
        fields: vec![
            crate::tui::ui::ResultField::new(
                "目标设备",
                crate::tui::ui::ResultValue::primary(disk).emphasized(),
            ),
            crate::tui::ui::ResultField::new(
                "盘型",
                crate::tui::ui::ResultValue {
                    text: target,
                    tone: crate::tui::ui::ResultTone::Accent,
                    bold: true,
                },
            ),
            crate::tui::ui::ResultField::new(
                "总容量",
                crate::tui::ui::ResultValue::primary(capacity).emphasized(),
            ),
            crate::tui::ui::ResultField::new(
                "验收",
                crate::tui::ui::ResultValue::primary(verification),
            ),
        ],
        table: result_table(provision),
        cards: vec![validation_card(provision), execution_card(provision)],
        footer: "Enter / Esc 返回制盘中心".into(),
    };
    crate::tui::ui::render_operation_result(frame, area, &spec);
}
