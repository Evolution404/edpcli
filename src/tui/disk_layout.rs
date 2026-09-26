//! Shared sector geometry and proportional disk bar for Provision and Inspect.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::state::ProvisionBarKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiskRegionKind {
    Protocol,
    Reserved,
    Unknown,
    Free,
    Plain,
    Boot,
    Share,
    Combined,
    Encrypt,
    Compatibility,
    Lce,
    Tail,
}

impl DiskRegionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Protocol => "EDP 协议区",
            Self::Reserved => "保留区域",
            Self::Unknown => "未知区域",
            Self::Free => "空闲",
            Self::Plain => "普通分区",
            Self::Boot => "启动区",
            Self::Share => "交换区",
            Self::Combined => "二合一区",
            Self::Encrypt => "保密区",
            Self::Compatibility => "兼容保留区",
            Self::Lce => "LCE",
            Self::Tail => "盘尾区域",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::Protocol => 120,
            Self::Lce => 115,
            Self::Boot | Self::Share | Self::Combined | Self::Encrypt | Self::Plain => 110,
            Self::Compatibility => 105,
            Self::Tail => 100,
            Self::Reserved => 90,
            Self::Free => 80,
            Self::Unknown => 10,
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskLayoutSegment {
    pub label: String,
    pub start_lba: u64,
    pub sector_count: u64,
    pub kind: DiskRegionKind,
}

impl DiskLayoutSegment {
    pub fn end_exclusive(&self) -> Result<u64, String> {
        self.start_lba
            .checked_add(self.sector_count)
            .ok_or_else(|| format!("{} LBA range overflows", self.label))
    }

