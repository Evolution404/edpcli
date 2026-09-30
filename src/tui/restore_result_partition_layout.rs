use super::*;
use crate::application::post_restore::PostRestorePartition;
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
};

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
        PostRestorePartitionState::Unsupported => ("暂不支持", crate::tui::ui::ResultTone::Muted),
    }
}

pub(super) fn render_partition_pane(
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

    use crate::tui::table_layout::{
        render_table_scrollbars, table_heading, visible_cell, TableKind,
    };
    let theme = crate::tui::theme::current();
    let kind = TableKind::ResultPartitions;
    let headings = [
        "分区",
        "状态",
        "文件系统",
        "LBA 范围",
        "容量",
        "密钥",
        "说明",
    ];
    let Some(view) = state.result_partition_table_view() else {
        return;
    };
    let order = state.table_column_order(kind);
    let layout = state.table_visual_layout(kind);
    let visual_widths = state.table_visual_widths(kind, &view.content_widths);
    let interaction = state.table_interaction(kind);
    let viewport = layout.layout_with_active(
        inner.width.saturating_sub(1),
        &visual_widths,
        interaction.viewport_offset(),
        Some(interaction.active_column()),
    );
    let header = Row::new(viewport.columns.iter().map(|column| {
        let logical = order[column.index];
        let label = table_heading(headings[logical], logical, interaction);
        Cell::from(visible_cell(&label, column))
            .style(theme.table_header(column.index == interaction.active_column(), focused))
    }));
    let selected = wizard.post_restore_workbench.selected_partition;
    let visible_sources = state.visible_result_partition_indices();
    let rows = visible_sources.iter().filter_map(|index| {
        outcome.assessment.partitions.get(*index).map(|partition| {
            let (status, status_tone) = partition_status(partition);
            let selected_row = selected == Some(*index);
            Row::new(
                viewport
                    .columns
                    .iter()
                    .map(|column| {
                        let logical = order[column.index];
                        let value = view.rows[*index].get(logical).cloned().unwrap_or_default();
                        let base = match logical {
                            1 => {
                                let _ = status;
                                tone_style(status_tone)
                            }
                            5 | 6 => theme.table_text_muted(),
                            _ => theme.table_text(),
                        };
                        let style = theme.table_cell(
                            base,
                            column.index == interaction.active_column(),
                            focused,
                        );
                        let style = theme.apply_selection(style, selected_row, focused);
                        Cell::from(visible_cell(&value, column)).style(style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
    });

    frame.render_widget(
        Table::new(rows, viewport.widths())
            .header(header)
            .column_spacing(1),
        inner,
    );
    render_table_scrollbars(
        frame,
        area,
        &viewport,
        visible_sources.len(),
        0,
        visible_sources
            .len()
            .min(inner.height.saturating_sub(1) as usize),
    );
}

pub(super) fn render_layout_pane(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
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
