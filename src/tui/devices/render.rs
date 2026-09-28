use super::*;
use crate::tui::{
    pane::PaneId,
    state::{DeviceInfoNodeKey, DeviceInfoTreeNode},
    ui::ViewportClass,
};

pub(super) fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let class = ViewportClass::for_width(area.width);
    let focus = state.devices_focused_pane();

    if class == ViewportClass::Compact {
        match focus {
            PaneId::DevicesTree => draw_device_tree(frame, area, state),
            PaneId::DevicesDetail => draw_device_detail(frame, area, state),
            _ => draw_device_list(frame, area, state),
        }
        return;
    }

    let (list_percent, tree_percent) = match class {
        ViewportClass::Standard => (44, 35),
        ViewportClass::Wide | ViewportClass::UltraWide => (40, 30),
        ViewportClass::Compact => unreachable!(),
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(list_percent),
            Constraint::Percentage(100 - list_percent),
        ])
        .split(area);
    let workbench = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(tree_percent),
            Constraint::Percentage(100 - tree_percent),
        ])
        .split(rows[1]);

    draw_device_list(frame, rows[0], state);
    draw_device_tree(frame, workbench[0], state);
    draw_device_detail(frame, workbench[1], state);
}

fn draw_device_list(frame: &mut Frame, list_area: ratatui::layout::Rect, state: &AppState) {
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

fn draw_device_tree(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesTree;
    let Some(_) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("请先在设备列表中选择设备。")
                .block(crate::tui::ui::card("设备信息", focused)),
            area,
        );
        return;
    };

    let selected = state.device_info_selected_key();
    let rows = state.device_info_tree_rows();
    let lines = rows
        .iter()
        .map(|row| device_tree_line(row, selected, focused))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("设备信息", focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesTree).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn device_tree_line(
    row: &DeviceInfoTreeNode,
    selected: DeviceInfoNodeKey,
    focused: bool,
) -> Line<'static> {
    let active = row.key == selected;
    let marker = if focused && active { "▌" } else { " " };
    let disclosure = if row.expandable {
        if row.expanded { "▾" } else { "▸" }
    } else {
        " "
    };
    let branch = match row.depth {
        0 => "",
        1 => "├─ ",
        _ => "  ├─ ",
    };
    let style = if active { accent() } else { secondary() };
    let mut spans = vec![
        Span::styled(marker, style),
        Span::raw(" "),
        Span::raw("  ".repeat(row.depth as usize)),
        Span::styled(disclosure, style),
        Span::raw(" "),
        Span::styled(branch, muted()),
        Span::styled(row.label.clone(), style.add_modifier(Modifier::BOLD)),
    ];
    if let Some(value) = &row.value {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(value.clone(), muted()));
    }
    Line::from(spans)
}

fn draw_device_detail(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesDetail;
    let Some(row) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("选择设备后显示详情。")
                .block(crate::tui::ui::card("设备详情", focused)),
            area,
        );
        return;
    };

    let key = state.device_info_selected_key();
    let title = state
        .device_info_tree_rows()
        .into_iter()
        .find(|node| node.key == key)
        .map(|node| node.label)
        .unwrap_or_else(|| "设备详情".into());
    let lines = device_detail_lines(state, row, key, area.width.saturating_sub(4) as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(title.as_str(), focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn device_detail_lines(
    state: &AppState,
    row: &crate::disk_scan::Row,
    key: DeviceInfoNodeKey,
    width: usize,
) -> Vec<Line<'static>> {
    match key {
        DeviceInfoNodeKey::Identity => identity_detail_lines(row),
        DeviceInfoNodeKey::Capacity => capacity_detail_lines(state, row, width),
        DeviceInfoNodeKey::TailGroup => tail_detail_lines(row),
        DeviceInfoNodeKey::LayoutSegment { start_lba, kind } => {
            segment_detail_lines(row, start_lba, kind)
        }
        DeviceInfoNodeKey::Status => status_detail_lines(row),
        DeviceInfoNodeKey::Backups => backup_detail_lines(row),
        DeviceInfoNodeKey::Protocol => protocol_detail_lines(row),
    }
}

fn identity_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    vec![
        section_line("硬件身份"),
        field_line("设备", format!("disk{}", row.disk)),
        field_line("容量", format_bytes(row.size)),
        field_line("接口", safe(&row.proto)),
        field_line("VID:PID", safe(&cells[1])),
        field_line("序列号", safe(row.serial.as_deref().unwrap_or("—"))),
        Line::from(""),
        section_line("协议身份"),
        field_line("onlyid", safe(identity.onlyid.as_deref().unwrap_or("—"))),
        field_line(
            "device_id",
            safe(identity.device_id.as_deref().unwrap_or("—")),
        ),
        field_line("部门", safe(row.dept.as_deref().unwrap_or("—"))),
        field_line("姓名", safe(row.user.as_deref().unwrap_or("—"))),
        field_line("名称", safe(row.label.as_deref().unwrap_or("—"))),
        field_line("盘型", safe(&cells[6])),
        Line::from(""),
        section_line("身份可靠性"),
        field_line("身份依据", device_identity_basis(row)),
    ]
}

