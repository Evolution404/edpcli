use super::*;

pub(super) fn draw_backup_create_choice(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(choice) = state.backup_create_choice() else {
        return;
    };
    let options = [
        ("Metadata", "协议/分区元数据与盘尾证据，速度快"),
        ("Deep", "进一步采集可验证分区/文件系统证据，耗时更长"),
    ];
    let mut lines = vec![
        Line::from(Span::styled("创建备份", accent())),
        Line::from(""),
    ];
    for (index, (name, description)) in options.into_iter().enumerate() {
        let active = index == choice.selected;
        lines.push(Line::from(vec![
            Span::styled(
                if active { "▌ " } else { "  " },
                if active {
                    selection_marker()
                } else {
                    Style::default()
                },
            ),
            Span::styled(name, if active { selected() } else { Style::default() }),
            Span::raw("  "),
            Span::styled(description, muted()),
        ]));
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled("j/k", accent()),
            Span::raw(" 选择   "),
            Span::styled("Enter/o", success()),
            Span::raw(" 确认   "),
            Span::styled("Esc/q", warning()),
            Span::raw(" 取消"),
        ]),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(focused_panel())
                    .title("备份类型"),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn draw_backups(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let (list_area, sidebar) = workspace_sidebar_layout(area);
    let visible_count = state.visible_backup_count();
    let total_count = state.backups().len();
    let count_label = if state.workspace_filter_active() {
        format!("{visible_count}/{total_count}")
    } else {
        total_count.to_string()
    };
    let backup_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(list_area);

    let summary_parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(38)])
        .split(backup_parts[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("总计 ", muted()),
            Span::styled(state.backups().len().to_string(), accent()),
            Span::styled("  ·  已选 ", muted()),
            Span::styled(state.backup_selection_count().to_string(), warning()),
        ]))
        .block(Block::default().borders(Borders::ALL).title("备份概览")),
        summary_parts[0],
    );
    let search_active = state.input_mode() == InputMode::Search;
    let search_filtered = state.workspace_filter_active();
    let search_text = if search_active {
        format!("/{}▌", safe(state.input_buffer()))
    } else if let Some(status) = state.search_status() {
        status
    } else {
        "/ 搜索姓名、部门、onlyid".to_string()
    };
    let search_style = if search_active {
        accent()
    } else if search_filtered {
        secondary()
    } else {
        muted()
    };
    frame.render_widget(
        Paragraph::new(search_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(search_style)
                    .title(if search_active {
                        "搜索 · 实时过滤"
                    } else {
                        "搜索"
                    }),
            )
            .style(search_style),
        summary_parts[1],
    );

    let title = if state.backup_scan_pending() {
        format!("备份列表 ({count_label}) · 扫描中…")
    } else {
        format!("备份列表 ({count_label})")
    };

    if visible_count == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(secondary());
        let inner = block.inner(backup_parts[1]);
        frame.render_widget(block, backup_parts[1]);
        let (heading, message, hint) = if state.workspace_filter_active() {
            (
                "没有匹配记录",
                "当前搜索条件没有匹配任何备份。",
                "继续输入可实时更新；清空搜索词后恢复全部备份。",
            )
        } else {
            (
                "暂无备份记录",
                "先在“设备”页面选择目标 U 盘，然后按 b 创建只读备份。",
                "备份创建完成后，这里会自动刷新。",
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
    } else {
        let window = visible_window(state.selected(), visible_count, backup_parts[1].height);
        let window_start = window.start;
        let rows = window
            .filter_map(|position| state.backup_at_visible(position))
            .map(|backup| {
                let (health, health_style) = backup_health(backup);
                TableRow::new(vec![
                    Cell::from(if state.backup_is_selected(&backup.path) {
                        "✓"
                    } else {
                        ""
                    })
                    .style(if state.backup_is_selected(&backup.path) {
                        warning()
                    } else {
                        muted()
                    }),
                    Cell::from(backup.index.to_string()).style(accent()),
                    Cell::from(safe(&backup.display_time)),
                    Cell::from(backup.provision_kind.short_name()).style(accent()),
                    Cell::from(
                        backup
                            .user
                            .as_deref()
                            .map(safe)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    Cell::from(
                        backup
                            .dept
                            .as_deref()
                            .map(safe)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    Cell::from(health).style(health_style),
                ])
            });
        let header =
            TableRow::new(["选", "#", "时间", "盘型", "姓名", "部门", "健康"]).style(accent());
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Length(17),
                Constraint::Length(23),
                Constraint::Length(12),
                Constraint::Min(22),
                Constraint::Length(11),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focused_panel())
                .title(title)
                .title_style(secondary()),
        )
        .row_highlight_style(selected())
        .highlight_symbol("▌ ");
        let mut table_state = TableState::default();
        table_state.select(Some(state.selected().saturating_sub(window_start)));
        frame.render_stateful_widget(table, backup_parts[1], &mut table_state);
    }

    if let Some((detail_area, animation_area)) = sidebar {
        let detail = if let Some(backup) = state.selected_backup() {
            let (health, health_style) = backup_health(backup);
            let content_width = detail_area.width.saturating_sub(2) as usize;
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("时间  ", muted()),
                    Span::raw(safe(&backup.display_time)),
                ]),
                Line::from(vec![
                    Span::styled("盘型  ", muted()),
                    Span::styled(backup.provision_kind.full_name(), accent()),
                ]),
                Line::from(format!(
                    "姓名  {}",
                    backup
                        .user
                        .as_deref()
                        .map(safe)
                        .unwrap_or_else(|| "—".into())
                )),
            ];
            lines.extend(wrapped_field_lines(
                "部门  ",
                backup.dept.as_deref().unwrap_or("—"),
                content_width,
            ));
            lines.extend([
                Line::from(format!(
                    "onlyid  {}",
                    backup
                        .onlyid
                        .as_deref()
                        .map(safe)
                        .unwrap_or_else(|| "—".into())
                )),
                Line::from(vec![
                    Span::styled("健康  ", muted()),
                    Span::styled(health, health_style.add_modifier(Modifier::BOLD)),
                ]),
            ]);
            lines.extend(wrapped_field_lines(
                "文件  ",
                &backup.file_name,
                content_width,
            ));
            lines.extend([
                Line::from(""),
                Line::from("提示：EDPB 保存协议与选定元数据范围，不保证包含普通分区全部用户文件。"),
                Line::from(""),
                Line::from(Span::styled(
                    "可用操作",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled("i", accent()),
                    Span::raw(" Inspect   "),
                    Span::styled("v", success()),
                    Span::raw(" 校验"),
                ]),
                Line::from(vec![
                    Span::styled("R", warning()),
                    Span::raw(" 恢复      "),
                    Span::styled("d", danger()),
                    Span::raw(" 删除"),
                ]),
                Line::from(vec![
                    Span::styled("a", accent()),
                    Span::raw(" 新建备份   "),
                    Span::styled("r", success()),
                    Span::raw(" 刷新"),
                ]),
            ]);
            Paragraph::new(lines)
        } else {
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "备份详情",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("选择一条备份后，这里会显示身份、健康状态和安全操作。"),
            ])
        }
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("备份详情")
                .title_style(secondary()),
        )
        .wrap(Wrap { trim: false });
        frame.render_widget(detail, detail_area);
        draw_workspace_animation(frame, animation_area, state, "BACKUP WORKSPACE");
    }
}

