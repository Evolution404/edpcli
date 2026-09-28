use super::*;

#[path = "render_helpers.rs"]
mod render_helpers;
use render_helpers::{draw_inspect_breadcrumb, inspect_field_status_style};

#[path = "tree_render.rs"]
mod tree_render;
use tree_render::draw_inspect_tree_pane;

#[path = "detail_render.rs"]
mod detail_render;
use detail_render::draw_inspect_object_panes;

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
    use super::super::state::{AdvancedInspectPanel, AdvancedInspectStage};

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
            let disk_layout = workspace.disk_layout.as_ref();
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
                if let Some(layout) = disk_layout {
                    layout.render_compact(
                        frame,
                        compact_area,
                        selected_row.map(|row| row.range.start_lba),
                    );
                } else {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!(
                                "容量布局不可用：{}",
                                safe(workspace.disk_layout_issue.as_deref().unwrap_or("证据不足"))
                            ),
                            warning(),
                        ))
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(panel())
                                .title("磁盘概览"),
                        ),
                        compact_area,
                    );
                }
            }

            if let Some(layout_area) = disk_layout_area {
                if let Some(layout) = disk_layout {
                    let total = layout.total_sectors;
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
                    layout.render_pane(
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
                            profile: crate::tui::disk_layout::DiskLayoutProfile::DetailedExact,
                            tail: state.disk_layout_tail_expansion(),
                            selected_segment: state.disk_layout_selected(),
                        },
                    );
                } else {
                    frame.render_widget(
                        Paragraph::new(vec![
                            Line::from(Span::styled("无法建立可靠容量布局", warning())),
                            Line::from(safe(
                                workspace.disk_layout_issue.as_deref().unwrap_or("证据不足"),
                            )),
                        ])
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(
                                    if advanced.panel == AdvancedInspectPanel::DiskLayout {
                                        focused_panel()
                                    } else {
                                        panel()
                                    },
                                )
                                .title("磁盘布局"),
                        ),
                        layout_area,
                    );
                }
            }

            if let Some(tree_area) = tree_area {
                draw_inspect_tree_pane(
                    frame,
                    tree_area,
                    &rows,
                    selected_index,
                    advanced.panel == AdvancedInspectPanel::Tree,
                );
            }
            draw_inspect_object_panes(
                frame,
                state,
                workspace,
                advanced,
                selected_row,
                overview_area,
                detail_area,
            );
        }
    }
}
