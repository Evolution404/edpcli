use super::*;
use crate::tui::state::{ProvisionResultPartition, ProvisionState};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Cell, Paragraph, Row, Table},
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

    let theme = crate::tui::theme::current();
    let active_column = state
        .provision()
        .result_workbench
        .partition_active_column(crate::tui::result_workbench::RESULT_PARTITION_COLUMN_COUNT);
    let header = Row::new(
        [
            "分区",
            "角色",
            "文件系统",
            "LBA 范围",
            "容量",
            "处理方式",
            "最终状态",
        ]
        .into_iter()
        .enumerate()
        .map(|(column, label)| {
            Cell::from(label).style(theme.table_header(column == active_column, focused))
        }),
    );

    let selected = state.provision().result_workbench.selected_partition;
    let rows = plan
        .partitions
        .iter()
        .enumerate()
        .map(|(index, partition)| {
            let end = partition.end_exclusive().saturating_sub(1);
            let filesystem = partition
                .filesystem
                .map(|kind| kind.config_token().to_string())
                .unwrap_or_else(|| "—".into());
            let action = if partition.selected_for_format {
                "格式化"
            } else {
                partition
                    .disposition
                    .map(disposition_label)
                    .unwrap_or("写入")
            };
            let (final_status, tone) = partition_final_status(state.provision(), plan, partition);
            let selected_row = selected == Some(index);
            let cell_style = |base: Style, column: usize| {
                theme.table_cell(base, column == active_column, focused)
            };
            Row::new(vec![
                Cell::from(format!("P{}", index + 1)).style(cell_style(theme.table_text(), 0)),
                Cell::from(
                    partition
                        .role
                        .map(crate::provision::PartitionRole::label)
                        .unwrap_or("普通分区"),
                )
                .style(cell_style(theme.table_text(), 1)),
                Cell::from(filesystem).style(cell_style(theme.table_text(), 2)),
                Cell::from(format!("{}..={end}", partition.start_lba))
                    .style(cell_style(theme.table_text(), 3)),
                Cell::from(crate::common::fmt_capacity(partition.size_bytes))
                    .style(cell_style(theme.table_text(), 4)),
                Cell::from(action).style(cell_style(theme.table_text(), 5)),
                Cell::from(final_status).style(cell_style(tone_style(tone), 6)),
            ])
            .style(theme.apply_selection(theme.table_text(), selected_row, focused))
        });

    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(22),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Min(18),
            ],
        )
        .header(header)
        .column_spacing(1),
        inner,
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
