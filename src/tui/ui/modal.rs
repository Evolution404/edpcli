use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, Borders, Clear, Widget},
    Frame,
};

use crate::tui::theme;

struct ModalBackground(Style);

impl Widget for ModalBackground {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, self.0);
    }
}

pub fn centered_modal_rect(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.saturating_sub(2).clamp(1, max_width.max(1));
    let height = area.height.saturating_sub(2).clamp(1, max_height.max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub fn render_modal(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    body: impl FnOnce(&mut Frame, Rect),
) {
    let theme = theme::current();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.modal_border())
        .title(Span::styled(format!(" {title} "), theme.modal_title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    body(frame, inner);
    frame.render_widget(ModalBackground(theme.modal_background()), area);
}
