use ratatui::text::{Line, Span};

use crate::tui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

pub fn status_badge<'a>(label: impl Into<String>, tone: BadgeTone) -> Line<'a> {
    let theme = theme::current();
    let style = match tone {
        BadgeTone::Neutral => theme.secondary_text(),
        BadgeTone::Accent => theme.accent(),
        BadgeTone::Success => theme.success(),
        BadgeTone::Warning => theme.warning(),
        BadgeTone::Danger => theme.danger(),
    };
    Line::from(Span::styled(format!("[{}]", label.into()), style))
}
