use super::*;
use crate::tui::{pane::PaneId, ui::ViewportClass};

pub(super) fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let class = ViewportClass::for_width(area.width);
    let focus = state.devices_focused_pane();
    if class == ViewportClass::Compact && focus == PaneId::DevicesSummary {
        draw_device_detail(frame, area, state);
        return;
    }
    if class == ViewportClass::Compact && focus == PaneId::DevicesStats {
        draw_device_stats(frame, area, state);
        return;
    }
    let (list_area, detail_area, stats_area) = if class == ViewportClass::Compact {
        (area, None, None)
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
            .split(area);
        if class == ViewportClass::Standard {
            (rows[0], Some(rows[1]), None)
        } else {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
                .split(rows[1]);
            (rows[0], Some(columns[0]), Some(columns[1]))
        }
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
                Line::from("Tab / Shift-Tab 切换工作区。"),
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
        let table_title = format!("{title} · h/l 横向滚动 · {}", viewport.position_label());
        let table = crate::tui::ui::data_table(
            &table_title,
            header,
            rows,
            viewport.widths(),
            focus == PaneId::DevicesList,
        );
        let mut table_state = TableState::default();
        table_state.select(Some(state.selected().saturating_sub(window_start)));
        frame.render_stateful_widget(table, list_area, &mut table_state);
    }

    if let Some(detail_area) = detail_area {
        if class == ViewportClass::Standard && focus == PaneId::DevicesStats {
            draw_device_stats(frame, detail_area, state);
        } else {
            draw_device_detail(frame, detail_area, state);
        }
    }
    if let Some(stats_area) = stats_area {
        draw_device_stats(frame, stats_area, state);
    }
}

fn draw_device_detail(frame: &mut Frame, detail_area: ratatui::layout::Rect, state: &AppState) {
    let detail = if let Some(row) = state.selected_device() {
        let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
        let cells = identity.display_cells();
        let device_id = identity.device_id.as_deref().unwrap_or("—");
        let content_width = detail_area.width.saturating_sub(2) as usize;
        let mut lines = vec![Line::from(format!("用户  {}", safe(&cells[4])))];
        lines.extend(wrapped_field_lines("部门  ", &cells[5], content_width));
        lines.extend(vec![
            Line::from(format!("当前状态  {}", device_status(row))),
            Line::from(vec![
                Span::styled("盘型  ", muted()),
                Span::styled(safe(&cells[6]), accent().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(format!("容量  {}", safe(&cells[0]))),
            Line::from(format!("VID:PID  {}", safe(&cells[1]))),
            Line::from(format!("型号  {}", safe(&cells[2]))),
            Line::from(format!("device_id  {}", safe(device_id))),
            Line::from(format!("onlyid  {}", safe(&cells[3]))),
            Line::from(format!("介质识别  {}", safe(identity.canonical_status()))),
        ]);
        if let Some(canonical) = &identity.canonical {
            lines.extend(
                canonical
                    .evidence_lines()
                    .into_iter()
                    .take(3)
                    .map(|line| Line::from(safe(&line))),
            );
        }
        lines.extend([
            Line::from(format!("设备  disk{}", row.disk)),
            Line::from(format!("总线  {}", safe(&row.proto))),
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
                Span::raw(" 详情/制盘    "),
                Span::styled("p", accent()),
                Span::raw(" 制盘    "),
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
            Line::from("Tab / Shift-Tab  切换工作区"),
        ])
    }
    .block(crate::tui::ui::card(
        state
            .selected_device()
            .map(|row| format!("当前设备 disk{}", row.disk))
            .unwrap_or_else(|| "当前设备".into()),
        state.devices_focused_pane() == PaneId::DevicesSummary,
    ))
    .scroll((
        state.pane_viewport(PaneId::DevicesSummary).scroll_y.offset as u16,
        0,
    ))
    .wrap(Wrap { trim: false });
    frame.render_widget(detail, detail_area);
}

fn draw_device_stats(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let devices = state.devices();
    let edp = devices
        .iter()
        .filter(|row| row.provision_kind != crate::provision::DiskProvisionKind::Plain)
        .count();
    let plain = devices
        .iter()
        .filter(|row| row.provision_kind == crate::provision::DiskProvisionKind::Plain)
        .count();
    let confirmed = devices.iter().map(|row| row.n_baks).sum::<usize>();
    let possible = devices.iter().map(|row| row.n_possible_baks).sum::<usize>();
    let abnormal = devices
        .iter()
        .filter(|row| row.denied || row.probe_error.is_some())
        .count();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("已检测设备  {}", devices.len())),
            Line::from(format!("EDP 设备  {edp}")),
            Line::from(format!("Plain  {plain}")),
            Line::from(format!("已确认备份  {confirmed}")),
            Line::from(format!("可能相关备份  {possible}")),
            Line::from(format!("健康异常  {abnormal}")),
        ])
        .block(crate::tui::ui::card(
            "总体统计",
            state.devices_focused_pane() == PaneId::DevicesStats,
        ))
        .scroll((
            state.pane_viewport(PaneId::DevicesStats).scroll_y.offset as u16,
            0,
        )),
        area,
    );
}
