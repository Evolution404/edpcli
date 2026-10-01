//! Shared sector geometry and proportional disk bar for Provision and Inspect.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::state::ProvisionBarKind;

pub use crate::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskLayoutProfile {
    CompactHuman,
    DetailedExact,
    EditorExact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TailExpansion {
    #[default]
    Collapsed,
    Expanded,
}

impl TailExpansion {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Collapsed => Self::Expanded,
            Self::Expanded => Self::Collapsed,
        };
    }
}

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
        let segment_count = visible.segments.len().max(1);
        let map_width = width.max(segment_count);
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
                        super::theme::current().disk_region_outline(selection.kind, true),
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

fn capacity_map_allocations(model: &DiskLayoutModel, cell_budget: usize) -> Vec<usize> {
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
    let style = super::theme::current().muted();
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
    let theme = super::theme::current();
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
    let theme = super::theme::current();
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
    let theme = super::theme::current();
    Line::from(
        model
            .segments
            .iter()
            .zip(allocations.iter().copied())
            .map(|(segment, width)| {
                let text = match content {
                    CapacityContent::Label => capacity_segment_label(segment, width),
                    CapacityContent::Value => {
                        capacity_segment_value(segment, width, model.total_sectors)
                    }
                    CapacityContent::Compact => capacity_segment_compact(segment, width),
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

fn capacity_segment_value(segment: &DiskLayoutSegment, width: usize, total: u64) -> String {
    if width < 8 {
        return String::new();
    }
    let capacity = format_sector_size(segment.sector_count);
    let full = format!("{capacity} · {}", percentage(segment.sector_count, total));
    if crate::tui::table_layout::display_width(&full) <= width {
        full
    } else if crate::tui::table_layout::display_width(&capacity) <= width {
        capacity
    } else {
        String::new()
    }
}

fn capacity_segment_compact(segment: &DiskLayoutSegment, width: usize) -> String {
    let capacity = format_sector_size(segment.sector_count);
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

fn capacity_map_marker_column(
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

/// A view over one canonical physical layout. Profiles only change its formatting.
pub struct DiskLayoutPresentation<'a> {
    model: &'a DiskLayoutModel,
    profile: DiskLayoutProfile,
    tail: TailExpansion,
}

impl<'a> DiskLayoutPresentation<'a> {
    pub fn new(
        model: &'a DiskLayoutModel,
        profile: DiskLayoutProfile,
        tail: TailExpansion,
    ) -> Self {
        Self {
            model,
            profile,
            tail,
        }
    }

    pub fn visible_model(&self) -> DiskLayoutModel {
        match self.tail {
            TailExpansion::Collapsed => self.model.collapsed_tail_model(),
            TailExpansion::Expanded => self.model.clone(),
        }
    }

    pub fn legend_lines(&self) -> Vec<String> {
        let visible = self.visible_model();
        let tail_start = if self.tail == TailExpansion::Expanded {
            self.model.tail_group().map(|tail| tail.start_lba)
        } else {
            None
        };
        visible
            .segments
            .iter()
            .map(|segment| {
                let label = if tail_start.is_some_and(|start| segment.start_lba >= start) {
                    let branch = if segment.end_exclusive().ok() == Some(visible.total_sectors) {
                        "└─"
                    } else {
                        "├─"
                    };
                    format!("{branch} {}", segment.label)
                } else {
                    segment.label.clone()
                };
                let capacity = match self.profile {
                    DiskLayoutProfile::CompactHuman => format_sector_size(segment.sector_count),
                    DiskLayoutProfile::DetailedExact | DiskLayoutProfile::EditorExact => {
                        format!(
                            "{} sectors / {} bytes",
                            segment.sector_count,
                            segment
                                .sector_count
                                .saturating_mul(crate::common::SECTOR as u64)
                        )
                    }
                };
                format!(
                    "{}  {}  {}  {}",
                    crate::ui::pad_to(&label, 18),
                    crate::ui::pad_to(&segment.closed_range(), 24),
                    crate::ui::pad_to(&capacity, 28),
                    percentage(segment.sector_count, visible.total_sectors)
                )
            })
            .collect()
    }

    pub fn compact_grid_lines(&self, width: usize) -> Vec<Line<'static>> {
        use crate::tui::table_layout::display_width;

        let visible = self.visible_model();
        let mut entries = vec![(
            None,
            format!("总容量  {}", format_sector_size(visible.total_sectors)),
        )];
        entries.extend(visible.segments.iter().map(|segment| {
            (
                Some(segment.kind),
                format!(
                    "{}  {}",
                    segment.label,
                    format_sector_size(segment.sector_count)
                ),
            )
        }));
        let available = width.max(1);
        let gap = 4usize;
        let max_entry_width = entries
            .iter()
            .map(|(kind, text)| display_width(text) + usize::from(kind.is_some()) * 2)
            .max()
            .unwrap_or(1)
            .max(1);
        let mut columns = entries.len().clamp(1, 4);
        while columns > 1
            && max_entry_width
                .saturating_mul(columns)
                .saturating_add(gap.saturating_mul(columns - 1))
                > available
        {
            columns -= 1;
        }
        let cell_width = if columns == 1 {
            available
        } else {
            available.saturating_sub(gap.saturating_mul(columns - 1)) / columns
        }
        .max(1);
        entries
            .chunks(columns)
            .map(|chunk| {
                let mut spans = vec![Span::raw("    ")];
                for (position, (kind, text)) in chunk.iter().enumerate() {
                    if position > 0 {
                        spans.push(Span::raw(" ".repeat(gap)));
                    }
                    let prefix_width = if let Some(kind) = kind {
                        spans.push(Span::styled(
                            "■ ",
                            super::theme::current().disk_region(*kind),
                        ));
                        2
                    } else {
                        0
                    };
                    let room = cell_width.saturating_sub(prefix_width);
                    let text = if display_width(text) <= room {
                        text.clone()
                    } else {
                        crate::tui::table_layout::truncate_cell(
                            text,
                            room,
                            crate::tui::table_layout::TruncatePolicy::Clip,
                        )
                    };
                    spans.push(Span::styled(
                        crate::ui::pad_to(&text, room),
                        if kind.is_some() {
                            super::theme::current().muted()
                        } else {
                            ratatui::style::Style::default()
                        },
                    ));
                }
                Line::from(spans)
            })
            .collect()
    }

    pub fn pane_line_count(&self, summary: &str, details: &[DiskLayoutDetail]) -> usize {
        self.visible_model().pane_line_count(summary, details)
            + usize::from(self.tail == TailExpansion::Expanded && self.model.tail_group().is_some())
    }
}

fn percentage(sectors: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".into();
    }
    let ratio = sectors as f64 * 100.0 / total as f64;
    if ratio > 0.0 && ratio < 0.01 {
        "<0.01%".into()
    } else {
        format!("{ratio:.2}%")
    }
}

fn format_sector_size(sectors: u64) -> String {
    crate::common::fmt_capacity_sectors(sectors)
}

impl DiskRegionKind {
    pub const fn visual_kind(self) -> ProvisionBarKind {
        match self {
            Self::Protocol | Self::Boot => ProvisionBarKind::Boot,
            Self::Share | Self::Combined => ProvisionBarKind::Share,
            Self::Encrypt => ProvisionBarKind::Encrypt,
            Self::Metadata
            | Self::Compatibility
            | Self::Reserved
            | Self::Lce
            | Self::BackupMirror
            | Self::RestoreNode
            | Self::Tail => ProvisionBarKind::Compatibility,
            Self::Plain => ProvisionBarKind::Plain,
            Self::Free | Self::Unknown | Self::Conflict => ProvisionBarKind::Free,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskLayoutDetailTone {
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskLayoutDetail {
    pub text: String,
    pub tone: DiskLayoutDetailTone,
    pub columns: Option<[String; 4]>,
    pub region_kind: Option<DiskRegionKind>,
    pub selected: bool,
}

impl DiskLayoutDetail {
    pub fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Muted,
            columns: None,
            region_kind: None,
            selected: false,
        }
    }

    pub fn accent(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Accent,
            columns: None,
            region_kind: None,
            selected: false,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Success,
            columns: None,
            region_kind: None,
            selected: false,
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Warning,
            columns: None,
            region_kind: None,
            selected: false,
        }
    }

    pub fn danger(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Danger,
            columns: None,
            region_kind: None,
            selected: false,
        }
    }

    pub fn region_header() -> Self {
        Self {
            text: String::new(),
            tone: DiskLayoutDetailTone::Accent,
            columns: Some([
                "区域".into(),
                "容量".into(),
                "LBA 范围".into(),
                "处理".into(),
            ]),
            region_kind: None,
            selected: false,
        }
    }

    pub fn region_columns(
        kind: DiskRegionKind,
        selected: bool,
        name: impl Into<String>,
        capacity: impl Into<String>,
        range: impl Into<String>,
        status: impl Into<String>,
        tone: DiskLayoutDetailTone,
    ) -> Self {
        Self {
            text: String::new(),
            tone,
            columns: Some([name.into(), capacity.into(), range.into(), status.into()]),
            region_kind: Some(kind),
            selected,
        }
    }
}

pub struct DiskLayoutPane<'a> {
    pub title: &'a str,
    pub summary: &'a str,
    pub details: &'a [DiskLayoutDetail],
    pub focused: bool,
    pub scroll_y: usize,
    pub profile: DiskLayoutProfile,
    pub tail: TailExpansion,
    pub selected_segment: usize,
    pub map_selection: Option<DiskCapacitySelection>,
    pub show_map_marker: bool,
    pub show_linked_selection: bool,
}

impl DiskLayoutModel {
    pub fn render_mini(&self, frame: &mut Frame<'_>, area: Rect, current_lba: Option<u64>) {
        let title = current_lba
            .map(|lba| format!("磁盘概览 · 当前 LBA{lba}"))
            .unwrap_or_else(|| "磁盘概览".into());
        let selection = current_lba.and_then(|lba| DiskCapacitySelection::from_lba(self, lba));
        let lines = DiskCapacityMap::new(self, DiskCapacityMapProfile::Mini)
            .with_tail(TailExpansion::Collapsed)
            .with_selection(selection)
            .with_marker(false)
            .lines(area.width.saturating_sub(2) as usize);
        frame.render_widget(
            Paragraph::new(lines).block(super::ui::panel(title, false)),
            area,
        );
    }

    pub fn legend_lines(&self) -> Vec<String> {
        DiskLayoutPresentation::new(
            self,
            DiskLayoutProfile::DetailedExact,
            TailExpansion::Expanded,
        )
        .legend_lines()
    }

    pub fn pane_line_count(&self, summary: &str, details: &[DiskLayoutDetail]) -> usize {
        usize::from(!summary.is_empty()) + 4 + usize::from(!details.is_empty()) + details.len()
    }

    pub fn render_pane(&self, frame: &mut Frame<'_>, area: Rect, pane: DiskLayoutPane<'_>) {
        let presentation = DiskLayoutPresentation::new(self, pane.profile, pane.tail);
        let visible = presentation.visible_model();
        let theme = super::theme::current();
        let compact =
            super::ui::ViewportClass::for_width(area.width) == super::ui::ViewportClass::Compact;
        let mut lines = Vec::<Line<'static>>::new();
        if !pane.summary.is_empty() {
            lines.push(Line::from(Span::styled(
                pane.summary.to_string(),
                theme.secondary_text(),
            )));
        }
        let selection = pane.map_selection.or_else(|| {
            pane.focused
                .then(|| {
                    visible
                        .segments
                        .get(pane.selected_segment)
                        .and_then(DiskCapacitySelection::from_segment)
                })
                .flatten()
        });
        lines.extend(
            DiskCapacityMap::new(self, DiskCapacityMapProfile::Compact)
                .with_tail(pane.tail)
                .with_selection(selection)
                .with_marker(pane.show_map_marker || pane.focused)
                .lines(area.width.saturating_sub(4) as usize),
        );
        if !pane.details.is_empty() {
            lines.push(Line::from(""));
        }
        let detail_width = area.width.saturating_sub(2) as usize;
        let region_headings = ["区域", "容量", "LBA 范围", "处理"];
        let region_rows = pane
            .details
            .iter()
            .filter_map(|detail| detail.columns.as_ref())
            .map(|columns| columns.to_vec())
            .collect::<Vec<_>>();
        let region_layout = crate::tui::table_layout::content_driven_layout(&region_headings)
            .with_column_spacing(2);
        let region_content_widths =
            crate::tui::table_layout::content_widths(&region_headings, &region_rows);
        let region_widths = region_layout.natural_widths(&region_content_widths, None);
        let region_total_width = 1usize.saturating_add(
            region_widths.iter().sum::<usize>()
                + region_widths
                    .len()
                    .saturating_sub(1)
                    .saturating_mul(region_layout.column_spacing()),
        );
        let compact_region_rows = compact || region_total_width > detail_width;
        for detail in pane.details {
            let style = match detail.tone {
                DiskLayoutDetailTone::Muted => theme.muted(),
                DiskLayoutDetailTone::Accent => theme.accent(),
                DiskLayoutDetailTone::Success => theme.success(),
                DiskLayoutDetailTone::Warning => theme.warning(),
                DiskLayoutDetailTone::Danger => theme.danger(),
            };
            if let Some([name, capacity, range, status]) = &detail.columns {
                let selected_visible =
                    detail.selected && (pane.focused || pane.show_linked_selection);
                let region_style = detail
                    .region_kind
                    .map(|kind| theme.disk_region_tree(kind, false));
                let is_header = detail.region_kind.is_none();
                let name_style = if is_header {
                    style
                } else {
                    region_style.unwrap_or_else(|| theme.muted())
                };
                let capacity_style = if is_header {
                    style
                } else {
                    theme.secondary_text()
                };
                let range_style = if is_header { style } else { theme.muted() };
                let marker_style = region_style.unwrap_or_else(|| theme.accent());
                let marker = if selected_visible { "▌" } else { " " };
                let row_style = if selected_visible {
                    theme.selection_overlay(pane.focused)
                } else {
                    Style::default()
                };
                let finish_row = |mut spans: Vec<Span<'static>>, used_width: usize| {
                    if selected_visible {
                        let padding = detail_width.saturating_sub(used_width);
                        if padding > 0 {
                            spans.push(Span::styled(" ".repeat(padding), row_style));
                        }
                    }
                    Line::from(spans).style(row_style)
                };
                let gap = region_layout.column_spacing();
                if compact_region_rows {
                    let name_width = region_widths.first().copied().unwrap_or_default();
                    let capacity_width = region_widths.get(1).copied().unwrap_or_default();
                    let name = crate::ui::pad_to(name, name_width);
                    let capacity = crate::ui::pad_to(capacity, capacity_width);
                    let used =
                        1 + crate::ui::disp_width(&name) + gap + crate::ui::disp_width(status);
                    lines.push(finish_row(
                        vec![
                            Span::styled(marker, marker_style),
                            Span::styled(name, name_style),
                            Span::raw(" ".repeat(gap)),
                            Span::styled(status.clone(), style),
                        ],
                        used,
                    ));
                    let used =
                        2 + crate::ui::disp_width(&capacity) + gap + crate::ui::disp_width(range);
                    lines.push(finish_row(
                        vec![
                            Span::raw("  "),
                            Span::styled(capacity, capacity_style),
                            Span::raw(" ".repeat(gap)),
                            Span::styled(range.clone(), range_style),
                        ],
                        used,
                    ));
                } else {
                    let name = crate::ui::pad_to(name, region_widths[0]);
                    let capacity = crate::ui::pad_to(capacity, region_widths[1]);
                    let range = crate::ui::pad_to(range, region_widths[2]);
                    let status = crate::ui::pad_to(status, region_widths[3]);
                    let used = 1
                        + crate::ui::disp_width(&name)
                        + crate::ui::disp_width(&capacity)
                        + crate::ui::disp_width(&range)
                        + crate::ui::disp_width(&status)
                        + gap.saturating_mul(3);
                    lines.push(finish_row(
                        vec![
                            Span::styled(marker, marker_style),
                            Span::styled(name, name_style),
                            Span::raw(" ".repeat(gap)),
                            Span::styled(capacity, capacity_style),
                            Span::raw(" ".repeat(gap)),
                            Span::styled(range, range_style),
                            Span::raw(" ".repeat(gap)),
                            Span::styled(status, style),
                        ],
                        used,
                    ));
                }
            } else {
                lines.push(Line::from(Span::styled(detail.text.clone(), style)));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(super::ui::card(pane.title.to_string(), pane.focused))
                .scroll((pane.scroll_y.min(u16::MAX as usize) as u16, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn capacity_map_fixture() -> DiskLayoutModel {
        DiskLayoutModel::new(
            1_000,
            vec![
                DiskLayoutSegment {
                    label: "启动区".into(),
                    start_lba: 0,
                    sector_count: 100,
                    kind: DiskRegionKind::Boot,
                },
                DiskLayoutSegment {
                    label: "交换区".into(),
                    start_lba: 100,
                    sector_count: 700,
                    kind: DiskRegionKind::Share,
                },
                DiskLayoutSegment {
                    label: "保密区".into(),
                    start_lba: 800,
                    sector_count: 200,
                    kind: DiskRegionKind::Encrypt,
                },
            ],
        )
    }

    #[test]
    fn capacity_map_profiles_share_one_renderer_with_page_specific_density() {
        let model = capacity_map_fixture();
        let full = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Full)
            .with_marker(false)
            .lines(80);
        assert_eq!(full.len(), 6);
        assert!(full[0].to_string().contains("0%"));
        assert!(full[0].to_string().contains("100%"));
        assert!(full[1].to_string().contains('┈'));
        assert!(full[2].to_string().contains('▄'));
        assert!(full[5].to_string().contains('▀'));

        let compact = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Compact).lines(80);
        assert_eq!(compact.len(), 3);
        assert!(compact[0].to_string().contains('▄'));
        assert!(compact[2].to_string().contains('▀'));
        assert!(!compact.iter().any(|line| line.to_string().contains('%')));

        let mini = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Mini).lines(80);
        assert_eq!(mini.len(), 1);
        let mini_text = mini[0].to_string();
        assert!(!mini_text.contains('%'));
        assert!(!mini_text.contains('▄'));
        assert!(!mini_text.contains('▀'));
        assert!(!mini_text.contains('▲'));
    }

    #[test]
    fn mini_capacity_map_highlights_the_region_containing_current_lba() {
        let model = capacity_map_fixture();
        let selection = DiskCapacitySelection::from_lba(&model, 850).expect("encrypt selection");
        let line = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Mini)
            .with_selection(Some(selection))
            .lines(80)
            .pop()
            .expect("mini line");
        let active_encrypt = super::super::theme::current()
            .disk_region_fill(DiskRegionKind::Encrypt, true)
            .bg;
        let normal_share = super::super::theme::current()
            .disk_region_fill(DiskRegionKind::Share, false)
            .bg;
        assert!(line
            .spans
            .iter()
            .any(|span| span.style.bg == active_encrypt));
        assert!(line.spans.iter().any(|span| span.style.bg == normal_share));
    }

    #[test]
    fn partition_status_stays_visible_at_60_80_100_and_120_columns() {
        let model = DiskLayoutModel::new(
            100_000,
            vec![DiskLayoutSegment {
                label: "保密区".into(),
                start_lba: 0,
                sector_count: 100_000,
                kind: DiskRegionKind::Encrypt,
            }],
        );
        let details = [DiskLayoutDetail::region_columns(
            DiskRegionKind::Encrypt,
            true,
            "保密区",
            "48.8 MiB",
            "LBA 0–99999",
            "⚠ 需重建",
            DiskLayoutDetailTone::Warning,
        )];
        for width in [60u16, 80, 100, 120] {
            let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
            terminal
                .draw(|frame| {
                    model.render_pane(
                        frame,
                        frame.area(),
                        DiskLayoutPane {
                            title: "布局",
                            summary: "disk6",
                            details: &details,
                            focused: true,
                            scroll_y: 0,
                            profile: DiskLayoutProfile::DetailedExact,
                            tail: TailExpansion::Collapsed,
                            selected_segment: 0,
                            map_selection: None,
                            show_map_marker: false,
                            show_linked_selection: false,
                        },
                    );
                })
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            let compact = screen
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            assert!(
                compact.contains("⚠需重建"),
                "width={width} screen={screen:?}"
            );
        }
    }

    #[test]
    fn linked_region_selection_uses_full_row_background_without_brightening_name() {
        let model = DiskLayoutModel::new(
            100_000,
            vec![DiskLayoutSegment {
                label: "保密区".into(),
                start_lba: 0,
                sector_count: 100_000,
                kind: DiskRegionKind::Encrypt,
            }],
        );
        let details = [DiskLayoutDetail::region_columns(
            DiskRegionKind::Encrypt,
            true,
            "保密区",
            "48.8 MiB",
            "LBA 0–99999",
            "⚠ 需重建",
            DiskLayoutDetailTone::Warning,
        )];
        let width = 100u16;
        let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
        terminal
            .draw(|frame| {
                model.render_pane(
                    frame,
                    frame.area(),
                    DiskLayoutPane {
                        title: "布局",
                        summary: "",
                        details: &details,
                        focused: false,
                        scroll_y: 0,
                        profile: DiskLayoutProfile::DetailedExact,
                        tail: TailExpansion::Collapsed,
                        selected_segment: 0,
                        map_selection: None,
                        show_map_marker: false,
                        show_linked_selection: true,
                    },
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let selected_y = (0..16u16)
            .find(|&y| {
                let text = (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>();
                text.chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect::<String>()
                    .contains("⚠需重建")
            })
            .expect("selected region row");
        let theme = super::super::theme::current();
        let selected_bg = theme
            .selection_overlay(false)
            .bg
            .expect("selection overlay background");
        assert_eq!(buffer[(1, selected_y)].bg, selected_bg);
        assert_eq!(
            buffer[(width - 2, selected_y)].bg,
            selected_bg,
            "selection background must fill the complete inner row"
        );
        assert_eq!(
            buffer[(2, selected_y)].fg,
            theme
                .disk_region_tree(DiskRegionKind::Encrypt, false)
                .fg
                .expect("base region foreground"),
            "selection must keep the normal region foreground instead of brightening it"
        );
    }

    #[test]
    fn tiny_lce_keeps_exact_legend_and_visible_bar_cell() {
        let model = DiskLayoutModel::new(
            1_000_000,
            vec![
                DiskLayoutSegment {
                    label: "未知区域".into(),
                    start_lba: 0,
                    sector_count: 999_994,
                    kind: DiskRegionKind::Unknown,
                },
                DiskLayoutSegment {
                    label: "LCE".into(),
                    start_lba: 999_994,
                    sector_count: 6,
                    kind: DiskRegionKind::Lce,
                },
            ],
        );
        assert_eq!(model.bar(40).len(), 40);
        assert!(model.bar(40).contains(&DiskRegionKind::Lce));
        assert!(model.legend_lines()[1].contains("[999994..999999]"));
        assert!(model.legend_lines()[1].contains("6 sectors"));
        assert!(model.legend_lines()[1].contains("<0.01%"));
        assert!(!model.legend_lines()[1].contains(" · "));
        assert!(model.legend_lines()[0].starts_with("未知区域"));
    }

    #[test]
    fn complete_contract_rejects_holes_overlap_and_wrong_last_sector() {
        let hole = DiskLayoutModel::new(
            10,
            vec![
                DiskLayoutSegment {
                    label: "a".into(),
                    start_lba: 0,
                    sector_count: 4,
                    kind: DiskRegionKind::Protocol,
                },
                DiskLayoutSegment {
                    label: "b".into(),
                    start_lba: 5,
                    sector_count: 5,
                    kind: DiskRegionKind::Unknown,
                },
            ],
        );
        assert!(hole.validate_complete().unwrap_err().contains("hole"));

        let overlap = DiskLayoutModel::new(
            10,
            vec![
                DiskLayoutSegment {
                    label: "a".into(),
                    start_lba: 0,
                    sector_count: 6,
                    kind: DiskRegionKind::Protocol,
                },
                DiskLayoutSegment {
                    label: "b".into(),
                    start_lba: 5,
                    sector_count: 5,
                    kind: DiskRegionKind::Unknown,
                },
            ],
        );
        assert!(overlap.validate_complete().unwrap_err().contains("overlap"));

        let short = DiskLayoutModel::new(
            10,
            vec![DiskLayoutSegment {
                label: "a".into(),
                start_lba: 0,
                sector_count: 9,
                kind: DiskRegionKind::Unknown,
            }],
        );
        assert!(short.validate_complete().unwrap_err().contains("ends at"));
    }

    #[test]
    fn canonical_layout_bar_preserves_lce_and_complete_coverage() {
        let model = DiskLayoutModel::canonical_edp(
            10_000,
            vec![
                DiskLayoutSegment {
                    label: "启动区".into(),
                    start_lba: 63,
                    sector_count: 37,
                    kind: DiskRegionKind::Boot,
                },
                DiskLayoutSegment {
                    label: "交换区".into(),
                    start_lba: 100,
                    sector_count: 4_900,
                    kind: DiskRegionKind::Share,
                },
                DiskLayoutSegment {
                    label: "保密区".into(),
                    start_lba: 5_000,
                    sector_count: 1_000,
                    kind: DiskRegionKind::Encrypt,
                },
            ],
            6_000,
            6,
        )
        .unwrap();
        assert_eq!(model.total_sectors, 10_000);
        assert_eq!(
            model
                .segments
                .iter()
                .find(|segment| segment.kind == DiskRegionKind::Lce)
                .unwrap()
                .sector_count,
            6
        );
        assert_eq!(model.bar(64).len(), 64);
        assert_eq!(
            model
                .segments
                .iter()
                .map(|segment| segment.sector_count)
                .sum::<u64>(),
            10_000
        );
        assert_eq!(
            model
                .collapsed_tail_model()
                .segments
                .last()
                .map(|segment| segment.kind),
            Some(DiskRegionKind::Tail)
        );
    }
}
