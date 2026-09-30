use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::application::post_restore::{PostRestorePartition, PostRestorePartitionState};
use crate::tui::pane::PaneId;
use crate::tui::state::AppState;

fn tone_style(tone: crate::tui::ui::ResultTone) -> Style {
    let theme = crate::tui::theme::current();
    match tone {
        crate::tui::ui::ResultTone::Primary => theme.body_text(),
        crate::tui::ui::ResultTone::Muted => theme.muted(),
        crate::tui::ui::ResultTone::Accent => theme.accent(),
        crate::tui::ui::ResultTone::Success => theme.success(),
        crate::tui::ui::ResultTone::Warning => theme.warning(),
        crate::tui::ui::ResultTone::Danger => theme.danger(),
    }
}

fn partition_status(
    partition: &PostRestorePartition,
) -> (&'static str, crate::tui::ui::ResultTone) {
    match partition.state {
        PostRestorePartitionState::Usable => ("可用", crate::tui::ui::ResultTone::Success),
        PostRestorePartitionState::NeedsFormat => {
            ("需要格式化", crate::tui::ui::ResultTone::Warning)
        }
        PostRestorePartitionState::PasswordRequired => {
            ("需要原密码", crate::tui::ui::ResultTone::Warning)
        }
        PostRestorePartitionState::CryptoMetadataInvalid => {
            ("加密元数据异常", crate::tui::ui::ResultTone::Danger)
        }
        PostRestorePartitionState::Unsupported => {
            ("暂不支持", crate::tui::ui::ResultTone::Muted)
        }
    }
}

fn render_partition_pane(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    focused: bool,
) {
    let block = crate::tui::ui::card("分区结果", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(wizard) = state.wizard() else {
        return;
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        frame.render_widget(Paragraph::new("没有恢复结果"), inner);
        return;
    };

    let theme = crate::tui::theme::current();
    let active_column = state.post_restore_result_active_column();
    let header = Row::new(
        [
            "分区",
            "状态",
            "文件系统",
            "LBA 范围",
            "容量",
            "密钥",
            "说明",
        ]
        .into_iter()
        .enumerate()
        .map(|(column, label)| {
            Cell::from(label).style(theme.table_header(column == active_column, focused))
        }),
    );

    let selected = wizard.post_restore_workbench.selected_partition;
    let rows = outcome
        .assessment
        .partitions
        .iter()
        .enumerate()
        .map(|(index, partition)| {
            let (status, status_tone) = partition_status(partition);
            let filesystem = partition
                .detected_filesystem
                .map(|value| value.label().to_string())
                .or_else(|| partition.filesystem_hint.clone())
                .unwrap_or_else(|| "—".into());
            let end = partition
                .start_lba
                .saturating_add(partition.sector_count)
                .saturating_sub(1);
            let capacity = crate::common::fmt_capacity(
                partition
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            );
            let key_state = if partition.requires_original_key {
                "需要原密钥域"
            } else {
                "无需原密钥"
            };
            let selected_row = selected == Some(index);
            let cell_style = |base: Style, column: usize| {
                theme.table_cell(base, column == active_column, focused)
            };

            Row::new(vec![
                Cell::from(format!("P{}", partition.index))
                    .style(cell_style(theme.table_text(), 0)),
                Cell::from(status).style(cell_style(tone_style(status_tone), 1)),
                Cell::from(filesystem).style(cell_style(theme.table_text(), 2)),
                Cell::from(format!("{}..={end}", partition.start_lba))
                    .style(cell_style(theme.table_text(), 3)),
                Cell::from(capacity).style(cell_style(theme.table_text(), 4)),
                Cell::from(key_state).style(cell_style(theme.table_text_muted(), 5)),
                Cell::from(crate::ui::sanitize_terminal_text(&partition.detail))
                    .style(cell_style(theme.table_text_muted(), 6)),
            ])
            .style(theme.apply_selection(theme.table_text(), selected_row, focused))
        });

    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Length(14),
                Constraint::Length(12),
                Constraint::Length(22),
                Constraint::Length(12),
                Constraint::Length(14),
                Constraint::Min(18),
            ],
        )
        .header(header)
        .column_spacing(1),
        inner,
    );
}

fn render_layout_pane(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let outer = crate::tui::ui::card("全盘布局", focused);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(wizard) = state.wizard() else {
        return;
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        frame.render_widget(Paragraph::new("没有恢复结果"), inner);
        return;
    };
    let Ok(model) = outcome.layout.as_ref() else {
        let message = outcome
            .layout
            .as_ref()
            .err()
            .map(String::as_str)
            .unwrap_or("恢复后全盘布局不可用");
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "元数据恢复已成功，但无法生成全盘布局。",
                    crate::tui::theme::current().warning(),
                )),
                Line::from(""),
                Line::from(crate::ui::sanitize_terminal_text(message)),
            ])
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    };

    let map_height = if inner.height >= 16 { 7 } else { 4 };
    let parts = Layout::vertical([
        Constraint::Length(map_height),
        Constraint::Length(1),
        Constraint::Min(4),
    ])
    .split(inner);
    let profile = if map_height >= 6 {
        crate::tui::disk_layout::DiskCapacityMapProfile::Full
    } else {
        crate::tui::disk_layout::DiskCapacityMapProfile::Compact
    };
    let lines = crate::tui::disk_layout::DiskCapacityMap::new(model, profile)
        .with_tail(crate::tui::disk_layout::TailExpansion::Collapsed)
        .with_selection(wizard.post_restore_workbench.region_selection())
        .with_marker(true)
        .lines(parts[0].width as usize);
    frame.render_widget(Paragraph::new(lines), parts[0]);
    crate::tui::result_workbench::render_result_region_list(
        frame,
        parts[2],
        &wizard.post_restore_workbench,
        model,
        focused,
    );
}

