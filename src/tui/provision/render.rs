use super::*;
use crate::tui::state::ProvisionFieldSection;
use crate::tui::state::{ProvisionReviewRowKind, ProvisionReviewTone};

const INPUT_EDITING_SLACK: usize = 2;

fn draw_provision_stepper(frame: &mut Frame, area: ratatui::layout::Rect, stage: ProvisionStage) {
    let current = match stage {
        ProvisionStage::SelectDisk => 0,
        ProvisionStage::Menu | ProvisionStage::Form => 1,
        ProvisionStage::Planning => 2,
        ProvisionStage::Review
        | ProvisionStage::ExportPath
        | ProvisionStage::Exporting
        | ProvisionStage::Confirm => 3,
        ProvisionStage::Running => 4,
        ProvisionStage::Result => 5,
    };
    let names = [
        "选择设备",
        "制盘配置",
        "分区预览",
        "计划确认",
        "执行",
        "完成",
    ];
    let class = crate::tui::ui::ViewportClass::for_width(area.width);
    let line = if class == crate::tui::ui::ViewportClass::Compact {
        Line::from(format!("{}/6  {}", current + 1, names[current]))
    } else {
        Line::from(
            names
                .into_iter()
                .enumerate()
                .flat_map(|(index, name)| {
                    let tone = if index == current {
                        accent()
                    } else if index < current {
                        success()
                    } else {
                        muted()
                    };
                    let mut spans = Vec::new();
                    if index > 0 {
                        spans.push(Span::styled(" ─ ", muted()));
                    }
                    spans.push(Span::styled(format!("{} {name}", index + 1), tone));
                    spans
                })
                .collect::<Vec<_>>(),
        )
    };
    frame.render_widget(Paragraph::new(line), area);
}

