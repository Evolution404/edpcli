use super::*;

pub(super) fn draw_device_list(frame: &mut Frame, list_area: ratatui::layout::Rect, state: &AppState) {
    let visible_count = state.visible_device_count();
    let total_count = state.devices().len();
    let count_label = if state.workspace_filter_active() {
        format!("{visible_count}/{total_count}")
    } else {
        total_count.to_string()
    };
    let title = if state.device_scan_pending() {
        format!("设备列表 ({count_label}) · 扫描中…")
    } else {
        format!("设备列表 ({count_label})")
    };

    if visible_count == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(secondary());
        let inner = block.inner(list_area);
        frame.render_widget(block, list_area);
        let (heading, message, hint) = if state.workspace_filter_active() {
            (
                "没有匹配设备",
                "当前搜索条件没有匹配任何设备。",
                "Esc 清除当前搜索条件。",
            )
        } else if state.device_scan_pending() {
            (
                "正在扫描设备",
                "正在读取外接存储设备及身份信息。",
                "扫描完成后列表会自动更新。",
            )
        } else {
            (
                "未发现可用设备",
                "当前没有检测到外接存储设备。",
                "按 r 刷新；插入 U 盘后可再次扫描。",
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    heading,
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message),
                Line::from(hint),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    use crate::tui::table_layout::{
        render_table_scrollbars, table_column_schema, table_heading, table_position_label,
        visible_cell, ColumnId, TableKind,
    };
    let columns = table_column_schema(TableKind::Devices).expect("device schema");
    let headings = columns
        .iter()
        .map(|column| column.heading)
        .collect::<Vec<_>>();
    let view = state
        .table_view_data(TableKind::Devices)
        .expect("device view data");
    let order = state.table_column_order(TableKind::Devices);
    let layout = state.table_visual_layout(TableKind::Devices);
    let visual_widths = state.table_visual_widths(TableKind::Devices, &view.content_widths);
    let interaction = state.table_interaction(TableKind::Devices);
    let viewport = layout.layout_with_active(
        list_area.width.saturating_sub(4),
        &visual_widths,
        interaction.viewport_offset(),
        Some(interaction.active_column()),
    );
    let window = visible_window(state.selected(), visible_count, list_area.height);
    let window_start = window.start;
    let window_len = window.len();
    let rows = window
        .filter_map(|position| state.device_source_index_at_visible(position))
        .map(|index| {
            let row = &state.devices()[index];
            let values = &view.rows[index];
            TableRow::new(
                viewport
                    .columns
                    .iter()
                    .map(|column| {
                        let logical = order[column.index];
                        let value = &values[logical];
                        let style = match columns[logical].id {
                            ColumnId::Device | ColumnId::ProvisionKind => accent(),
                            ColumnId::State => device_status_style(row),
                            ColumnId::Backups => secondary(),
                            ColumnId::Model => muted(),
                            _ => Style::default(),
                        };
                        let style = if column.index == interaction.active_column() {
                            style.add_modifier(Modifier::BOLD)
                        } else {
                            style
                        };
                        Cell::from(visible_cell(value, column)).style(style)
                    })
                    .collect::<Vec<_>>(),
            )
        });
    let header = TableRow::new(
        viewport
            .columns
            .iter()
            .map(|column| {
                let logical = order[column.index];
                let label = table_heading(headings[logical], logical, interaction);
                let style = if column.index == interaction.active_column() {
                    accent().add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    secondary().add_modifier(Modifier::BOLD)
                };
                Cell::from(visible_cell(&label, column)).style(style)
            })
            .collect::<Vec<_>>(),
    );
    let table_title = format!(
        "{title} · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认 · {}",
        table_position_label(&layout, interaction, &viewport)
    );
    let table = crate::tui::ui::data_table(
        &table_title,
        header,
        rows,
        viewport.widths(),
        state.devices_focused_pane() == PaneId::DevicesList,
    );
    let mut table_state = TableState::default();
    table_state.select(Some(state.selected().saturating_sub(window_start)));
    frame.render_stateful_widget(table, list_area, &mut table_state);
    render_table_scrollbars(
        frame,
        list_area,
        &viewport,
        visible_count,
        window_start,
        window_len,
    );
}

