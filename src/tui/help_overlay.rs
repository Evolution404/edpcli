use ratatui::{
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{
    help_context,
    keymap::{HelpBinding, TABLE_HELP},
    state::AppState,
    theme, ui,
};

fn binding_line(state: &AppState, binding: &HelpBinding) -> Line<'static> {
    let theme = theme::current();
    let spec = super::actions::describe(state, binding);
    Line::from(vec![
        Span::styled(format!("{:<22}", spec.keys), theme.accent()),
        Span::styled(
            spec.label,
            if !spec.available_in(state) {
                theme.muted()
            } else {
                match spec.risk {
                    super::actions::ActionRisk::ReadOnly => theme.text(),
                    _ => theme.warning(),
                }
            },
        ),
        Span::styled(
            spec.disabled_reason
                .map(|reason| format!(" · 不可用：{reason}"))
                .unwrap_or_default(),
            theme.muted(),
        ),
    ])
}

fn push_section(
    state: &AppState,
    lines: &mut Vec<Line<'static>>,
    title: &str,
    bindings: &[HelpBinding],
) {
    let theme = theme::current();
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled(
        title.to_string(),
        theme.secondary_accent(),
    )));
    lines.extend(bindings.iter().map(|binding| binding_line(state, binding)));
}

fn help_lines(state: &AppState) -> Vec<Line<'static>> {
    let help = help_context::resolve(state);
    let mut lines = Vec::new();
    if !help.bindings.is_empty() {
        push_section(state, &mut lines, "当前上下文", help.bindings);
    }
    if help.include_table {
        push_section(state, &mut lines, "表格", TABLE_HELP);
    }
    push_section(state, &mut lines, help.global_title, help.global);
    lines
}

fn metrics(state: &AppState) -> (usize, usize) {
    let size = state.viewport_size();
    let popup = ui::centered_modal_rect(
        ratatui::layout::Rect::new(0, 0, size.width, size.height),
        92,
        30,
    );
    let width = popup.width.saturating_sub(2);
    let rows = usize::from(popup.height.saturating_sub(3)).max(1);
    let count = ui::text::wrap_lines(help_lines(state), width).len();
    (count, rows)
}

impl AppState {
    pub(crate) fn scroll_help(&mut self, action: super::keymap::TuiAction) {
        use super::keymap::TuiAction;
        let (count, rows) = metrics(self);
        let max = count.saturating_sub(rows);
        let offset = self.help_scroll().min(max);
        let next = match action {
            TuiAction::MoveUp => offset.saturating_sub(1),
            TuiAction::MoveDown => offset.saturating_add(1).min(max),
            TuiAction::PageUp => offset.saturating_sub(rows),
            TuiAction::PageDown => offset.saturating_add(rows).min(max),
            TuiAction::HalfPageUp => offset.saturating_sub((rows / 2).max(1)),
            TuiAction::HalfPageDown => offset.saturating_add((rows / 2).max(1)).min(max),
            TuiAction::Top => 0,
            TuiAction::Bottom => max,
            _ => offset,
        };
        self.set_help_scroll(next);
    }
}

pub(super) fn draw_help_overlay(frame: &mut Frame, state: &AppState) {
    use ratatui::layout::Rect;
    let help = help_context::resolve(state);
    let popup = ui::centered_modal_rect(frame.area(), 92, 30);
    ui::render_modal(
        frame,
        popup,
        &format!("快捷键 · {}", help.title),
        |frame, inner| {
            let body = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(1),
            );
            let lines = ui::text::wrap_lines(help_lines(state), body.width);
            let count = lines.len();
            let paragraph = Paragraph::new(lines);
            let offset = state
                .help_scroll()
                .min(count.saturating_sub(usize::from(body.height)));
            frame.render_widget(
                paragraph.scroll((offset.min(u16::MAX as usize) as u16, 0)),
                body,
            );
            if inner.height > 0 {
                frame.render_widget(
                    Paragraph::new(format!(
                        "Esc 关闭 · PgUp/PgDn · {}/{}",
                        (offset + 1).min(count),
                        count
                    ))
                    .style(theme::current().muted()),
                    Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
                );
            }
        },
    );
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
