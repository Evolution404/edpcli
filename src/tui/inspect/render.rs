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

#[path = "evidence_table_render.rs"]
mod evidence_table_render;

#[path = "detail_render.rs"]
mod detail_render;
use detail_render::draw_inspect_object_panes;

#[path = "sector_render.rs"]
mod sector_render;
use sector_render::draw_sector_inspector;

#[cfg(test)]
#[path = "sector_render_tests.rs"]
mod sector_render_tests;

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
    use super::super::state::{AdvancedInspectPanel, AdvancedInspectStage};

    if advanced.stage != AdvancedInspectStage::Browser {
        return;
    }
    let Some(workspace) = advanced.result.as_ref() else {
        return;
    };

    if advanced.view_mode == crate::tui::state::InspectViewMode::Hex && advanced.sector.is_some() {
        draw_sector_inspector(frame, area, state);
        draw_inspect_jump_modal(frame, state);
        return;
    }

    let rows = state.advanced_inspect_tree_rows();
    let selected_index = advanced.tree_selected.min(rows.len().saturating_sub(1));
    let selected_row = rows.get(selected_index);
    let disk_layout = workspace.disk_layout.as_ref();
    let layout =
        crate::tui::inspect_layout::InspectBrowserLayout::from_content_area(area, advanced.panel);
    draw_inspect_breadcrumb(frame, layout.breadcrumb_area, state);
    let compact_layout_area = layout.compact_layout_area;
    let tree_area = layout.tree_area;
    let overview_area = layout.overview_area;
    let detail_area = layout.detail_area;

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

    if let Some(tree_area) = tree_area {
        draw_inspect_tree_pane(
            frame,
            tree_area,
            &rows,
            selected_index,
            advanced.panel == AdvancedInspectPanel::Tree,
            state
                .pane_viewport(crate::tui::pane::PaneId::InspectTree)
                .scroll_x,
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
    draw_inspect_jump_modal(frame, state);
}

pub(super) fn draw_advanced_inspect_overlay(frame: &mut Frame, state: &AppState) {
    use super::super::state::AdvancedInspectStage;

    let Some(advanced) = state.advanced_inspect() else {
        return;
    };
    match advanced.stage {
        AdvancedInspectStage::Browser => {}
        AdvancedInspectStage::Running => {
            let area = crate::tui::ui::centered_modal_rect(frame.area(), 52, 9);
            crate::tui::ui::render_modal(frame, area, "检查", |frame, inner| {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Min(1),
                    ])
                    .split(inner);
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        crate::tui::animation::spinner_glyph(state.animation_frame()),
                        secondary().add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Center),
                    rows[0],
                );
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        "正在建立全盘结构树",
                        secondary().add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Center),
                    rows[1],
                );
                frame.render_widget(
                    Paragraph::new("正在读取协议上下文…").alignment(Alignment::Center),
                    rows[2],
                );
                frame.render_widget(
                    Paragraph::new(safe(&advanced.source.label()))
                        .style(muted())
                        .alignment(Alignment::Center),
                    rows[3],
                );
                frame.render_widget(
                    Paragraph::new("只读分析")
                        .style(muted())
                        .alignment(Alignment::Center),
                    rows[4],
                );
            });
        }
        AdvancedInspectStage::Failed => {
            let area = crate::tui::ui::centered_modal_rect(frame.area(), 58, 9);
            crate::tui::ui::render_modal(frame, area, "检查失败", |frame, inner| {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Min(2),
                        Constraint::Length(1),
                    ])
                    .split(inner);
                frame.render_widget(
                    Paragraph::new(Span::styled("无法建立全盘结构", danger()))
                        .alignment(Alignment::Center),
                    rows[0],
                );
                frame.render_widget(
                    Paragraph::new(safe(
                        advanced
                            .message
                            .as_deref()
                            .unwrap_or("检查任务未返回可用结果"),
                    ))
                    .wrap(Wrap { trim: false })
                    .alignment(Alignment::Center),
                    rows[1],
                );
                frame.render_widget(
                    Paragraph::new("Enter / Esc 关闭")
                        .style(muted())
                        .alignment(Alignment::Center),
                    rows[2],
                );
            });
        }
    }
}
