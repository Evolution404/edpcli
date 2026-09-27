//! Shared sector geometry and proportional disk bar for Provision and Inspect.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::state::ProvisionBarKind;

pub use crate::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

impl DiskRegionKind {
    pub const fn visual_kind(self) -> ProvisionBarKind {
        match self {
            Self::Protocol | Self::Boot => ProvisionBarKind::Boot,
            Self::Share | Self::Combined => ProvisionBarKind::Share,
            Self::Encrypt => ProvisionBarKind::Encrypt,
            Self::Compatibility | Self::Reserved | Self::Lce | Self::Tail => {
                ProvisionBarKind::Compatibility
            }
            Self::Plain => ProvisionBarKind::Plain,
            Self::Free | Self::Unknown => ProvisionBarKind::Free,
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
}

impl DiskLayoutDetail {
    pub fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Muted,
            columns: None,
        }
    }

    pub fn accent(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Accent,
            columns: None,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Success,
            columns: None,
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Warning,
            columns: None,
        }
    }

    pub fn danger(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: DiskLayoutDetailTone::Danger,
            columns: None,
        }
    }

    pub fn partition_columns(
        name: impl Into<String>,
        range: impl Into<String>,
        capacity: impl Into<String>,
        status: impl Into<String>,
        tone: DiskLayoutDetailTone,
    ) -> Self {
        Self {
            text: String::new(),
            tone,
            columns: Some([name.into(), range.into(), capacity.into(), status.into()]),
        }
    }
}

pub struct DiskLayoutPane<'a> {
    pub title: &'a str,
    pub summary: &'a str,
    pub details: &'a [DiskLayoutDetail],
    pub focused: bool,
    pub scroll_y: usize,
}

impl DiskLayoutModel {
    pub fn render_compact(&self, frame: &mut Frame<'_>, area: Rect, current_lba: Option<u64>) {
        let title = match current_lba {
            Some(lba) => format!("磁盘概览 · 当前 LBA{lba} · 全盘布局可下钻"),
            None => "磁盘概览 · 全盘布局可下钻".into(),
        };
        frame.render_widget(
            Paragraph::new(self.bar_line(area.width.saturating_sub(4) as usize))
                .block(super::ui::panel(title, false)),
            area,
        );
    }

    pub fn bar_line(&self, width: usize) -> Line<'static> {
        self.bar_line_with_label(width, "")
    }

    pub fn bar_line_with_label(&self, width: usize, label: &'static str) -> Line<'static> {
        let bar = self.bar(width);
        let mut spans = vec![Span::raw(label), Span::raw("[")];
        let mut start = 0;
        while start < bar.len() {
            let kind = bar[start];
            let mut end = start + 1;
            while end < bar.len() && bar[end] == kind {
                end += 1;
            }
            spans.push(Span::styled(
                "━".repeat(end - start),
                super::theme::current().disk_region(kind),
            ));
            start = end;
        }
        spans.push(Span::raw("]"));
        Line::from(spans)
    }

    pub fn legend_lines(&self) -> Vec<String> {
        self.segments
            .iter()
            .map(|segment| {
                let percent = if self.total_sectors == 0 {
                    "0.00%".into()
                } else {
                    let ratio = segment.sector_count as f64 * 100.0 / self.total_sectors as f64;
                    if ratio > 0.0 && ratio < 0.01 {
                        "<0.01%".into()
                    } else {
                        format!("{ratio:.2}%")
                    }
                };
                format!(
                    "{}  {}  {}  {}",
                    crate::ui::pad_to(&segment.label, 18),
                    crate::ui::pad_to(&segment.closed_range(), 24),
                    crate::ui::pad_to(&format!("{} sectors", segment.sector_count), 18),
                    percent
                )
            })
            .collect()
    }

    pub fn pane_line_count(&self, summary: &str, details: &[DiskLayoutDetail]) -> usize {
        usize::from(!summary.is_empty())
            + 1
            + usize::from(!self.segments.is_empty())
            + self.segments.len()
            + usize::from(!details.is_empty())
            + details.len()
    }

    pub fn render_pane(&self, frame: &mut Frame<'_>, area: Rect, pane: DiskLayoutPane<'_>) {
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
        lines.push(self.bar_line(area.width.saturating_sub(4) as usize));
        if !self.segments.is_empty() {
            lines.push(Line::from(""));
            if !compact {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}  {}  {}  {}",
                        crate::ui::pad_to("区域", 18),
                        crate::ui::pad_to("LBA 范围", 24),
                        crate::ui::pad_to("扇区数", 18),
                        "占比"
                    ),
                    theme.secondary_text(),
                )));
            }
        }
        for (segment, text) in self.segments.iter().zip(self.legend_lines()) {
            if compact {
                lines.push(Line::from(vec![
                    Span::styled("■ ", theme.disk_region(segment.kind)),
                    Span::styled(segment.label.clone(), theme.muted()),
                ]));
                lines.push(Line::from(format!(
                    "  {}  {} sector",
                    segment.closed_range(),
                    segment.sector_count
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("■ ", theme.disk_region(segment.kind)),
                    Span::styled(text, theme.muted()),
                ]));
            }
        }
        if !pane.details.is_empty() {
            lines.push(Line::from(""));
        }
        for detail in pane.details {
            let style = match detail.tone {
                DiskLayoutDetailTone::Muted => theme.muted(),
                DiskLayoutDetailTone::Accent => theme.accent(),
                DiskLayoutDetailTone::Success => theme.success(),
                DiskLayoutDetailTone::Warning => theme.warning(),
                DiskLayoutDetailTone::Danger => theme.danger(),
            };
            if let Some([name, range, capacity, status]) = &detail.columns {
                if compact {
                    lines.push(Line::from(vec![
                        Span::styled(crate::ui::pad_to(name, 18), theme.muted()),
                        Span::styled(status.clone(), style),
                    ]));
                    lines.push(Line::from(format!("  {range}  {capacity}")));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(crate::ui::pad_to(name, 18), theme.muted()),
                        Span::styled(crate::ui::pad_to(range, 24), theme.muted()),
                        Span::styled(crate::ui::pad_to(capacity, 14), theme.muted()),
                        Span::styled(status.clone(), style),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(detail.text.clone(), style)));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(if pane.focused {
                            theme.focused_panel()
                        } else {
                            theme.panel()
                        })
                        .title(pane.title.to_string()),
                )
                .scroll((pane.scroll_y.min(u16::MAX as usize) as u16, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup_metadata::Lba7CompatibilityGeometry;
    use crate::inspect_target::InspectDiskContext;
    use ratatui::{backend::TestBackend, Terminal};

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
        let details = [DiskLayoutDetail::partition_columns(
            "保密区",
            "LBA 0–99999",
            "48.8 MiB",
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
    fn inspect_topology_and_provision_share_the_same_bar_allocation() {
        let mut context =
            InspectDiskContext::new(vec![0; crate::common::METADATA_IMAGE_LEN], None, 10_000);
        context.lce = Some(Lba7CompatibilityGeometry {
            start_lba: 6_000,
            sector_count: 6,
            lba7_pointer_entries: Vec::new(),
            official_partition_mode: None,
            chs_expected_start_lba: None,
        });
        let topology = crate::application::inspect_tree::build_inspect_topology(&context);
        let model = DiskLayoutModel::from_topology(&topology);
        assert_eq!(model.total_sectors, 10_000);
        assert_eq!(
            model
                .segments
                .iter()
                .find(|segment| segment.label.starts_with("LCE"))
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
    }
}
