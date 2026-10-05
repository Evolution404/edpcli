//! Complete, stable notice snapshots; input never reaches the underlying form.
use super::{state::AppState, ui};
use ratatui::{layout::Rect, text::Line, widgets::Paragraph, Frame};

fn lines(state: &AppState, width: u16) -> Vec<Line<'static>> {
    let Some(message) = state.notice_details() else {
        return Vec::new();
    };
    ui::text::wrap_lines(
        message
            .text()
            .lines()
            .map(|line| Line::from(crate::ui::sanitize_terminal_text(line)).style(message.style()))
            .collect(),
        width,
    )
}

pub(crate) fn metrics(state: &AppState) -> (usize, usize) {
    let size = state.viewport_size();
    let popup = ui::centered_modal_rect(Rect::new(0, 0, size.width, size.height), 92, 28);
    (
        lines(state, popup.width.saturating_sub(2)).len(),
        usize::from(popup.height.saturating_sub(3)).max(1),
    )
}

pub(super) fn draw(frame: &mut Frame, state: &AppState) {
    let popup = ui::centered_modal_rect(frame.area(), 92, 28);
    ui::render_modal(frame, popup, "消息详情", |frame, inner| {
        let body = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let lines = lines(state, body.width);
        let count = lines.len();
        let offset = state
            .notice_scroll()
            .min(count.saturating_sub(usize::from(body.height)));
        frame.render_widget(
            Paragraph::new(lines).scroll((offset.min(u16::MAX as usize) as u16, 0)),
            body,
        );
        if inner.height > 0 {
            frame.render_widget(
                Paragraph::new(format!(
                    "Esc/F2 关闭 · PgUp/PgDn · {}/{}",
                    (offset + 1).min(count),
                    count
                ))
                .style(super::theme::current().muted()),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
    });
}

#[cfg(test)]
#[path = "notice_tests.rs"]
mod tests;
