//! UI-neutral whole-disk layout model shared by Inspect and Provision frontends.

use crate::application::inspect_tree::{
    DiskRegionSemantic, InspectChildren, InspectNode, InspectTopology,
};
use crate::protocol::edpf::EdpPartitionType;

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

    fn from_protocol_partition_type(partition_type: u32) -> Self {
        match EdpPartitionType::from_raw(partition_type) {
            Some(EdpPartitionType::Boot) => Self::Boot,
            Some(EdpPartitionType::Share) => Self::Share,
            Some(EdpPartitionType::Encrypt) => Self::Encrypt,
            None => Self::Unknown,
        }
    }

    fn from_region_semantic(semantic: DiskRegionSemantic) -> Self {
        match semantic {
            DiskRegionSemantic::Protocol => Self::Protocol,
            DiskRegionSemantic::PartitionTable => Self::Reserved,
            DiskRegionSemantic::PlainPartition => Self::Plain,
            DiskRegionSemantic::Unallocated => Self::Free,
            DiskRegionSemantic::Lce => Self::Lce,
            DiskRegionSemantic::Tail
            | DiskRegionSemantic::TailForensic
            | DiskRegionSemantic::TailMetadataMirror
            | DiskRegionSemantic::TailRestoreNode => Self::Tail,
            DiskRegionSemantic::Partition { partition_type } => {
                Self::from_protocol_partition_type(partition_type)
            }
            DiskRegionSemantic::MbrPartition { partition_type } => {
                if partition_type == 0x0e {
                    Self::Boot
                } else {
                    Self::Plain
                }
            }
            DiskRegionSemantic::Unknown | DiskRegionSemantic::Conflict => Self::Unknown,
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

    pub fn from_topology(topology: &InspectTopology) -> Self {
        fn collect(node: &InspectNode, claims: &mut Vec<DiskLayoutSegment>) {
            if node.region_semantic == Some(DiskRegionSemantic::Conflict) {
                if let InspectChildren::Materialized(children) = &node.children {
                    for child in children {
                        collect(child, claims);
                    }
                }
                return;
            }
            let kind = node
                .region_semantic
                .map(DiskRegionKind::from_region_semantic)
                .unwrap_or(DiskRegionKind::Unknown);
            claims.push(DiskLayoutSegment {
                label: node.label.clone(),
                start_lba: node.range.start_lba,
                sector_count: node.range.sector_count,
                kind,
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
        let width = width.clamp(8, 512);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::inspect_tree::{
        DiskRegionSemantic, InspectChildren, InspectNode, InspectNodeKind, InspectNodeRange,
        InspectTopology,
    };
    use crate::edpb::SemanticStatus;

    fn region(
        label: &str,
        start_lba: u64,
        count: u64,
        semantic: DiskRegionSemantic,
    ) -> InspectNode {
        InspectNode {
            id: "renamable".into(),
            label: label.into(),
            kind: InspectNodeKind::Extent,
            range: InspectNodeRange::sectors(start_lba, count),
            children: InspectChildren::Materialized(Vec::new()),
            decoder: None,
            status: SemanticStatus::Identified,
            region_semantic: Some(semantic),
        }
    }

    #[test]
    fn topology_mapping_uses_typed_semantic_not_id_or_label_text() {
        let root = InspectNode {
            id: "root".into(),
            label: "disk".into(),
            kind: InspectNodeKind::Device,
            range: InspectNodeRange::sectors(0, 40),
            children: InspectChildren::Materialized(vec![
                region("renamed protocol", 0, 13, DiskRegionSemantic::Protocol),
                region(
                    "renamed share",
                    13,
                    20,
                    DiskRegionSemantic::Partition { partition_type: 2 },
                ),
                region("renamed tail", 33, 7, DiskRegionSemantic::Tail),
            ]),
            decoder: None,
            status: SemanticStatus::Identified,
            region_semantic: None,
        };
        let model = DiskLayoutModel::from_topology(&InspectTopology { root });
        assert_eq!(model.segments[0].kind, DiskRegionKind::Protocol);
        assert_eq!(model.segments[1].kind, DiskRegionKind::Share);
        assert_eq!(model.segments[2].kind, DiskRegionKind::Tail);
    }
}
