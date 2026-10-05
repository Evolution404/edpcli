use super::review_layout_render::draw_final_layout;
use super::review_plan_render::draw_partition_plan;
use super::review_summary_render::draw_execution_summary;
use super::review_target_render::draw_target_line;
use super::*;

pub(super) fn draw_provision_review(
    frame: &mut Frame,
    main_area: ratatui::layout::Rect,
    state: &AppState,
) {
    let view = match state.provision_confirmation_view_model() {
        Ok(view) => view,
        Err(message) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("计划确认不可用", danger())),
                    Line::from(""),
                    Line::from(safe(&message)),
                ])
                .block(crate::tui::ui::card("计划确认", true))
                .wrap(Wrap { trim: true }),
                main_area,
            );
            return;
        }
    };
    let selected = state
        .provision_review_selected_region()
        .min(view.regions.len().saturating_sub(1));
    let focused = state.provision_focused_pane();
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(main_area);
    draw_target_line(frame, sections[0], &view);

    let body = sections[1];
    let class = crate::tui::ui::ViewportClass::for_width(body.width);
    let multi_pane = matches!(
        class,
        crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
    ) && body.height >= 18;

    let footer = if class == crate::tui::ui::ViewportClass::Compact {
        "Tab / Shift-Tab 切换窗口 · Enter 写入确认 · Esc 返回修改"
    } else {
        "j/k 选择区域 · o 详情 · Tab / Shift-Tab 切换窗口 · Enter 写入确认 · e 导出 · Esc 返回修改"
    };
    frame.render_widget(
        Paragraph::new(footer)
            .alignment(ratatui::layout::Alignment::Center)
            .style(muted()),
        sections[2],
    );

    if multi_pane {
        let layout_height = if body.height >= 26 { 9 } else { 7 };
        let rows =
            Layout::vertical([Constraint::Length(layout_height), Constraint::Min(8)]).split(body);
        draw_final_layout(
            frame,
            rows[0],
            &view,
            selected,
            state.disk_layout_tail_expansion(),
            focused == crate::tui::pane::PaneId::ProvisionDiskLayout,
        );
        let table_height = view.regions.len().saturating_add(3).clamp(6, 16) as u16;
        let lower =
            Layout::vertical([Constraint::Length(table_height), Constraint::Min(8)]).split(rows[1]);
        draw_partition_plan(
            frame,
            lower[0],
            &view,
            selected,
            focused == crate::tui::pane::PaneId::ProvisionPartitionPlan,
        );
        draw_execution_summary(
            frame,
            lower[1],
            &view,
            selected,
            state.provision().review_details_expanded,
            focused == crate::tui::pane::PaneId::ProvisionExecutionSummary,
        );
        return;
    }

    match focused {
        crate::tui::pane::PaneId::ProvisionDiskLayout => draw_final_layout(
            frame,
            body,
            &view,
            selected,
            state.disk_layout_tail_expansion(),
            true,
        ),
        crate::tui::pane::PaneId::ProvisionExecutionSummary => draw_execution_summary(
            frame,
            body,
            &view,
            selected,
            state.provision().review_details_expanded,
            true,
        ),
        _ => draw_partition_plan(frame, body, &view, selected, true),
    }
}