fn capacity_detail_lines(
    state: &AppState,
    row: &crate::disk_scan::Row,
    width: usize,
) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        let reason = row
            .canonical_layout()
            .err()
            .unwrap_or_else(|| "证据不足".into());
        return vec![Line::from(Span::styled(
            format!("无法建立可靠容量布局：{}", safe(&reason)),
            warning(),
        ))];
    };
    let collapsed = model.collapsed_tail_model();
    let presentation = crate::tui::disk_layout::DiskLayoutPresentation::new(
        &collapsed,
        crate::tui::disk_layout::DiskLayoutProfile::CompactHuman,
        crate::tui::disk_layout::TailExpansion::Collapsed,
    );
    let mut lines = vec![
        section_line("全盘布局"),
        presentation.bar_line(width.max(8)),
    ];
    lines.extend(presentation.compact_grid_lines(width.max(8)));
    lines.push(Line::from(""));
    lines.push(section_line("区域列表"));
    lines.push(Line::from(Span::styled(
        format!(
            "{}  {}  {}  {}",
            crate::ui::pad_to("区域", 18),
            crate::ui::pad_to("LBA 范围", 24),
            crate::ui::pad_to("容量", 14),
            "占比"
        ),
        secondary(),
    )));
    for segment in &collapsed.segments {
        lines.push(Line::from(format!(
            "{}  {}  {}  {}",
            crate::ui::pad_to(&segment.label, 18),
            crate::ui::pad_to(&segment.closed_range(), 24),
            crate::ui::pad_to(
                &format_bytes(
                    segment
                        .sector_count
                        .saturating_mul(crate::common::SECTOR as u64),
                ),
                14,
            ),
            percentage(segment.sector_count, model.total_sectors)
        )));
    }
    if state
        .device_info_tree_rows()
        .iter()
        .any(|node| node.key == DeviceInfoNodeKey::TailGroup)
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "尾部区域可直接在左侧按 o 展开，不需要进入 Inspect。",
            muted(),
        )));
    }
    lines
}

fn tail_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        return vec![Line::from(Span::styled("尾部布局证据不足", warning()))];
    };
    let Some(tail) = model.tail_group() else {
        return vec![Line::from("当前介质没有独立尾部区域。")];
    };
    let mut lines = vec![
        field_line("LBA 范围", format!("[{}..{}]", tail.start_lba, tail.end_exclusive - 1)),
        field_line(
            "容量",
            format_bytes(
                (tail.end_exclusive - tail.start_lba)
                    .saturating_mul(crate::common::SECTOR as u64),
            ),
        ),
        Line::from(""),
        section_line("尾部结构"),
    ];
    for child in tail.children {
        lines.push(Line::from(format!(
            "{}  {}  {}",
            crate::ui::pad_to(&child.label, 20),
            crate::ui::pad_to(&child.closed_range(), 24),
            format_bytes(
                child
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            )
        )));
    }
    lines
}

fn segment_detail_lines(
    row: &crate::disk_scan::Row,
    start_lba: u64,
    kind: crate::disk_layout::DiskRegionKind,
) -> Vec<Line<'static>> {
    let Ok(model) = row.canonical_layout() else {
        return vec![Line::from(Span::styled("区域布局证据不足", warning()))];
    };
    let Some(segment) = model
        .segments
        .iter()
        .find(|segment| segment.start_lba == start_lba && segment.kind == kind)
    else {
        return vec![Line::from(Span::styled(
            "所选区域已不在当前设备布局中。",
            warning(),
        ))];
    };
    vec![
        field_line("区域类型", segment.kind.label()),
        field_line("LBA 范围", segment.closed_range()),
        field_line("起始 LBA", segment.start_lba.to_string()),
        field_line(
            "结束 LBA",
            segment
                .start_lba
                .saturating_add(segment.sector_count)
                .saturating_sub(1)
                .to_string(),
        ),
        field_line("扇区数量", segment.sector_count.to_string()),
        field_line(
            "容量",
            format_bytes(
                segment
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            ),
        ),
        field_line(
            "占比",
            percentage(segment.sector_count, model.total_sectors),
        ),
        Line::from(""),
        Line::from(Span::styled(
            "需要逐扇区、Hex 或字段证据时按 i 进入 Inspect。",
            muted(),
        )),
    ]
}

