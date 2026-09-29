use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTone {
    Primary,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultValue {
    pub text: String,
    pub tone: ResultTone,
    pub bold: bool,
}

impl ResultValue {
    pub fn primary(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ResultTone::Primary,
            bold: false,
        }
    }

    pub fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ResultTone::Muted,
            bold: false,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ResultTone::Success,
            bold: false,
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ResultTone::Warning,
            bold: false,
        }
    }

    pub fn danger(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ResultTone::Danger,
            bold: false,
        }
    }

    pub fn emphasized(mut self) -> Self {
        self.bold = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultField {
    pub label: String,
    pub value: ResultValue,
}

impl ResultField {
    pub fn new(label: impl Into<String>, value: ResultValue) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResultTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<ResultValue>>,
    pub widths: Vec<Constraint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultCard {
    pub title: String,
    pub lines: Vec<ResultValue>,
}

#[derive(Debug, Clone)]
pub struct OperationResultSpec {
    pub title: String,
    pub status: String,
    pub tone: ResultTone,
    pub fields: Vec<ResultField>,
    pub table: Option<ResultTable>,
    pub cards: Vec<ResultCard>,
    pub footer: String,
}

fn tone_style(tone: ResultTone) -> Style {
    let theme = crate::tui::theme::current();
    match tone {
        ResultTone::Primary => theme.body_text(),
        ResultTone::Muted => theme.muted(),
        ResultTone::Accent => theme.accent(),
        ResultTone::Success => theme.success(),
        ResultTone::Warning => theme.warning(),
        ResultTone::Danger => theme.danger(),
    }
}

fn value_style(value: &ResultValue) -> Style {
    let style = tone_style(value.tone);
    if value.bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn badge_tone(tone: ResultTone) -> crate::tui::ui::BadgeTone {
    match tone {
        ResultTone::Success => crate::tui::ui::BadgeTone::Success,
        ResultTone::Warning => crate::tui::ui::BadgeTone::Warning,
        ResultTone::Danger => crate::tui::ui::BadgeTone::Danger,
        ResultTone::Primary | ResultTone::Muted | ResultTone::Accent => {
            crate::tui::ui::BadgeTone::Neutral
        }
    }
}

fn summary_lines(spec: &OperationResultSpec) -> Vec<Line<'static>> {
    let theme = crate::tui::theme::current();
    let mut lines = vec![crate::tui::ui::status_badge(
        spec.status.clone(),
        badge_tone(spec.tone),
    )];
    for chunk in spec.fields.chunks(2) {
        let mut spans = Vec::new();
        for (index, field) in chunk.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("    "));
            }
            spans.push(Span::styled(format!("{}  ", field.label), theme.muted()));
            spans.push(Span::styled(
                field.value.text.clone(),
                value_style(&field.value),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn render_summary(frame: &mut Frame, area: Rect, spec: &OperationResultSpec) {
    frame.render_widget(
        Paragraph::new(summary_lines(spec))
            .block(
                crate::tui::ui::card(spec.title.clone(), true).border_style(tone_style(spec.tone)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_table(frame: &mut Frame, area: Rect, table: &ResultTable) {
    if area.width < 88 || table.headers.is_empty() {
        let mut lines = Vec::new();
        for row in &table.rows {
            lines.push(Line::from(
                row.iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
            ));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(crate::tui::ui::card(table.title.clone(), false))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let header = Row::new(
        table
            .headers
            .iter()
            .map(|header| Cell::from(header.clone()))
            .collect::<Vec<_>>(),
    )
    .style(crate::tui::theme::current().table_header(false, false));

    let rows = table.rows.iter().map(|row| {
        Row::new(
            row.iter()
                .map(|cell| Cell::from(cell.text.clone()).style(value_style(cell)))
                .collect::<Vec<_>>(),
        )
    });

    frame.render_widget(
        Table::new(rows, table.widths.clone())
            .header(header)
            .block(crate::tui::ui::card(table.title.clone(), false)),
        area,
    );
}

fn render_card(frame: &mut Frame, area: Rect, card: &ResultCard) {
    let lines = card
        .lines
        .iter()
        .map(|value| Line::from(Span::styled(value.text.clone(), value_style(value))))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(card.title.clone(), false))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn card_constraints(cards: usize) -> Vec<Constraint> {
    if cards == 0 {
        return Vec::new();
    }
    let each = (100 / cards.max(1)) as u16;
    (0..cards)
        .map(|index| {
            if index + 1 == cards {
                Constraint::Min(4)
            } else {
                Constraint::Percentage(each)
            }
        })
        .collect()
}

pub fn render_operation_result(frame: &mut Frame, area: Rect, spec: &OperationResultSpec) {
    let summary_height = 3 + spec.fields.len().div_ceil(2) as u16;
    let root = Layout::vertical([
        Constraint::Length(summary_height),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .split(area);

    render_summary(frame, root[0], spec);

    match (&spec.table, spec.cards.is_empty()) {
        (Some(table), false) if area.width >= 110 => {
            let body = Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)])
                .split(root[1]);
            render_table(frame, body[0], table);
            let cards = Layout::default()
                .direction(Direction::Vertical)
                .constraints(card_constraints(spec.cards.len()))
                .split(body[1]);
            for (index, card) in spec.cards.iter().enumerate() {
                render_card(frame, cards[index], card);
            }
        }
        (Some(table), false) => {
            let body = Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)])
                .split(root[1]);
            render_table(frame, body[0], table);
            let cards = Layout::default()
                .direction(Direction::Vertical)
                .constraints(card_constraints(spec.cards.len()))
                .split(body[1]);
            for (index, card) in spec.cards.iter().enumerate() {
                render_card(frame, cards[index], card);
            }
        }
        (Some(table), true) => render_table(frame, root[1], table),
        (None, false) => {
            let cards = Layout::default()
                .direction(Direction::Vertical)
                .constraints(card_constraints(spec.cards.len()))
                .split(root[1]);
            for (index, card) in spec.cards.iter().enumerate() {
                render_card(frame, cards[index], card);
            }
        }
        (None, true) => {}
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            spec.footer.clone(),
            crate::tui::theme::current().accent(),
        )))
        .alignment(ratatui::layout::Alignment::Center),
        root[2],
    );
}
