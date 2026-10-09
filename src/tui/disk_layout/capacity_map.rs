//! Shared capacity map with visible small-region blocks and linked selection.
use super::{
    format_layout_capacity, percentage, DiskLayoutModel, DiskLayoutSegment, DiskRegionKind,
    TailExpansion,
};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskCapacityMapProfile {
    Full,
    Compact,
    Mini,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskCapacitySelection {
    pub start_lba: u64,
    pub end_exclusive: u64,
    pub kind: DiskRegionKind,
}

impl DiskCapacitySelection {
    pub fn from_segment(segment: &DiskLayoutSegment) -> Option<Self> {
        Some(Self {
            start_lba: segment.start_lba,
            end_exclusive: segment.end_exclusive().ok()?,
            kind: segment.kind,
        })
    }

    pub fn from_lba(model: &DiskLayoutModel, lba: u64) -> Option<Self> {
        model
            .segments
            .iter()
            .find(|segment| {
                segment
                    .end_exclusive()
                    .ok()
                    .is_some_and(|end| segment.start_lba <= lba && lba < end)
            })
            .and_then(Self::from_segment)
    }
}

pub struct DiskCapacityMap<'a> {
    model: &'a DiskLayoutModel,
    profile: DiskCapacityMapProfile,
    tail: TailExpansion,
    selection: Option<DiskCapacitySelection>,
    show_marker: bool,
}

impl<'a> DiskCapacityMap<'a> {
    pub fn new(model: &'a DiskLayoutModel, profile: DiskCapacityMapProfile) -> Self {
        Self {
            model,
            profile,
            tail: TailExpansion::Collapsed,
            selection: None,
            show_marker: matches!(profile, DiskCapacityMapProfile::Full),
        }
    }

    pub fn with_tail(mut self, tail: TailExpansion) -> Self {
        self.tail = tail;
        self
    }

    pub fn with_selection(mut self, selection: Option<DiskCapacitySelection>) -> Self {
        self.selection = selection;
        self
    }

    pub fn with_marker(mut self, show_marker: bool) -> Self {
        self.show_marker = show_marker;
        self
    }

    pub fn visible_model(&self) -> DiskLayoutModel {
        match self.tail {
            TailExpansion::Collapsed => self.model.collapsed_tail_model(),
            TailExpansion::Expanded => self.model.clone(),
        }
    }

    pub fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let visible = self.visible_model();
        let map_width = width.max(visible.segments.len().max(1));
        let allocations = capacity_map_allocations(&visible, map_width);
        let mut lines = Vec::new();

        match self.profile {
            DiskCapacityMapProfile::Full => {
                lines.extend(capacity_map_axis_lines(map_width));
                lines.push(capacity_map_half_band_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    true,
                ));
                lines.push(capacity_map_content_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    CapacityContent::Label,
                ));
                lines.push(capacity_map_content_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    CapacityContent::Value,
                ));
                lines.push(capacity_map_half_band_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    false,
                ));
            }
            DiskCapacityMapProfile::Compact => {
                lines.push(capacity_map_half_band_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    true,
                ));
                lines.push(capacity_map_content_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    CapacityContent::Compact,
                ));
                lines.push(capacity_map_half_band_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                    false,
                ));
            }
            DiskCapacityMapProfile::Mini => {
                lines.push(capacity_map_mini_line(
                    &visible,
                    &allocations,
                    self.selection.as_ref(),
                ));
            }
        }

        if self.show_marker && !matches!(self.profile, DiskCapacityMapProfile::Mini) {
            if let Some(selection) = self.selection.as_ref() {
                let marker = capacity_map_marker_column(&visible, &allocations, selection);
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(marker)),
                    Span::styled(
                        "▲",
                        crate::tui::theme::current().disk_region_outline(selection.kind, true),
                    ),
                ]));
            } else {
                lines.push(Line::from(""));
            }
        }
        lines
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapacityContent {
    Label,
    Value,
    Compact,
}

