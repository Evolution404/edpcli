use super::*;

pub(super) fn inspect_field_status_style(
    status: crate::application::inspect::InspectFieldStatus,
) -> Style {
    match status {
        crate::application::inspect::InspectFieldStatus::Known => {
            crate::tui::theme::current().table_text()
        }
        crate::application::inspect::InspectFieldStatus::Unknown => warning(),
        crate::application::inspect::InspectFieldStatus::Reserved => muted(),
        crate::application::inspect::InspectFieldStatus::Preserved => success(),
    }
}

pub(super) fn draw_inspect_breadcrumb(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let Some(model) = state.advanced_inspect_breadcrumb() else {
        return;
    };
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(20)])
        .split(area);
    frame.render_widget(
        Paragraph::new(safe(&model.display())).style(muted()),
        parts[0],
    );
    frame.render_widget(
        Paragraph::new(model.escape_hint()).style(accent()),
        parts[1],
    );
}