pub(super) fn draw_provision(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let provision = state.provision();
    let sections = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    draw_provision_stepper(frame, sections[0], provision.stage);
    let (main_area, sidebar) = if provision.stage == ProvisionStage::Running {
        (sections[1], None)
    } else {
        workspace_sidebar_layout(sections[1])
    };

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
                .block(crate::tui::ui::card("固定目标", false))
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
                .block(crate::tui::ui::card("写盘保护", false))
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
            let title = format!(
                "制盘 · 先选择 USB 目标 · h/l 横向滚动 · {}",
                viewport.position_label()
            );
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
            );
            let table = crate::tui::ui::data_table(&title, header, rows, viewport.widths(), true);
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
            let title = format!(
                "制盘中心 · 选择方案 · h/l 横向滚动 · {}",
                viewport.position_label()
            );
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
            .bottom_margin(1);
            let table = crate::tui::ui::data_table(&title, header, rows, viewport.widths(), true);
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
            let rows = state.provision_compact_field_rows_typed();
            let mut section_metrics: HashMap<ProvisionFieldSection, (usize, usize, usize, usize)> =
                HashMap::new();
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
            let mut current_section: Option<ProvisionFieldSection> = None;
            let mut selected_line = 0usize;
            for (section, indexes) in rows {
                if current_section != Some(section) {
                    form_lines.push(Line::from(""));
                    form_lines.push(Line::from(Span::styled(section.label(), secondary())));
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
                        .get(&section)
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
                        .block(crate::tui::ui::card(
                            "参数",
                            focused_pane == crate::tui::pane::PaneId::ProvisionParameters,
                        ))
                        .scroll((scroll as u16, 0))
                        .wrap(Wrap { trim: false }),
                    form_area,
                );
            }

            if let Some(layout_area) = layout_area {
                let layout_model = state.provision_layout_model();
                let layout_details = state.provision_layout_editor_details();
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
                let summary = state.provision_review_summary_rows();
                let lines = summary
                    .iter()
                    .map(|row| {
                        let style = match row.tone {
                            ProvisionReviewTone::Muted => match row.kind {
                                ProvisionReviewRowKind::Action => accent(),
                                _ => muted(),
                            },
                            ProvisionReviewTone::Accent => accent(),
                            ProvisionReviewTone::Success => success(),
                            ProvisionReviewTone::Warning => warning(),
                        };
                        Line::from(Span::styled(safe(&row.text), style))
                    })
                    .collect::<Vec<_>>();
                let scroll = state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionSummary)
                    .scroll_y
                    .offset
                    .min(lines.len().saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(crate::tui::ui::card(
                            "计划摘要",
                            focused_pane == crate::tui::pane::PaneId::ProvisionSummary,
                        ))
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
                let changes = state.provision_review_change_rows();
                let lines = changes
                    .iter()
                    .map(|row| {
                        let style = match row.tone {
                            ProvisionReviewTone::Muted => muted(),
                            ProvisionReviewTone::Accent => accent(),
                            ProvisionReviewTone::Success => success(),
                            ProvisionReviewTone::Warning => warning(),
                        };
                        let mut line = Line::from(Span::styled(safe(&row.text), style));
                        if let Some(label) = row.badge {
                            let tone = match row.tone {
                                ProvisionReviewTone::Success => crate::tui::ui::BadgeTone::Success,
                                ProvisionReviewTone::Warning => crate::tui::ui::BadgeTone::Warning,
                                ProvisionReviewTone::Accent => crate::tui::ui::BadgeTone::Accent,
                                ProvisionReviewTone::Muted => crate::tui::ui::BadgeTone::Neutral,
                            };
                            let mut badge = crate::tui::ui::status_badge(label, tone);
                            line.spans.insert(0, Span::raw(" "));
                            line.spans.splice(0..0, badge.spans.drain(..));
                        }
                        line
                    })
                    .collect::<Vec<_>>();
                let scroll = state
                    .pane_viewport(crate::tui::pane::PaneId::ProvisionChanges)
                    .scroll_y
                    .offset
                    .min(lines.len().saturating_sub(1));
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(crate::tui::ui::card(
                            "变更明细",
                            focused_pane == crate::tui::pane::PaneId::ProvisionChanges,
                        ))
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
            draw_provision_running(frame, main_area, state);
        }
        ProvisionStage::Result => {
            use crate::application::provision::ProvisionExecutionStatus as Status;
            let result_style = match provision.result_status {
                Some(Status::Success) => success(),
                Some(Status::CompletedWithWarnings | Status::PartialFormatFailure) => warning(),
                Some(Status::FatalFailure) | None => danger(),
            };
            let result_title = match provision.result_status {
                Some(Status::Success) => "制盘成功",
                Some(Status::CompletedWithWarnings) => "制盘完成，存在警告",
                Some(Status::PartialFormatFailure) => "部分完成：格式化失败",
                Some(Status::FatalFailure) | None => "制盘失败",
            };
            let badge_tone = match provision.result_status {
                Some(Status::Success) => crate::tui::ui::BadgeTone::Success,
                Some(Status::CompletedWithWarnings | Status::PartialFormatFailure) => {
                    crate::tui::ui::BadgeTone::Warning
                }
                Some(Status::FatalFailure) | None => crate::tui::ui::BadgeTone::Danger,
            };
            let mut lines = vec![
                crate::tui::ui::status_badge(result_title, badge_tone),
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
            if let Some(run) = &provision.run {
                let elapsed = run
                    .last_activity_at
                    .duration_since(run.started_at)
                    .as_secs();
                lines.push(Line::from(format!("总耗时  {elapsed} 秒")));
                let mut phases = std::collections::BTreeMap::new();
                for event in &run.log {
                    phases
                        .entry(event.phase)
                        .and_modify(|last: &mut (std::time::Instant, std::time::Instant)| {
                            last.1 = event.emitted_at
                        })
                        .or_insert((event.emitted_at, event.emitted_at));
                }
                for (phase, (first, last)) in phases {
                    lines.push(Line::from(format!(
                        "{}  {} 秒",
                        phase.label(),
                        last.duration_since(first).as_secs()
                    )));
                }
                lines.push(Line::from(Span::styled("最近进度事件", accent())));
                for event in run.log.iter().rev().take(6).rev() {
                    lines.push(Line::from(safe(&format!(
                        "[{}/{}] {}  {}",
                        event.current,
                        event.total,
                        event.phase.label(),
                        event.step.label()
                    ))));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter / Esc 返回制盘中心",
                accent(),
            )));
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .block(crate::tui::ui::card("制盘结果", true).border_style(result_style)),
                main_area,
            );
        }
    }
}

