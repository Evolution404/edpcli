use super::*;

pub(super) fn draw_provision_selection(
    frame: &mut Frame,
    main_area: ratatui::layout::Rect,
    state: &AppState,
) {
    use crate::tui::table_layout::{
        display_width, render_table_scrollbars, table_heading, table_position_label, visible_cell,
        TableKind,
    };
    let headings = ["设备", "容量", "USB 身份", "盘型", "onlyid"];
    let values = (0..state.item_count())
        .filter_map(|index| {
            let row = state.provision_device_at(index)?;
            Some(vec![
                format!("disk{}", row.disk),
                crate::common::fmt_capacity(row.size),
                format!("{}:{}", safe(&row.vid), safe(&row.pid)),
                row.confirmed_provision_kind()
                    .map(|kind| kind.full_name().to_string())
                    .unwrap_or_else(|| "未知 / 未确认".into()),
                safe(row.onlyid.as_deref().unwrap_or("—")),
            ])
        })
        .collect::<Vec<_>>();
    let mut content_widths = headings.map(display_width);
    for row in &values {
        for (index, value) in row.iter().enumerate() {
            content_widths[index] = content_widths[index].max(display_width(value));
        }
    }
    let order = state.table_column_order(TableKind::ProvisionDevices);
    let layout = state.table_visual_layout(TableKind::ProvisionDevices);
    let visual_widths = state.table_visual_widths(TableKind::ProvisionDevices, &content_widths);
    let interaction = state.table_interaction(TableKind::ProvisionDevices);
    let viewport = layout.layout_with_active(
        main_area.width.saturating_sub(4),
        &visual_widths,
        interaction.viewport_offset(),
        Some(interaction.active_column()),
    );
    let row_total = values.len();
    let window = visible_window(state.selected(), row_total, main_area.height);
    let row_start = window.start;
    let row_visible = window.len();
    let rows = window.filter_map(|index| {
        let row = values.get(index)?;
        let source = state.provision_device_at(index)?;
        let kind = source.confirmed_provision_kind();
        Some(TableRow::new(
            viewport
                .columns
                .iter()
                .map(|column| {
                    let logical = order[column.index];
                    let base = if logical == 3 {
                        kind.map(|kind| crate::tui::theme::current().provision_kind(kind))
                            .unwrap_or_else(warning)
                    } else {
                        Style::default()
                    };
                    let style = crate::tui::theme::current().table_cell(
                        base,
                        column.index == interaction.active_column(),
                        true,
                    );
                    Cell::from(visible_cell(&row[logical], column)).style(style)
                })
                .collect::<Vec<_>>(),
        ))
    });
    let title = format!(
        "制盘 · 选择 USB 目标 · {}",
        table_position_label(&layout, interaction, &viewport)
    );
    let header = TableRow::new(
        viewport
            .columns
            .iter()
            .map(|column| {
                let logical = order[column.index];
                let label = table_heading(headings[logical], logical, interaction);
                Cell::from(visible_cell(&label, column)).style(
                    crate::tui::theme::current()
                        .table_header(column.index == interaction.active_column(), true),
                )
            })
            .collect::<Vec<_>>(),
    );
    let table = crate::tui::ui::data_table(&title, header, rows, viewport.widths(), true);
    let mut table_state = ratatui::widgets::TableState::default();
    if row_total > 0 {
        table_state.select(Some(state.selected().saturating_sub(row_start)));
    }
    frame.render_stateful_widget(table, main_area, &mut table_state);
    render_table_scrollbars(
        frame,
        main_area,
        &viewport,
        row_total,
        row_start,
        row_visible,
    );
}
