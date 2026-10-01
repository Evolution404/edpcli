use super::review_render_style::action_style;
use super::*;
use crate::tui::state::{ProvisionConfirmationDataEffect, ProvisionConfirmationViewModel};

pub(super) fn draw_partition_plan(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    view: &ProvisionConfirmationViewModel,
    selected: usize,
    focused: bool,
) {
    let visible_capacity = area.height.saturating_sub(3).max(1) as usize;
    let window_len = visible_capacity.min(view.regions.len().max(1));
    let max_start = view.regions.len().saturating_sub(window_len);
    let window_start = selected.saturating_sub(window_len / 2).min(max_start);
    let window_end = window_start
        .saturating_add(window_len)
        .min(view.regions.len());

    let (header, widths, rows) = if area.width >= 100 {
        wide_rows(view, window_start, window_end)
    } else if area.width >= 72 {
        normal_rows(view, window_start, window_end)
    } else {
        compact_rows(view, window_start, window_end)
    };

    let title = format!(
        "分区执行计划 · {}/{}",
        if view.regions.is_empty() {
            0
        } else {
            selected + 1
        },
        view.regions.len()
    );
    let table = crate::tui::ui::data_table(&title, header, rows, widths, focused);
    let mut table_state = TableState::default();
    if !view.regions.is_empty() {
        table_state.select(Some(selected.saturating_sub(window_start)));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn wide_rows(
    view: &ProvisionConfirmationViewModel,
    start: usize,
    end: usize,
) -> (TableRow<'static>, Vec<Constraint>, Vec<TableRow<'static>>) {
    let rows = view.regions[start..end]
        .iter()
        .map(|region| {
            TableRow::new([
                Cell::from(safe(&region.label)),
                Cell::from(region.selection.start_lba.to_string()).style(muted()),
                Cell::from(AppState::format_sector_size(region.sector_count)),
                Cell::from(region.action.label()).style(action_style(region.action)),
                Cell::from(region.data_effect.label()).style(data_style(region.data_effect)),
                Cell::from(region.filesystem_effect.label()).style(muted()),
            ])
        })
        .collect();
    (
        TableRow::new(["区域", "起点 LBA", "容量", "动作", "数据", "文件系统"]),
        vec![
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Min(12),
        ],
        rows,
    )
}

fn normal_rows(
    view: &ProvisionConfirmationViewModel,
    start: usize,
    end: usize,
) -> (TableRow<'static>, Vec<Constraint>, Vec<TableRow<'static>>) {
    let rows = view.regions[start..end]
        .iter()
        .map(|region| {
            TableRow::new([
                Cell::from(safe(&region.label)),
                Cell::from(AppState::format_sector_size(region.sector_count)),
                Cell::from(region.action.label()).style(action_style(region.action)),
                Cell::from(region.data_effect.label()).style(data_style(region.data_effect)),
                Cell::from(region.filesystem_effect.label()).style(muted()),
            ])
        })
        .collect();
    (
        TableRow::new(["区域", "容量", "动作", "数据", "文件系统"]),
        vec![
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Min(12),
        ],
        rows,
    )
}

fn compact_rows(
    view: &ProvisionConfirmationViewModel,
    start: usize,
    end: usize,
) -> (TableRow<'static>, Vec<Constraint>, Vec<TableRow<'static>>) {
    let rows = view.regions[start..end]
        .iter()
        .map(|region| {
            TableRow::new([
                Cell::from(safe(&region.label)),
                Cell::from(region.action.label()).style(action_style(region.action)),
                Cell::from(region.data_effect.label()).style(data_style(region.data_effect)),
            ])
        })
        .collect();
    (
        TableRow::new(["区域", "动作", "数据"]),
        vec![
            Constraint::Percentage(36),
            Constraint::Percentage(40),
            Constraint::Percentage(24),
        ],
        rows,
    )
}

fn data_style(effect: ProvisionConfirmationDataEffect) -> Style {
    match effect {
        ProvisionConfirmationDataEffect::Clear => warning(),
        ProvisionConfirmationDataEffect::Migrate => accent(),
        _ => success(),
    }
}
