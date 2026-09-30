use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::theme;

#[derive(Debug, Clone)]
pub struct OverviewMetric {
    pub label: String,
    pub value: usize,
    pub style: Style,
}

impl OverviewMetric {
    pub fn new(label: impl Into<String>, value: usize, style: Style) -> Self {
        Self {
            label: label.into(),
            value,
            style,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OverviewSearch {
    pub text: String,
    pub active: bool,
    pub filtered: bool,
}

pub fn workspace_overview(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    metrics: &[OverviewMetric],
    search: &OverviewSearch,
) {
    let search_width = if area.width >= 100 {
        38
    } else if area.width >= 72 {
        30
    } else {
        24.min(area.width.saturating_div(2).max(12))
    };
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(search_width.min(area.width.saturating_sub(20))),
        ])
        .split(area);

    let muted = theme::current().muted();
    let mut spans = Vec::new();
    for (index, metric) in metrics.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ·  ", muted));
        }
        spans.push(Span::styled(format!("{} ", metric.label), muted));
        spans.push(Span::styled(metric.value.to_string(), metric.style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::current().pane_border(false))
                .title(title),
        ),
        parts[0],
    );

    let search_style = if search.active {
        theme::current().accent()
    } else if search.filtered {
        theme::current().secondary_accent()
    } else {
        muted
    };
    frame.render_widget(
        Paragraph::new(search.text.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if search.active {
                        theme::current().pane_border(true)
                    } else {
                        theme::current().pane_border(false)
                    })
                    .title(if search.active {
                        "搜索 · 实时过滤"
                    } else {
                        "搜索"
                    }),
            )
            .style(search_style),
        parts[1],
    );
}
