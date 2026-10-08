use super::*;

#[path = "capacity_render.rs"]
mod capacity_render;
#[path = "detail_render.rs"]
mod detail_render;

fn backup_cell_style(
    backup: &crate::application::BackupWorkspaceItem,
    id: crate::tui::table_layout::ColumnId,
    checked: bool,
) -> Style {
    use crate::tui::table_layout::ColumnId;
    match id {
        ColumnId::Selected => {
            if checked {
                warning()
            } else {
                muted()
            }
        }
        ColumnId::Index => accent(),
        ColumnId::VidPid => secondary(),
        ColumnId::Health => crate::tui::theme::current().backup_health(backup.health()),
        ColumnId::ProvisionKind => backup
            .provision_kind
            .map(|kind| crate::tui::theme::current().provision_kind(kind))
            .unwrap_or_else(warning),
        _ => Style::default(),
    }
}

fn backup_tree_scroll_offset(
    content_len: usize,
    selected_index: usize,
    visible_rows: usize,
) -> usize {
    let visible_rows = visible_rows.max(1);
    let max_offset = content_len.saturating_sub(visible_rows);
    if selected_index < visible_rows {
        0
    } else {
        selected_index
            .saturating_add(1)
            .saturating_sub(visible_rows)
            .min(max_offset)
    }
}

