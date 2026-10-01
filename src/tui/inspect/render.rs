use super::*;
use crate::tui::state::AdvancedInspectPrompt;

#[path = "render_helpers.rs"]
mod render_helpers;
use render_helpers::{draw_inspect_breadcrumb, inspect_field_status_style};

#[path = "tree_render.rs"]
mod tree_render;
use tree_render::draw_inspect_tree_pane;

#[path = "field_table_render.rs"]
mod field_table_render;

#[path = "detail_render.rs"]
mod detail_render;
use detail_render::draw_inspect_object_panes;

#[path = "sector_render.rs"]
mod sector_render;
use sector_render::draw_sector_inspector;

fn draw_inspect_jump_modal(frame: &mut Frame, state: &AppState) {
    let Some(AdvancedInspectPrompt::Jump { input, error, .. }) = state.advanced_inspect_prompt()
    else {
        return;
    };
    let height = if error.is_some() { 10 } else { 9 };
    let area = crate::tui::ui::centered_modal_rect(frame.area(), 52, height);
    crate::tui::ui::render_modal(frame, area, "跳转到扇区", |frame, inner| {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);
        frame.render_widget(Paragraph::new("LBA 扇区号"), rows[0]);
        frame.render_widget(
            Paragraph::new(safe(input)).block(Block::default().borders(Borders::ALL)),
            rows[1],
        );
        if let Some(message) = error.as_deref() {
            frame.render_widget(
                Paragraph::new(Span::styled(safe(message), danger())).wrap(Wrap { trim: false }),
                rows[2],
            );
        }
        frame.render_widget(
            Paragraph::new("Enter 跳转        Esc 取消").alignment(Alignment::Center),
            rows[3],
        );
    });
}

pub(super) fn draw_advanced_inspect(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(advanced) = state.advanced_inspect() else {
        return;
    };
    use super::super::state::{AdvancedInspectPanel, AdvancedInspectStage, InspectViewMode};

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
            let panel_index = advanced.view_mode.tab_index();
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
                Tabs::new(["1 业务字段", "2 原始字段", "3 Hex"])
                    .select(panel_index)
                    .style(tab())
                    .highlight_style(active_tab())
                    .divider(Span::styled(" │ ", muted())),
                browser[1],
            );

            let content = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(browser[2]);
            let compact_layout_area = content[0];
            let content_area = content[1];
            let class = crate::tui::ui::ViewportClass::for_width(content_area.width);
            let (tree_area, overview_area, detail_area) = if class
                == crate::tui::ui::ViewportClass::Compact
            {
                match advanced.panel {
                    AdvancedInspectPanel::Tree => {
                        let parts = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                            .split(content_area);
                        (Some(parts[1]), Some(parts[0]), None)
                    }
                    AdvancedInspectPanel::Overview => (None, Some(content_area), None),
                    AdvancedInspectPanel::Detail => (None, None, Some(content_area)),
                }
            } else {
                let vertical = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(60), Constraint::Min(4)])
                    .split(content_area);
                let upper = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(29), Constraint::Percentage(71)])
                    .split(vertical[0]);
                (Some(upper[0]), Some(upper[1]), Some(vertical[1]))
            };

            if let Some(layout) = disk_layout {
                layout.render_mini(
                    frame,
                    compact_layout_area,
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
                    .block(crate::tui::ui::panel("磁盘概览", false)),
                    compact_layout_area,
                );
            }

            if advanced.sector.is_some() && advanced.view_mode == InspectViewMode::Hex {
                draw_sector_inspector(frame, content_area, state);
            } else {
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
            draw_inspect_jump_modal(frame, state);
        }
    }
}