pub(super) fn draw_backup_delete(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(delete) = state.backup_delete() else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled("删除备份", danger())),
        Line::from(vec![
            Span::styled("文件: ", accent()),
            Span::raw(safe(&delete.path.display().to_string())),
        ]),
        Line::from("安全规则：固定选中时 SHA-256 → 删除前重新扫描 → 内容复核 → 至少保留该盘 1 份备份 → 删除单文件 .edpb"),
    ];
    match delete.stage {
        WizardStage::Confirm => {
            lines.push(Line::from(Span::styled(
                "这是不可撤销操作。请输入 YES 确认删除：",
                warning(),
            )));
            lines.push(Line::from(format!("> {}", safe(&delete.confirmation))));
        }
        WizardStage::Running => {
            lines.push(Line::from(
                "正在复核并删除；q / Esc / Ctrl-C 将延迟到安全结束点。",
            ));
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已结束；Esc 返回备份列表。"));
        }
    }
    if let Some(message) = &delete.message {
        lines.push(Line::from(safe(message)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(danger())
                    .title("危险操作 · 删除备份")
                    .title_style(danger()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn draw_backup_batch_delete(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(batch) = state.backup_batch_delete() else {
        return;
    };
    use super::super::state::BackupBatchDeleteStage;

    let planned = batch
        .prepared
        .as_ref()
        .map(|plan| plan.targets.len())
        .unwrap_or_else(|| state.backup_selection_count());
    let mut lines = vec![
        Line::from(Span::styled("批量删除备份", danger())),
        Line::from(format!("当前勾选: {} 份", state.backup_selection_count())),
        Line::from("安全规则：新鲜扫描逐项固定路径 + SHA-256 → 一次性保留底线检查 → 固定 DeletePlan → 执行时逐条复核。"),
        Line::from(""),
    ];
    match batch.stage {
        BackupBatchDeleteStage::Planning => {
            lines.push(Line::from(Span::styled(
                "正在生成固定批量删除计划…",
                secondary(),
            )));
        }
        BackupBatchDeleteStage::Review => {
            lines.extend([
                Line::from(Span::styled(
                    format!("计划已固定：将删除 {planned} 份备份。"),
                    warning(),
                )),
                Line::from("Enter 进入最终 YES 确认；Esc 取消计划并保留勾选。"),
            ]);
        }
        BackupBatchDeleteStage::Confirm => {
            lines.extend([
                Line::from(Span::styled(
                    format!("不可撤销：即将删除 {planned} 份备份。"),
                    danger(),
                )),
                Line::from(vec![
                    Span::raw("精确输入 "),
                    Span::styled("YES", danger()),
                    Span::raw(" 后按 Enter： "),
                    Span::styled(safe(&batch.confirmation), input_focused()),
                ]),
            ]);
        }
        BackupBatchDeleteStage::Running => {
            lines.push(Line::from(Span::styled(
                "正在按固定计划逐条复核并删除；退出请求延迟到安全结束点。",
                warning(),
            )));
        }
        BackupBatchDeleteStage::Result => {
            lines.push(Line::from(Span::styled(
                safe(batch.message.as_deref().unwrap_or("批量删除流程结束")),
                success(),
            )));
            lines.push(Line::from("Enter / Esc 返回备份列表。"));
        }
    }
    if batch.stage != BackupBatchDeleteStage::Result {
        if let Some(message) = &batch.message {
            lines.push(Line::from(Span::styled(safe(message), muted())));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(danger())
                    .title("危险操作 · 批量删除")
                    .title_style(danger()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn draw_backup_prune(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(prune) = state.backup_prune() else {
        return;
    };
    use super::super::state::BackupPruneStage;

    let mut lines = vec![
        Line::from(Span::styled("备份保留策略清理", warning())),
        Line::from("按同盘组执行 keep-N；原始盘备份与保留底线由 application 层统一保护。"),
        Line::from(""),
    ];
    match prune.stage {
        BackupPruneStage::Input => {
            lines.extend([
                Line::from(vec![
                    Span::styled("每组保留最近 N 份快照: ", accent()),
                    Span::styled(safe(&prune.keep_input), input_focused()),
                ]),
                Line::from("仅输入正整数；Enter 生成只读清理计划，Esc 取消。"),
            ]);
        }
        BackupPruneStage::Planning => {
            lines.push(Line::from(Span::styled(
                "正在扫描备份并生成固定候选快照…",
                secondary(),
            )));
        }
        BackupPruneStage::Review => {
            if let Some(prepared) = prune.prepared.as_ref() {
                lines.extend([
                    Line::from(format!("keep-N: {}", prepared.keep)),
                    Line::from(format!("原盘备份: {} 份", prepared.originals)),
                    Line::from(format!(
                        "计划删除: {} 份   清理后快照: {} 份",
                        prepared.plan.targets.len(),
                        prepared.retained_snapshots
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Enter 进入 YES 确认；执行时逐条按固定 SHA-256 复核。",
                        warning(),
                    )),
                ]);
            }
        }
        BackupPruneStage::Confirm => {
            let count = prune
                .prepared
                .as_ref()
                .map(|prepared| prepared.plan.targets.len())
                .unwrap_or(0);
            lines.extend([
                Line::from(Span::styled(
                    format!("即将删除 {count} 份旧备份，这是不可撤销操作。"),
                    danger(),
                )),
                Line::from(vec![
                    Span::raw("精确输入 "),
                    Span::styled("YES", danger()),
                    Span::raw(" 后按 Enter： "),
                    Span::styled(safe(&prune.confirmation), input_focused()),
                ]),
            ]);
        }
        BackupPruneStage::Running => {
            lines.push(Line::from(Span::styled(
                "正在逐条摘要复核并删除；退出请求会延迟到安全结束点。",
                warning(),
            )));
        }
        BackupPruneStage::Result => {
            lines.push(Line::from(Span::styled(
                safe(prune.message.as_deref().unwrap_or("清理流程结束")),
                success(),
            )));
            lines.push(Line::from("Enter / Esc 返回备份列表。"));
        }
    }
    if prune.stage != BackupPruneStage::Result {
        if let Some(message) = &prune.message {
            lines.push(Line::from(Span::styled(safe(message), danger())));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(warning())
                    .title("备份清理 · keep-N")
                    .title_style(warning()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// 把类型化写盘事件映射为向导 Running 阶段的单行显示文本。
/// 直接从事件类型映射，不经 ANSI 文本反解析；调用方负责经 `safe` 消毒。
pub(super) fn write_progress_text(event: &crate::application::WriteEvent) -> String {
    use crate::application::WriteEvent;
    match event {
        WriteEvent::BackupCreated { path } => format!(
            "备份完成：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::BackupCreatedIsNopwd => {
            "本份备份为免密状态快照（还原不会回到加密原盘）".to_string()
        }
        WriteEvent::RestoreMatchesHeader { onlyid, count, .. } => {
            format!("onlyid={onlyid} 匹配 {count} 个备份")
        }
        WriteEvent::RestoreMatchRow {
            index,
            time,
            is_nopwd,
            ..
        } => format!(
            "[{index}] {time} {}",
            if *is_nopwd {
                "免密状态"
            } else {
                "加密原盘"
            }
        ),
        WriteEvent::RestoreSelectionRetry { message } => message.clone(),
        WriteEvent::BackupShaVerified { .. } => "备份 SHA-256 校验通过".to_string(),
        WriteEvent::RestoreSnapshotNopwdWarning => {
            "该备份为免密状态快照；dry-run 不作还原".to_string()
        }
        WriteEvent::RestoreDryRunNotice { .. } => "[dry-run] 还原预览完成，未写入".to_string(),
        WriteEvent::RestoreTargetHeader { path } => format!(
            "还原目标已确认：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::RestoreWriteCompleted => "已还原，读回校验通过；请拔出重插".to_string(),
    }
}
