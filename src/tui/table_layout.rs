//! One cell-width aware column allocator and horizontal viewport for TUI tables.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatePolicy {
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TableKind {
    Devices,
    Backups,
    RelatedBackups,
    InspectFields,
    ResultPartitions,
}

pub fn row_window(selected: usize, total: usize, capacity: usize) -> std::ops::Range<usize> {
    let capacity = capacity.max(1);
    let selected = selected.min(total.saturating_sub(1));
    let start = selected
        .saturating_sub(capacity / 2)
        .min(total.saturating_sub(capacity));
    start..(start + capacity).min(total)
}

pub fn table_row_window(selected: usize, total: usize, area_height: u16) -> std::ops::Range<usize> {
    row_window(
        selected,
        total,
        usize::from(area_height.saturating_sub(3)).max(1),
    )
}

impl TableKind {
    /// Exhaustive registry used by the table-scroll gate. Adding a new table kind requires
    /// updating this registry and the exhaustive index below, otherwise compilation fails.
    pub const ALL: [Self; 5] = [
        Self::Devices,
        Self::Backups,
        Self::RelatedBackups,
        Self::InspectFields,
        Self::ResultPartitions,
    ];

    pub const fn gate_index(self) -> usize {
        match self {
            Self::Devices => 0,
            Self::Backups => 1,
            Self::RelatedBackups => 2,
            Self::InspectFields => 3,
            Self::ResultPartitions => 4,
        }
    }
}

#[path = "table/schema.rs"]
mod schema;
pub use schema::{identity_column_specs, table_column_schema, ColumnId, TableColumnSpec};

pub fn table_column_copyable(kind: TableKind, logical_column: usize) -> bool {
    table_column_schema(kind)
        .and_then(|columns| columns.get(logical_column).copied())
        .is_none_or(|column| column.copyable)
}

fn normalize_copied_cell(value: &str) -> String {
    // Table view cells are sanitized before rendering, so normalize both raw C0
    // separators and their visible control-picture forms for TSV payloads.
    value.replace(['\t', '\r', '\n', '⇥', '␍', '␊'], " ")
}

pub fn copy_cell_value(
    kind: TableKind,
    logical_column: usize,
    values: &[String],
) -> Option<String> {
    table_column_copyable(kind, logical_column)
        .then(|| values.get(logical_column))
        .flatten()
        .map(|value| normalize_copied_cell(value))
}

