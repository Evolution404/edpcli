use super::*;

pub(super) fn draw_inspect(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(workspace) = state.inspect_data() else {
        return;
    };
    let Some(selected_index) = state.inspect_selected_lba() else {
        return;
    };
    let Some(item) = workspace.items.get(selected_index as usize) else {
        return;
    };
    let mode = state.inspect_mode().unwrap_or(InspectMode::Fields);
    let method = item.method.as_deref().unwrap_or("raw");
    let mut lines = vec![
        Line::from(format!("来源: {}", safe(&workspace.source))),
        Line::from(format!("LBA{} · {}", item.lba, safe(method))),
        Line::from(""),
    ];
    match mode {
        InspectMode::Fields => {
            if item.fields.is_empty() {
                lines.push(Line::from("未检测到已知结构化字段。"));
            } else {
                for field in &item.fields {
                    let group = field
                        .group
                        .as_deref()
                        .map(|value| format!("{} · ", safe(value)))
                        .unwrap_or_default();
                    lines.push(Line::from(format!(
                        "{group}{}  {}",
                        safe(&field.label),
                        safe(&field.value)
                    )));
                    for child in &field.children {
                        lines.push(Line::from(format!(
                            "  └─ {}  {}",
                            safe(&child.label),
                            safe(&child.value)
                        )));
                    }
                }
            }
            for note in &item.notes {
                lines.push(Line::from(format!("注: {}", safe(note))));
            }
        }
        InspectMode::DecodedHex => lines.extend(plain_hex_lines(
            item.decoded.as_deref().unwrap_or(item.raw.as_slice()),
        )),
        InspectMode::RawHex => lines.extend(plain_hex_lines(&item.raw)),
    }
    let mode_index = match mode {
        InspectMode::Fields => 0,
        InspectMode::DecodedHex => 1,
        InspectMode::RawHex => 2,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focused_panel())
        .title("Inspect");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let inspect_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    let tabs = Tabs::new(["字段", "Decoded Hex", "Raw Hex"])
        .select(mode_index)
        .style(tab())
        .highlight_style(active_tab())
        .divider(Span::styled(" │ ", muted()));
    frame.render_widget(tabs, inspect_chunks[0]);

    let scroll = state.inspect_scroll().unwrap_or(0).min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inspect_chunks[1],
    );
}

fn inspect_field_status_style(status: crate::application::inspect::InspectFieldStatus) -> Style {
    match status {
        crate::application::inspect::InspectFieldStatus::Known => accent(),
        crate::application::inspect::InspectFieldStatus::Unknown
        | crate::application::inspect::InspectFieldStatus::Reserved => muted(),
        crate::application::inspect::InspectFieldStatus::Preserved => success(),
    }
}

