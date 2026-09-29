use ratatui::{
    layout::Constraint,
    widgets::{Row, Table},
};

use crate::tui::theme;

use super::panel::panel;

pub fn data_table<'a>(
    title: &'a str,
    header: Row<'a>,
    rows: impl IntoIterator<Item = Row<'a>>,
    widths: impl IntoIterator<Item = Constraint>,
    focused: bool,
) -> Table<'a> {
    Table::new(rows, widths)
        .style(theme::current().pane_surface(focused))
        .header(header.style(theme::current().accent()))
        .block(panel(title, focused))
        .row_highlight_style(theme::current().selection_overlay(focused))
        .highlight_symbol("▌ ")
}