pub(super) fn capacity_map_allocations(model: &DiskLayoutModel, cell_budget: usize) -> Vec<usize> {
    let count = model.segments.len();
    if count == 0 {
        return Vec::new();
    }
    let min_width: usize = if cell_budget >= count.saturating_mul(3) {
        3
    } else if cell_budget >= count.saturating_mul(2) {
        2
    } else {
        1
    };
    let mut allocations = vec![min_width; count];
    let remaining = cell_budget.saturating_sub(min_width.saturating_mul(count));
    if remaining == 0 {
        return allocations;
    }

    let total = model
        .segments
        .iter()
        .map(|segment| u128::from(segment.sector_count))
        .sum::<u128>()
        .max(1);
    let mut assigned = 0usize;
    let mut remainders = Vec::with_capacity(count);
    for (index, segment) in model.segments.iter().enumerate() {
        let scaled = u128::from(segment.sector_count) * remaining as u128;
        let extra = (scaled / total) as usize;
        allocations[index] += extra;
        assigned += extra;
        remainders.push((scaled % total, index));
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, index) in remainders
        .into_iter()
        .take(remaining.saturating_sub(assigned))
    {
        allocations[index] += 1;
    }
    allocations
}

fn capacity_map_axis_lines(width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut labels = vec![' '; width];
    for (percent, label) in [
        (0usize, "0%"),
        (25, "25%"),
        (50, "50%"),
        (75, "75%"),
        (100, "100%"),
    ] {
        let target = width.saturating_sub(1).saturating_mul(percent) / 100;
        let label_width = label.len().min(width);
        let start = if percent == 0 {
            0
        } else if percent == 100 {
            width.saturating_sub(label_width)
        } else {
            target
                .saturating_sub(label_width / 2)
                .min(width.saturating_sub(label_width))
        };
        for (index, ch) in label.chars().take(label_width).enumerate() {
            labels[start + index] = ch;
        }
    }
    let mut ticks = vec!['┈'; width];
    for percent in [0usize, 25, 50, 75, 100] {
        let index = width.saturating_sub(1).saturating_mul(percent) / 100;
        ticks[index] = '┊';
    }
    let style = crate::tui::theme::current().muted();
    vec![
        Line::from(Span::styled(labels.into_iter().collect::<String>(), style)),
        Line::from(Span::styled(ticks.into_iter().collect::<String>(), style)),
    ]
}

