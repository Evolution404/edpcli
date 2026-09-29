use super::*;

fn backup_table_values(
    backup: &crate::application::BackupWorkspaceItem,
    checked: bool,
    columns: &[crate::tui::table_layout::TableColumnSpec],
) -> Vec<(String, Style)> {
    use crate::tui::table_layout::ColumnId;
    let (health, health_style) = backup_health(backup);
    let identity = crate::application::identity::WorkspaceIdentity::from_backup(backup);
    let cells = identity.display_cells();
    columns
        .iter()
        .map(|column| match column.id {
            ColumnId::Selected => (
                if checked { "✓".into() } else { String::new() },
                if checked { warning() } else { muted() },
            ),
            ColumnId::Index => (backup.index.to_string(), accent()),
            ColumnId::Name => (safe(&backup.file_name), Style::default()),
            ColumnId::Time => (safe(&backup.display_time), Style::default()),
            ColumnId::Capacity => (safe(&cells[0]), Style::default()),
            ColumnId::VidPid => (safe(&cells[1]), secondary()),
            ColumnId::Model => (safe(&cells[2]), Style::default()),
            ColumnId::Onlyid => (safe(&cells[3]), Style::default()),
            ColumnId::User => (safe(&cells[4]), Style::default()),
            ColumnId::Dept => (safe(&cells[5]), Style::default()),
            ColumnId::ProvisionKind => (
                safe(&cells[6]),
                backup
                    .provision_kind
                    .map(|kind| crate::tui::theme::current().provision_kind(kind))
                    .unwrap_or_else(warning),
            ),
            ColumnId::Health => (health.into(), health_style),
            _ => unreachable!("backup schema only contains backup and identity columns"),
        })
        .collect()
}

pub(super) fn draw_backup_create_choice(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(_choice) = state.backup_create_choice() else {
        return;
    };
    crate::tui::ui::render_action_confirmation_modal(
        frame,
        area,
        crate::tui::ui::ActionConfirmationSpec {
            title: "创建元数据备份",
            headline: "创建只读元数据备份？",
            details: vec![
                Line::from("✓ 物理身份 / 几何 / 分区结构 / EDP 协议元数据"),
                Line::from("✗ 不读取文件系统目录和用户文件"),
                Line::from(Span::styled("这是只读操作，不需要介质写入授权。", muted())),
            ],
            tone: crate::tui::ui::ConfirmationTone::Neutral,
        },
    );
}

