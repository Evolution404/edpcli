use super::*;

pub(super) fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let (list_area, sidebar) = if area.width >= 150 {
        workspace_sidebar_layout(area)
    } else {
        (area, None)
    };
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
                "没有匹配记录",
                "当前搜索条件没有匹配任何设备。",
                "继续输入可实时更新；清空搜索词后恢复全部设备。",
            )
        } else {
            (
                "暂无设备数据",
                "未检测到符合条件的存储设备。",
                "按 r 刷新设备；插入 U 盘后可再次扫描。",
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
                Line::from(""),
                Line::from("Tab / Shift-Tab 切换设备与备份标签。"),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            inner,
        );
    } else {
        use crate::tui::table_layout::{
            layout_for, table_column_schema, truncate_cell, ColumnId, TableKind,
        };
        let columns = table_column_schema(TableKind::Devices).expect("device schema");
        let headings = columns
            .iter()
            .map(|column| column.heading)
            .collect::<Vec<_>>();
        let view = state
            .table_view_data(TableKind::Devices)
            .expect("device view data");
        let layout = layout_for(TableKind::Devices);
        let viewport = layout.layout(
            list_area.width.saturating_sub(4),
            &view.content_widths,
            state.table_scroll_offset(TableKind::Devices),
        );
        let window = visible_window(state.selected(), visible_count, list_area.height);
        let window_start = window.start;
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
                            let value = &values[column.index];
                            let style = match columns[column.index].id {
                                ColumnId::Device | ColumnId::ProvisionKind => accent(),
                                ColumnId::VidPid => secondary(),
                                ColumnId::Bus => {
                                    if row.proto == "USB" {
                                        success()
                                    } else {
                                        warning()
                                    }
                                }
                                ColumnId::State => device_status_style(row),
                                _ => Style::default(),
                            };
                            Cell::from(truncate_cell(
                                value,
                                usize::from(column.width),
                                column.truncate_policy,
                            ))
                            .style(style)
                        })
                        .collect::<Vec<_>>(),
                )
            });
        let header = TableRow::new(
            viewport
                .columns
                .iter()
                .map(|column| {
                    truncate_cell(
                        headings[column.index],
                        usize::from(column.width),
                        column.truncate_policy,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .style(accent());
        let table = Table::new(rows, viewport.widths())
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(focused_panel())
                    .title(format!(
                        "{title} · h/l 横向滚动 · {}",
                        viewport.position_label()
                    ))
                    .title_style(secondary()),
            )
            .row_highlight_style(selected())
            .highlight_symbol("▌ ");
        let mut table_state = TableState::default();
        table_state.select(Some(state.selected().saturating_sub(window_start)));
        frame.render_stateful_widget(table, list_area, &mut table_state);
    }

    if let Some((detail_area, animation_area)) = sidebar {
        let detail = if let Some(row) = state.selected_device() {
            let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
            let cells = identity.display_cells();
            let device_id = identity.device_id.as_deref().unwrap_or("—");
            let content_width = detail_area.width.saturating_sub(2) as usize;
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("盘型  ", muted()),
                    Span::styled(safe(&cells[6]), accent().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(format!("容量  {}", safe(&cells[0]))),
                Line::from(format!("VID:PID  {}", safe(&cells[1]))),
                Line::from(format!("型号  {}", safe(&cells[2]))),
                Line::from(format!("device_id  {}", safe(device_id))),
                Line::from(format!("onlyid  {}", safe(&cells[3]))),
                Line::from(format!("姓名  {}", safe(&cells[4]))),
            ];
            lines.extend(wrapped_field_lines("部门  ", &cells[5], content_width));
            lines.extend([
                Line::from(format!("设备  disk{}", row.disk)),
                Line::from(format!("总线  {}", safe(&row.proto))),
                Line::from(format!("当前状态  {}", device_status(row))),
                Line::from(match row.n_possible_baks {
                    0 => format!("已有备份  {} 份", row.n_baks),
                    possible => format!("已有备份  {} 份 · 可能相关 {} 份", row.n_baks, possible),
                }),
                Line::from(""),
                Line::from(Span::styled(
                    "可用操作",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled("Enter", accent()),
                    Span::raw(" 制盘      "),
                    Span::styled("i", accent()),
                    Span::raw(" Inspect"),
                ]),
                Line::from(vec![
                    Span::styled("b", accent()),
                    Span::raw(" 新建备份  "),
                    Span::styled("r", success()),
                    Span::raw(" 刷新"),
                ]),
            ]);
            Paragraph::new(lines)
        } else {
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "操作与信息",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("选择设备后，这里会显示身份、所属人员、备份数量和可用操作。"),
                Line::from(""),
                Line::from("r  刷新设备"),
                Line::from("/  搜索设备"),
                Line::from("Tab / Shift-Tab  切换设备 / 备份标签"),
            ])
        }
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("设备详情")
                .title_style(secondary()),
        )
        .wrap(Wrap { trim: false });
        frame.render_widget(detail, detail_area);
        draw_workspace_animation(frame, animation_area, state, "DEVICE WORKSPACE");
    }
}
