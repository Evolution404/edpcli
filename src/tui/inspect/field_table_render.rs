use super::inspect_field_status_style;
use super::*;
use crate::application::inspect::AdvancedInspectItem;

pub(super) fn draw_inspect_field_table(
    frame: &mut Frame,
    state: &AppState,
    item: &AdvancedInspectItem,
    detail_area: ratatui::layout::Rect,
    detail_focus: bool,
    detail_offset: usize,
) {
    use crate::tui::table_layout::{
        display_width, render_table_scrollbars, table_heading, table_position_label, visible_cell,
        TableKind,
    };

    let headings = crate::tui::state::INSPECT_DETAIL_HEADINGS;
    let values = state.advanced_inspect_detail_rows();
    let mut content_widths = headings.map(display_width);
    for row in &values {
        for (index, value) in row.cells.iter().enumerate() {
            content_widths[index] = content_widths[index].max(display_width(value));
        }
    }
    let order = state.table_column_order(TableKind::InspectFields);
    let layout = state.table_visual_layout(TableKind::InspectFields);
    let visual_widths = state.table_visual_widths(TableKind::InspectFields, &content_widths);
    let interaction = state.table_interaction(TableKind::InspectFields);
    let viewport = layout.layout_with_active(
        detail_area.width.saturating_sub(3),
        &visual_widths,
        interaction.viewport_offset(),
        Some(interaction.active_column()),
    );
    let visible_rows = detail_area.height.saturating_sub(3).max(1) as usize;
    let row_start = detail_offset.min(values.len().saturating_sub(1));
    let row_end = row_start.saturating_add(visible_rows).min(values.len());
    let selected = state
        .pane_viewport(crate::tui::pane::PaneId::InspectDetail)
        .selected
        .unwrap_or(0);
    let rows = values[row_start..row_end]
        .iter()
        .enumerate()
        .map(|(index, row)| {
            TableRow::new(
                viewport
                    .columns
                    .iter()
                    .map(|column| {
                        let logical = order[column.index];
                        Cell::from(visible_cell(&safe(&row.cells[logical]), column)).style(
                            if column.index == interaction.active_column() {
                                Modifier::BOLD.into()
                            } else {
                                Style::default()
                            },
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .style(if row_start + index == selected {
                accent().add_modifier(Modifier::REVERSED)
            } else {
                inspect_field_status_style(item.fields[row.field_index].status)
            })
        });
    let header = TableRow::new(
        viewport
            .columns
            .iter()
            .map(|column| {
                let logical = order[column.index];
                let label = table_heading(headings[logical], logical, interaction);
                Cell::from(visible_cell(&label, column)).style(
                    if column.index == interaction.active_column() {
                        accent().add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        secondary().add_modifier(Modifier::BOLD)
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(
        Table::new(rows, viewport.widths()).header(header).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if detail_focus {
                    focused_panel()
                } else {
                    panel()
                })
                .title(format!(
                    "字段详情 · 行 {}–{} / {} · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认 · {}",
                    if values.is_empty() { 0 } else { row_start + 1 },
                    row_end,
                    values.len(),
                    table_position_label(&layout, interaction, &viewport)
                )),
        ),
        detail_area,
    );
    render_table_scrollbars(
        frame,
        detail_area,
        &viewport,
        values.len(),
        row_start,
        row_end.saturating_sub(row_start),
    );
}