fn capacity_map_half_band_line(
    model: &DiskLayoutModel,
    allocations: &[usize],
    selection: Option<&DiskCapacitySelection>,
    top: bool,
) -> Line<'static> {
    let theme = crate::tui::theme::current();
    let glyph = if top { "▄" } else { "▀" };
    Line::from(
        model
            .segments
            .iter()
            .zip(allocations.iter().copied())
            .map(|(segment, width)| {
                Span::styled(
                    glyph.repeat(width),
                    // Leave bg unset so the unused half-cell inherits the enclosing surface.
                    theme.disk_region_half_block(
                        segment.kind,
                        capacity_segment_active(segment, selection),
                    ),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn capacity_map_mini_line(
    model: &DiskLayoutModel,
    allocations: &[usize],
    selection: Option<&DiskCapacitySelection>,
) -> Line<'static> {
    let theme = crate::tui::theme::current();
    Line::from(
        model
            .segments
            .iter()
            .zip(allocations.iter().copied())
            .map(|(segment, width)| {
                Span::styled(
                    " ".repeat(width),
                    theme.disk_region_fill(
                        segment.kind,
                        capacity_segment_active(segment, selection),
                    ),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn capacity_map_content_line(
    model: &DiskLayoutModel,
    allocations: &[usize],
    selection: Option<&DiskCapacitySelection>,
    content: CapacityContent,
) -> Line<'static> {
    let theme = crate::tui::theme::current();
    Line::from(
        model
            .segments
            .iter()
            .zip(allocations.iter().copied())
            .map(|(segment, width)| {
                let text = match content {
                    CapacityContent::Label => capacity_segment_label(segment, width),
                    CapacityContent::Value => capacity_segment_value(segment, width, model),
                    CapacityContent::Compact => capacity_segment_compact(segment, width, model),
                };
                let active = capacity_segment_active(segment, selection);
                Span::styled(
                    center_capacity_label(&text, width),
                    theme
                        .disk_region_fill(segment.kind, active)
                        .patch(theme.disk_region_content_text(segment.kind, active)),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn capacity_segment_label(segment: &DiskLayoutSegment, width: usize) -> String {
    if width > 0 && crate::tui::table_layout::display_width(&segment.label) <= width {
        segment.label.clone()
    } else {
        String::new()
    }
}

fn capacity_segment_value(
    segment: &DiskLayoutSegment,
    width: usize,
    model: &DiskLayoutModel,
) -> String {
    if width < 8 {
        return String::new();
    }
    let capacity = format_layout_capacity(model, segment.sector_count);
    let full = format!(
        "{capacity} · {}",
        percentage(segment.sector_count, model.total_sectors)
    );
    if crate::tui::table_layout::display_width(&full) <= width {
        full
    } else if crate::tui::table_layout::display_width(&capacity) <= width {
        capacity
    } else {
        String::new()
    }
}

fn capacity_segment_compact(
    segment: &DiskLayoutSegment,
    width: usize,
    model: &DiskLayoutModel,
) -> String {
    let capacity = format_layout_capacity(model, segment.sector_count);
    let full = format!("{} {capacity}", segment.label);
    if crate::tui::table_layout::display_width(&full) <= width {
        full
    } else if crate::tui::table_layout::display_width(&segment.label) <= width {
        segment.label.clone()
    } else {
        String::new()
    }
}

fn center_capacity_label(label: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clipped = crate::tui::table_layout::truncate_cell(
        label,
        width,
        crate::tui::table_layout::TruncatePolicy::Clip,
    );
    let used = crate::tui::table_layout::display_width(&clipped);
    let left = width.saturating_sub(used) / 2;
    let right = width.saturating_sub(used).saturating_sub(left);
    format!("{}{}{}", " ".repeat(left), clipped, " ".repeat(right))
}

fn capacity_segment_active(
    segment: &DiskLayoutSegment,
    selection: Option<&DiskCapacitySelection>,
) -> bool {
    let Some(selection) = selection else {
        return false;
    };
    segment
        .end_exclusive()
        .ok()
        .is_some_and(|end| segment.start_lba < selection.end_exclusive && end > selection.start_lba)
}

pub(super) fn capacity_map_marker_column(
    model: &DiskLayoutModel,
    allocations: &[usize],
    selection: &DiskCapacitySelection,
) -> usize {
    let active_mid =
        u128::from(selection.start_lba).saturating_add(u128::from(selection.end_exclusive)) / 2;
    let mut cursor = 0usize;
    for (segment, width) in model.segments.iter().zip(allocations.iter().copied()) {
        let Ok(segment_end) = segment.end_exclusive() else {
            cursor = cursor.saturating_add(width);
            continue;
        };
        if segment.start_lba < selection.end_exclusive && segment_end > selection.start_lba {
            let segment_start = u128::from(segment.start_lba);
            let segment_len = u128::from(segment.sector_count).max(1);
            let relative = active_mid
                .clamp(segment_start, u128::from(segment_end).saturating_sub(1))
                .saturating_sub(segment_start);
            let offset = ((relative.saturating_mul(width as u128)) / segment_len)
                .min(width.saturating_sub(1) as u128) as usize;
            return cursor.saturating_add(offset);
        }
        cursor = cursor.saturating_add(width);
    }
    0
}