    pub fn closed_range(&self) -> String {
        self.start_lba
            .checked_add(self.sector_count)
            .and_then(|end| {
                crate::application::inspect_tree::format_lba_closed_range(self.start_lba, end)
            })
            .unwrap_or_else(|| "[无效范围]".into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskLayoutModel {
    pub total_sectors: u64,
    pub segments: Vec<DiskLayoutSegment>,
}

pub struct DiskLayoutPane<'a> {
    pub title: &'a str,
    pub summary: &'a str,
    pub details: &'a [String],
    pub focused: bool,
    pub scroll_y: usize,
}

impl DiskLayoutModel {
    pub fn new(total_sectors: u64, mut segments: Vec<DiskLayoutSegment>) -> Self {
        segments.sort_by_key(|segment| segment.start_lba);
        Self {
            total_sectors,
            segments,
        }
    }

    pub fn validate_complete(&self) -> Result<(), String> {
        if self.total_sectors == 0 {
            return if self.segments.is_empty() {
                Ok(())
            } else {
                Err("zero-sector disk layout must not contain segments".into())
            };
        }
        let Some(first) = self.segments.first() else {
            return Err("non-empty disk has no layout segments".into());
        };
        if first.start_lba != 0 {
            return Err(format!(
                "disk layout starts at LBA{} instead of LBA0",
                first.start_lba
            ));
        }
        for segment in &self.segments {
            if segment.sector_count == 0 {
                return Err(format!("{} has zero sectors", segment.label));
            }
            if segment.end_exclusive()? > self.total_sectors {
                return Err(format!("{} exceeds physical disk", segment.label));
            }
        }
        for pair in self.segments.windows(2) {
            let left_end = pair[0].end_exclusive()?;
            if left_end != pair[1].start_lba {
                return Err(if left_end < pair[1].start_lba {
                    format!("disk layout hole [{}..{})", left_end, pair[1].start_lba)
                } else {
                    format!("disk layout overlap at LBA{}", pair[1].start_lba)
                });
            }
        }
        let end = self.segments.last().expect("non-empty").end_exclusive()?;
        if end != self.total_sectors {
            return Err(format!(
                "disk layout ends at LBA{} instead of {}",
                end, self.total_sectors
            ));
        }
        Ok(())
    }

    pub fn from_claims(
        total_sectors: u64,
        claims: Vec<DiskLayoutSegment>,
        default_kind: DiskRegionKind,
    ) -> Self {
        if total_sectors == 0 {
            return Self::new(0, Vec::new());
        }
        let mut boundaries = vec![0, total_sectors];
        for claim in &claims {
            if claim.sector_count == 0 || claim.start_lba >= total_sectors {
                continue;
            }
            boundaries.push(claim.start_lba);
            boundaries.push(
                claim
                    .end_exclusive()
                    .unwrap_or(total_sectors)
                    .min(total_sectors),
            );
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut segments: Vec<DiskLayoutSegment> = Vec::new();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end {
                continue;
            }
            let winner = claims
                .iter()
                .filter(|claim| {
                    claim.sector_count > 0
                        && claim.start_lba <= start
                        && claim
                            .end_exclusive()
                            .is_ok_and(|claim_end| claim_end >= end)
                })
                .max_by_key(|claim| claim.kind.priority());
            let (kind, label) = winner
                .map(|claim| (claim.kind, claim.label.clone()))
                .unwrap_or((default_kind, default_kind.label().into()));
            if let Some(last) = segments.last_mut() {
                if last.kind == kind
                    && last.label == label
                    && last.end_exclusive().ok() == Some(start)
                {
                    last.sector_count = last.sector_count.saturating_add(end - start);
                    continue;
                }
            }
            segments.push(DiskLayoutSegment {
                label,
                start_lba: start,
                sector_count: end - start,
                kind,
            });
        }
        let model = Self::new(total_sectors, segments);
        debug_assert!(model.validate_complete().is_ok());
        model
    }

    pub fn from_topology(topology: &crate::application::inspect_tree::InspectTopology) -> Self {
        use crate::application::inspect_tree::{InspectChildren, InspectNode};

        fn node_kind(node: &InspectNode) -> DiskRegionKind {
            if node.id == "region.protocol" {
                DiskRegionKind::Protocol
            } else if node.id == "region.lce" {
                DiskRegionKind::Lce
            } else if node.id == "region.tail" {
                DiskRegionKind::Tail
            } else if node.id.starts_with("region.unknown") {
                DiskRegionKind::Unknown
            } else if node.id.starts_with("region.partition") || node.label.starts_with("MBR P") {
                if node.label.ends_with("type1") || node.label.contains("type=0x0E") {
                    DiskRegionKind::Boot
                } else if node.label.ends_with("type2") {
                    DiskRegionKind::Share
                } else if node.label.ends_with("type4") {
                    DiskRegionKind::Encrypt
                } else {
                    DiskRegionKind::Plain
                }
            } else {
                DiskRegionKind::Unknown
            }
        }

        fn collect(node: &InspectNode, claims: &mut Vec<DiskLayoutSegment>) {
            if node.id.starts_with("region.conflict") {
                if let InspectChildren::Materialized(children) = &node.children {
                    for child in children {
                        collect(child, claims);
                    }
                }
                return;
            }
            claims.push(DiskLayoutSegment {
                label: node.label.clone(),
                start_lba: node.range.start_lba,
                sector_count: node.range.sector_count,
                kind: node_kind(node),
            });
        }

        let mut claims = Vec::new();
        if let InspectChildren::Materialized(children) = &topology.root.children {
            for node in children {
                collect(node, &mut claims);
            }
        }
        Self::from_claims(
            topology.root.range.sector_count,
            claims,
            DiskRegionKind::Unknown,
        )
    }

    pub fn bar(&self, width: usize) -> Vec<DiskRegionKind> {
        let width = width.clamp(8, 96);
        if self.segments.is_empty() {
            return vec![DiskRegionKind::Free; width];
        }
        let baseline = usize::from(self.segments.len() <= width);
        let remaining = width.saturating_sub(baseline * self.segments.len());
        let total_weight = self
            .segments
            .iter()
            .map(|segment| u128::from(segment.sector_count))
            .sum::<u128>()
            .max(1);
        let mut allocations = Vec::with_capacity(self.segments.len());
        let mut assigned = 0usize;
        let mut remainders = Vec::with_capacity(self.segments.len());
        for (index, segment) in self.segments.iter().enumerate() {
            let scaled = u128::from(segment.sector_count) * remaining as u128;
            let count = baseline + (scaled / total_weight) as usize;
            allocations.push(count);
            assigned += count;
            remainders.push((scaled % total_weight, index));
        }
        remainders.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, index) in remainders.into_iter().take(width.saturating_sub(assigned)) {
            allocations[index] += 1;
        }
        let mut cells = Vec::with_capacity(width);
        for (segment, count) in self.segments.iter().zip(allocations) {
            cells.extend(std::iter::repeat_n(segment.kind, count));
        }
        cells.truncate(width);
        while cells.len() < width {
            cells.push(DiskRegionKind::Free);
        }
        cells
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
                    "{} {} · {} sectors · {percent}",
                    segment.label,
                    segment.closed_range(),
                    segment.sector_count
                )
            })
            .collect()
    }

    pub fn pane_line_count(&self, summary: &str, details: &[String]) -> usize {
        usize::from(!summary.is_empty())
            + 1
            + usize::from(!self.segments.is_empty())
            + self.segments.len()
            + usize::from(!details.is_empty())
            + details.len()
    }

    pub fn render_pane(&self, frame: &mut Frame<'_>, area: Rect, pane: DiskLayoutPane<'_>) {
        let theme = super::theme::current();
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
        }
        for (segment, text) in self.segments.iter().zip(self.legend_lines()) {
            lines.push(Line::from(vec![
                Span::styled("■ ", theme.disk_region(segment.kind)),
                Span::styled(text, theme.muted()),
            ]));
        }
        if !pane.details.is_empty() {
            lines.push(Line::from(""));
        }
        for detail in pane.details {
            let style = if detail.starts_with('✗') {
                theme.danger()
            } else if detail.starts_with('✓') {
                theme.success()
            } else if detail.starts_with("当前:") {
                theme.accent()
            } else {
                theme.muted()
            };
            lines.push(Line::from(Span::styled(detail.clone(), style)));
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
        assert!(model.legend_lines()[1].contains("[999994..999999] · 6 sectors · <0.01%"));
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