fn verification_lines(state: &AppState) -> Vec<(String, crate::tui::ui::ResultTone)> {
    let Some(wizard) = state.wizard() else {
        return vec![];
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        return vec![];
    };
    let mut lines = vec![
        (
            format!(
                "元数据写入  {}",
                if outcome.report.metadata_restored {
                    "完成 ✓"
                } else {
                    "未完成"
                }
            ),
            if outcome.report.metadata_restored {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Danger
            },
        ),
        (
            format!(
                "读回验证    {}",
                if outcome.report.readback_verified {
                    "通过 ✓"
                } else {
                    "未通过"
                }
            ),
            if outcome.report.readback_verified {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Danger
            },
        ),
        (
            format!(
                "恢复工件    {} 项",
                outcome.report.restored_artifact_ids.len()
            ),
            crate::tui::ui::ResultTone::Primary,
        ),
        (
            format!("设备状态    {}", outcome.device_state),
            crate::tui::ui::ResultTone::Primary,
        ),
        (
            format!(
                "全盘布局    {}",
                if outcome.layout.is_ok() {
                    "已验证 ✓"
                } else {
                    "投影失败 · 不影响元数据恢复成功"
                }
            ),
            if outcome.layout.is_ok() {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Warning
            },
        ),
    ];
    if let Err(message) = &outcome.layout {
        lines.push((
            format!("布局说明    {}", crate::ui::sanitize_terminal_text(message)),
            crate::tui::ui::ResultTone::Warning,
        ));
    }
    for issue in &outcome.assessment.issues {
        lines.push((
            format!("检查提示    {}", crate::ui::sanitize_terminal_text(issue)),
            crate::tui::ui::ResultTone::Warning,
        ));
    }
    if let Some(message) = &wizard.message {
        lines.push((
            format!("当前提示    {}", crate::ui::sanitize_terminal_text(message)),
            crate::tui::ui::ResultTone::Accent,
        ));
    }
    lines
}

fn render_verification_pane(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    focused: bool,
) {
    let block = crate::tui::ui::card("验收与执行", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(wizard) = state.wizard() else {
        return;
    };
    let offset = wizard
        .post_restore_workbench
        .viewport(PaneId::ResultVerification)
        .scroll_y
        .offset;
    let mut lines = verification_lines(state)
        .into_iter()
        .skip(offset)
        .take(inner.height as usize)
        .map(|(text, tone)| Line::from(Span::styled(text, tone_style(tone))))
        .collect::<Vec<_>>();

    if lines.len() < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Enter", crate::tui::theme::current().accent()),
            Span::raw(" 处理分区  "),
            Span::styled("j/k", crate::tui::theme::current().muted()),
            Span::raw(" 移动  "),
            Span::styled("h/l", crate::tui::theme::current().muted()),
            Span::raw(" 激活列  "),
            Span::styled("Tab/Ctrl-w", crate::tui::theme::current().muted()),
            Span::raw(" 切换窗口"),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

pub(super) fn draw_post_restore_result(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        return;
    };

    let needs_attention = outcome.layout.is_err()
        || !outcome.assessment.issues.is_empty()
        || outcome
            .assessment
            .partitions
            .iter()
            .any(|partition| partition.state != PostRestorePartitionState::Usable);
    let (status, tone) = if needs_attention {
        (
            "⚠ 元数据恢复成功 · 需要后续处理",
            crate::tui::ui::ResultTone::Warning,
        )
    } else {
        (
            "✓ 元数据恢复成功",
            crate::tui::ui::ResultTone::Success,
        )
    };
    let detail = format!(
        "disk{} · {} 个分区 · 文件数据未恢复 · Esc 完成",
        wizard.disk,
        outcome.assessment.partitions.len()
    );
    let hero = crate::tui::result_workbench::ResultHero::new(
        "恢复结果",
        status,
        detail,
        tone,
    );
    let slots = crate::tui::result_workbench::render_result_workbench_shell(
        frame,
        area,
        &wizard.post_restore_workbench,
        &hero,
    );

    for slot in slots {
        match slot.pane {
            PaneId::ResultPartitions => {
                render_partition_pane(frame, slot.area, state, slot.focused)
            }
            PaneId::ResultDiskLayout => {
                render_layout_pane(frame, slot.area, state, slot.focused)
            }
            PaneId::ResultVerification => {
                render_verification_pane(frame, slot.area, state, slot.focused)
            }
            _ => {}
        }
    }
}
