use ratatui::{
    text::{Line, Span},
    widgets::{Block, Borders},
};

use crate::tui::theme;

pub fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let theme = theme::current();
    let title = title.into();
    let mut spans = Vec::with_capacity(title.spans.len() + usize::from(focused));
    if focused {
        spans.push(Span::styled(
            theme.pane_title_prefix(true),
            theme.pane_title(true),
        ));
    }
    spans.extend(title.spans);
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme.pane_border_type(focused))
        .border_style(theme.pane_border(focused))
        .title(Line::from(spans))
        .title_style(theme.pane_title(focused))
}
