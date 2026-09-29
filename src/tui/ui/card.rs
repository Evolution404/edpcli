use ratatui::{text::Line, widgets::Block};

use crate::tui::theme;

use super::panel::panel;

pub fn card<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    panel(title, focused).style(theme::current().pane_surface(focused))
}