pub(super) fn draw_backups(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    if let Some(run) = state.backup_verify_run() {
        let mut lines = vec![
            Line::from(Span::styled("备份校验进行中", accent())),
            Line::from(format!("对象  {}", safe(&run.path.display().to_string()))),
            Line::from(format!(
                "当前阶段  {} · {} · {}/{}",
                run.latest.phase.label(),
                run.latest.step.label(),
                run.latest.current,
                run.latest.total
            )),
            Line::from(""),
            Line::from(Span::styled("运行日志", secondary())),
        ];
        lines.extend(run.log.iter().map(|event| {
            Line::from(safe(&format!(
                "[{}/{}] {}  {}",
                event.current,
                event.total,
                event.phase.label(),
                event.detail.as_deref().unwrap_or(event.step.label())
            )))
        }));
        frame.render_widget(
            Paragraph::new(lines)
                .block(crate::tui::ui::card("备份校验", true))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    use crate::tui::pane::PaneId;
    use crate::tui::ui::ViewportClass;
    let focused = state.backups_focused_pane();
    let class = ViewportClass::for_width(area.width);
    let (list_area, detail_area, coverage_area) = if class == ViewportClass::Compact {
        match focused {
            PaneId::BackupSummary => (None, Some(area), None),
            PaneId::BackupCoverage => (None, None, Some(area)),
            _ => (Some(area), None, None),
        }
    } else {
        let parts = Layout::vertical([Constraint::Percentage(56), Constraint::Min(8)]).split(area);
        if class == ViewportClass::Standard {
            if focused == PaneId::BackupCoverage {
                (Some(parts[0]), None, Some(parts[1]))
            } else {
                (Some(parts[0]), Some(parts[1]), None)
            }
        } else {
            let bottom =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(parts[1]);
            (Some(parts[0]), Some(bottom[0]), Some(bottom[1]))
        }
    };
    if let Some(list_area) = list_area {
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

        let counts = crate::tui::overview::ProvisionKindCounts::from_kinds(
            state.backups().iter().map(|backup| backup.provision_kind),
        );
        let metrics = crate::tui::overview::overview_metrics(counts);
        let search = crate::tui::overview::overview_search(state, "/ 搜索身份、容量、型号、文件名");
        crate::tui::ui::workspace_overview(frame, backup_parts[0], "备份概览", &metrics, &search);

        let title = if state.backup_scan_pending() {
            format!(
                "备份列表 ({count_label}) · 已选 {} · 扫描中…",
                state.backup_selection_count()
            )
        } else {
            format!(
                "备份列表 ({count_label}) · 已选 {}",
                state.backup_selection_count()
            )
        };

        if visible_count == 0 {
            let pane_focused = focused == PaneId::BackupsList;
            let block = crate::tui::ui::card(title, pane_focused);
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
            use crate::tui::table_layout::{
                display_width, render_table_scrollbars, table_column_schema, table_heading,
                table_position_label, visible_cell, TableKind,
            };
            let columns = table_column_schema(TableKind::Backups).expect("backup schema");
            let headings = columns
                .iter()
                .map(|column| column.heading)
                .collect::<Vec<_>>();
            let mut content_widths = headings
                .iter()
                .map(|heading| display_width(heading))
                .collect::<Vec<_>>();
            for backup in state.backups() {
                for (index, (value, _)) in backup_table_values(backup, false, &columns)
                    .iter()
                    .enumerate()
                {
                    content_widths[index] = content_widths[index].max(display_width(value));
                }
            }
            let order = state.table_column_order(TableKind::Backups);
            let layout = state.table_visual_layout(TableKind::Backups);
            let visual_widths = state.table_visual_widths(TableKind::Backups, &content_widths);
            let interaction = state.table_interaction(TableKind::Backups);
            let viewport = layout.layout_with_active(
                backup_parts[1].width.saturating_sub(4),
                &visual_widths,
                interaction.viewport_offset(),
                Some(interaction.active_column()),
            );
            let pane_focused =
                state.backups_focused_pane() == crate::tui::pane::PaneId::BackupsList;
            let window = visible_window(state.selected(), visible_count, backup_parts[1].height);
            let window_start = window.start;
            let window_len = window.len();
            let rows = window
                .filter_map(|position| state.backup_at_visible(position))
                .map(|backup| {
                    let values = backup_table_values(
                        backup,
                        state.backup_is_selected(&backup.path),
                        &columns,
                    );
                    TableRow::new(
                        viewport
                            .columns
                            .iter()
                            .map(|column| {
                                let logical = order[column.index];
                                let (value, style) = &values[logical];
                                Cell::from(visible_cell(value, column)).style(
                                    crate::tui::theme::current().table_cell(
                                        *style,
                                        column.index == interaction.active_column(),
                                        pane_focused,
                                    ),
                                )
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
                        let style = crate::tui::theme::current().table_header(
                            column.index == interaction.active_column(),
                            pane_focused,
                        );
                        Cell::from(visible_cell(&label, column)).style(style)
                    })
                    .collect::<Vec<_>>(),
            );
            let table_title = format!(
                "{title} · {}",
                table_position_label(&layout, interaction, &viewport)
            );
            let table = crate::tui::ui::data_table(
                &table_title,
                header,
                rows,
                viewport.widths(),
                pane_focused,
            );
            let mut table_state = TableState::default();
            table_state.select(Some(state.selected().saturating_sub(window_start)));
            frame.render_stateful_widget(table, backup_parts[1], &mut table_state);
            render_table_scrollbars(
                frame,
                backup_parts[1],
                &viewport,
                visible_count,
                window_start,
                window_len,
            );
        }
    }
    if let Some(detail_area) = detail_area {
        let detail = if let Some(backup) = state.selected_backup() {
            let (health, health_style) = backup_health(backup);
            let identity = crate::application::identity::WorkspaceIdentity::from_backup_against(
                backup,
                state.selected_device(),
            );
            let cells = identity.display_cells();
            let content_width = detail_area.width.saturating_sub(2) as usize;
            use crate::application::media_identity::MediaRelationship;
            let (relation_label, relation_tone) =
                match identity.canonical.as_ref().map(|value| value.relationship) {
                    Some(MediaRelationship::SamePhysicalMedia) => {
                        ("已确认 · 物理介质", crate::tui::ui::BadgeTone::Success)
                    }
                    Some(MediaRelationship::DifferentMedia) => {
                        ("硬件冲突", crate::tui::ui::BadgeTone::Danger)
                    }
                    Some(
                        MediaRelationship::SameEdpInstance
                        | MediaRelationship::SameControlledLineage,
                    ) => ("协议相关 · 物理未确认", crate::tui::ui::BadgeTone::Warning),
                    Some(
                        MediaRelationship::ProbableSameMedia
                        | MediaRelationship::ModelOnlyMatch
                        | MediaRelationship::Ambiguous,
                    ) => (
                        "可能相关 · 不可唯一确认",
                        crate::tui::ui::BadgeTone::Warning,
                    ),
                    None => ("身份未验证", crate::tui::ui::BadgeTone::Neutral),
                };
            let mut lines = vec![
                crate::tui::ui::status_badge(relation_label, relation_tone),
                Line::from(vec![
                    Span::styled("健康  ", muted()),
                    Span::styled(health, health_style.add_modifier(Modifier::BOLD)),
                ]),
                Line::from(Span::styled(
                    "不备份目录和用户文件；恢复仅用于结构与协议元数据。",
                    warning(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("盘型  ", muted()),
                    Span::styled(
                        safe(&cells[6]),
                        backup
                            .provision_kind
                            .map(|kind| crate::tui::theme::current().provision_kind_emphasis(kind))
                            .unwrap_or_else(warning),
                    ),
                ]),
                Line::from(format!("容量  {}", safe(&cells[0]))),
                Line::from(format!("VID:PID  {}", safe(&cells[1]))),
                Line::from(format!("型号  {}", safe(&cells[2]))),
                Line::from(format!(
                    "device_id  {}",
                    safe(identity.device_id.as_deref().unwrap_or("—"))
                )),
                Line::from(format!("onlyid  {}", safe(&cells[3]))),
                Line::from(format!("介质识别  {}", safe(identity.canonical_status()))),
                Line::from(format!("姓名  {}", safe(&cells[4]))),
            ];
            if let Some(canonical) = &identity.canonical {
                lines.extend(
                    canonical
                        .evidence_lines()
                        .into_iter()
                        .take(3)
                        .map(|line| Line::from(safe(&line))),
                );
            }
            lines.extend(wrapped_field_lines("部门  ", &cells[5], content_width));
            lines.extend([
                Line::from(format!("备份时间  {}", safe(&backup.display_time))),
                Line::from(format!("备份编号  #{}", backup.index)),
            ]);
            if let Some(sha) = &backup.content_sha256 {
                lines.push(Line::from(format!("SHA-256  {}", safe(sha))));
            }
            lines.extend(wrapped_field_lines(
                "文件  ",
                &backup.file_name,
                content_width,
            ));
            lines.extend([
                Line::from(""),
                Line::from("EDPB 仅保存结构与协议元数据；不保存目录或用户文件。"),
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
                    Span::styled("b", accent()),
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
        .block(crate::tui::ui::card(
            "备份详情",
            focused == crate::tui::pane::PaneId::BackupSummary,
        ))
        .scroll((
            state
                .pane_viewport(crate::tui::pane::PaneId::BackupSummary)
                .scroll_y
                .offset
                .min(u16::MAX as usize) as u16,
            0,
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(detail, detail_area);
    }
    if let Some(coverage_area) = coverage_area {
        draw_backup_coverage(frame, coverage_area, state);
    }
}

fn draw_backup_coverage(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let mut lines = Vec::new();
    match state.selected_backup() {
        Some(backup) => match backup.coverage.as_ref() {
            Some(coverage) => {
                lines.push(Line::from(format!(
                    "{} 区域 · {} Extent · {} Artifact",
                    coverage.regions.len(),
                    coverage.extent_count,
                    coverage.artifact_count
                )));
                for region in &coverage.regions {
                    let label = match region.role.as_str() {
                        "protocol" => "EDP 主协议区",
                        "data" | "front" => "数据前部",
                        "lce" => "LCE",
                        "tail" => "盘尾",
                        "filesystem" => "文件系统",
                        _ => region.role.as_str(),
                    };
                    let (bar, count) = match region.total_sectors {
                        Some(total) if total > 0 => {
                            let cells = (region.captured_sectors.min(total).saturating_mul(10)
                                / total) as usize;
                            (
                                format!("{}{}", "█".repeat(cells), "░".repeat(10 - cells)),
                                format!("{}/{} sector", region.captured_sectors, total),
                            )
                        }
                        _ => (
                            "??????????".into(),
                            format!("{} sector / 总量未知", region.captured_sectors),
                        ),
                    };
                    let (status, tone) = match region.completeness {
                        crate::edpb::ArtifactCompleteness::Complete => {
                            ("完整", crate::tui::ui::BadgeTone::Success)
                        }
                        crate::edpb::ArtifactCompleteness::Partial => {
                            ("部分", crate::tui::ui::BadgeTone::Warning)
                        }
                        crate::edpb::ArtifactCompleteness::NotCaptured => {
                            ("未采集", crate::tui::ui::BadgeTone::Neutral)
                        }
                    };
                    lines.push(Line::from(format!("{}  {}  {}", safe(label), bar, count)));
                    lines.push(crate::tui::ui::status_badge(status, tone));
                }
            }
            None => lines.push(Line::from("Manifest 覆盖范围不可用；备份完整性未确认。")),
        },
        None => lines.push(Line::from(
            "选择一条备份查看 Region / Extent / Artifact 覆盖范围。",
        )),
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "目录和用户文件不在备份范围内。",
        warning(),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(
                "区域覆盖 · Manifest",
                state.backups_focused_pane() == crate::tui::pane::PaneId::BackupCoverage,
            ))
            .scroll((
                state
                    .pane_viewport(crate::tui::pane::PaneId::BackupCoverage)
                    .scroll_y
                    .offset
                    .min(u16::MAX as usize) as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn draw_backup_delete(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(delete) = state.backup_delete() else {
        return;
    };
    if delete.stage == WizardStage::Confirm {
        crate::tui::ui::render_action_confirmation_modal(
            frame,
            area,
            crate::tui::ui::ActionConfirmationSpec {
                title: "删除备份",
                headline: "永久删除当前备份？",
                details: vec![
                    Line::from(vec![
                        Span::styled("文件  ", muted()),
                        Span::raw(safe(&delete.path.display().to_string())),
                    ]),
                    Line::from("删除前仍会重新扫描并复核固定 SHA-256 与保留底线。"),
                    Line::from(Span::styled(
                        "此操作不可撤销，但不是目标介质写入。",
                        warning(),
                    )),
                ],
                tone: crate::tui::ui::ConfirmationTone::Destructive,
            },
        );
        return;
    }
    let mut lines = vec![
        Line::from(Span::styled("删除备份", danger())),
        Line::from(vec![
            Span::styled("文件: ", accent()),
            Span::raw(safe(&delete.path.display().to_string())),
        ]),
        Line::from("安全规则：固定选中时 SHA-256 → 删除前重新扫描 → 内容复核 → 至少保留该盘 1 份备份 → 删除单文件 .edpb"),
    ];
    match delete.stage {
        WizardStage::Running => {
            lines.push(Line::from(
                "正在复核并删除；q / Esc / Ctrl-C 将延迟到安全结束点。",
            ));
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已结束；Esc 返回备份列表。"));
        }
        _ => {}
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
                Line::from("Enter 打开删除确认；Esc 取消计划并保留勾选。"),
            ]);
        }
        BackupBatchDeleteStage::Confirm => {
            crate::tui::ui::render_action_confirmation_modal(
                frame,
                area,
                crate::tui::ui::ActionConfirmationSpec {
                    title: "批量删除备份",
                    headline: "确认执行批量删除？",
                    details: vec![
                        Line::from(format!("将删除 {planned} 份已固定备份。")),
                        Line::from("执行时逐条复核路径、SHA-256 与保留底线。"),
                        Line::from(Span::styled(
                            "此操作不可撤销，但不是目标介质写入。",
                            warning(),
                        )),
                    ],
                    tone: crate::tui::ui::ConfirmationTone::Destructive,
                },
            );
            return;
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
                    Line::from(format!("受管备份: {} 份", prepared.managed_backups)),
                    Line::from(format!(
                        "计划删除: {} 份   清理后快照: {} 份",
                        prepared.plan.targets.len(),
                        prepared.retained_backups
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Enter 打开清理确认；执行时逐条按固定 SHA-256 复核。",
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
            crate::tui::ui::render_action_confirmation_modal(
                frame,
                area,
                crate::tui::ui::ActionConfirmationSpec {
                    title: "备份清理确认",
                    headline: "确认执行 keep-N 清理？",
                    details: vec![
                        Line::from(format!("将删除 {count} 份旧备份。")),
                        Line::from("删除前逐条复核固定摘要与保留底线。"),
                        Line::from(Span::styled(
                            "此操作不可撤销，但不是目标介质写入。",
                            warning(),
                        )),
                    ],
                    tone: crate::tui::ui::ConfirmationTone::Destructive,
                },
            );
            return;
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
        WriteEvent::RestoreMatchesHeader { onlyid, count, .. } => {
            format!("onlyid={onlyid} 匹配 {count} 个备份")
        }
        WriteEvent::RestoreMatchRow { index, time, .. } => {
            format!("[{index}] {time}")
        }
        WriteEvent::RestoreSelectionRetry { message } => message.clone(),
        WriteEvent::BackupShaVerified { .. } => "备份 SHA-256 校验通过".to_string(),
        WriteEvent::RestoreDryRunNotice { .. } => "[dry-run] 还原预览完成，未写入".to_string(),
        WriteEvent::RestoreTargetHeader { path } => format!(
            "还原目标已确认：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::RestoreWriteCompleted => {
            "元数据恢复成功；文件系统未恢复，部分分区可能需要格式化".to_string()
        }
        WriteEvent::PostRestoreAssessment { assessment } => {
            use crate::application::post_restore::PostRestorePartitionState;
            let needs_format = assessment
                .partitions
                .iter()
                .filter(|partition| partition.state == PostRestorePartitionState::NeedsFormat)
                .count();
            let password_required = assessment
                .partitions
                .iter()
                .filter(|partition| partition.state == PostRestorePartitionState::PasswordRequired)
                .count();
            let invalid = assessment
                .partitions
                .iter()
                .filter(|partition| {
                    partition.state == PostRestorePartitionState::CryptoMetadataInvalid
                })
                .count();
            format!(
                "恢复后检查：{} 个分区；需格式化 {}；需原密码 {}；加密元数据异常 {}",
                assessment.partitions.len(),
                needs_format,
                password_required,
                invalid
            )
        }
    }
}
