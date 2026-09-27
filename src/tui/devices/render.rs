use super::*;
use crate::tui::{pane::PaneId, state::DeviceSummarySection, theme, ui::ViewportClass};

pub(super) fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let class = ViewportClass::for_width(area.width);
    let focus = state.devices_focused_pane();

    if class == ViewportClass::Compact {
        match focus {
            PaneId::DevicesSummary => {
                draw_device_summary(frame, area, state);
                return;
            }
            PaneId::DevicesStats => {
                draw_device_stats(frame, area, state);
                return;
            }
            _ => {}
        }
    }

    let (list_area, summary_area, stats_area) = if class == ViewportClass::Compact {
        (area, None, None)
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
            .split(area);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(rows[1]);
        (rows[0], Some(columns[0]), Some(columns[1]))
    };

    draw_device_list(frame, list_area, state);

    if let Some(summary_area) = summary_area {
        draw_device_summary(frame, summary_area, state);
    }
    if let Some(stats_area) = stats_area {
        draw_device_stats(frame, stats_area, state);
    }
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

fn draw_device_summary(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesSummary;
    let title = state
        .selected_device()
        .map(|row| format!("当前设备 · disk{}", row.disk))
        .unwrap_or_else(|| "当前设备".into());

    let Some(row) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("选择设备后，可在这里查看身份、容量布局、状态、备份关系和协议摘要。")
                .block(crate::tui::ui::card(title.as_str(), focused))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    };

    let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
    let cells = identity.display_cells();
    let mut lines = Vec::<Line<'static>>::new();

    push_section_header(
        &mut lines,
        state,
        DeviceSummarySection::Identity,
        "身份信息",
        focused,
    );
    if state.device_summary_section_expanded(DeviceSummarySection::Identity) {
        lines.push(field_line("身份依据", device_identity_basis(row)));
        lines.push(field_line(
            "onlyid",
            safe(identity.onlyid.as_deref().unwrap_or("—")),
        ));
        lines.push(field_line(
            "device_id",
            safe(identity.device_id.as_deref().unwrap_or("—")),
        ));
        lines.push(field_line(
            "序列号",
            safe(row.serial.as_deref().unwrap_or("—")),
        ));
        lines.push(field_line("VID:PID", safe(&cells[1])));
    }

    push_section_header(
        &mut lines,
        state,
        DeviceSummarySection::Capacity,
        "容量布局",
        focused,
    );
    if state.device_summary_section_expanded(DeviceSummarySection::Capacity) {
        let model = device_layout_model(row);
        let bar_width = usize::from(area.width.saturating_sub(6));
        lines.push(model.bar_line_with_label(bar_width, "  "));
        lines.extend(capacity_legend_lines(
            &model,
            &format_sector_size(row.size / crate::common::SECTOR as u64),
            usize::from(area.width.saturating_sub(6)),
        ));
        let layout_complete = match row.confirmed_provision_kind() {
            Some(crate::provision::DiskProvisionKind::Plain) => row.partition_table.is_some(),
            Some(_) => row.existing_profile_for_prefill().is_some(),
            None => false,
        };
        if !layout_complete {
            let message = row
                .partition_table_error
                .as_deref()
                .map(|error| format!("  分区表读取失败：{}", safe(error)))
                .unwrap_or_else(|| "  布局未完整读取；未知区域不推断为空闲空间".to_string());
            lines.push(Line::from(Span::styled(message, warning())));
        }
    }

    push_section_header(
        &mut lines,
        state,
        DeviceSummarySection::Status,
        "状态与诊断",
        focused,
    );
    if state.device_summary_section_expanded(DeviceSummarySection::Status) {
        lines.push(field_line("当前状态", device_status(row)));
        lines.push(field_line("总线", safe(&row.proto)));
        if let Some(error) = &row.probe_error {
            lines.push(field_line("原因", safe(error)));
        } else if row.denied {
            lines.push(field_line("原因", "需要管理员权限读取原始设备"));
        } else if row.proto != "USB" {
            lines.push(field_line("原因", "当前设备不是受支持的 USB 整盘目标"));
        } else {
            lines.push(field_line("操作状态", "可执行制盘 / 备份 / 检查"));
        }
    }

    push_section_header(
        &mut lines,
        state,
        DeviceSummarySection::Backups,
        "备份关系",
        focused,
    );
    if state.device_summary_section_expanded(DeviceSummarySection::Backups) {
        lines.push(field_line("已确认", format!("{} 份", row.n_baks)));
        lines.push(field_line(
            "可能相关",
            format!("{} 份", row.n_possible_baks),
        ));
    }

    push_section_header(
        &mut lines,
        state,
        DeviceSummarySection::Protocol,
        "协议摘要",
        focused,
    );
    if state.device_summary_section_expanded(DeviceSummarySection::Protocol) {
        lines.push(field_line("盘型", safe(&cells[6])));
        lines.push(field_line(
            "分区记录",
            row.partitions
                .as_ref()
                .map(|parts| format!("{} 条", parts.len()))
                .unwrap_or_else(|| "未读取".into()),
        ));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(title.as_str(), focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesSummary).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn push_section_header(
    lines: &mut Vec<Line<'static>>,
    state: &AppState,
    section: DeviceSummarySection,
    label: &'static str,
    focused: bool,
) {
    let selected = state.device_summary_selected_section() == section;
    let marker = if focused && selected { "▌" } else { " " };
    let disclosure = if state.device_summary_section_expanded(section) {
        "▾"
    } else {
        "▸"
    };
    let style = if selected { accent() } else { secondary() };
    lines.push(Line::from(vec![
        Span::styled(marker, style),
        Span::raw(" "),
        Span::styled(disclosure, style),
        Span::raw(" "),
        Span::styled(label, style.add_modifier(Modifier::BOLD)),
    ]));
}

fn field_line(label: &'static str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(crate::ui::pad_to(label, 12), muted()),
        Span::raw(value.into()),
    ])
}

