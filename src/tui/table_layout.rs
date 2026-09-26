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
    ProvisionDevices,
    ProvisionMenu,
    InspectFields,
}

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
        Devices => vec![
            column(7, 9, 12, 100, 1, true),
            column(8, 9, 12, 80, 1, false),
            column(4, 5, 7, 75, 1, false),
            column(9, 9, 12, 70, 1, false),
            column(10, 17, 28, 30, 2, false),
            column(8, 12, 20, 40, 1, false),
            column(6, 10, 20, 35, 1, false),
            column(8, 16, 40, 10, 3, false),
            column(12, 17, 25, 95, 1, true),
        ],
        Backups => vec![
            column(3, 3, 4, 95, 1, true),
            column(3, 4, 6, 90, 1, false),
            column(12, 17, 20, 70, 1, false),
            column(12, 20, 25, 100, 1, true),
            column(6, 12, 20, 40, 1, false),
            column(8, 18, 40, 20, 3, false),
            column(8, 11, 15, 80, 1, false),
        ],
        ProvisionDevices => vec![
            column(7, 9, 12, 100, 1, true),
            column(8, 12, 14, 80, 1, false),
            column(9, 13, 20, 50, 1, false),
            column(12, 18, 25, 95, 1, true),
            column(8, 15, 24, 35, 1, false),
        ],
        ProvisionMenu => vec![
            column(3, 4, 5, 100, 1, true),
            column(12, 24, 36, 90, 1, false),
            column(16, 35, 80, 30, 3, false),
        ],
        InspectFields => vec![
            column(7, 7, 12, 100, 1, true),
            column(3, 4, 8, 95, 1, true),
            column(8, 14, 28, 35, 1, false),
            column(10, 24, 48, 90, 2, true),
            column(10, 26, 64, 85, 3, false),
            column(9, 24, 72, 40, 2, false),
            column(9, 24, 72, 30, 2, false),
            column(9, 24, 72, 25, 2, false),
            column(8, 12, 18, 25, 1, false),
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
    pub truncate_policy: TruncatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableViewport {
    pub columns: Vec<VisibleColumn>,
    pub first_scrollable_column: usize,
    pub scrollable_columns: usize,
}

impl TableViewport {
    pub fn widths(&self) -> Vec<ratatui::layout::Constraint> {
        self.columns
            .iter()
            .map(|column| ratatui::layout::Constraint::Length(column.width))
            .collect()
    }

    pub fn position_label(&self) -> String {
        if self.scrollable_columns == 0 {
            "1/1 列".into()
        } else {
            format!(
                "{}/{} 列",
                self.first_scrollable_column + 1,
                self.scrollable_columns
            )
        }
    }

    pub fn project<T: Clone>(&self, values: &[T]) -> Vec<T> {
        self.columns
            .iter()
            .filter_map(|column| values.get(column.index).cloned())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveTableLayout {
    specs: Vec<AdaptiveColumnSpec>,
}

impl AdaptiveTableLayout {
    pub fn new(specs: Vec<AdaptiveColumnSpec>) -> Self {
        Self { specs }
    }

    pub fn specs(&self) -> &[AdaptiveColumnSpec] {
        &self.specs
    }

    pub fn scrollable_count(&self) -> usize {
        self.specs.iter().filter(|spec| !spec.pinned).count()
    }

    pub fn layout(&self, width: u16, content_widths: &[usize], scroll: usize) -> TableViewport {
        if self.specs.is_empty() || width == 0 {
            return TableViewport {
                columns: Vec::new(),
                first_scrollable_column: 0,
                scrollable_columns: self.scrollable_count(),
            };
        }
        let scrollable = self
            .specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| (!spec.pinned).then_some(index))
            .collect::<Vec<_>>();
        let offset = scroll.min(scrollable.len().saturating_sub(1));
        let mut selected = self
            .specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| spec.pinned.then_some(index))
            .collect::<Vec<_>>();
        let min_sum = |indices: &[usize]| {
            indices
                .iter()
                .map(|&index| usize::from(self.specs[index].min_width.max(1)))
                .sum::<usize>()
                + indices.len().saturating_sub(1)
        };
        while min_sum(&selected) > usize::from(width) && selected.len() > 1 {
            let removable = selected
                .iter()
                .enumerate()
                .filter(|(_, index)| **index != 0)
                .min_by_key(|(_, index)| self.specs[**index].priority)
                .map(|(position, _)| position)
                .unwrap_or(selected.len() - 1);
            selected.remove(removable);
        }
        for &index in scrollable.iter().skip(offset) {
            let mut candidate = selected.clone();
            candidate.push(index);
            if min_sum(&candidate) <= usize::from(width) {
                selected.push(index);
            } else if selected.is_empty() {
                selected.push(index);
                break;
            } else {
                break;
            }
        }
        if selected.is_empty() {
            selected.push(0);
        }
        selected.sort_unstable();
        let spacing = selected.len().saturating_sub(1);
        let usable = usize::from(width).saturating_sub(spacing);
        let mut widths = selected
            .iter()
            .map(|&index| usize::from(self.specs[index].min_width.max(1)))
            .collect::<Vec<_>>();
        if widths.iter().sum::<usize>() > usable {
            let mut excess = widths.iter().sum::<usize>() - usable;
            let mut order = (0..selected.len()).collect::<Vec<_>>();
            order.sort_by_key(|&position| self.specs[selected[position]].priority);
            for position in order {
                let shrink = excess.min(widths[position].saturating_sub(1));
                widths[position] -= shrink;
                excess -= shrink;
                if excess == 0 {
                    break;
                }
            }
        }
        let mut remaining = usable.saturating_sub(widths.iter().sum::<usize>());
        let mut priority_order = (0..selected.len()).collect::<Vec<_>>();
        priority_order
            .sort_by_key(|&position| std::cmp::Reverse(self.specs[selected[position]].priority));
        for &position in &priority_order {
            let index = selected[position];
            let spec = self.specs[index];
            let preferred = usize::from(spec.preferred_width)
                .max(content_widths.get(index).copied().unwrap_or(0))
                .min(usize::from(spec.max_width.max(spec.min_width)));
            let add = remaining.min(preferred.saturating_sub(widths[position]));
            widths[position] += add;
            remaining -= add;
        }
        while remaining > 0 {
            let mut progressed = false;
            for &position in &priority_order {
                let spec = self.specs[selected[position]];
                for _ in 0..spec.weight.max(1) {
                    if remaining == 0 || widths[position] >= usize::from(spec.max_width) {
                        break;
                    }
                    widths[position] += 1;
                    remaining -= 1;
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        TableViewport {
            columns: selected
                .into_iter()
                .zip(widths)
                .map(|(index, width)| VisibleColumn {
                    index,
                    width: width.min(u16::MAX as usize) as u16,
                    truncate_policy: self.specs[index].truncate_policy,
                })
                .collect(),
            first_scrollable_column: offset,
            scrollable_columns: scrollable.len(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalScrollState {
    offset: usize,
}

impl HorizontalScrollState {
    pub const fn offset(self) -> usize {
        self.offset
    }

    pub fn set_offset(&mut self, offset: usize, layout: &AdaptiveTableLayout) {
        self.offset = offset.min(layout.scrollable_count().saturating_sub(1));
    }

    pub fn right(&mut self, layout: &AdaptiveTableLayout) -> bool {
        let next = self
            .offset
            .saturating_add(1)
            .min(layout.scrollable_count().saturating_sub(1));
        let changed = next != self.offset;
        self.offset = next;
        changed
    }

    pub fn left(&mut self) -> bool {
        let changed = self.offset > 0;
        self.offset = self.offset.saturating_sub(1);
        changed
    }
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
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
