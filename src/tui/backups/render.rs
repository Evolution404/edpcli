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
    let block = crate::tui::ui::card("设备", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let nodes = state.backup_device_tree_nodes();
    let selected = state
        .backup_device_tree_selected()
        .min(nodes.len().saturating_sub(1));
    let mut lines = Vec::with_capacity(nodes.len().max(1));

    let inner_width = usize::from(inner.width);
    let marker_width = 2usize;
    let count_width = state.backup_device_tree_count_width();
    let gap_width = 1usize;
    let info_width = inner_width
        .saturating_sub(marker_width)
        .saturating_sub(count_width)
        .saturating_sub(gap_width)
        .max(1);
    let selected_info_width = state
        .backup_device_tree_row_parts(selected)
        .map(|(prefix, label, _count)| {
            crate::tui::table_layout::display_width(prefix)
                .saturating_add(crate::tui::table_layout::display_width(&safe(&label)))
        })
        .unwrap_or(info_width);
    let scroll_x = state
        .backup_device_tree_scroll_offset()
        .min(selected_info_width.saturating_sub(info_width));
    let theme = crate::tui::theme::current();

    for (index, node) in nodes.iter().enumerate() {
        let active = index == selected;
        let marker = if active && focused { "▌ " } else { "  " };
        let Some((prefix, label, count)) = state.backup_device_tree_row_parts(index) else {
            continue;
        };
        let full_info = format!("{prefix}{}", safe(&label));
        let row_scroll_x = if node.depth == 0 { 0 } else { scroll_x };
        let mut info =
            crate::tui::table_layout::slice_display_cells(&full_info, row_scroll_x, info_width);
        let visible_width = crate::tui::table_layout::display_width(&info);
        if visible_width < info_width {
            info.push_str(&" ".repeat(info_width - visible_width));
        }
        let count_text = format!("{count:>count_width$}");

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
            Span::styled(count_text, row_style),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled("暂无备份设备", muted())));
    }
    let visible_rows = usize::from(inner.height).max(1);
    let scroll_offset = backup_tree_scroll_offset(nodes.len(), selected, visible_rows);
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll_offset.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
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

        let counts = crate::tui::overview::ProvisionKindCounts::from_kinds(
            state.backups().iter().map(|backup| backup.provision_kind),
        );
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
                    Span::raw(" 检查      "),
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
                    "备份信息",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("选择一条备份后，这里会显示身份、健康状态和安全操作。"),
            ])
        }
        .block(crate::tui::ui::card(
            "备份信息",
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
    use crate::application::backup_restore_preview::BackupRestoreRegionKind;
    use crate::tui::disk_layout::{DiskCapacityMap, DiskCapacityMapProfile};

    let mut lines = Vec::new();
    match state.selected_backup() {
        Some(backup) => match backup.restore_preview.as_ref() {
            Some(preview) => {
                lines.push(Line::from(Span::styled(
                    "容量地图",
                    secondary().add_modifier(Modifier::BOLD),
                )));
                match preview.layout.as_ref() {
                    Some(layout) => {
                        let map_width = area.width.saturating_sub(4) as usize;
                        let profile = if map_width >= 36 {
                            DiskCapacityMapProfile::Compact
                        } else {
                            DiskCapacityMapProfile::Mini
                        };
                        lines.extend(
                            DiskCapacityMap::new(layout, profile)
                                .with_marker(false)
                                .lines(map_width),
                        );
                    }
                    None => lines.push(Line::from(Span::styled(
                        safe(preview.layout_error.as_deref().unwrap_or("容量布局不可用")),
                        warning(),
                    ))),
                }

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "区域恢复状态",
                    secondary().add_modifier(Modifier::BOLD),
                )));
                for region in &preview.region_statuses {
                    let (status, status_style) = match region.kind {
                        BackupRestoreRegionKind::CompleteBytes => ("✓ 完整恢复", success()),
                        BackupRestoreRegionKind::StructureOnly => ("✓ 结构恢复", accent()),
                        BackupRestoreRegionKind::OutOfScope => ("— 不在备份范围", muted()),
                        BackupRestoreRegionKind::PartialOrInvalid => ("⚠ 不完整", danger()),
                    };
                    let mut spans = vec![
                        Span::styled(format!("{}  ", safe(&region.label)), Style::default()),
                        Span::styled(status, status_style.add_modifier(Modifier::BOLD)),
                    ];
                    if let Some(detail) = region.detail.as_deref() {
                        spans.push(Span::styled(format!(" · {}", safe(detail)), muted()));
                    }
                    lines.push(Line::from(spans));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "恢复能力",
                    secondary().add_modifier(Modifier::BOLD),
                )));
                if let Some(contract) = preview.restore_contract.as_ref() {
                    lines.push(Line::from(Span::styled(
                        if contract.restores_partition_structure {
                            "✓ 分区结构"
                        } else {
                            "— 分区结构 · 不在恢复合同"
                        },
                        if contract.restores_partition_structure {
                            success()
                        } else {
                            muted()
                        },
                    )));
                    if preview.is_plain {
                        lines.push(Line::from(Span::styled("— EDP 协议 · 不适用", muted())));
                        lines.push(Line::from(Span::styled("— LCE · 不适用", muted())));
                    } else {
                        lines.push(Line::from(Span::styled(
                            if contract.restores_edp_protocol {
                                "✓ EDP 协议元数据"
                            } else {
                                "— EDP 协议 · 不在恢复合同"
                            },
                            if contract.restores_edp_protocol {
                                success()
                            } else {
                                muted()
                            },
                        )));
                        let lce_restorable = preview.region_statuses.iter().any(|region| {
                            region.label == "LCE"
                                && region.kind == BackupRestoreRegionKind::CompleteBytes
                        });
                        lines.push(Line::from(Span::styled(
                            if lce_restorable {
                                "✓ LCE"
                            } else {
                                "⚠ LCE · 不完整或不可恢复"
                            },
                            if lce_restorable { success() } else { warning() },
                        )));
                    }
                    lines.push(Line::from(Span::styled(
                        if contract.restores_filesystem {
                            "✓ 原文件系统状态"
                        } else {
                            "— 原文件系统状态 · 不包含"
                        },
                        if contract.restores_filesystem {
                            success()
                        } else {
                            muted()
                        },
                    )));
                    lines.push(Line::from(Span::styled(
                        if contract.restores_user_data {
                            "✓ 用户文件内容"
                        } else {
                            "— 目录树 / 用户文件内容 · 不包含"
                        },
                        if contract.restores_user_data {
                            success()
                        } else {
                            muted()
                        },
                    )));
                    if contract.post_restore_assessment_required {
                        lines.push(Line::from(Span::styled(
                            "⚠ 恢复后需要执行后置评估",
                            warning(),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "恢复合同不可用；不能声明可恢复范围。",
                        warning(),
                    )));
                }
            }
            None => lines.push(Line::from(Span::styled(
                "恢复范围不可用；备份未提供可验证的恢复合同。",
                warning(),
            ))),
        },
        None => lines.push(Line::from("选择一条备份查看恢复后的容量布局和恢复能力。")),
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(
                "恢复范围",
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
