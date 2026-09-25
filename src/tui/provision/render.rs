use super::*;

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
                Span::styled(device_status(row), device_status_style(row)),
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
            let rows = (0..state.item_count()).filter_map(|index| {
                let row = state.provision_device_at(index)?;
                Some(TableRow::new(vec![
                    Cell::from(format!("disk{}", row.disk)),
                    Cell::from(format!("{:.2} GiB", row.size as f64 / 1_073_741_824.0)),
                    Cell::from(format!("{}:{}", safe(&row.vid), safe(&row.pid))),
                    Cell::from(device_status(row)),
                    Cell::from(safe(row.onlyid.as_deref().unwrap_or("—"))),
                ]))
            });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(9),
                    Constraint::Length(12),
                    Constraint::Length(13),
                    Constraint::Min(16),
                    Constraint::Length(15),
                ],
            )
            .header(TableRow::new(["设备", "容量", "USB 身份", "盘型", "onlyid"]).style(accent()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("制盘 · 先选择 USB 目标"),
            )
            .row_highlight_style(selected())
            .highlight_symbol("▶ ");
            let mut table_state = ratatui::widgets::TableState::default();
            if state.item_count() > 0 {
                table_state.select(Some(state.selected()));
            }
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::BackupPrompt => {
            let choices = ["先保存当前盘，再选择制盘模式", "不保存，直接选择制盘模式"];
            let mut lines = vec![
                Line::from(Span::styled("制盘前是否保存当前盘？", secondary())),
                Line::from(""),
                Line::from(Span::styled(
                    safe(&state.provision_backup_summary()),
                    warning(),
                )),
                Line::from("保存会创建当前盘的 EDPB 元数据备份，不会修改 U 盘。"),
                Line::from(""),
            ];
            for (index, choice) in choices.iter().enumerate() {
                lines.push(Line::from(if index == state.selected() {
                    vec![
                        Span::styled("▶ ", selected()),
                        Span::styled(*choice, selected()),
                    ]
                } else {
                    vec![Span::raw("  "), Span::raw(*choice)]
                }));
            }
            lines.extend([
                Line::from(""),
                Line::from(vec![
                    Span::styled("↑/↓", accent()),
                    Span::raw(" 选择   "),
                    Span::styled("Enter", success()),
                    Span::raw(" 确认   "),
                    Span::styled("Esc", warning()),
                    Span::raw(" 返回设备页"),
                ]),
            ]);
            if let Some(message) = &provision.message {
                lines.push(Line::from(Span::styled(safe(message), danger())));
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(secondary())
                            .title("制盘前保存"),
                    )
                    .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::BackupSaving => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈ 正在保存当前盘", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在创建制盘前 EDPB 备份…"),
                    )),
                    Line::from("完成前不会进入制盘模式选择。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("保存当前盘"),
                ),
                main_area,
            );
        }
        ProvisionStage::Menu => {
            let rows = ProvisionKind::ALL
                .into_iter()
                .enumerate()
                .map(|(index, kind)| {
                    TableRow::new(vec![
                        Cell::from(Span::styled(format!("{index}"), provision_kind_style(kind))),
                        Cell::from(Span::styled(kind.title(), provision_kind_style(kind))),
                        Cell::from(kind.description()),
                    ])
                });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(4),
                    Constraint::Length(30),
                    Constraint::Min(28),
                ],
            )
            .header(
                TableRow::new(["#", "制盘方案", "布局 / 行为"])
                    .style(accent())
                    .bottom_margin(1),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(secondary())
                    .title("制盘中心 · 选择方案")
                    .title_style(secondary()),
            )
            .row_highlight_style(selected())
            .highlight_symbol("▶ ");
            let mut table_state = TableState::default();
            table_state.select(Some(state.selected()));
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::Form => {
            let wide = main_area.width >= 96;
            let (form_area, layout_area) = if wide {
                let areas =
                    Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                        .split(main_area);
                (areas[0], areas[1])
            } else {
                let areas =
                    Layout::vertical([Constraint::Percentage(62), Constraint::Percentage(38)])
                        .split(main_area);
                (areas[0], areas[1])
            };
            let content_width = form_area.width.saturating_sub(2) as usize;
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
                    .clamp(6, 24);
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
                        if active { "▶ " } else { "  " },
                        if active { accent() } else { Style::default() },
                    ));
                    spans.push(Span::styled(fit_display_width(label, label_width), muted()));
                    spans.push(Span::raw(" "));

                    let editable_active = active && state.provision_selected_field_is_editable();
                    let shown = if editable_active {
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
                        if active { selected() } else { Style::default() },
                    ));
                    if editable_active && shown_width < value_width {
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
            let mut shortcuts = vec![Span::styled("↑/↓", accent()), Span::raw(" 字段   ")];
            if state.provision_selected_field_is_editable() {
                shortcuts.extend([
                    Span::styled("←/→", accent()),
                    Span::raw(" 光标   "),
                    Span::styled("输入/Backspace", secondary()),
                    Span::raw(" 编辑   "),
                ]);
            } else {
                shortcuts.extend([Span::styled("Space", secondary()), Span::raw(" 切换   ")]);
            }
            shortcuts.extend([
                Span::styled("Enter", success()),
                Span::raw(" 生成计划   "),
                Span::styled("Esc", warning()),
                Span::raw(" 返回"),
            ]);
            form_lines.push(Line::from(shortcuts));
            if let Some(message) = &provision.message {
                form_lines.push(Line::from(Span::styled(safe(message), danger())));
            }

            let visible_height = form_area.height.saturating_sub(2) as usize;
            let scroll = selected_line.saturating_sub(visible_height.saturating_sub(3));
            frame.render_widget(
                Paragraph::new(form_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(provision_kind_style(provision.kind))
                            .title("参数"),
                    )
                    .scroll((scroll as u16, 0))
                    .wrap(Wrap { trim: false }),
                form_area,
            );

            let bar_width = layout_area.width.saturating_sub(10) as usize;
            let bar = state.provision_layout_bar(bar_width);
            let mut bar_spans = vec![Span::styled("比例 [", muted())];
            let mut run_start = 0usize;
            while run_start < bar.len() {
                let kind = bar[run_start];
                let mut run_end = run_start + 1;
                while run_end < bar.len() && bar[run_end] == kind {
                    run_end += 1;
                }
                let style = partition_style(kind);
                bar_spans.push(Span::styled("━".repeat(run_end - run_start), style));
                run_start = run_end;
            }
            bar_spans.push(Span::styled("]", muted()));
            let bar_line = Line::from(bar_spans);
            let legend_line = if provision.kind == ProvisionKind::Plain {
                Line::from(vec![
                    Span::styled("■", partition_style(ProvisionBarKind::Plain)),
                    Span::raw(" 普通分区  "),
                    Span::styled("■", partition_style(ProvisionBarKind::Free)),
                    Span::raw(" 空闲"),
                ])
            } else {
                Line::from(vec![
                    Span::styled("■", partition_style(ProvisionBarKind::Boot)),
                    Span::raw(" 启动  "),
                    Span::styled("■", partition_style(ProvisionBarKind::Share)),
                    Span::raw(" 交换/二合一  "),
                    Span::styled("■", partition_style(ProvisionBarKind::Encrypt)),
                    Span::raw(" 保密  "),
                    Span::styled("■", partition_style(ProvisionBarKind::Compatibility)),
                    Span::raw(" 兼容  "),
                    Span::styled("■", partition_style(ProvisionBarKind::Free)),
                    Span::raw(" 空闲"),
                ])
            };
            let raw_layout_lines = state.provision_layout_editor_lines();
            let mut layout_lines = Vec::new();
            for (index, line) in raw_layout_lines.into_iter().enumerate() {
                if index == 3 {
                    layout_lines.push(bar_line.clone());
                    layout_lines.push(legend_line.clone());
                }
                let style = if line.starts_with("✗") {
                    danger()
                } else if line.starts_with("✓") {
                    success()
                } else if line.starts_with("当前:") {
                    accent()
                } else {
                    muted()
                };
                layout_lines.push(Line::from(Span::styled(safe(&line), style)));
            }
            frame.render_widget(
                Paragraph::new(layout_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(secondary())
                            .title("实时布局"),
                    )
                    .wrap(Wrap { trim: false }),
                layout_area,
            );
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
                        .border_style(secondary())
                        .title("只读规划"),
                ),
                main_area,
            );
        }
        ProvisionStage::Review => {
            let mut lines = vec![
                Line::from(Span::styled("计划已通过全部只读校验", success())),
                Line::from(""),
                Line::from(Span::styled(
                    provision.kind.title(),
                    provision_kind_style(provision.kind),
                )),
            ];
            if let Some(prepared) = provision.prepared.as_ref() {
                match prepared {
                    ProvisionPrepared::Plain(prepared) => {
                        let plan = &prepared.plan;
                        lines.push(Line::from(format!(
                            "目标: disk{} · 恢复普通盘 · {} 个 MBR 主分区 · {} sectors",
                            prepared.disk,
                            plan.partitions.len(),
                            plan.total_sectors
                        )));
                        lines.push(Line::from(format!(
                            "来源状态: {}   来源 LCE cleanup: {}",
                            prepared.source_kind.short_name(),
                            prepared
                                .source_lce_start_lba
                                .map(|lba| format!("LBA{lba}..{}", lba + 5))
                                .unwrap_or_else(|| "无".into())
                        )));
                        lines.push(Line::from(format!(
                            "事务触碰: {} sectors   最高写入 LBA: {}",
                            prepared.write_plan.touched_sector_count(),
                            prepared.write_plan.highest_touched_lba().unwrap_or(0)
                        )));
                        lines.push(Line::from("LBA3 已从目标盘捕获并绑定；写入前将再次复核。"));
                        for (index, part) in plan.partitions.iter().enumerate() {
                            lines.push(Line::from(format!(
                                "P{} LBA{}..{} · {} sectors · {} · 卷标:{}",
                                index + 1,
                                part.start_lba,
                                part.end_lba().unwrap_or(part.start_lba),
                                part.sector_count,
                                part.filesystem.windows_format_name(),
                                safe(&part.volume_label)
                            )));
                        }
                        for gap in &plan.gaps {
                            lines.push(Line::from(format!(
                                "空闲 LBA{}..{} · {} sectors",
                                gap.start_lba,
                                gap.end_lba(),
                                gap.sector_count
                            )));
                        }
                        lines.push(Line::from(Span::styled(
                            "将清除 EDP 协议状态并重建上述普通分区；这不是安全擦除。",
                            warning(),
                        )));
                    }
                    ProvisionPrepared::Official(prepared) => {
                        lines.extend([
                            Line::from(format!(
                                "目标: disk{}  {}",
                                prepared.disk,
                                safe(&prepared.device_id)
                            )),
                            Line::from(format!(
                                "容量: {} sectors   LCE: LBA{}",
                                prepared.write_image.total_sectors, prepared.lce_start_lba
                            )),
                            Line::from(format!(
                                "事务触碰: {} sectors   最高写入 LBA: {}",
                                prepared.write_image.touched_sector_count(),
                                prepared.write_image.highest_touched_lba().unwrap_or(0)
                            )),
                            Line::from("LBA3 已从目标盘捕获并绑定；写入前将再次复核。"),
                            Line::from("先写协议/LCE 并验证，再对勾选的分区单独格式化并验证。"),
                            Line::from(format!(
                                "初始化密码强制修改: {}",
                                if prepared.force_change_password {
                                    "是"
                                } else {
                                    "否"
                                }
                            )),
                            Line::from(format!(
                                "取消密码复杂性验证: {}",
                                if prepared.pass_info_policy.cancel_password_complexity_check {
                                    "是"
                                } else {
                                    "否"
                                }
                            )),
                            Line::from(format!(
                                "交换区密码最大错误次数: {}",
                                prepared.pass_info_policy.max_share_password_errors
                            )),
                            Line::from(format!(
                                "保密区密码最大错误次数: {}",
                                prepared.pass_info_policy.max_encrypt_password_errors
                            )),
                            Line::from("制盘后格式化:"),
                        ]);
                        for choice in &prepared.format_targets {
                            let target = &choice.target;
                            lines.push(Line::from(format!(
                                "{} {} type{} {} {}{} 卷标:{}",
                                if !target.format_capable {
                                    "—"
                                } else if choice.selected {
                                    "☑"
                                } else {
                                    "☐"
                                },
                                target.role.label(),
                                target.geometry.partition_type.raw(),
                                if !target.format_capable {
                                    "不可格式化"
                                } else if target.physically_encrypted {
                                    "加密"
                                } else {
                                    "明文"
                                },
                                choice
                                    .filesystem
                                    .map(|format| format.windows_format_name())
                                    .unwrap_or("—"),
                                target
                                    .visible_mbr_type
                                    .map(|mbr| format!(" / MBR 0x{mbr:02X}"))
                                    .unwrap_or_default(),
                                if target.format_capable {
                                    choice.volume_label.as_str()
                                } else {
                                    "—"
                                }
                            )));
                        }
                        if let Some(target_plan) = &prepared.target_plan {
                            lines.push(Line::from(""));
                            lines.push(Line::from(format!(
                                "未分配空间: {} sectors",
                                target_plan.unallocated_sectors
                            )));
                            for part in &target_plan.partitions {
                                let end = part.geometry.start_lba + part.geometry.sector_count - 1;
                                let action = match part.action {
                                    crate::provision::PartitionAction::PreserveExact => {
                                        "原数据可保留 · 复用原 FileKey · 不写数据区"
                                    }
                                    crate::provision::PartitionAction::Rebuild => {
                                        "将重建 · 原数据不可原样保留"
                                    }
                                };
                                lines.push(Line::from(format!(
                                    "{} LBA{}..{} ({} sectors): {}",
                                    part.geometry.role.label(),
                                    part.geometry.start_lba,
                                    end,
                                    part.geometry.sector_count,
                                    action
                                )));
                                lines.push(Line::from(format!("  {}", part.reason)));
                            }
                            if target_plan.partitions.iter().all(|part| {
                                part.action == crate::provision::PartitionAction::Rebuild
                            }) {
                                lines.push(Line::from(vec![
                                    Span::styled("E", secondary()),
                                    Span::raw(" 导出与该目标绑定的稀疏制盘镜像"),
                                ]));
                            }
                        }
                    }
                }
            }
            if let Some(message) = &provision.message {
                lines.push(Line::from(Span::styled(safe(message), success())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", danger()),
                Span::raw(" 进入最终 YES 确认   "),
                Span::styled("Esc", warning()),
                Span::raw(" 返回修改"),
            ]));
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(success())
                            .title("计划预览"),
                    )
                    .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::ExportPath => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("导出目标绑定制盘镜像", secondary())),
                    Line::from(""),
                    Line::from("镜像包含目标盘硬件身份和原始 LBA3，不应写入另一块不同 U 盘。"),
                    Line::from(vec![
                        Span::styled("输出路径  ", muted()),
                        Span::styled(safe(&provision.export_path), selected()),
                    ]),
                    Line::from(""),
                    Line::from("直接输入编辑路径 · Backspace 删除 · Enter 开始导出 · Esc 返回"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
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
                        .border_style(secondary())
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
                        Span::styled(safe(&provision.confirmation), selected()),
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
