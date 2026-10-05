//! Shared sector geometry and proportional disk bar for Provision and Inspect.

use crate::common::fmt_sector_percentage as percentage;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

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

#[path = "disk_layout/capacity_map.rs"]
mod capacity_map;
#[cfg(test)]
use capacity_map::{capacity_map_allocations, capacity_map_marker_column};
pub use capacity_map::{DiskCapacityMap, DiskCapacityMapProfile, DiskCapacitySelection};

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

    pub fn pane_line_count(&self, summary: &str, details: &[DiskLayoutDetail]) -> usize {
        self.visible_model().pane_line_count(summary, details)
            + usize::from(self.tail == TailExpansion::Expanded && self.model.tail_group().is_some())
    }
}

fn format_sector_size(sectors: u64) -> String {
    crate::common::fmt_capacity_sectors(sectors)
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
#[path = "disk_layout/tests.rs"]
mod tests;
