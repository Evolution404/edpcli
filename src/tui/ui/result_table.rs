use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use super::operation_result::{value_style, ResultValue};
use super::result_supplement::{render_result_supplement, ResultSupplement};

#[derive(Debug, Clone)]
pub struct ResultTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<ResultValue>>,
    pub widths: Vec<Constraint>,
    pub supplement: Option<ResultSupplement>,
}

fn render_table_body(frame: &mut Frame, area: Rect, table: &ResultTable) {
    if area.width < 88 || table.headers.is_empty() {
        let lines = table
            .rows
            .iter()
            .map(|row| {
                Line::from(
                    row.iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" · "),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
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
    frame.render_widget(Table::new(rows, table.widths.clone()).header(header), area);
}

pub(super) fn render_result_table(frame: &mut Frame, area: Rect, table: &ResultTable) {
    let block = crate::tui::ui::card(table.title.clone(), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(supplement) = table.supplement.as_ref() else {
        render_table_body(frame, inner, table);
        return;
    };

    let table_height = (table.rows.len() as u16)
        .saturating_add(u16::from(!table.headers.is_empty()))
        .max(1)
        .min(inner.height);
    let supplement_height = 8u16;
    if inner.height < table_height.saturating_add(supplement_height) {
        render_table_body(frame, inner, table);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(table_height),
        Constraint::Length(1),
        Constraint::Min(supplement_height),
    ])
    .split(inner);
    render_table_body(frame, chunks[0], table);
    render_result_supplement(frame, chunks[2], supplement);
}
