use super::*;

const INPUT_EDITING_SLACK: usize = 2;

pub(super) fn draw_provision(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let provision = state.provision();
    let (main_area, sidebar) = workspace_sidebar_layout(area);

    let target_lines = if let Some(row) = if provision.stage == ProvisionStage::SelectDisk {
        state.provision_device_at(state.selected())
    } else {
        state.selected_device()
    } {
        vec![
            Line::from(vec![
                Span::styled(format!("disk{}", row.disk), accent()),
                Span::raw(format!(
                    "  {:.2} GiB",
                    row.size as f64 / 1024.0 / 1024.0 / 1024.0
                )),
            ]),
            Line::from(vec![
                Span::styled("接口  ", muted()),
                Span::styled(safe(&row.proto), secondary()),
                Span::raw("   "),
                Span::styled(format!("{}:{}", safe(&row.vid), safe(&row.pid)), muted()),
            ]),
            Line::from(vec![
                Span::styled("盘型  ", muted()),
                Span::styled(row.provision_kind.full_name(), device_status_style(row)),
            ]),
            Line::from(vec![
                Span::styled("标签  ", muted()),
                Span::raw(safe(row.onlyid.as_deref().unwrap_or("未读取"))),
            ]),
            Line::from(vec![
                Span::styled("用户  ", muted()),
                Span::raw(safe(row.user.as_deref().unwrap_or("未读取"))),
            ]),
        ]
    } else {
        vec![
            Line::from(Span::styled("未固定目标 USB", danger())),
            Line::from("请在左侧列表选择可用 USB 目标盘。"),
        ]
    };

    if let Some((side_top, side_bottom)) = sidebar {
        frame.render_widget(
            Paragraph::new(target_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("固定目标")
                        .title_style(accent()),
                )
                .wrap(Wrap { trim: true }),
            side_top,
        );
        if let Some(side_bottom) = side_bottom {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("安全不变量", warning())),
                    Line::from("• 仅允许 USB 整盘目标"),
                    Line::from("• LBA3 厂商数据原样保留"),
                    Line::from("• 写前固定硬件身份/容量"),
                    Line::from("• MBR 最后提交"),
                    Line::from("• 协议写入失败回滚；格式化失败保留制盘"),
                    Line::from("• 保留分区保持原位置与密钥材料"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(warning())
                        .title("写盘保护"),
                )
                .wrap(Wrap { trim: true }),
                side_bottom,
            );
        }
    }

    match provision.stage {
        ProvisionStage::SelectDisk => {
            use crate::tui::table_layout::{display_width, layout_for, truncate_cell, TableKind};
            let headings = ["设备", "容量", "USB 身份", "盘型", "onlyid"];
            let values = (0..state.item_count())
                .filter_map(|index| {
                    let row = state.provision_device_at(index)?;
                    Some(vec![
                        format!("disk{}", row.disk),
                        format!("{:.2} GiB", row.size as f64 / 1_073_741_824.0),
                        format!("{}:{}", safe(&row.vid), safe(&row.pid)),
                        row.provision_kind.full_name().to_string(),
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
            let layout = layout_for(TableKind::ProvisionDevices);
            let viewport = layout.layout(
                main_area.width.saturating_sub(4),
                &content_widths,
                state.table_scroll_offset(TableKind::ProvisionDevices),
            );
            let rows = (0..state.item_count()).filter_map(|index| {
                let row = values.get(index)?;
                Some(TableRow::new(
                    viewport
                        .columns
                        .iter()
                        .map(|column| {
                            Cell::from(truncate_cell(
                                &row[column.index],
                                usize::from(column.width),
                                column.truncate_policy,
                            ))
                        })
                        .collect::<Vec<_>>(),
                ))
            });
            let table = Table::new(rows, viewport.widths())
                .header(
                    TableRow::new(
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
                    .style(accent()),
                )
                .block(Block::default().borders(Borders::ALL).title(format!(
                    "制盘 · 先选择 USB 目标 · h/l 横向滚动 · {}",
                    viewport.position_label()
                )))
                .row_highlight_style(selected())
                .highlight_symbol("▌ ");
            let mut table_state = ratatui::widgets::TableState::default();
            if state.item_count() > 0 {
                table_state.select(Some(state.selected()));
            }
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::Menu => {
            use crate::tui::table_layout::{display_width, layout_for, truncate_cell, TableKind};
            let headings = ["#", "制盘方案", "布局 / 行为"];
            let mut content_widths = headings.map(display_width);
            for (index, kind) in ProvisionKind::ALL.into_iter().enumerate() {
                for (column, value) in [
                    index.to_string(),
                    kind.title().into(),
                    kind.description().into(),
                ]
                .iter()
                .enumerate()
                {
                    content_widths[column] = content_widths[column].max(display_width(value));
                }
            }
            let layout = layout_for(TableKind::ProvisionMenu);
            let viewport = layout.layout(
                main_area.width.saturating_sub(4),
                &content_widths,
                state.table_scroll_offset(TableKind::ProvisionMenu),
            );
            let rows = ProvisionKind::ALL
                .into_iter()
                .enumerate()
                .map(|(index, kind)| {
                    let values = [
                        index.to_string(),
                        kind.title().into(),
                        kind.description().into(),
                    ];
                    TableRow::new(
                        viewport
                            .columns
                            .iter()
                            .map(|column| {
                                Cell::from(truncate_cell(
                                    &values[column.index],
                                    usize::from(column.width),
                                    column.truncate_policy,
                                ))
                                .style(provision_kind_style(kind))
                            })
                            .collect::<Vec<_>>(),
                    )
                });
            let table = Table::new(rows, viewport.widths())
                .header(
                    TableRow::new(
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
                    .style(accent())
                    .bottom_margin(1),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title(format!(
                            "制盘中心 · 选择方案 · h/l 横向滚动 · {}",
                            viewport.position_label()
                        ))
                        .title_style(secondary()),
                )
                .row_highlight_style(selected())
                .highlight_symbol("▌ ");
            let mut table_state = TableState::default();
            table_state.select(Some(state.selected()));
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::Form => {
            let wide = main_area.width >= 96;
            let focused_pane = state.provision_focused_pane();
            let (form_area, layout_area) = if wide {
                let areas =
                    Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                        .split(main_area);
                (Some(areas[0]), Some(areas[1]))
            } else if focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout {
                (None, Some(main_area))
            } else {
                (Some(main_area), None)
            };
            let form_geometry = form_area.unwrap_or(main_area);
            let content_width = form_geometry.width.saturating_sub(2) as usize;
            let separator = " │ ";
            let separator_width = crate::ui::disp_width(separator);

            let fields = state.provision_visible_fields();
            let rows = state.provision_compact_field_rows();
            let mut section_metrics: HashMap<&str, (usize, usize, usize, usize)> = HashMap::new();
            for (section, indexes) in &rows {
                let entry = section_metrics.entry(*section).or_insert((0, 0, 0, 0));
                for (position, index) in indexes.iter().copied().enumerate() {
                    let (label, value, secret) = &fields[index];
                    let label_width = crate::ui::disp_width(label);
                    let shown_width = if value.is_empty() {
                        crate::ui::disp_width("〈请输入〉")
                    } else if *secret {
                        value.chars().count()
                    } else {
                        crate::ui::disp_width(&safe(value))
                    }
                    .saturating_add(INPUT_EDITING_SLACK)
                    .clamp(8, 26);
                    if position == 0 {
                        entry.0 = entry.0.max(label_width);
                        entry.2 = entry.2.max(shown_width);
                    } else {
                        entry.1 = entry.1.max(label_width);
                        entry.3 = entry.3.max(shown_width);
                    }
                }
            }

            let mut form_lines = vec![Line::from(vec![
                Span::styled(provision.kind.title(), provision_kind_style(provision.kind)),
                Span::raw("  "),
                Span::styled(provision.kind.description(), muted()),
            ])];
            let mut current_section: Option<&str> = None;
            let mut selected_line = 0usize;
            for (section, indexes) in rows {
                if current_section != Some(section) {
                    form_lines.push(Line::from(""));
                    form_lines.push(Line::from(Span::styled(section, secondary())));
                    current_section = Some(section);
                }
                let mut spans = Vec::new();
                let row_selected = indexes.contains(&provision.field_selected);
                if row_selected {
                    selected_line = form_lines.len();
                }
                let two_columns = indexes.len() == 2;
                for (position, index) in indexes.into_iter().enumerate() {
                    let (label, value, secret) = &fields[index];
                    let active = index == provision.field_selected;
                    if position > 0 {
                        spans.push(Span::styled(separator, muted()));
                    }
                    let metrics = section_metrics
                        .get(section)
                        .copied()
                        .unwrap_or((0, 0, 8, 8));
                    let min_right_width = 2 + metrics.1 + 1 + metrics.3.max(8);
                    let desired_left_width = 2 + metrics.0 + 1 + metrics.2.max(8);
                    let max_left_width = content_width
                        .saturating_sub(separator_width)
                        .saturating_sub(min_right_width)
                        .max(8);
                    let section_left_width = desired_left_width.min(max_left_width);
                    let cell_width = if two_columns {
                        if position == 0 {
                            section_left_width
                        } else {
                            content_width
                                .saturating_sub(section_left_width)
                                .saturating_sub(separator_width)
                        }
                    } else {
                        content_width
                    };
                    let label_width = if position == 0 { metrics.0 } else { metrics.1 };
                    let label_width = label_width.min(cell_width.saturating_sub(4));
                    let value_width = cell_width
                        .saturating_sub(2)
                        .saturating_sub(label_width)
                        .saturating_sub(1)
                        .max(1);

                    spans.push(Span::styled(
                        if active { "▌ " } else { "  " },
                        if active {
                            selection_marker()
                        } else {
                            Style::default()
                        },
                    ));
                    spans.push(Span::styled(fit_display_width(label, label_width), muted()));
                    spans.push(Span::raw(" "));

                    let editable_active = active && state.provision_selected_field_is_editable();
                    let editing_active = editable_active && state.input_mode() == InputMode::Insert;
                    let shown = if editing_active {
                        input_value_window(
                            value,
                            state.provision_field_cursor(),
                            value_width,
                            *secret,
                        )
                    } else if value.is_empty() {
                        fit_display_width("〈请输入〉", value_width)
                    } else if *secret {
                        fit_display_width(&"•".repeat(value.chars().count()), value_width)
                    } else {
                        fit_display_width(value, value_width)
                    };
                    let shown_width = crate::ui::disp_width(&shown).min(value_width);
                    spans.push(Span::styled(
                        shown,
                        if editing_active {
                            input_focused()
                        } else if active {
                            selected()
                        } else {
                            input()
                        },
                    ));
                    if editing_active && shown_width < value_width {
                        spans.push(Span::raw(" ".repeat(value_width - shown_width)));
                    }
                }
                form_lines.push(Line::from(spans));
            }
            if let Some(hint) = state.provision_field_hint(provision.field_selected) {
                form_lines.push(Line::from(""));
                form_lines.push(Line::from(vec![
                    Span::styled("提示  ", secondary()),
                    Span::styled(safe(&hint), muted()),
                ]));
            }
            form_lines.push(Line::from(""));
            let mut shortcuts = Vec::new();
            if state.input_mode() == InputMode::Insert {
                shortcuts.extend([
                    Span::styled("INSERT", accent().add_modifier(Modifier::BOLD)),
                    Span::raw("   "),
                    Span::styled("←/→", accent()),
                    Span::raw(" 光标   "),
                    Span::styled("输入/Backspace", secondary()),
                    Span::raw(" 编辑   "),
                    Span::styled("Enter/Esc", success()),
                    Span::raw(" 完成编辑"),
                ]);
            } else {
                shortcuts.extend([Span::styled("NORMAL", muted()), Span::raw("   ")]);
                shortcuts.extend([Span::styled("↑/↓", accent()), Span::raw(" 字段   ")]);
                if state.provision_selected_field_is_editable() {
                    shortcuts.extend([Span::styled("i", secondary()), Span::raw(" 编辑   ")]);
                } else {
                    shortcuts.extend([Span::styled("Space", secondary()), Span::raw(" 切换   ")]);
                }
                shortcuts.extend([
                    Span::styled("Enter", success()),
                    Span::raw(" 生成计划   "),
                    Span::styled("Esc", warning()),
                    Span::raw(" 返回"),
                ]);
            }
            form_lines.push(Line::from(shortcuts));
            if let Some(message) = &provision.message {
                form_lines.push(Line::from(Span::styled(safe(message), danger())));
            }

            let visible_height = form_geometry.height.saturating_sub(2) as usize;
            let scroll = selected_line.saturating_sub(visible_height.saturating_sub(3));
            if let Some(form_area) = form_area {
                frame.render_widget(
                    Paragraph::new(form_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(
                                    if focused_pane == crate::tui::pane::PaneId::ProvisionParameters
                                    {
                                        focused_panel()
                                    } else {
                                        panel()
                                    },
                                )
                                .title("参数"),
                        )
                        .scroll((scroll as u16, 0))
                        .wrap(Wrap { trim: false }),
                    form_area,
                );
            }

            if let Some(layout_area) = layout_area {
                let layout_model = state.provision_layout_model();
                let layout_details = state.provision_layout_editor_lines();
                let layout_summary = format!(
                    "{} · {} sectors",
                    provision.kind.title(),
                    layout_model.total_sectors
                );
                layout_model.render_pane(
                    frame,
                    layout_area,
                    crate::tui::disk_layout::DiskLayoutPane {
                        title: "磁盘布局",
                        summary: &layout_summary,
                        details: &layout_details,
                        focused: focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout,
                        scroll_y: state
                            .pane_viewport(crate::tui::pane::PaneId::ProvisionDiskLayout)
                            .scroll_y
                            .offset,
                    },
                );
            }
        }
        ProvisionStage::Planning => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈  正在生成精确计划", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在只读检查目标盘…"),
                    )),
                    Line::from("此阶段不写盘；正在计算 LCE、分区边界与协议元数据。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("只读规划"),
                ),
                main_area,
            );
        }
        ProvisionStage::Review => {
            let focused_pane = state.provision_focused_pane();
            let wide = main_area.width >= 108;
            let (summary_area, layout_area, changes_area) = if wide {
                let areas = Layout::horizontal([
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                    Constraint::Percentage(30),
                ])
                .split(main_area);
                (Some(areas[0]), Some(areas[1]), Some(areas[2]))
            } else {
                match focused_pane {
                    crate::tui::pane::PaneId::ProvisionDiskLayout => (None, Some(main_area), None),
                    crate::tui::pane::PaneId::ProvisionChanges => (None, None, Some(main_area)),
                    _ => (Some(main_area), None, None),
                }
            };

            if let Some(summary_area) = summary_area {
                let summary = state.provision_review_summary_lines();
                let lines = summary
                    .iter()
                    .map(|line| {
                        let style = if line.starts_with('✓') {
                            success()
                        } else if line.starts_with('⚠') {
                            warning()
                        } else {
                            muted()
                        };
                        Line::from(Span::styled(safe(line), style))
                    })
                    .collect::<Vec<_>>();
                let scroll = state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionSummary)
                    .scroll_y
                    .offset
                    .min(lines.len().saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(
                                    if focused_pane == crate::tui::pane::PaneId::ProvisionSummary {
                                        focused_panel()
                                    } else {
                                        panel()
                                    },
                                )
                                .title("计划摘要"),
                        )
                        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                        .wrap(Wrap { trim: false }),
                    summary_area,
                );
            }

            if let Some(layout_area) = layout_area {
                let layout_model = state.provision_layout_model();
                let layout_summary = format!(
                    "{} · {} sectors",
                    provision.kind.title(),
                    layout_model.total_sectors
                );
                layout_model.render_pane(
                    frame,
                    layout_area,
                    crate::tui::disk_layout::DiskLayoutPane {
                        title: "磁盘布局",
                        summary: &layout_summary,
                        details: &[],
                        focused: focused_pane == crate::tui::pane::PaneId::ProvisionDiskLayout,
                        scroll_y: state
                            .pane_viewport(crate::tui::pane::PaneId::ProvisionDiskLayout)
                            .scroll_y
                            .offset,
                    },
                );
            }

            if let Some(changes_area) = changes_area {
                let changes = state.provision_review_change_lines();
                let lines = changes
                    .iter()
                    .map(|line| {
                        let style = if line.starts_with('⚠') {
                            warning()
                        } else {
                            muted()
                        };
                        Line::from(Span::styled(safe(line), style))
                    })
                    .collect::<Vec<_>>();
                let scroll = state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionChanges)
                    .scroll_y
                    .offset
                    .min(lines.len().saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(
                                    if focused_pane == crate::tui::pane::PaneId::ProvisionChanges {
                                        focused_panel()
                                    } else {
                                        panel()
                                    },
                                )
                                .title("变更明细"),
                        )
                        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                        .wrap(Wrap { trim: false }),
                    changes_area,
                );
            }
        }
        ProvisionStage::ExportPath => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("导出目标绑定制盘镜像", secondary())),
                    Line::from(""),
                    Line::from("镜像包含目标盘硬件身份和原始 LBA3，不应写入另一块不同 U 盘。"),
                    Line::from(vec![
                        Span::styled("输出路径  ", muted()),
                        Span::styled(safe(&provision.export_path), input_focused()),
                    ]),
                    Line::from(""),
                    Line::from("直接输入编辑路径 · Backspace 删除 · Enter 开始导出 · Esc 返回"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("镜像导出"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Exporting => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈ 正在导出稀疏制盘镜像", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在写入镜像并执行 fsync…"),
                    )),
                    Line::from("导出完成前保持当前计划不变。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(focused_panel())
                        .title("镜像导出"),
                ),
                main_area,
            );
        }
        ProvisionStage::Confirm => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("破坏性写盘最终确认", danger())),
                    Line::from(""),
                    Line::from("请重新核对目标盘和计划。此操作会修改真实物理介质。"),
                    Line::from(vec![
                        Span::raw("精确输入 "),
                        Span::styled("YES", danger()),
                        Span::raw(" 后按 Enter： "),
                        Span::styled(safe(&provision.confirmation), input_focused()),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled("Esc 返回计划页，不会写盘。", warning())),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(danger())
                        .title("最终确认"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Running => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◆  安全事务执行中", warning())),
                    Line::from(""),
                    Line::from(safe(
                        provision.message.as_deref().unwrap_or("正在执行事务写盘…"),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "q / Esc / Ctrl-C 不会中断介质事务；退出请求只会在安全检查点生效。",
                        danger(),
                    )),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(warning())
                        .title("事务执行"),
                ),
                main_area,
            );
        }
        ProvisionStage::Result => {
            let has_format_failure = provision
                .message
                .as_deref()
                .is_some_and(|message| message.contains("格式化：✗"));
            let mut lines = vec![
                Line::from(Span::styled(
                    "制盘流程已到达安全结束点",
                    if has_format_failure {
                        warning()
                    } else {
                        success()
                    },
                )),
                Line::from(""),
            ];
            lines.extend(
                provision
                    .message
                    .as_deref()
                    .unwrap_or("操作结束")
                    .lines()
                    .map(|line| Line::from(safe(line))),
            );
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter / Esc 返回制盘中心",
                accent(),
            )));
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(success())
                        .title("结果"),
                ),
                main_area,
            );
        }
    }
}
