use ratatui::{
    text::Line,
    widgets::{Block, Borders},
};

use crate::tui::theme;

pub fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let theme = theme::current();
    Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            theme.focused_panel()
        } else {
            theme.subtle_border()
        })
        .title(title)
        .title_style(if focused {
            theme.accent()
        } else {
            theme.secondary_text()
        })
}
