//! Shared sector geometry and proportional disk bar for Provision and Inspect.

use ratatui::text::{Line, Span};

use super::state::ProvisionBarKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskLayoutSegment {
    pub label: String,
    pub start_lba: u64,
    pub sector_count: u64,
    pub kind: ProvisionBarKind,
}

impl DiskLayoutSegment {
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

impl DiskLayoutModel {
    pub fn new(total_sectors: u64, mut segments: Vec<DiskLayoutSegment>) -> Self {
        segments.sort_by_key(|segment| segment.start_lba);
        Self {
            total_sectors,
            segments,
        }
    }

    pub fn from_topology(topology: &crate::application::inspect_tree::InspectTopology) -> Self {
        use crate::application::inspect_tree::InspectChildren;
        let segments = match &topology.root.children {
            InspectChildren::Materialized(children) => children
                .iter()
                .map(|node| {
                    let kind = if node.id == "region.protocol" {
                        ProvisionBarKind::Boot
                    } else if node.id == "region.lce" {
                        ProvisionBarKind::Compatibility
                    } else if node.id.starts_with("region.unknown") {
                        ProvisionBarKind::Unknown
                    } else if node.id.starts_with("region.partition") {
                        ProvisionBarKind::Share
                    } else if node.id == "region.tail" {
                        ProvisionBarKind::Compatibility
                    } else {
                        ProvisionBarKind::Plain
                    };
                    DiskLayoutSegment {
                        label: node.label.clone(),
                        start_lba: node.range.start_lba,
                        sector_count: node.range.sector_count,
                        kind,
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        Self::new(topology.root.range.sector_count, segments)
    }

    pub fn bar(&self, width: usize) -> Vec<ProvisionBarKind> {
        let width = width.clamp(8, 96);
        if self.segments.is_empty() {
            return vec![ProvisionBarKind::Free; width];
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
            cells.push(ProvisionBarKind::Free);
        }
        cells
    }

    pub fn bar_line(&self, width: usize) -> Line<'static> {
        let bar = self.bar(width);
        let mut spans = vec![Span::raw("[")];
        let mut start = 0;
        while start < bar.len() {
            let kind = bar[start];
            let mut end = start + 1;
            while end < bar.len() && bar[end] == kind {
                end += 1;
            }
            spans.push(Span::styled(
                "━".repeat(end - start),
                super::theme::current().partition(kind),
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
                    kind: ProvisionBarKind::Unknown,
                },
                DiskLayoutSegment {
                    label: "LCE".into(),
                    start_lba: 999_994,
                    sector_count: 6,
                    kind: ProvisionBarKind::Compatibility,
                },
            ],
        );
        assert_eq!(model.bar(40).len(), 40);
        assert!(model.bar(40).contains(&ProvisionBarKind::Compatibility));
        assert!(model.legend_lines()[1].contains("[999994..999999] · 6 sectors · <0.01%"));
        assert!(model.legend_lines()[0].starts_with("未知区域"));
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
