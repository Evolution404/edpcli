use ratatui::{text::Line, widgets::Paragraph};

use crate::tui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerTone {
    Info,
    Success,
    Warning,
    Danger,
}

pub fn notice_banner<'a>(message: impl Into<Line<'a>>, tone: BannerTone) -> Paragraph<'a> {
    let theme = theme::current();
    let style = match tone {
        BannerTone::Info => theme.accent(),
        BannerTone::Success => theme.success(),
        BannerTone::Warning => theme.warning(),
        BannerTone::Danger => theme.danger(),
    };
    Paragraph::new(message.into()).style(style)
}