pub fn copy_row_values(kind: TableKind, order: &[usize], values: &[String]) -> String {
    order
        .iter()
        .copied()
        .filter(|logical| table_column_copyable(kind, *logical))
        .filter_map(|logical| values.get(logical))
        .map(|value| normalize_copied_cell(value))
        .collect::<Vec<_>>()
        .join("\t")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Ascending => "↑",
            Self::Descending => "↓",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSort {
    pub column: usize,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TableInteractionState {
    scroll_x: usize,
    active_column: usize,
    sort: Option<TableSort>,
}

impl TableInteractionState {
    pub const fn viewport_offset(self) -> usize {
        self.scroll_x
    }

    pub const fn offset(self) -> usize {
        self.scroll_x
    }

    pub fn set_offset(&mut self, offset: usize) {
        self.scroll_x = offset;
    }

    pub const fn active_column(self) -> usize {
        self.active_column
    }

    pub const fn sort(self) -> Option<TableSort> {
        self.sort
    }

    fn normalize_columns(&mut self, layout: &AdaptiveTableLayout) {
        let count = layout.specs().len();
        self.active_column = self.active_column.min(count.saturating_sub(1));
        if self.sort.is_some_and(|sort| sort.column >= count) {
            self.sort = None;
        }
    }

    pub fn move_active(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if reverse {
            self.active_column.saturating_sub(1)
        } else {
            self.active_column.saturating_add(1).min(count - 1)
        };
        let changed = next != self.active_column;
        if !changed {
            return false;
        }
        self.active_column = next;
        self.ensure_active_visible(layout, content_widths, viewport_width);
        true
    }

    pub fn move_active_bounded(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if reverse {
            self.active_column.saturating_sub(1)
        } else {
            self.active_column.saturating_add(1).min(count - 1)
        };
        if next == self.active_column {
            return false;
        }
        self.active_column = next;
        self.ensure_active_visible_bounded(layout, content_widths, viewport_width);
        true
    }

    pub fn move_active_edge(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        last: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if last { count - 1 } else { 0 };
        let changed = next != self.active_column;
        self.active_column = next;
        self.ensure_active_visible(layout, content_widths, viewport_width);
        changed
    }

    pub fn move_active_edge_bounded(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        last: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let count = layout.specs().len();
        if count == 0 {
            return false;
        }
        let next = if last { count - 1 } else { 0 };
        let changed = next != self.active_column;
        self.active_column = next;
        self.ensure_active_visible_bounded(layout, content_widths, viewport_width);
        changed
    }

    fn ensure_active_visible(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
    ) {
        let viewport_width = usize::from(viewport_width.max(1));
        let (start, end) =
            layout.column_span(content_widths, Some(self.active_column), self.active_column);
        let current_end = self.scroll_x.saturating_add(viewport_width);
        if start < self.scroll_x {
            self.scroll_x = start;
        } else if end > current_end {
            self.scroll_x = if end.saturating_sub(start) > viewport_width {
                // An oversized active column must open at its header/left edge. Aligning the
                // right edge would hide the heading and reproduce the historic table bug.
                start
            } else {
                end.saturating_sub(viewport_width)
            };
        }
        self.scroll_x = self.scroll_x.min(layout.max_scroll(
            content_widths,
            Some(self.active_column),
            viewport_width as u16,
        ));
    }

    fn ensure_active_visible_bounded(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
    ) {
        let viewport_width = usize::from(viewport_width.max(1));
        let (start, end) = layout.column_span(content_widths, None, self.active_column);
        let current_end = self.scroll_x.saturating_add(viewport_width);
        if start < self.scroll_x {
            self.scroll_x = start;
        } else if end > current_end {
            self.scroll_x = if end.saturating_sub(start) > viewport_width {
                start
            } else {
                end.saturating_sub(viewport_width)
            };
        }
        self.scroll_x =
            self.scroll_x
                .min(layout.max_scroll(content_widths, None, viewport_width as u16));
    }

    pub fn scroll_viewport(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let max_scroll =
            layout.max_scroll(content_widths, Some(self.active_column), viewport_width);
        let next = if reverse {
            self.scroll_x.saturating_sub(2)
        } else {
            self.scroll_x.saturating_add(2).min(max_scroll)
        };
        let changed = next != self.scroll_x;
        self.scroll_x = next;
        changed
    }

    pub fn scroll_viewport_bounded(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
        reverse: bool,
    ) -> bool {
        self.normalize_columns(layout);
        let max_scroll = layout.max_scroll(content_widths, None, viewport_width);
        let next = if reverse {
            self.scroll_x.saturating_sub(2)
        } else {
            self.scroll_x.saturating_add(2).min(max_scroll)
        };
        let changed = next != self.scroll_x;
        self.scroll_x = next;
        changed
    }

    pub fn toggle_sort_for(&mut self, logical_column: usize) {
        self.sort = Some(match self.sort {
            Some(TableSort {
                column,
                direction: SortDirection::Ascending,
            }) if column == logical_column => TableSort {
                column,
                direction: SortDirection::Descending,
            },
            _ => TableSort {
                column: logical_column,
                direction: SortDirection::Ascending,
            },
        });
    }

    pub fn set_active_column(&mut self, column: usize) {
        self.active_column = column;
    }

    pub fn ensure_active_visible_for_layout(
        &mut self,
        layout: &AdaptiveTableLayout,
        content_widths: &[usize],
        viewport_width: u16,
    ) {
        self.ensure_active_visible(layout, content_widths, viewport_width);
    }

    pub fn clear_sort(&mut self) -> bool {
        let changed = self.sort.is_some();
        self.sort = None;
        changed
    }
}

#[path = "table/projection.rs"]
mod projection;
pub use projection::{
    backup_health_text, backup_table_view, device_table_view, related_backup_table_view,
    TableViewData,
};
pub(crate) use projection::{
    backup_table_view_with_search, device_table_view_with_search, smart_cell_cmp,
};

fn column(
    min: u16,
    preferred: u16,
    max: u16,
    priority: u8,
    weight: u16,
    pinned: bool,
) -> AdaptiveColumnSpec {
    AdaptiveColumnSpec {
        min_width: min,
        preferred_width: preferred,
        max_width: max,
        priority,
        weight,
        truncate_policy: TruncatePolicy::Ellipsis,
        pinned,
    }
}

pub fn layout_for(kind: TableKind) -> AdaptiveTableLayout {
    use TableKind::*;
    let specs = match kind {
        Devices | Backups | RelatedBackups | ResultPartitions => table_column_schema(kind)
            .expect("workspace tables have a column schema")
            .into_iter()
            .map(|column| column.layout)
            .collect(),
        InspectFields => vec![
            column(9, 10, 12, 100, 1, true),
            column(7, 7, 12, 98, 1, true),
            column(3, 4, 8, 96, 1, true),
            column(10, 24, 48, 94, 2, true),
            column(10, 26, 64, 90, 3, false),
            column(8, 14, 28, 45, 1, false),
            column(9, 24, 72, 40, 2, false),
            column(9, 24, 72, 35, 2, false),
            column(9, 24, 72, 30, 2, false),
            column(8, 12, 18, 70, 1, false),
            column(10, 24, 40, 20, 1, false),
        ],
    };
    AdaptiveTableLayout::new(specs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveColumnSpec {
    pub min_width: u16,
    pub preferred_width: u16,
    pub max_width: u16,
    pub priority: u8,
    pub weight: u16,
    pub truncate_policy: TruncatePolicy,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleColumn {
    pub index: usize,
    pub width: u16,
    pub full_width: usize,
    pub clip_left: usize,
    pub truncate_policy: TruncatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableViewport {
    pub columns: Vec<VisibleColumn>,
    pub scroll_x: usize,
    pub total_width: usize,
    pub viewport_width: usize,
}

impl TableViewport {
    pub fn widths(&self) -> Vec<ratatui::layout::Constraint> {
        self.columns
            .iter()
            .map(|column| ratatui::layout::Constraint::Length(column.width))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveTableLayout {
    specs: Vec<AdaptiveColumnSpec>,
    column_spacing: usize,
}

impl AdaptiveTableLayout {
    pub fn new(specs: Vec<AdaptiveColumnSpec>) -> Self {
        Self {
            specs,
            column_spacing: 1,
        }
    }

    pub fn with_column_spacing(mut self, column_spacing: usize) -> Self {
        self.column_spacing = column_spacing;
        self
    }

    pub const fn column_spacing(&self) -> usize {
        self.column_spacing
    }

    pub fn specs(&self) -> &[AdaptiveColumnSpec] {
        &self.specs
    }

    pub(crate) fn natural_widths(
        &self,
        content_widths: &[usize],
        active_column: Option<usize>,
    ) -> Vec<usize> {
        self.specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let content = content_widths.get(index).copied().unwrap_or(0);
                let base = content
                    .max(usize::from(spec.preferred_width))
                    .max(usize::from(spec.min_width.max(1)));
                if Some(index) == active_column {
                    base
                } else {
                    base.min(usize::from(spec.max_width.max(spec.min_width)))
                }
            })
            .collect()
    }

    pub fn total_width(&self, content_widths: &[usize], active_column: Option<usize>) -> usize {
        let widths = self.natural_widths(content_widths, active_column);
        widths.iter().sum::<usize>()
            + widths
                .len()
                .saturating_sub(1)
                .saturating_mul(self.column_spacing)
    }

    pub fn max_scroll(
        &self,
        content_widths: &[usize],
        active_column: Option<usize>,
        viewport_width: u16,
    ) -> usize {
        self.total_width(content_widths, active_column)
            .saturating_sub(usize::from(viewport_width))
    }

    pub fn column_span(
        &self,
        content_widths: &[usize],
        active_column: Option<usize>,
        index: usize,
    ) -> (usize, usize) {
        let widths = self.natural_widths(content_widths, active_column);
        let index = index.min(widths.len().saturating_sub(1));
        let start = widths
            .iter()
            .take(index)
            .sum::<usize>()
            .saturating_add(index.saturating_mul(self.column_spacing));
        (
            start,
            start.saturating_add(widths.get(index).copied().unwrap_or(0)),
        )
    }

    pub fn layout(&self, width: u16, content_widths: &[usize], scroll: usize) -> TableViewport {
        self.layout_with_active(width, content_widths, scroll, None)
    }

    pub fn layout_with_active(
        &self,
        width: u16,
        content_widths: &[usize],
        scroll_x: usize,
        active_column: Option<usize>,
    ) -> TableViewport {
        let viewport_width = usize::from(width);
        if self.specs.is_empty() || viewport_width == 0 {
            return TableViewport {
                columns: Vec::new(),
                scroll_x: 0,
                total_width: 0,
                viewport_width,
            };
        }

        let widths = self.natural_widths(content_widths, active_column);
        let total_width = widths.iter().sum::<usize>()
            + widths
                .len()
                .saturating_sub(1)
                .saturating_mul(self.column_spacing);
        let scroll_x = scroll_x.min(total_width.saturating_sub(viewport_width));
        let viewport_end = scroll_x.saturating_add(viewport_width);

        let mut columns = Vec::new();
        let mut start = 0usize;
        for (index, full_width) in widths.iter().copied().enumerate() {
            let end = start.saturating_add(full_width);
            let visible_start = start.max(scroll_x);
            let visible_end = end.min(viewport_end);
            if visible_start < visible_end {
                columns.push(VisibleColumn {
                    index,
                    width: (visible_end - visible_start).min(u16::MAX as usize) as u16,
                    full_width,
                    clip_left: visible_start.saturating_sub(start),
                    truncate_policy: if Some(index) == active_column {
                        TruncatePolicy::Clip
                    } else {
                        self.specs[index].truncate_policy
                    },
                });
            }
            start = end.saturating_add(self.column_spacing);
            if start >= viewport_end {
                break;
            }
        }

        TableViewport {
            columns,
            scroll_x,
            total_width,
            viewport_width,
        }
    }
}

pub(crate) fn content_driven_layout(headings: &[&str]) -> AdaptiveTableLayout {
    AdaptiveTableLayout::new(
        headings
            .iter()
            .map(|heading| {
                let width = display_width(heading).max(1).min(u16::MAX as usize) as u16;
                AdaptiveColumnSpec {
                    min_width: width,
                    preferred_width: width,
                    max_width: u16::MAX,
                    priority: 0,
                    weight: 1,
                    truncate_policy: TruncatePolicy::Ellipsis,
                    pinned: false,
                }
            })
            .collect(),
    )
}

pub(crate) fn content_widths(headings: &[&str], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = headings
        .iter()
        .map(|heading| display_width(heading))
        .collect::<Vec<_>>();
    for row in rows {
        debug_assert_eq!(row.len(), widths.len());
        for (index, value) in row.iter().enumerate().take(widths.len()) {
            widths[index] = widths[index].max(display_width(value));
        }
    }
    widths
}

pub fn table_heading(heading: &str, index: usize, interaction: TableInteractionState) -> String {
    let marker = interaction
        .sort()
        .filter(|sort| sort.column == index)
        .map(|sort| sort.direction.marker())
        .unwrap_or("");
    if marker.is_empty() {
        heading.to_string()
    } else {
        format!("{heading} {marker}")
    }
}

pub fn table_position_label(
    layout: &AdaptiveTableLayout,
    interaction: TableInteractionState,
) -> String {
    let total = layout.specs().len().max(1);
    format!(
        "当前列 {}/{}",
        interaction.active_column().min(total - 1) + 1,
        total
    )
}

pub fn table_scrollbar_visibility(
    viewport: &TableViewport,
    row_total: usize,
    row_visible: usize,
) -> (bool, bool) {
    (
        viewport.total_width > viewport.viewport_width,
        row_visible > 0 && row_total > row_visible,
    )
}

pub fn render_table_scrollbars(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    viewport: &TableViewport,
    row_total: usize,
    row_start: usize,
    row_visible: usize,
) {
    use ratatui::{
        layout::Margin,
        widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
    };

    let (horizontal, vertical) = table_scrollbar_visibility(viewport, row_total, row_visible);
    if horizontal && area.width > 2 && area.height > 1 {
        let horizontal_positions = viewport
            .total_width
            .saturating_sub(viewport.viewport_width)
            .saturating_add(1);
        let mut state = ScrollbarState::new(horizontal_positions)
            .position(viewport.scroll_x)
            .viewport_content_length(viewport.viewport_width);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .thumb_symbol("━")
            .track_symbol(Some("─"))
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(crate::tui::theme::current().accent())
            .track_style(crate::tui::theme::current().muted());
        frame.render_stateful_widget(scrollbar, area.inner(Margin::new(1, 0)), &mut state);
    }

    if vertical && area.height > 2 && area.width > 1 {
        let vertical_positions = row_total.saturating_sub(row_visible).saturating_add(1);
        let mut state = ScrollbarState::new(vertical_positions)
            .position(row_start)
            .viewport_content_length(row_visible);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(Some("│"))
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(crate::tui::theme::current().accent())
            .track_style(crate::tui::theme::current().muted());
        frame.render_stateful_widget(scrollbar, area.inner(Margin::new(0, 1)), &mut state);
    }
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn visible_cell(text: &str, column: &VisibleColumn) -> String {
    let base = truncate_cell(text, column.full_width, column.truncate_policy);
    slice_display_cells(&base, column.clip_left, usize::from(column.width))
}

pub fn slice_display_cells(text: &str, start: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let end = start.saturating_add(width);
    let mut out = String::new();
    let mut position = 0usize;
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        let cells = display_width(grapheme);
        let grapheme_start = position;
        let grapheme_end = position.saturating_add(cells);
        position = grapheme_end;
        if grapheme_end <= start {
            continue;
        }
        if grapheme_start >= end {
            break;
        }
        if grapheme_start >= start && grapheme_end <= end {
            out.push_str(grapheme);
        } else {
            let overlap_start = grapheme_start.max(start);
            let overlap_end = grapheme_end.min(end);
            out.push_str(&" ".repeat(overlap_end.saturating_sub(overlap_start)));
        }
    }
    out
}

pub fn truncate_cell(text: &str, width: usize, policy: TruncatePolicy) -> String {
    if display_width(text) <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let ellipsis = if policy == TruncatePolicy::Ellipsis {
        "…"
    } else {
        ""
    };
    let target = width.saturating_sub(display_width(ellipsis));
    let mut out = String::new();
    let mut used = 0;
    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        let cells = display_width(grapheme);
        if used + cells > target {
            break;
        }
        out.push_str(grapheme);
        used += cells;
    }
    out.push_str(ellipsis);
    out
}