fn draw_sector_inspector(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    use super::super::state::SectorInspectMode;

    let Some(sector) = state.advanced_inspect_sector() else {
        return;
    };
    let item = state.advanced_inspect_sector_item();
    let active_field = state.advanced_inspect_sector_active_field();
    let absolute = (sector.lba as u128) * crate::common::SECTOR as u128 + sector.cursor as u128;
    let decode_issue = item.and_then(|item| item.decode_error.as_deref());
    let header = vec![
        Line::from(vec![
            Span::styled(
                format!("LBA{}  ", sector.lba),
                secondary().add_modifier(Modifier::BOLD),
            ),
            Span::styled(sector.mode.label(), accent().add_modifier(Modifier::BOLD)),
            Span::raw(if sector.pending { "  · 读取中" } else { "" }),
        ]),
        Line::from(format!(
            "byte +0x{:03X} / absolute 0x{:X} / row {:02}/32",
            sector.cursor,
            absolute,
            sector.cursor / 16 + 1
        )),
        Line::from(
            sector
                .error
                .as_deref()
                .or(decode_issue)
                .map(|value| format!("decode: {}", safe(value)))
                .unwrap_or_else(|| "decode: available/raw".into()),
        ),
    ];
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Sector Inspector"),
        ),
        vertical[0],
    );

    let main = if area.width >= 100 {
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(vertical[1]);
        (parts[0], parts[1])
    } else {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(10)])
            .split(vertical[1]);
        (parts[0], parts[1])
    };

    let mut hex_lines = Vec::new();
    if let Some(item) = item {
        let decoded = item.decoded.as_deref();
        let display = match sector.mode {
            SectorInspectMode::Raw => item.raw.as_slice(),
            SectorInspectMode::Decode | SectorInspectMode::Mixed => {
                decoded.unwrap_or(item.raw.as_slice())
            }
        };
        for row in 0..32usize {
            let offset = row * 16;
            let row_abs = (sector.lba as u128) * crate::common::SECTOR as u128 + offset as u128;
            let mut spans = vec![
                Span::styled(format!("+0x{offset:03X}  "), muted()),
                Span::styled(format!("0x{row_abs:012X}  "), muted()),
            ];
            for column in 0..16usize {
                let index = offset + column;
                let byte = display[index];
                let absolute = sector
                    .lba
                    .saturating_mul(crate::common::SECTOR as u64)
                    .saturating_add(index as u64);
                let active_status = active_field.as_ref().and_then(|field| {
                    (absolute >= field.range.start && absolute < field.range.end_exclusive)
                        .then_some(field.status)
                });
                let style = if index == sector.cursor {
                    selected()
                } else if let Some(status) = active_status {
                    inspect_field_status_style(status)
                } else if sector.mode == SectorInspectMode::Mixed
                    && decoded.is_some_and(|decoded| decoded[index] != item.raw[index])
                {
                    secondary()
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!("{byte:02X} "), style));
                if column == 7 {
                    spans.push(Span::raw(" "));
                }
            }
            let ascii = display[offset..offset + 16]
                .iter()
                .map(|byte| {
                    if (0x20..=0x7e).contains(byte) {
                        *byte as char
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            spans.push(Span::styled(format!(" |{ascii}|"), muted()));
            hex_lines.push(Line::from(spans));
        }
    } else {
        hex_lines.push(Line::from(if sector.pending {
            "正在后台读取当前扇区…"
        } else {
            "当前扇区没有可显示的数据。"
        }));
    }
    let visible_rows = main.0.height.saturating_sub(2).max(1) as usize;
    let cursor_row = sector.cursor / 16;
    let max_scroll = 32usize.saturating_sub(visible_rows);
    let scroll = cursor_row
        .saturating_sub(visible_rows / 2)
        .min(max_scroll)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(hex_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("32 × 16 Hex + ASCII"),
            )
            .scroll((scroll, 0)),
        main.0,
    );

    let mut details = Vec::new();
    if let Some(item) = item {
        let raw = item.raw.get(sector.cursor).copied().unwrap_or_default();
        let decoded = item
            .decoded
            .as_ref()
            .and_then(|bytes| bytes.get(sector.cursor))
            .copied();
        details.push(Line::from(format!("Raw byte: 0x{raw:02X} ({raw})")));
        if let Some(decoded) = decoded {
            details.push(Line::from(format!("Decoded:  0x{decoded:02X} ({decoded})")));
        }
        details.push(Line::from(""));
        if let Some(field) = active_field.as_ref() {
            details.push(Line::from(Span::styled(
                safe(&field.label),
                accent().add_modifier(Modifier::BOLD),
            )));
            details.push(Line::from(format!("Value: {}", safe(&field.value))));
            details.push(Line::from(vec![
                Span::raw(format!("Type: {:?} / Status: ", field.field_type)),
                Span::styled(
                    format!("{:?}", field.status),
                    inspect_field_status_style(field.status).add_modifier(Modifier::BOLD),
                ),
            ]));
            details.push(Line::from(format!(
                "Range: 0x{:X}..0x{:X}",
                field.range.start, field.range.end_exclusive
            )));
            if sector.field_expanded {
                details.push(Line::from(""));
                details.push(Line::from(format!(
                    "bits: b7={} b6={} b5={} b4={} b3={} b2={} b1={} b0={}",
                    (raw >> 7) & 1,
                    (raw >> 6) & 1,
                    (raw >> 5) & 1,
                    (raw >> 4) & 1,
                    (raw >> 3) & 1,
                    (raw >> 2) & 1,
                    (raw >> 1) & 1,
                    raw & 1
                )));
                for child in &field.children {
                    details.push(Line::from(format!(
                        "{} = {}",
                        safe(&child.label),
                        safe(&child.value)
                    )));
                }
            } else {
                details.push(Line::from("o 展开 bit / child 详情"));
            }
        } else {
            details.push(Line::from(Span::styled("Unknown byte", muted())));
            details.push(Line::from("当前 byte 不属于已知 Field；不推测语义。"));
            if sector.field_expanded {
                details.push(Line::from(format!("bits: {:08b}", raw)));
            }
        }
    } else if let Some(error) = sector.error.as_deref() {
        details.push(Line::from(Span::styled(safe(error), danger())));
    }
    if let Some(yank) = state.advanced_inspect_yank_register() {
        details.push(Line::from(""));
        details.push(Line::from(vec![
            Span::styled("Yank  ", muted()),
            Span::styled(safe(yank), secondary()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Typed / Field"),
            )
            .wrap(Wrap { trim: false }),
        main.1,
    );

    frame.render_widget(
        Paragraph::new(Line::from(
            "←/→ byte · j/k ±16B · PgUp/PgDn sector · r/d/m mode · o bit · y value · Y raw · Esc 返回树",
        ))
        .block(Block::default().borders(Borders::TOP)),
        vertical[2],
    );
}

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
            if advanced.sector.is_some() {
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
                AdvancedInspectPanel::Tree => 0,
                AdvancedInspectPanel::Overview => 1,
                AdvancedInspectPanel::Detail => 2,
            };
            let browser = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(area);
            frame.render_widget(
                Tabs::new(["结构树", "节点概览", "节点详情"])
                    .select(panel_index)
                    .style(tab())
                    .highlight_style(active_tab())
                    .divider(Span::styled(" │ ", muted())),
                browser[0],
            );
            let content_area = browser[1];
            let compact = content_area.width < 92 || content_area.height < 14;
            let (tree_area, overview_area, detail_area) = if compact {
                match advanced.panel {
                    AdvancedInspectPanel::Tree => (Some(content_area), None, None),
                    AdvancedInspectPanel::Overview => (None, Some(content_area), None),
                    AdvancedInspectPanel::Detail => (None, None, Some(content_area)),
                }
            } else if content_area.width >= 140 {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(42),
                        Constraint::Percentage(27),
                        Constraint::Percentage(31),
                    ])
                    .split(content_area);
                (Some(parts[0]), Some(parts[1]), Some(parts[2]))
            } else {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
                    .split(content_area);
                let right = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(9), Constraint::Min(4)])
                    .split(parts[1]);
                (Some(parts[0]), Some(right[0]), Some(right[1]))
            };

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
                    let content = format!(
                        "{marker}{icon}{}  [{}..{})",
                        safe(&row.label),
                        row.range.start_lba,
                        row.range.end_lba_exclusive()
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

            let mut overview_lines = vec![Line::from(vec![
                Span::styled("来源  ", muted()),
                Span::styled(safe(&workspace.source), accent()),
            ])];
            let mut detail_lines = Vec::new();
            if let Some(row) = selected_row {
                let kind = match row.kind {
                    InspectNodeKind::Device => "Device",
                    InspectNodeKind::Region => "Region",
                    InspectNodeKind::Extent => "Extent",
                    InspectNodeKind::Sector => "Sector",
                    InspectNodeKind::Structure => "Structure",
                    InspectNodeKind::Group => "Group",
                    InspectNodeKind::Field => "Field",
                    InspectNodeKind::Partition => "Partition",
                    InspectNodeKind::UnknownRange => "UnknownRange",
                };
                let status = match row.status {
                    crate::edpb::SemanticStatus::Identified => "identified",
                    crate::edpb::SemanticStatus::Unknown => "unknown",
                };
                overview_lines.extend([
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("节点  ", muted()),
                        Span::styled(safe(&row.label), secondary().add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(format!("类型: {kind}")),
                    Line::from(format!(
                        "LBA: {}..{}",
                        row.range.start_lba,
                        row.range.end_lba_exclusive()
                    )),
                    Line::from(format!("Sector count: {}", row.range.sector_count)),
                    Line::from(format!("状态: {status}")),
                ]);
                if let Some(byte_range) = row.range.byte_range {
                    overview_lines.push(Line::from(format!(
                        "Byte: 0x{:X}..0x{:X}",
                        byte_range.start, byte_range.end_exclusive
                    )));
                }
                if let Some(decoder) = row.decoder {
                    overview_lines.push(Line::from(format!("Decoder: {decoder:?}")));
                }

                match row.kind {
                    InspectNodeKind::Sector => {
                        let lba = row.range.start_lba;
                        if let Some(item) = workspace.items.iter().find(|item| item.lba == lba) {
                            detail_lines.push(Line::from(vec![
                                Span::styled("已缓存  ", success()),
                                Span::raw(format!("LBA{lba}")),
                            ]));
                            detail_lines.push(Line::from(format!(
                                "RAW SHA-256: {}",
                                safe(&item.raw_sha256)
                            )));
                            detail_lines.push(Line::from(
                                "Enter 打开 Sector Inspector；首次进入自动补齐 Decode。",
                            ));
                        } else {
                            detail_lines
                                .push(Line::from(Span::styled("该扇区尚未按需读取。", warning())));
                            detail_lines.push(Line::from(
                                "Enter 打开 Sector Inspector 并后台读取当前 sector。",
                            ));
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
                                "Type: {:?}   Status: {:?}",
                                field.field_type, field.status
                            )));
                            let raw = field
                                .raw
                                .iter()
                                .take(32)
                                .map(|byte| format!("{byte:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            detail_lines.push(Line::from(format!("Raw: {raw}")));
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
                frame.render_widget(
                    Paragraph::new(overview_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(if advanced.panel == AdvancedInspectPanel::Overview {
                                    accent()
                                } else {
                                    muted()
                                })
                                .title("节点概览"),
                        )
                        .wrap(Wrap { trim: false }),
                    overview_area,
                );
            }

            if let Some(detail_area) = detail_area {
                let detail_focus = advanced.panel == AdvancedInspectPanel::Detail;
                let scroll = advanced.detail_scroll.min(u16::MAX as usize) as u16;
                frame.render_widget(
                    Paragraph::new(detail_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(if detail_focus { accent() } else { muted() })
                                .title("节点详情"),
                        )
                        .wrap(Wrap { trim: false })
                        .scroll((scroll, 0)),
                    detail_area,
                );
            }
        }
    }
}