fn capacity_legend_lines(
    model: &crate::tui::disk_layout::DiskLayoutModel,
    total: &str,
    width: usize,
) -> Vec<Line<'static>> {
    use crate::tui::table_layout::display_width;

    #[derive(Clone)]
    struct Entry {
        kind: Option<crate::application::disk_layout::DiskRegionKind>,
        text: String,
    }

    let mut entries = vec![Entry {
        kind: None,
        text: format!("总容量  {total}"),
    }];
    entries.extend(model.segments.iter().map(|segment| Entry {
        kind: Some(segment.kind),
        text: format!(
            "{}  {}",
            segment.label,
            format_sector_size(segment.sector_count)
        ),
    }));

    if entries.is_empty() {
        return Vec::new();
    }

    let available = width.max(1);
    let gap = 4usize;
    let max_entry_width = entries
        .iter()
        .map(|entry| display_width(&entry.text) + usize::from(entry.kind.is_some()) * 2)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut columns = entries.len().clamp(1, 4);
    while columns > 1
        && max_entry_width
            .saturating_mul(columns)
            .saturating_add(gap.saturating_mul(columns - 1))
            > available
    {
        columns -= 1;
    }
    let cell_width = if columns == 1 {
        available
    } else {
        available.saturating_sub(gap.saturating_mul(columns - 1)) / columns
    }
    .max(1);

    let mut lines = Vec::new();
    for chunk in entries.chunks(columns) {
        let mut spans = vec![Span::raw("    ")];
        for (position, entry) in chunk.iter().enumerate() {
            if position > 0 {
                spans.push(Span::raw(" ".repeat(gap)));
            }
            let prefix_width = if let Some(kind) = entry.kind {
                spans.push(Span::styled("■ ", theme::current().disk_region(kind)));
                2
            } else {
                0
            };
            let text_width = display_width(&entry.text);
            let room = cell_width.saturating_sub(prefix_width);
            let text = if text_width <= room {
                entry.text.clone()
            } else {
                crate::tui::table_layout::truncate_cell(
                    &entry.text,
                    room,
                    crate::tui::table_layout::TruncatePolicy::Clip,
                )
            };
            let padded = crate::ui::pad_to(&text, room);
            spans.push(Span::styled(
                padded,
                if entry.kind.is_some() {
                    muted()
                } else {
                    Style::default()
                },
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn device_layout_model(row: &crate::disk_scan::Row) -> crate::tui::disk_layout::DiskLayoutModel {
    use crate::application::disk_layout::{DiskLayoutSegment, DiskRegionKind};
    use crate::provision::{DiskProvisionKind, PartitionRole};

    let total_sectors = row.size / crate::common::SECTOR as u64;

    if row.confirmed_provision_kind() == Some(DiskProvisionKind::Plain) {
        let context = crate::inspect_target::InspectDiskContext::new_with_partition_table(
            vec![0; crate::common::METADATA_IMAGE_LEN],
            None,
            total_sectors,
            Some(DiskProvisionKind::Plain),
            row.partition_table.clone(),
            row.partition_table_error.clone(),
        );
        let topology = crate::application::inspect_tree::build_inspect_topology(&context);
        return crate::tui::disk_layout::DiskLayoutModel::from_topology(&topology);
    }

    let mut claims = Vec::<DiskLayoutSegment>::new();
    let reserved = total_sectors.min(crate::provision::OFFICIAL_PARTITION_START_SECTOR);
    if reserved > 0 {
        claims.push(DiskLayoutSegment {
            label: "EDP 协议/保留".into(),
            start_lba: 0,
            sector_count: reserved,
            kind: DiskRegionKind::Reserved,
        });
    }

    if let Some(profile) = row.existing_profile_for_prefill() {
        for partition in profile.partitions {
            let kind = match partition.role {
                PartitionRole::Boot => DiskRegionKind::Boot,
                PartitionRole::Share => DiskRegionKind::Share,
                PartitionRole::Encrypt => DiskRegionKind::Encrypt,
                PartitionRole::BootShareCombined => DiskRegionKind::Combined,
                PartitionRole::CompatibilityReserve => DiskRegionKind::Compatibility,
            };
            claims.push(DiskLayoutSegment {
                label: partition.role.label().into(),
                start_lba: partition.start_lba,
                sector_count: partition.sector_count,
                kind,
            });
        }
    }

    crate::tui::disk_layout::DiskLayoutModel::from_claims(
        total_sectors,
        claims,
        DiskRegionKind::Unknown,
    )
}

fn format_sector_size(sectors: u64) -> String {
    let bytes = sectors.saturating_mul(crate::common::SECTOR as u64);
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

fn draw_device_stats(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let devices = state.devices();
    let available = devices
        .iter()
        .filter(|row| row.proto == "USB" && !row.denied && row.probe_error.is_none())
        .count();
    let needs_attention = devices.len().saturating_sub(available);
    let edp = devices
        .iter()
        .filter(|row| {
            row.confirmed_provision_kind()
                .is_some_and(|kind| kind != crate::provision::DiskProvisionKind::Plain)
        })
        .count();
    let plain = devices
        .iter()
        .filter(|row| {
            row.confirmed_provision_kind() == Some(crate::provision::DiskProvisionKind::Plain)
        })
        .count();
    let denied = devices.iter().filter(|row| row.denied).count();
    let read_errors = devices
        .iter()
        .filter(|row| row.probe_error.is_some())
        .count();

    let lines = vec![
        Line::from(format!("总设备      {}", devices.len())),
        Line::from(Span::styled(format!("可用        {available}"), success())),
        Line::from(Span::styled(
            format!("需处理      {needs_attention}"),
            if needs_attention > 0 {
                warning()
            } else {
                muted()
            },
        )),
        Line::from(""),
        Line::from(format!("EDP         {edp}")),
        Line::from(format!("普通盘      {plain}")),
        Line::from(""),
        Line::from(format!("需权限      {denied}")),
        Line::from(format!("读取异常    {read_errors}")),
        Line::from(""),
        Line::from(if state.device_scan_pending() {
            Span::styled("扫描状态    ● 正在刷新", accent())
        } else {
            Span::styled("扫描状态    已完成", secondary())
        }),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(
                "设备状态",
                state.devices_focused_pane() == PaneId::DevicesStats,
            ))
            .scroll((
                state.pane_viewport(PaneId::DevicesStats).scroll_y.offset as u16,
                0,
            )),
        area,
    );
}
