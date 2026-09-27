use ratatui::text::{Line, Span};

use crate::tui::theme;

pub fn key_hints<'a>(hints: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (index, (key, description)) in hints.iter().enumerate() {
        if index != 0 {
            spans.push(Span::raw("  ·  "));
        }
        spans.push(Span::styled(*key, theme::current().accent()));
        spans.push(Span::raw(format!(" {description}")));
    }
    Line::from(spans)
}
