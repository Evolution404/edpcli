use super::*;
use crate::tui::state::{ProvisionResultPartition, ProvisionState};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Cell, Paragraph, Row, Table},
};

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

fn partition_final_status(
    provision: &ProvisionState,
    plan: &crate::tui::state::ProvisionResultSnapshot,
    partition: &ProvisionResultPartition,
) -> (String, crate::tui::ui::ResultTone) {
    let outcome = provision.result_outcome.as_ref();
    if plan.target == crate::provision::ProvisionTarget::Plain {
        return if outcome.is_some() {
            (
                "已写入 · 读回通过".into(),
                crate::tui::ui::ResultTone::Success,
            )
        } else {
            ("未确认".into(), crate::tui::ui::ResultTone::Warning)
        };
    }

    if partition.selected_for_format {
        if let (
            Some(role),
            Some(crate::application::provision::ProvisionCommitOutcome::Official(report)),
        ) = (partition.role, outcome.map(|value| &value.commit))
        {
            if let Some(format) = report.formats.iter().find(|item| item.role == role) {
                return if format.result.is_ok() {
                    (
                        "已格式化 · 读回通过".into(),
                        crate::tui::ui::ResultTone::Success,
                    )
                } else {
                    ("格式化失败".into(), crate::tui::ui::ResultTone::Warning)
                };
            }
        }
        return ("已写入".into(), crate::tui::ui::ResultTone::Primary);
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
    let Some(plan) = state.provision().result_plan.as_ref() else {
        frame.render_widget(Paragraph::new("没有可显示的分区结果"), inner);
        return;
    };

    use crate::tui::table_layout::{
        render_table_scrollbars, table_heading, visible_cell, TableKind,
    };
    let theme = crate::tui::theme::current();
    let kind = TableKind::ResultPartitions;
    let headings = ["分区", "角色", "文件系统", "容量", "处理方式", "最终状态"];
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
    let selected = state.provision().result_workbench.selected_partition;
    let visible_sources = state.visible_result_partition_indices();
    let rows = visible_sources.iter().filter_map(|index| {
        plan.partitions.get(*index).map(|partition| {
            let (final_status, tone) = partition_final_status(state.provision(), plan, partition);
            Row::new(
                viewport
                    .columns
                    .iter()
                    .map(|column| {
                        let logical = order[column.index];
                        let value = view.rows[*index].get(logical).cloned().unwrap_or_default();
                        let base = if logical == 5 {
                            let _ = &final_status;
                            tone_style(tone)
                        } else {
                            theme.table_text()
                        };
                        let style = theme.table_cell(
                            base,
                            column.index == interaction.active_column(),
                            focused,
                        );
                        Cell::from(visible_cell(&value, column)).style(style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
    });

    frame.render_widget(
        Table::new(rows, viewport.widths())
            .header(header)
            .column_spacing(0),
        inner,
    );
    if let Some(visual_row) = selected.and_then(|selected| {
        visible_sources
            .iter()
            .position(|source| *source == selected)
    }) {
        let row_y = inner.y.saturating_add(1).saturating_add(visual_row as u16);
        if row_y < inner.bottom() && inner.width > 1 {
            frame.render_widget(
                Block::default().style(theme.selection_overlay(focused)),
                Rect::new(inner.x, row_y, inner.width.saturating_sub(1), 1),
            );
        }
    }
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
    let Some(plan) = state.provision().result_plan.as_ref() else {
        frame.render_widget(
            Paragraph::new("无法重建结果布局").block(crate::tui::ui::card("全盘布局", focused)),
            area,
        );
        return;
    };
    let Ok(model) = plan.disk_layout_model() else {
        frame.render_widget(
            Paragraph::new("结果布局不可用").block(crate::tui::ui::card("全盘布局", focused)),
            area,
        );
        return;
    };

    let outer = crate::tui::ui::card("全盘布局", focused);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

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
    let lines = crate::tui::disk_layout::DiskCapacityMap::new(&model, profile)
        .with_tail(crate::tui::disk_layout::TailExpansion::Collapsed)
        .with_selection(state.provision().result_workbench.region_selection())
        .with_marker(true)
        .lines(parts[0].width as usize);
    frame.render_widget(Paragraph::new(lines), parts[0]);
    crate::tui::result_workbench::render_result_region_list(
        frame,
        parts[2],
        &state.provision().result_workbench,
        &model,
        focused,
    );
}