fn draw_provision_running(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let run = state.provision().run.as_ref();
    let latest = run.and_then(|run| run.latest.as_ref());
    let progress = latest
        .map(|event| {
            format!(
                "{}/{} 步 · {}%",
                event.current,
                event.total,
                event.current.saturating_mul(100) / event.total
            )
        })
        .unwrap_or_else(|| "等待进度事件".into());
    let phase = latest.map(|event| event.phase.label()).unwrap_or("准备中");
    let step = latest
        .map(|event| event.step.label())
        .unwrap_or("等待第一步");
    if area.height < 18 {
        let lines = [
            Line::from(format!("总体进度  {progress}")),
            Line::from(format!("当前阶段  {phase}")),
            Line::from(format!("当前步骤  {step}")),
            Line::from("运行日志  详见较高窗口"),
            Line::from("安全提示  退出请求仅在安全检查点生效"),
        ];
        frame.render_widget(
            Paragraph::new(lines.to_vec()).block(crate::tui::ui::card("安全事务执行中", true)),
            area,
        );
        return;
    }

    let areas = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Min(4),
        Constraint::Length(3),
    ])
    .split(area);
    let now = std::time::Instant::now();
    let elapsed = run
        .map(|run| now.duration_since(run.started_at).as_secs())
        .unwrap_or(0);
    let activity = run
        .map(|run| now.duration_since(run.last_activity_at).as_secs())
        .unwrap_or(0);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("总体进度  {progress}")),
            Line::from(format!(
                "运行时间  {elapsed} 秒    最近活动  {activity} 秒前"
            )),
        ])
        .block(crate::tui::ui::card("安全事务执行中", true)),
        areas[0],
    );
    if let Some(event) = latest {
        let gauge_area = ratatui::layout::Rect {
            x: areas[0].x.saturating_add(2),
            y: areas[0].y.saturating_add(3),
            width: areas[0].width.saturating_sub(4),
            height: 1,
        };
        frame.render_widget(
            ratatui::widgets::Gauge::default()
                .ratio(event.current as f64 / event.total as f64)
                .gauge_style(accent()),
            gauge_area,
        );
    }
    let middle = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(areas[1]);
    frame.render_widget(
        Paragraph::new(format!("当前阶段  {phase}")).block(crate::tui::ui::card("当前阶段", false)),
        middle[0],
    );
    let mut step_lines = vec![Line::from(format!("当前步骤  {step}"))];
    if let Some(work) = latest.and_then(|event| event.work) {
        step_lines.push(Line::from(format!(
            "扇区活动  {}/{}",
            work.current, work.total
        )));
    }
    frame.render_widget(
        Paragraph::new(step_lines).block(crate::tui::ui::card("当前任务", false)),
        middle[1],
    );
    let mut log_lines = Vec::new();
    if let Some(run) = run {
        let viewport = state
            .provision()
            .pane_focus
            .viewport(crate::tui::pane::PaneId::ProvisionRunLog);
        let available = usize::from(areas[2].height.saturating_sub(2));
        let offset = if viewport.selected.is_none() {
            run.log.len().saturating_sub(available)
        } else {
            viewport.scroll_y.offset
        };
        log_lines.extend(run.log.iter().skip(offset).take(available).map(|event| {
            let detail = match event.step {
                crate::application::progress::Step::PartitionFormat(role) => {
                    format!(" {}", role.label())
                }
                _ => String::new(),
            };
            let work = event
                .work
                .map(|work| format!("  {:?} {}/{}", work.phase, work.current, work.total))
                .unwrap_or_default();
            Line::from(safe(&format!(
                "[{}/{}] {}  {}{}{}",
                event.current,
                event.total,
                event.phase.label(),
                event.step.label(),
                detail,
                work
            )))
        }));
    }
    if log_lines.is_empty() {
        log_lines.push(Line::from("等待进度事件"));
    }
    frame.render_widget(
        Paragraph::new(log_lines).block(crate::tui::ui::card(
            "运行日志 · j/k 滚动 · G 跟随末尾",
            true,
        )),
        areas[2],
    );
    frame.render_widget(
        Paragraph::new("q / Esc / Ctrl-C 退出请求只在安全检查点生效；介质事务继续受保护。")
            .block(crate::tui::ui::card("安全提示", false)),
        areas[3],
    );
}