fn status_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let mut lines = vec![
        field_line("总体状态", device_status(row)),
        field_line("接口", safe(&row.proto)),
        field_line(
            "介质类型",
            row.confirmed_provision_kind()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "未确认".into()),
        ),
        field_line(
            "容量布局",
            if row.canonical_layout().is_ok() {
                "完整"
            } else {
                "证据不足"
            },
        ),
        field_line(
            "身份依据",
            device_identity_basis(row),
        ),
        field_line("确认备份", format!("{} 份", row.n_baks)),
        field_line("可能相关", format!("{} 份", row.n_possible_baks)),
    ];
    if let Some(error) = &row.probe_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("读取异常：{}", safe(error)),
            warning(),
        )));
    } else if row.denied {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "需要管理员权限读取原始设备。",
            warning(),
        )));
    }
    lines
}

fn backup_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    vec![
        section_line("当前设备相关备份"),
        field_line("已确认", format!("{} 份", row.n_baks)),
        field_line("可能相关", format!("{} 份", row.n_possible_baks)),
        Line::from(""),
        Line::from(Span::styled(
            "本页只展示当前已加载的设备关联证据；全局备份管理请切换到“备份”Tab。",
            muted(),
        )),
    ]
}

fn protocol_detail_lines(row: &crate::disk_scan::Row) -> Vec<Line<'static>> {
    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    vec![
        field_line("介质类型", safe(&cells[6])),
        field_line(
            "分区记录",
            row.partitions
                .as_ref()
                .map(|parts| format!("{} 条", parts.len()))
                .unwrap_or_else(|| "未读取".into()),
        ),
        field_line(
            "LCE",
            if row.lce.is_some() { "已确认" } else { "未确认" },
        ),
        field_line("部门", safe(row.dept.as_deref().unwrap_or("—"))),
        field_line("姓名", safe(row.user.as_deref().unwrap_or("—"))),
        field_line("名称", safe(row.label.as_deref().unwrap_or("—"))),
        field_line("onlyid", safe(row.onlyid.as_deref().unwrap_or("—"))),
        Line::from(""),
        Line::from(Span::styled(
            "完整 LBA0～12 / LCE / raw / 字段解析请按 i 进入 Inspect。",
            muted(),
        )),
    ]
}

fn section_line(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        secondary().add_modifier(Modifier::BOLD),
    ))
}

fn field_line(label: &'static str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 14), muted()),
        Span::raw(value.into()),
    ])
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.2} kB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn percentage(sectors: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".into();
    }
    let ratio = sectors as f64 * 100.0 / total as f64;
    if ratio > 0.0 && ratio < 0.01 {
        "<0.01%".into()
    } else {
        format!("{ratio:.2}%")
    }
}

fn device_identity_basis(row: &crate::disk_scan::Row) -> &'static str {
    use crate::application::media_identity::SerialQuality;

    match row
        .identity_pin
        .as_ref()
        .map(|pin| pin.snapshot.hardware.serial_quality)
    {
        Some(SerialQuality::Usable) => "强 · 硬件序列号 + VID:PID + 容量",
        Some(SerialQuality::Suspicious) => "中 · 序列号可疑，结合 VID:PID + 容量",
        Some(SerialQuality::Missing) if row.device_id.is_some() && row.onlyid.is_some() => {
            "中 · EDP device_id + onlyid + 硬件特征"
        }
        Some(SerialQuality::Missing) => "弱 · 仅硬件型号/容量等非唯一特征",
        None if row.serial.is_some() => "待确认 · 已读取序列号，身份快照未建立",
        None if row.device_id.is_some() || row.onlyid.is_some() => "待确认 · 仅协议身份可用",
        None => "未建立可靠身份依据",
    }
}
