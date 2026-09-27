//! UI-neutral whole-disk layout model shared by Inspect and Provision frontends.

#[cfg(test)]
use crate::application::inspect_tree::{
    DiskRegionSemantic, InspectChildren, InspectNode, InspectTopology,
};
use crate::application::partition_table::PartitionTableSnapshot;
use crate::backup_metadata::{
    TAIL_END4_MIRROR_OFFSET_SECTORS, TAIL_METADATA_MIRROR_OFFSET_SECTORS,
    TAIL_METADATA_MIRROR_SECTORS,
};
use crate::inspect_target::InspectDiskContext;
use crate::protocol::edpf::EdpPartitionType;
use crate::provision::PartitionRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiskRegionKind {
    Protocol,
    Metadata,
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
    BackupMirror,
    RestoreNode,
    Tail,
}

impl DiskRegionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Protocol => "EDP 协议区",
            Self::Metadata => "元数据",
            Self::Reserved => "保留区域",
            Self::Unknown => "未知区域",
            Self::Free => "空闲区域",
            Self::Plain => "普通分区",
            Self::Boot => "启动区",
            Self::Share => "交换区",
            Self::Combined => "二合一区",
            Self::Encrypt => "保密区",
            Self::Compatibility => "兼容保留区",
            Self::Lce => "LCE",
            Self::BackupMirror => "历史备份镜像",
            Self::RestoreNode => "restore-node",
            Self::Tail => "尾部区域",
        }
    }

    #[cfg(test)]
    const fn priority(self) -> u8 {
        match self {
            Self::Protocol => 120,
            Self::Lce => 115,
            Self::BackupMirror | Self::RestoreNode => 114,
            Self::Boot | Self::Share | Self::Combined | Self::Encrypt | Self::Plain => 110,
            Self::Compatibility => 105,
            Self::Tail => 100,
            Self::Metadata => 95,
            Self::Reserved => 90,
            Self::Free => 80,
            Self::Unknown => 10,
        }
    }

    pub const fn from_partition_role(role: PartitionRole) -> Self {
        match role {
            PartitionRole::Boot => Self::Boot,
            PartitionRole::Share => Self::Share,
            PartitionRole::BootShareCombined => Self::Combined,
            PartitionRole::Encrypt => Self::Encrypt,
            PartitionRole::CompatibilityReserve => Self::Compatibility,
        }
    }

    #[cfg(test)]
    fn from_protocol_partition_type(partition_type: u32) -> Self {
        match EdpPartitionType::from_raw(partition_type) {
            Some(EdpPartitionType::Boot) => Self::Boot,
            Some(EdpPartitionType::Share) => Self::Share,
            Some(EdpPartitionType::Encrypt) => Self::Encrypt,
            None => Self::Unknown,
        }
    }

    #[cfg(test)]
    fn from_region_semantic(semantic: DiskRegionSemantic) -> Self {
        match semantic {
            DiskRegionSemantic::Protocol => Self::Protocol,
            DiskRegionSemantic::PartitionTable => Self::Metadata,
            DiskRegionSemantic::PlainPartition => Self::Plain,
            DiskRegionSemantic::Unallocated => Self::Free,
            DiskRegionSemantic::Lce => Self::Lce,
            DiskRegionSemantic::Tail => Self::Tail,
            DiskRegionSemantic::TailMetadataMirror => Self::BackupMirror,
            DiskRegionSemantic::TailRestoreNode => Self::RestoreNode,
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

#[derive(Debug, Clone, Copy)]
pub struct DiskLayoutTailGroup<'a> {
    pub start_lba: u64,
    pub end_exclusive: u64,
    pub children: &'a [DiskLayoutSegment],
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

    pub fn canonical_from_known(
        total_sectors: u64,
        mut known: Vec<DiskLayoutSegment>,
    ) -> Result<Self, String> {
        if total_sectors == 0 {
            return if known.is_empty() {
                Ok(Self::new(0, Vec::new()))
            } else {
                Err("zero-sector disk cannot contain physical extents".into())
            };
        }

        known.sort_by_key(|segment| segment.start_lba);
        for segment in &known {
            if segment.kind == DiskRegionKind::Unknown {
                return Err(format!("{} has unknown physical ownership", segment.label));
            }
            if segment.sector_count == 0 {
                return Err(format!("{} has zero sectors", segment.label));
            }
            if segment.start_lba >= total_sectors || segment.end_exclusive()? > total_sectors {
                return Err(format!("{} exceeds physical disk", segment.label));
            }
        }
        for pair in known.windows(2) {
            if pair[0].end_exclusive()? > pair[1].start_lba {
                return Err(format!(
                    "disk layout overlap at LBA{} between {} and {}",
                    pair[1].start_lba, pair[0].label, pair[1].label
                ));
            }
        }

        let mut segments = Vec::with_capacity(known.len().saturating_mul(2).saturating_add(1));
        let mut cursor = 0u64;
        for segment in known {
            if cursor < segment.start_lba {
                segments.push(DiskLayoutSegment {
                    label: "空闲区域".into(),
                    start_lba: cursor,
                    sector_count: segment.start_lba - cursor,
                    kind: DiskRegionKind::Free,
                });
            }
            cursor = segment.end_exclusive()?;
            segments.push(segment);
        }
        if cursor < total_sectors {
            segments.push(DiskLayoutSegment {
                label: "空闲区域".into(),
                start_lba: cursor,
                sector_count: total_sectors - cursor,
                kind: DiskRegionKind::Free,
            });
        }

        let model = Self::new(total_sectors, segments);
        model.validate_complete()?;
        Ok(model)
    }

    pub fn canonical_edp(
        total_sectors: u64,
        mut partitions: Vec<DiskLayoutSegment>,
        lce_start_lba: u64,
        lce_sector_count: u64,
    ) -> Result<Self, String> {
        if total_sectors < 13 {
            return Err("EDP disk is shorter than LBA0-12 protocol area".into());
        }
        let mut known = Vec::with_capacity(partitions.len().saturating_add(4));
        known.push(DiskLayoutSegment {
            label: "EDP 主协议区".into(),
            start_lba: 0,
            sector_count: 13,
            kind: DiskRegionKind::Protocol,
        });
        known.append(&mut partitions);
        known.push(DiskLayoutSegment {
            label: "LCE".into(),
            start_lba: lce_start_lba,
            sector_count: lce_sector_count,
            kind: DiskRegionKind::Lce,
        });
        if total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS + TAIL_METADATA_MIRROR_SECTORS {
            known.push(DiskLayoutSegment {
                label: "历史备份镜像".into(),
                start_lba: total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS,
                sector_count: TAIL_METADATA_MIRROR_SECTORS,
                kind: DiskRegionKind::BackupMirror,
            });
        }
        if total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS {
            known.push(DiskLayoutSegment {
                label: "restore-node".into(),
                start_lba: total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS,
                sector_count: 1,
                kind: DiskRegionKind::RestoreNode,
            });
        }
        Self::canonical_from_known(total_sectors, known)
    }

    pub fn canonical_inspect_context(context: &InspectDiskContext) -> Result<Self, String> {
        if context.is_plain() {
            let table = context
                .partition_table
                .as_ref()
                .ok_or_else(|| "普通盘分区表尚未完整读取".to_string())?;
            return Self::canonical_plain(context.total_sectors, table);
        }

        let mode = context
            .provision_kind
            .and_then(|kind| kind.official_mode())
            .ok_or_else(|| "EDP 盘型尚未确认".to_string())?;
        if context.partitions.len() != mode.partition_types().len() {
            return Err("LBA12 分区数量与已确认模式不一致".into());
        }
        let mut partitions = Vec::with_capacity(context.partitions.len());
        for (index, partition) in context.partitions.iter().enumerate() {
            let partition_type = EdpPartitionType::from_raw(partition.partition_type)
                .ok_or_else(|| format!("LBA12 entry{index} 分区类型未知"))?;
            if partition_type != mode.partition_types()[index] {
                return Err(format!("LBA12 entry{index} 分区类型与已确认模式不一致"));
            }
            let role = crate::provision::official_partition_role(mode, index, partition_type);
            partitions.push(DiskLayoutSegment {
                label: role.label().into(),
                start_lba: partition.start_sector,
                sector_count: partition.sector_count,
                kind: DiskRegionKind::from_partition_role(role),
            });
        }
        let lce = context
            .lce
            .as_ref()
            .ok_or_else(|| "LBA7 LCE 几何尚未确认".to_string())?;
        Self::canonical_edp(
            context.total_sectors,
            partitions,
            lce.start_lba,
            lce.sector_count,
        )
    }

    pub fn canonical_plain(
        total_sectors: u64,
        table: &PartitionTableSnapshot,
    ) -> Result<Self, String> {
        let mut known = Vec::with_capacity(table.table_extents.len() + table.partitions.len());
        known.extend(table.table_extents.iter().map(|extent| DiskLayoutSegment {
            label: extent.label.clone(),
            start_lba: extent.start_lba,
            sector_count: extent.sector_count,
            kind: DiskRegionKind::Metadata,
        }));
        known.extend(table.partitions.iter().map(|partition| DiskLayoutSegment {
            label: partition.display_label(),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            kind: DiskRegionKind::Plain,
        }));
        Self::canonical_from_known(total_sectors, known)
    }

    pub fn canonical_plain_plan(
        total_sectors: u64,
        mut partitions: Vec<DiskLayoutSegment>,
    ) -> Result<Self, String> {
        let mut known = Vec::with_capacity(partitions.len().saturating_add(1));
        known.push(DiskLayoutSegment {
            label: "MBR".into(),
            start_lba: 0,
            sector_count: 1,
            kind: DiskRegionKind::Metadata,
        });
        known.append(&mut partitions);
        Self::canonical_from_known(total_sectors, known)
    }

    pub fn tail_group(&self) -> Option<DiskLayoutTailGroup<'_>> {
        let index = self
            .segments
            .iter()
            .position(|segment| segment.kind == DiskRegionKind::Lce)?;
        let children = &self.segments[index..];
        Some(DiskLayoutTailGroup {
            start_lba: children.first()?.start_lba,
            end_exclusive: self.total_sectors,
            children,
        })
    }

    pub fn collapsed_tail_model(&self) -> Self {
        let Some(tail) = self.tail_group() else {
            return self.clone();
        };
        let mut segments = self
            .segments
            .iter()
            .take_while(|segment| segment.start_lba < tail.start_lba)
            .cloned()
            .collect::<Vec<_>>();
        segments.push(DiskLayoutSegment {
            label: "尾部区域".into(),
            start_lba: tail.start_lba,
            sector_count: tail.end_exclusive - tail.start_lba,
            kind: DiskRegionKind::Tail,
        });
        Self::new(self.total_sectors, segments)
    }

    /// Legacy topology projection kept only for unit compatibility tests.
    /// Production layouts must use `canonical_*` constructors and fail closed.
    #[cfg(test)]
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

    /// Legacy topology projection kept only for unit compatibility tests.
    /// It is not a physical layout fact source.
    #[cfg(test)]
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
