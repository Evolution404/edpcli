use super::*;

fn inspect_field_status_style(status: crate::application::inspect::InspectFieldStatus) -> Style {
    match status {
        crate::application::inspect::InspectFieldStatus::Known => accent(),
        crate::application::inspect::InspectFieldStatus::Unknown
        | crate::application::inspect::InspectFieldStatus::Reserved => muted(),
        crate::application::inspect::InspectFieldStatus::Preserved => success(),
    }
}

fn draw_inspect_breadcrumb(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(model) = state.advanced_inspect_breadcrumb() else {
        return;
    };
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(20)])
        .split(area);
    frame.render_widget(
        Paragraph::new(safe(&model.display())).style(muted()),
        parts[0],
    );
    frame.render_widget(
        Paragraph::new(model.escape_hint()).style(accent()),
        parts[1],
    );
}

#[path = "sector_render.rs"]
mod sector_render;
use sector_render::draw_sector_inspector;

pub(super) fn draw_advanced_inspect(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(advanced) = state.advanced_inspect() else {
        return;
    };
    use super::super::state::{AdvancedInspectPanel, AdvancedInspectPrompt, AdvancedInspectStage};
    use crate::application::inspect_tree::InspectNodeKind;

    match advanced.stage {
        AdvancedInspectStage::Running => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "◈ 正在建立全盘结构树",
                        secondary().add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(safe(
                        advanced
                            .message
                            .as_deref()
                            .unwrap_or("正在读取协议上下文并识别磁盘区域…"),
                    )),
                    Line::from("只读任务不会修改物理盘。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("Inspect · 全盘浏览"),
                ),
                area,
            );
        }
        AdvancedInspectStage::Browser => {
            if advanced.sector.is_some() && advanced.panel == AdvancedInspectPanel::Detail {
                draw_sector_inspector(frame, area, state);
                return;
            }
            let Some(workspace) = advanced.result.as_ref() else {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled("全盘结构加载失败", danger())),
                        Line::from(""),
                        Line::from(safe(
                            advanced
                                .message
                                .as_deref()
                                .unwrap_or("未取得 Inspect workspace"),
                        )),
                        Line::from(""),
                        Line::from("Esc 关闭"),
                    ])
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(danger())
                            .title("Inspect · 全盘浏览"),
                    )
                    .wrap(Wrap { trim: false }),
                    area,
                );
                return;
            };

            let rows = state.advanced_inspect_tree_rows();
            let selected_index = advanced.tree_selected.min(rows.len().saturating_sub(1));
            let selected_row = rows.get(selected_index);
            let panel_index = match advanced.panel {
                AdvancedInspectPanel::Tree | AdvancedInspectPanel::Overview => 0,
                AdvancedInspectPanel::Detail => 1,
                AdvancedInspectPanel::DiskLayout => 3,
            };
            let disk_layout =
                crate::tui::disk_layout::DiskLayoutModel::from_topology(&workspace.topology);
            let browser = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(area);
            draw_inspect_breadcrumb(frame, browser[0], state);
            frame.render_widget(
                Tabs::new(["1 业务字段", "2 原始字段", "3 Hex", "4 全盘布局"])
                    .select(panel_index)
                    .style(tab())
                    .highlight_style(active_tab())
                    .divider(Span::styled(" │ ", muted())),
                browser[1],
            );
            let content_area = browser[2];
            let class = crate::tui::ui::ViewportClass::for_width(content_area.width);
            let (disk_layout_area, compact_layout_area, tree_area, overview_area, detail_area) =
                if advanced.panel == AdvancedInspectPanel::DiskLayout {
                    (Some(content_area), None, None, None, None)
                } else if class == crate::tui::ui::ViewportClass::Compact {
                    match advanced.panel {
                        AdvancedInspectPanel::Tree => {
                            let parts = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Percentage(60),
                                    Constraint::Percentage(40),
                                ])
                                .split(content_area);
                            (None, None, Some(parts[1]), Some(parts[0]), None)
                        }
                        AdvancedInspectPanel::Overview => {
                            (None, None, None, Some(content_area), None)
                        }
                        AdvancedInspectPanel::Detail => {
                            (None, None, None, None, Some(content_area))
                        }
                        AdvancedInspectPanel::DiskLayout => unreachable!(),
                    }
                } else {
                    let vertical = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Percentage(58),
                            Constraint::Length(3),
                            Constraint::Min(4),
                        ])
                        .split(content_area);
                    let upper = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(29), Constraint::Percentage(71)])
                        .split(vertical[0]);
                    (
                        None,
                        Some(vertical[1]),
                        Some(upper[0]),
                        Some(upper[1]),
                        Some(vertical[2]),
                    )
                };

            if let Some(compact_area) = compact_layout_area {
                disk_layout.render_compact(
                    frame,
                    compact_area,
                    selected_row.map(|row| row.range.start_lba),
                );
            }

            if let Some(layout_area) = disk_layout_area {
                let total = disk_layout.total_sectors;
                let gib = total as f64 * crate::common::SECTOR as f64 / 1_073_741_824.0;
                let disk_status = match &advanced.source {
                    crate::tui::state::AdvancedInspectSource::Disk(disk) => state
                        .devices()
                        .iter()
                        .find(|row| row.disk == *disk)
                        .map(device_status)
                        .unwrap_or_else(|| "状态未读取".into()),
                    crate::tui::state::AdvancedInspectSource::Backup(_) => "备份镜像".into(),
                };
                let summary = format!(
                    "{gib:.2} GiB / {total} sectors · {} · {}",
                    safe(&workspace.source),
                    safe(&disk_status)
                );
                disk_layout.render_pane(
                    frame,
                    layout_area,
                    crate::tui::disk_layout::DiskLayoutPane {
                        title: "磁盘布局",
                        summary: &summary,
                        details: &[],
                        focused: advanced.panel == AdvancedInspectPanel::DiskLayout,
                        scroll_y: state
                            .pane_viewport(crate::tui::pane::PaneId::InspectDiskLayout)
                            .scroll_y
                            .offset,
                    },
                );
            }

            let tree_focus = advanced.panel == AdvancedInspectPanel::Tree;
            if let Some(tree_area) = tree_area {
                let visible = visible_window(selected_index, rows.len(), tree_area.height);
                let tree_lines = visible.map(|index| {
                    let row = &rows[index];
                    let indent = "  ".repeat(row.depth);
                    let marker = if row.expandable {
                        if row.expanded {
                            "− "
                        } else {
                            "+ "
                        }
                    } else {
                        "· "
                    };
                    let icon = match row.kind {
                        InspectNodeKind::Device => "◆ ",
                        InspectNodeKind::Region => "◇ ",
                        InspectNodeKind::Extent => "▰ ",
                        InspectNodeKind::Sector => "□ ",
                        InspectNodeKind::Structure => "▱ ",
                        InspectNodeKind::Group => "≡ ",
                        InspectNodeKind::Field => "• ",
                        InspectNodeKind::Partition => "▣ ",
                        InspectNodeKind::UnknownRange => "? ",
                    };
                    let kind_style = match row.kind {
                        InspectNodeKind::Device => secondary().add_modifier(Modifier::BOLD),
                        InspectNodeKind::Region | InspectNodeKind::Partition => accent(),
                        InspectNodeKind::Extent | InspectNodeKind::Structure => success(),
                        InspectNodeKind::Sector | InspectNodeKind::Field => Style::default(),
                        InspectNodeKind::Group | InspectNodeKind::UnknownRange => muted(),
                    };
                    let content = if row.kind == InspectNodeKind::Sector {
                        format!("{marker}{icon}{}", safe(&row.label))
                    } else {
                        let range = crate::application::inspect_tree::format_lba_closed_range(
                            row.range.start_lba,
                            row.range.end_lba_exclusive(),
                        )
                        .unwrap_or_else(|| "[空区间]".into());
                        format!("{marker}{icon}{} {range}", safe(&row.label))
                    };
                    let available = usize::from(tree_area.width)
                        .saturating_sub(2)
                        .saturating_sub(row.depth.saturating_mul(2))
                        .saturating_sub(2)
                        .saturating_sub(1);
                    let content = crate::tui::table_layout::truncate_cell(
                        &content,
                        available,
                        crate::tui::table_layout::TruncatePolicy::Ellipsis,
                    );
                    let focused = tree_focus && index == selected_index;
                    Line::from(vec![
                        Span::raw(indent),
                        Span::styled(
                            if focused { "▌ " } else { "  " },
                            if focused {
                                selection_marker()
                            } else {
                                Style::default()
                            },
                        ),
                        Span::styled(content, if focused { selected() } else { kind_style }),
                        Span::raw(" "),
                    ])
                });
                frame.render_widget(
                    Paragraph::new(tree_lines.collect::<Vec<_>>())
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(if tree_focus { focused_panel() } else { panel() })
                                .title(format!(
                                    "结构树  {}/{}",
                                    selected_index.saturating_add(1),
                                    rows.len()
                                )),
                        )
                        .wrap(Wrap { trim: false }),
                    tree_area,
                );
            }

            let mut overview_lines = Vec::new();
            let mut detail_lines = Vec::new();
            if let Some(row) = selected_row {
                let item = matches!(row.kind, InspectNodeKind::Sector | InspectNodeKind::Field)
                    .then(|| {
                        workspace
                            .items
                            .iter()
                            .find(|item| item.lba == row.range.start_lba)
                    })
                    .flatten();
                let fields: &[crate::application::inspect::InspectField] = match row.kind {
                    InspectNodeKind::Sector => item.map_or(&[], |item| item.fields.as_slice()),
                    InspectNodeKind::Field => item
                        .and_then(|item| {
                            item.fields.iter().find(|field| {
                                row.range
                                    .byte_range
                                    .is_some_and(|range| range == field.range)
                            })
                        })
                        .map_or(&[], std::slice::from_ref),
                    _ => &[],
                };
                let summary = crate::application::inspect_summary::summarize_node(
                    crate::application::inspect_summary::InspectSummarySource {
                        kind: row.kind,
                        label: &row.label,
                        range: row.range,
                        decoder: row.decoder,
                        status: row.status,
                        region_semantic: row.region_semantic,
                        fields,
                        parse_state: item.map_or(
                            crate::application::inspect::InspectParseState::Parsed,
                            |item| item.parse_state,
                        ),
                        diagnostics: item.map_or(&[], |item| item.diagnostics.as_slice()),
                    },
                );
                overview_lines.push(Line::from(Span::styled(
                    safe(&summary.title),
                    secondary().add_modifier(Modifier::BOLD),
                )));
                overview_lines.push(Line::from(Span::styled(
                    safe(&summary.subtitle),
                    if summary.alerts.is_empty() {
                        muted()
                    } else {
                        warning()
                    },
                )));
                overview_lines.push(Line::from(Span::styled(safe(&summary.location), muted())));
                for section in &summary.sections {
                    overview_lines.push(Line::from(""));
                    overview_lines.push(Line::from(Span::styled(safe(&section.title), accent())));
                    let label_width = section
                        .items
                        .iter()
                        .map(|item| crate::tui::table_layout::display_width(&item.label))
                        .max()
                        .unwrap_or(0);
                    for item in &section.items {
                        let padding = label_width
                            .saturating_sub(crate::tui::table_layout::display_width(&item.label))
                            + 2;
                        overview_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{}{}", safe(&item.label), " ".repeat(padding)),
                                muted(),
                            ),
                            Span::raw(safe(&item.value)),
                        ]));
                    }
                }
                for alert in &summary.alerts {
                    overview_lines.push(Line::from(Span::styled(safe(&alert.message), warning())));
                }
                overview_lines.push(Line::from(vec![
                    Span::styled("来源  ", muted()),
                    Span::styled(safe(&workspace.source), muted()),
                ]));

                match row.kind {
                    InspectNodeKind::Sector => {
                        let lba = row.range.start_lba;
                        if let Some(item) = workspace.items.iter().find(|item| item.lba == lba) {
                            if !item.fields.is_empty() {
                                let mut previous_group: Option<&str> = None;
                                for field in &item.fields {
                                    let group = field.group.as_deref();
                                    if group != previous_group {
                                        if let Some(group) = group {
                                            detail_lines.push(Line::from(Span::styled(
                                                safe(group),
                                                accent().add_modifier(Modifier::BOLD),
                                            )));
                                        }
                                        previous_group = group;
                                    }
                                    detail_lines.push(Line::from(format!(
                                        "{}: {}",
                                        safe(&field.label),
                                        safe(&field.value)
                                    )));
                                    for child in &field.children {
                                        detail_lines.push(Line::from(format!(
                                            "  {}: {}",
                                            safe(&child.label),
                                            safe(&child.value)
                                        )));
                                    }
                                }
                            } else if let Some(meta_text) = &item.meta_text {
                                detail_lines
                                    .extend(meta_text.lines().map(|line| Line::from(safe(line))));
                            } else {
                                detail_lines.push(Line::from(format!(
                                    "RAW SHA-256: {}",
                                    safe(&item.raw_sha256)
                                )));
                            }
                            for note in &item.notes {
                                detail_lines.push(Line::from(safe(note)));
                            }
                            detail_lines.push(Line::from("Enter 打开 Sector Inspector/Hex"));
                        } else {
                            match state.advanced_inspect_preview_state(lba) {
                                crate::tui::state::PreviewLoadState::Failed {
                                    message,
                                    attempts,
                                } => {
                                    overview_lines.push(Line::from(Span::styled(
                                        format!("读取失败（第 {attempts} 次）：{}", safe(&message)),
                                        warning(),
                                    )));
                                    detail_lines.push(Line::from(
                                        "按 r 显式重试预览，或 Enter 打开 Sector Inspector 重试。",
                                    ));
                                }
                                crate::tui::state::PreviewLoadState::Pending { .. } => {
                                    detail_lines.push(Line::from("正在后台读取当前扇区…"));
                                }
                                _ => {
                                    detail_lines.push(Line::from(Span::styled(
                                        "该扇区尚未按需读取。",
                                        warning(),
                                    )));
                                    detail_lines.push(Line::from(
                                        "Enter 打开 Sector Inspector 并后台读取当前 sector。",
                                    ));
                                }
                            }
                        }
                    }
                    InspectNodeKind::Field => {
                        let field = row.range.byte_range.and_then(|range| {
                            workspace
                                .items
                                .iter()
                                .flat_map(|item| item.fields.iter())
                                .find(|field| field.range == range)
                        });
                        if let Some(field) = field {
                            detail_lines.push(Line::from(Span::styled(
                                safe(&field.label),
                                accent().add_modifier(Modifier::BOLD),
                            )));
                            detail_lines.push(Line::from(format!("Value: {}", safe(&field.value))));
                            detail_lines.push(Line::from(format!(
                                "Source LBA: {} · Group: {}",
                                field.range.start_lba(),
                                safe(field.group.as_deref().unwrap_or("—"))
                            )));
                            detail_lines.push(Line::from(format!(
                                "Offset: 0x{:X} · Length: {} B",
                                field.range.start,
                                field.range.len()
                            )));
                            let hex = |bytes: &[u8]| {
                                bytes
                                    .iter()
                                    .map(|byte| format!("{byte:02X}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            };
                            detail_lines.push(Line::from(format!("Raw: {}", hex(&field.raw))));
                            detail_lines
                                .push(Line::from(format!("Decoded: {}", hex(&field.decoded))));
                            detail_lines.push(Line::from(format!(
                                "FieldLogical: {}",
                                field
                                    .field_logical
                                    .as_deref()
                                    .map(hex)
                                    .unwrap_or_else(|| "—".into())
                            )));
                            detail_lines.push(Line::from(format!(
                                "Transform: {}",
                                field
                                    .transform
                                    .map(|transform| format!("{transform:?}"))
                                    .unwrap_or_else(|| "—".into())
                            )));
                            detail_lines.push(Line::from(format!(
                                "Type: {:?}   Status: {:?}",
                                field.field_type, field.status
                            )));
                        } else {
                            detail_lines.push(Line::from("字段详情尚未 materialize。"));
                        }
                    }
                    InspectNodeKind::Group => {
                        detail_lines.push(Line::from("分页控制节点。"));
                        detail_lines.push(Line::from("Enter / o 切换当前 lazy sector 窗口。"));
                    }
                    _ => {
                        detail_lines.push(Line::from("o 展开/折叠当前节点。"));
                        detail_lines.push(Line::from("Enter 查看或进入当前节点。"));
                    }
                }
            } else {
                overview_lines.push(Line::from("当前没有可选节点。"));
                detail_lines.push(Line::from("当前没有可选节点。"));
            }
            if let Some(prompt) = advanced.prompt.as_ref() {
                detail_lines.push(Line::from(""));
                match prompt {
                    AdvancedInspectPrompt::Jump { unit, input } => {
                        detail_lines.push(Line::from(Span::styled(
                            "Jump to",
                            accent().add_modifier(Modifier::BOLD),
                        )));
                        detail_lines.push(Line::from(format!("> {}", safe(input))));
                        detail_lines.push(Line::from(format!("Unit: {}", unit.label())));
                        detail_lines.push(Line::from(
                            "Enter 跳转 · Space 切换 LBA / byte offset · Esc 取消",
                        ));
                    }
                    AdvancedInspectPrompt::Search { input } => {
                        detail_lines.push(Line::from(Span::styled(
                            "结构化搜索",
                            accent().add_modifier(Modifier::BOLD),
                        )));
                        detail_lines.push(Line::from(format!("/{}", safe(input))));
                        detail_lines.push(Line::from(
                            "搜索 Region / Extent / Structure / Group / Field label 与 typed value",
                        ));
                        detail_lines.push(Line::from("Enter 定位 · Esc 取消"));
                    }
                }
            }

            if let Some(message) = advanced.message.as_deref() {
                detail_lines.push(Line::from(""));
                detail_lines.push(Line::from(Span::styled(safe(message), danger())));
            }

            if let Some(overview_area) = overview_area {
                let overview_scroll = state
                    .pane_viewport(crate::tui::pane::PaneId::InspectOverview)
                    .scroll_y
                    .offset
                    .min(overview_lines.len().saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(overview_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(if advanced.panel == AdvancedInspectPanel::Overview {
                                    focused_panel()
                                } else {
                                    panel()
                                })
                                .title("对象快照"),
                        )
                        .scroll((overview_scroll.min(u16::MAX as usize) as u16, 0))
                        .wrap(Wrap { trim: false }),
                    overview_area,
                );
            }

            if let Some(detail_area) = detail_area {
                let detail_focus = advanced.panel == AdvancedInspectPanel::Detail;
                let detail_offset = state
                    .pane_viewport(crate::tui::pane::PaneId::InspectDetail)
                    .scroll_y
                    .offset;
                let field_item = selected_row
                    .filter(|row| row.kind == InspectNodeKind::Sector)
                    .and_then(|row| {
                        workspace
                            .items
                            .iter()
                            .find(|item| item.lba == row.range.start_lba)
                    })
                    .filter(|item| !item.fields.is_empty());
                if let Some(item) = field_item {
                    use crate::tui::table_layout::{
                        display_width, layout_for, truncate_cell, TableKind,
                    };
                    let headings = super::super::state::INSPECT_DETAIL_HEADINGS;
                    let values = state.advanced_inspect_detail_rows();
                    let mut content_widths = headings.map(display_width);
                    for row in &values {
                        for (index, value) in row.cells.iter().enumerate() {
                            content_widths[index] = content_widths[index].max(display_width(value));
                        }
                    }
                    let layout = layout_for(TableKind::InspectFields);
                    let viewport = layout.layout(
                        detail_area.width.saturating_sub(3),
                        &content_widths,
                        state
                            .pane_viewport(crate::tui::pane::PaneId::InspectDetail)
                            .scroll_x,
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
                                        Cell::from(truncate_cell(
                                            &safe(&row.cells[column.index]),
                                            usize::from(column.width),
                                            column.truncate_policy,
                                        ))
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
                                truncate_cell(
                                    headings[column.index],
                                    usize::from(column.width),
                                    column.truncate_policy,
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                    .style(accent());
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
                                    "字段详情 · 行 {}–{} / {} · 列 {}",
                                    if values.is_empty() { 0 } else { row_start + 1 },
                                    row_end,
                                    values.len(),
                                    viewport.position_label()
                                )),
                        ),
                        detail_area,
                    );
                } else {
                    let detail_scroll = detail_offset.min(detail_lines.len().saturating_sub(1));
                    frame.render_widget(
                        Paragraph::new(detail_lines)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_style(if detail_focus {
                                        focused_panel()
                                    } else {
                                        panel()
                                    })
                                    .title("字段详情 / Evidence"),
                            )
                            .wrap(Wrap { trim: false })
                            .scroll((detail_scroll.min(u16::MAX as usize) as u16, 0)),
                        detail_area,
                    );
                }
            }
        }
    }
}