fn draw_backup_device_tree(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    use crate::tui::pane::PaneId;

    let focused = state.backups_focused_pane() == PaneId::BackupDevices;
    let block = crate::tui::ui::card("备份设备", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let nodes = state.backup_device_tree_snapshot();
    let selected = state
        .backup_device_tree_selected()
        .min(nodes.len().saturating_sub(1));
    let mut lines = Vec::with_capacity(nodes.len().max(1));

    let inner_width = usize::from(inner.width);
    let marker_width = 2usize;
    let count_width = nodes
        .first()
        .map_or(3, |root| root.count.to_string().len() + 2)
        .max(3);
    let gap_width = 1usize;
    let info_width = inner_width
        .saturating_sub(marker_width)
        .saturating_sub(count_width)
        .saturating_sub(gap_width)
        .max(1);
    let selected_info_width = nodes
        .get(selected)
        .map(|node| state.backup_device_tree_node_parts(node))
        .map(|(prefix, label, _count)| {
            crate::tui::table_layout::display_width(prefix)
                .saturating_add(crate::tui::table_layout::display_width(&safe(&label)))
        })
        .unwrap_or(info_width);
    let scroll_x = state
        .backup_device_tree_scroll_offset()
        .min(selected_info_width.saturating_sub(info_width));
    let theme = crate::tui::theme::current();

    let visible_rows = usize::from(inner.height).max(1);
    let scroll_offset = backup_tree_scroll_offset(nodes.len(), selected, visible_rows);
    for (index, node) in nodes
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
    {
        let active = index == selected;
        let marker = if active && focused { "▌ " } else { "  " };
        let (prefix, label, count) = state.backup_device_tree_node_parts(node);
        let full_info = format!("{prefix}{}", safe(&label));
        let row_scroll_x = if node.depth == 0 { 0 } else { scroll_x };
        let mut info =
            crate::tui::table_layout::slice_display_cells(&full_info, row_scroll_x, info_width);
        let visible_width = crate::tui::table_layout::display_width(&info);
        if visible_width < info_width {
            info.push_str(&" ".repeat(info_width - visible_width));
        }
        let count_text = format!("{:>count_width$}", format!("[{count}]"));

        let count_style = theme.apply_selection(
            theme.table_cell(theme.accent().add_modifier(Modifier::BOLD), active, focused),
            active,
            focused,
        );
        let base = if node.depth == 0 {
            secondary().add_modifier(Modifier::BOLD)
        } else {
            theme.table_text()
        };
        let row_style = if active {
            theme.apply_selection(theme.table_cell(base, true, focused), true, focused)
        } else {
            base
        };
        lines.push(Line::from(vec![
            Span::styled(marker, row_style),
            Span::styled(info, row_style),
            Span::styled(" ".repeat(gap_width), row_style),
            Span::styled(count_text, count_style),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled("暂无备份设备", muted())));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_backups(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    if let Some(run) = state.backup_verify_run() {
        let mut lines = vec![
            Line::from(Span::styled("备份校验进行中", accent())),
            Line::from(format!("对象  {}", safe(&run.path.display().to_string()))),
            Line::from(format!(
                "当前阶段  {} · {} · {:.2}%",
                run.latest.phase.label(),
                run.latest.step.label(),
                f64::from(run.latest.overall.basis_points()) / 100.0
            )),
            Line::from(""),
            Line::from(Span::styled("运行日志", secondary())),
        ];
        lines.extend(run.log.iter().map(|event| {
            Line::from(safe(&format!(
                "[{:.2}%] {}  {}",
                f64::from(event.overall.basis_points()) / 100.0,
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
    let (list_area, detail_area, coverage_area) =
        crate::tui::backup_layout::pane_areas(area, focused);
    if let Some(list_area) = list_area {
        let snapshot = state.backup_view_snapshot();
        let visible_count = snapshot.indices.len();
        let total_count = state.backups().len();
        let count_label = if state.workspace_filter_active() || state.backup_device_filter_active()
        {
            format!("{visible_count}/{total_count}")
        } else {
            total_count.to_string()
        };
        let backup_parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(4)])
            .split(list_area);

        let counts = state.backup_overview_counts();
        let metrics = crate::tui::overview::overview_metrics(counts);
        let search = crate::tui::overview::overview_search(state, "/ 搜索身份、容量、型号、文件名");
        crate::tui::ui::workspace_overview(frame, backup_parts[0], "备份概览", &metrics, &search);

        let (tree_area, table_area) = if class == ViewportClass::Compact {
            if focused == PaneId::BackupDevices {
                (Some(backup_parts[1]), None)
            } else {
                (None, Some(backup_parts[1]))
            }
        } else {
            let sidebar_width = class
                .backup_device_sidebar_width()
                .expect("non-compact backups layout must have a sidebar");
            let body = Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(24)])
                .split(backup_parts[1]);
            (Some(body[0]), Some(body[1]))
        };
        if let Some(tree_area) = tree_area {
            draw_backup_device_tree(frame, tree_area, state);
        }
        let Some(table_area) = table_area else {
            return;
        };

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
            let inner = block.inner(table_area);
            frame.render_widget(block, table_area);
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
                render_table_scrollbars, table_column_schema, table_heading, table_position_label,
                visible_cell, TableKind,
            };
            let columns = table_column_schema(TableKind::Backups).expect("backup schema");
            let headings = columns
                .iter()
                .map(|column| column.heading)
                .collect::<Vec<_>>();
            let view = state
                .table_view_data(TableKind::Backups)
                .expect("backup table projection");
            let content_widths = &snapshot.content_widths;
            let order = state.table_column_order(TableKind::Backups);
            let layout = state.table_visual_layout(TableKind::Backups);
            let visual_widths = state.table_visual_widths(TableKind::Backups, content_widths);
            let interaction = state.table_interaction(TableKind::Backups);
            let viewport = layout.layout_with_active(
                table_area.width.saturating_sub(4),
                &visual_widths,
                interaction.viewport_offset(),
                Some(interaction.active_column()),
            );
            let pane_focused =
                state.backups_focused_pane() == crate::tui::pane::PaneId::BackupsList;
            let window = crate::tui::table_layout::table_row_window(
                state.selected(),
                visible_count,
                table_area.height,
            );
            let window_start = window.start;
            let window_len = window.len();
            let rows = window
                .filter_map(|position| snapshot.indices.get(position).copied())
                .map(|index| {
                    let backup = &state.backups()[index];
                    let values = &view.rows[index];
                    let checked = state.backup_is_selected(&backup.path);
                    TableRow::new(
                        viewport
                            .columns
                            .iter()
                            .map(|column| {
                                let logical = order[column.index];
                                let value = if columns[logical].id
                                    == crate::tui::table_layout::ColumnId::Selected
                                {
                                    if checked {
                                        "✓"
                                    } else {
                                        ""
                                    }
                                } else {
                                    &values[logical]
                                };
                                let style = backup_cell_style(backup, columns[logical].id, checked);
                                Cell::from(visible_cell(value, column)).style(
                                    crate::tui::theme::current().table_cell(
                                        style,
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
            let table_title = format!("{title} · {}", table_position_label(&layout, interaction));
            let table = crate::tui::ui::data_table(
                &table_title,
                header,
                rows,
                viewport.widths(),
                pane_focused,
            );
            let mut table_state = TableState::default();
            table_state.select(Some(state.selected().saturating_sub(window_start)));
            frame.render_stateful_widget(table, table_area, &mut table_state);
            render_table_scrollbars(
                frame,
                table_area,
                &viewport,
                visible_count,
                window_start,
                window_len,
            );
        }
    }
    if let Some(detail_area) = detail_area {
        detail_render::draw_backup_metadata(frame, detail_area, state);
    }
    if let Some(coverage_area) = coverage_area {
        capacity_render::draw_backup_capacity(frame, coverage_area, state);
    }
}

fn draw_backup_status_modal(frame: &mut Frame, title: &str, lines: Vec<Line<'static>>) {
    let height = (lines.len() as u16).saturating_add(2).clamp(5, 12);
    let modal = crate::tui::ui::centered_modal_rect(frame.area(), 78, height);
    crate::tui::ui::render_modal(frame, modal, title, |frame, inner| {
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    });
}

fn backup_status_message(
    message: Option<&crate::tui::ui::UiMessage>,
    fallback: &'static str,
) -> Line<'static> {
    match message {
        Some(message) => Line::from(Span::styled(
            format!("{} {}", message.marker(), safe(message.text())),
            message.style(),
        )),
        None => Line::from(Span::styled(
            format!("◌ {fallback}"),
            crate::tui::theme::current().secondary_accent(),
        )),
    }
}

pub(super) fn draw_backup_delete(frame: &mut Frame, state: &AppState) {
    let Some(delete) = state.backup_delete() else {
        return;
    };
    if delete.stage == WizardStage::Confirm {
        crate::tui::ui::render_action_confirmation_modal(
            frame,
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
    if delete.stage == WizardStage::Running {
        draw_backup_status_modal(
            frame,
            "删除备份 · 执行中",
            vec![
                Line::from(format!(
                    "文件  {}",
                    safe(&delete.path.display().to_string())
                )),
                backup_status_message(delete.message.as_ref(), "正在复核并删除备份…"),
                Line::from(Span::styled(
                    "q / Esc / Ctrl-C 将延迟到安全结束点。",
                    warning(),
                )),
            ],
        );
    }
}

pub(super) fn draw_backup_batch_delete(frame: &mut Frame, state: &AppState) {
    let Some(batch) = state.backup_batch_delete() else {
        return;
    };
    use super::super::state::BackupBatchDeleteStage;

    let planned = batch
        .prepared
        .as_ref()
        .map(|plan| plan.targets.len())
        .unwrap_or_else(|| state.backup_selection_count());
    match batch.stage {
        BackupBatchDeleteStage::Planning => {
            draw_backup_status_modal(
                frame,
                "批量删除 · 生成计划",
                vec![
                    Line::from(format!("已勾选  {} 份备份", state.backup_selection_count())),
                    Line::from("正在新鲜扫描并逐项固定路径与 SHA-256…"),
                    Line::from(Span::styled("计划生成期间不会删除任何文件。", muted())),
                ],
            );
        }
        BackupBatchDeleteStage::Confirm => {
            crate::tui::ui::render_action_confirmation_modal(
                frame,
                crate::tui::ui::ActionConfirmationSpec {
                    title: "批量删除备份",
                    headline: "确认执行批量删除？",
                    details: vec![
                        Line::from(format!("固定计划将删除 {planned} 份备份。")),
                        Line::from("执行时逐条复核路径、SHA-256 与保留底线。"),
                        Line::from(Span::styled(
                            "此操作不可撤销，但不是目标介质写入。",
                            warning(),
                        )),
                    ],
                    tone: crate::tui::ui::ConfirmationTone::Destructive,
                },
            );
        }
        BackupBatchDeleteStage::Running => {
            draw_backup_status_modal(
                frame,
                "批量删除 · 执行中",
                vec![
                    Line::from(format!("固定目标  {planned} 份")),
                    backup_status_message(batch.message.as_ref(), "正在按固定计划逐条复核并删除…"),
                    Line::from(Span::styled("退出请求会延迟到安全结束点。", warning())),
                ],
            );
        }
    }
}

pub(super) fn draw_backup_prune(frame: &mut Frame, state: &AppState) {
    let Some(prune) = state.backup_prune() else {
        return;
    };
    use super::super::state::BackupPruneStage;

    match prune.stage {
        BackupPruneStage::Input => {
            draw_backup_status_modal(
                frame,
                "备份清理 · keep-N",
                vec![
                    Line::from("按同盘组保留最近 N 份快照。"),
                    Line::from(vec![
                        Span::styled("每组保留  ", muted()),
                        Span::styled(safe(&prune.keep_input), input_focused()),
                    ]),
                    Line::from("Enter 生成只读清理计划 · Esc 取消"),
                ],
            );
        }
        BackupPruneStage::Planning => {
            draw_backup_status_modal(
                frame,
                "备份清理 · 生成计划",
                vec![
                    Line::from(format!("keep-N  {}", safe(&prune.keep_input))),
                    Line::from("正在扫描备份并生成固定候选快照…"),
                    Line::from(Span::styled("计划生成期间不会删除任何文件。", muted())),
                ],
            );
        }
        BackupPruneStage::Confirm => {
            let count = prune
                .prepared
                .as_ref()
                .map(|prepared| prepared.plan.targets.len())
                .unwrap_or(0);
            crate::tui::ui::render_action_confirmation_modal(
                frame,
                crate::tui::ui::ActionConfirmationSpec {
                    title: "备份清理确认",
                    headline: "确认执行 keep-N 清理？",
                    details: if let Some(prepared) = prune.prepared.as_ref() {
                        vec![
                            Line::from(format!("keep-N  {}", prepared.keep)),
                            Line::from(format!("受管备份  {} 份", prepared.managed_backups)),
                            Line::from(format!(
                                "计划删除  {count} 份 · 清理后保留 {} 份",
                                prepared.retained_backups
                            )),
                            Line::from("删除前逐条复核固定摘要与保留底线。"),
                            Line::from(Span::styled(
                                "此操作不可撤销，但不是目标介质写入。",
                                warning(),
                            )),
                        ]
                    } else {
                        vec![Line::from("清理计划不可用。")]
                    },
                    tone: crate::tui::ui::ConfirmationTone::Destructive,
                },
            );
        }
        BackupPruneStage::Running => {
            let count = prune
                .prepared
                .as_ref()
                .map(|prepared| prepared.plan.targets.len())
                .unwrap_or(0);
            draw_backup_status_modal(
                frame,
                "备份清理 · 执行中",
                vec![
                    Line::from(format!("固定目标  {count} 份")),
                    backup_status_message(
                        prune.message.as_ref(),
                        "正在逐条复核摘要并清理固定候选…",
                    ),
                    Line::from(Span::styled("退出请求会延迟到安全结束点。", warning())),
                ],
            );
        }
    }
}
