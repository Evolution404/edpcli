//! 全盘 Inspect 的只读拓扑模型。
//!
//! 本模块只表达磁盘结构与 lazy children，不保存 TUI 展开/焦点等界面状态。
//! 大范围 sector 通过 `LazySectors` 延迟 materialize，避免按磁盘容量分配节点。

use super::inspect::{AbsoluteByteRange, InspectDecoderKind, InspectField, InspectFieldKey};
use crate::edpb::SemanticStatus;
use crate::inspect_target::InspectDiskContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectNodeKind {
    Device,
    Region,
    Extent,
    Sector,
    Structure,
    Group,
    Field,
    Partition,
    UnknownRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskRegionSemantic {
    Protocol,
    PartitionTable,
    PlainPartition,
    Unallocated,
    Lce,
    Tail,
    TailMetadataMirror,
    TailRestoreNode,
    Partition { partition_type: u32 },
    MbrPartition { partition_type: u8 },
    Unknown,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectNodeRange {
    pub start_lba: u64,
    pub sector_count: u64,
    pub byte_range: Option<AbsoluteByteRange>,
}

impl InspectNodeRange {
    pub fn sectors(start_lba: u64, sector_count: u64) -> Self {
        Self {
            start_lba,
            sector_count,
            byte_range: None,
        }
    }

    pub fn from_bytes(range: AbsoluteByteRange) -> Self {
        let start_lba = range.start_lba();
        let sector_count = if range.end_exclusive == range.start {
            0
        } else {
            range.end_lba().saturating_sub(start_lba).saturating_add(1)
        };
        Self {
            start_lba,
            sector_count,
            byte_range: Some(range),
        }
    }

    pub fn end_lba_exclusive(self) -> u64 {
        self.start_lba.saturating_add(self.sector_count)
    }

    pub fn contains_lba(self, lba: u64) -> bool {
        lba >= self.start_lba && lba < self.end_lba_exclusive()
    }
}

/// Render a non-empty half-open LBA extent as a single closed UI range.
/// Invalid or empty extents have no visible closed-range representation.
pub fn format_lba_closed_range(start: u64, end_exclusive: u64) -> Option<String> {
    (end_exclusive > start).then(|| format!("[{start}..{}]", end_exclusive - 1))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectChildren {
    None,
    Materialized(Vec<InspectNode>),
    LazySectors { start_lba: u64, sector_count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectNode {
    pub id: String,
    pub label: String,
    pub kind: InspectNodeKind,
    pub range: InspectNodeRange,
    pub children: InspectChildren,
    pub decoder: Option<InspectDecoderKind>,
    pub status: SemanticStatus,
    pub region_semantic: Option<DiskRegionSemantic>,
}

impl InspectNode {
    pub fn materialize_sector_page(&self, offset: u64, limit: usize) -> Vec<InspectNode> {
        let InspectChildren::LazySectors {
            start_lba,
            sector_count,
        } = self.children
        else {
            return Vec::new();
        };
        if offset >= sector_count || limit == 0 {
            return Vec::new();
        }
        let count = (sector_count - offset).min(limit as u64);
        (0..count)
            .map(|index| {
                let lba = start_lba + offset + index;
                let mut node = sector_stub(lba, self.decoder, self.status);
                node.region_semantic = self.region_semantic;
                node
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectLazySectorLocation {
    pub node_path: Vec<String>,
    pub start_lba: u64,
    pub sector_count: u64,
}

fn node_paths_match<F>(
    node: &InspectNode,
    path: &mut Vec<String>,
    predicate: &F,
    out: &mut Vec<Vec<String>>,
) where
    F: Fn(&InspectNode) -> bool,
{
    path.push(node.id.clone());
    if predicate(node) {
        out.push(path.clone());
    }
    if let InspectChildren::Materialized(children) = &node.children {
        for child in children {
            node_paths_match(child, path, predicate, out);
        }
    }
    path.pop();
}

fn lazy_sector_location(
    node: &InspectNode,
    lba: u64,
    path: &mut Vec<String>,
) -> Option<InspectLazySectorLocation> {
    if !node.range.contains_lba(lba) {
        return None;
    }
    path.push(node.id.clone());
    match &node.children {
        InspectChildren::LazySectors {
            start_lba,
            sector_count,
        } if lba >= *start_lba && lba < start_lba.saturating_add(*sector_count) => {
            Some(InspectLazySectorLocation {
                node_path: path.clone(),
                start_lba: *start_lba,
                sector_count: *sector_count,
            })
        }
        InspectChildren::Materialized(children) => {
            for child in children {
                if let Some(found) = lazy_sector_location(child, lba, path) {
                    return Some(found);
                }
            }
            path.pop();
            None
        }
        _ => {
            path.pop();
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTopology {
    pub root: InspectNode,
}

impl InspectTopology {
    pub fn regions_for_lba(&self, lba: u64) -> Vec<&InspectNode> {
        let InspectChildren::Materialized(children) = &self.root.children else {
            return Vec::new();
        };
        children
            .iter()
            .filter(|node| node.range.contains_lba(lba))
            .collect()
    }

    pub fn primary_region_for_lba(&self, lba: u64) -> Option<&InspectNode> {
        self.regions_for_lba(lba).into_iter().next()
    }

    pub fn lazy_sector_location(&self, lba: u64) -> Option<InspectLazySectorLocation> {
        lazy_sector_location(&self.root, lba, &mut Vec::new())
    }

    pub fn find_label_path(&self, query: &str) -> Option<Vec<String>> {
        self.find_label_paths(query).into_iter().next()
    }

    pub fn find_label_paths(&self, query: &str) -> Vec<Vec<String>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        node_paths_match(
            &self.root,
            &mut Vec::new(),
            &|node| node.label.to_lowercase().contains(&query),
            &mut out,
        );
        out
    }
}

fn lazy_extent(
    id: impl Into<String>,
    label: impl Into<String>,
    start_lba: u64,
    sector_count: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    region_semantic: Option<DiskRegionSemantic>,
) -> InspectNode {
    InspectNode {
        id: id.into(),
        label: label.into(),
        kind: match region_semantic {
            Some(DiskRegionSemantic::Unknown) => InspectNodeKind::UnknownRange,
            Some(
                DiskRegionSemantic::Partition { .. } | DiskRegionSemantic::MbrPartition { .. },
            ) => InspectNodeKind::Partition,
            _ => InspectNodeKind::Extent,
        },
        range: InspectNodeRange::sectors(start_lba, sector_count),
        children: InspectChildren::LazySectors {
            start_lba,
            sector_count,
        },
        decoder,
        status,
        region_semantic,
    }
}

fn region_with_extent(
    id: impl Into<String>,
    label: impl Into<String>,
    start_lba: u64,
    sector_count: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    region_semantic: DiskRegionSemantic,
) -> InspectNode {
    let id = id.into();
    let extent = lazy_extent(
        format!("{id}.extent"),
        "扇区范围",
        start_lba,
        sector_count,
        decoder,
        status,
        Some(region_semantic),
    );
    InspectNode {
        id,
        label: label.into(),
        kind: if region_semantic == DiskRegionSemantic::Unknown {
            InspectNodeKind::UnknownRange
        } else {
            InspectNodeKind::Region
        },
        range: InspectNodeRange::sectors(start_lba, sector_count),
        children: InspectChildren::Materialized(vec![extent]),
        decoder,
        status,
        region_semantic: Some(region_semantic),
    }
}

fn sector_stub(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
) -> InspectNode {
    let children = if lba == 0 && decoder == Some(InspectDecoderKind::Protocol) {
        let partition_table = AbsoluteByteRange {
            start: 0x1be,
            end_exclusive: 0x1fe,
        };
        InspectChildren::Materialized(vec![InspectNode {
            id: "structure.mbr.partition_table".into(),
            label: "MBR 分区表".into(),
            kind: InspectNodeKind::Structure,
            range: InspectNodeRange::from_bytes(partition_table),
            children: InspectChildren::None,
            decoder,
            status: SemanticStatus::Identified,
            region_semantic: None,
        }])
    } else {
        InspectChildren::None
    };
    InspectNode {
        id: format!("sector.{lba}"),
        label: format!("LBA{lba}"),
        kind: InspectNodeKind::Sector,
        range: InspectNodeRange::sectors(lba, 1),
        children,
        decoder,
        status,
        region_semantic: None,
    }
}

pub fn field_node(index: usize, field: &InspectField) -> InspectNode {
    let is_elabel = field.key == InspectFieldKey::Lba8Elabel;
    let children = if is_elabel {
        field
            .children
            .iter()
            .enumerate()
            .map(|(child_index, child)| {
                let label = match child.label.as_str() {
                    "Dept" => "部门",
                    "User" => "用户",
                    other => other,
                };
                InspectNode {
                    id: format!("child.{child_index}"),
                    label: format!("{label}  {}", child.value),
                    kind: InspectNodeKind::Field,
                    range: InspectNodeRange::from_bytes(field.range),
                    children: InspectChildren::None,
                    decoder: None,
                    status: SemanticStatus::Identified,
                    region_semantic: None,
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    InspectNode {
        id: format!("field.{}.{}", field.range.start, index),
        label: if is_elabel {
            format!("E_LABEL [{}]", field.children.len())
        } else {
            field.label.clone()
        },
        kind: InspectNodeKind::Field,
        range: InspectNodeRange::from_bytes(field.range),
        children: if children.is_empty() {
            InspectChildren::None
        } else {
            InspectChildren::Materialized(children)
        },
        decoder: None,
        status: SemanticStatus::Identified,
        region_semantic: None,
    }
}

pub fn sector_node_with_fields(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
) -> InspectNode {
    let mut node = sector_stub(lba, decoder, status);
    let mut children = match node.children {
        InspectChildren::Materialized(children) => children,
        InspectChildren::None | InspectChildren::LazySectors { .. } => Vec::new(),
    };
    children.extend(
        fields
            .iter()
            .enumerate()
            .map(|(index, field)| field_node(index, field)),
    );
    node.children = if children.is_empty() {
        InspectChildren::None
    } else {
        InspectChildren::Materialized(children)
    };
    node
}

fn clip_range(start: u64, count: u64, total: u64) -> Option<(u64, u64)> {
    if start >= total || count == 0 {
        return None;
    }
    let end = start.saturating_add(count).min(total);
    (end > start).then_some((start, end - start))
}

fn merged_claimed_ranges(mut ranges: Vec<(u64, u64)>, total: u64) -> Vec<(u64, u64)> {
    ranges.retain(|(start, count)| *start < total && *count > 0);
    for (start, count) in &mut ranges {
        *count = start
            .saturating_add(*count)
            .min(total)
            .saturating_sub(*start);
    }
    ranges.sort_unstable_by_key(|(start, _)| *start);
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (start, count) in ranges {
        let end = start + count;
        match merged.last_mut() {
            Some((last_start, last_count)) if start <= last_start.saturating_add(*last_count) => {
                let last_end = last_start.saturating_add(*last_count);
                *last_count = last_end.max(end) - *last_start;
            }
            _ => merged.push((start, count)),
        }
    }
    merged
}

fn unknown_gaps(claimed: &[(u64, u64)], total: u64) -> Vec<(u64, u64)> {
    let mut cursor = 0u64;
    let mut out = Vec::new();
    for &(start, count) in claimed {
        if cursor < start {
            out.push((cursor, start - cursor));
        }
        cursor = cursor.max(start.saturating_add(count));
    }
    if cursor < total {
        out.push((cursor, total - cursor));
    }
    out
}

pub fn build_inspect_topology(context: &InspectDiskContext) -> InspectTopology {
    let total = context.total_sectors;
    let root = |regions: Vec<InspectNode>| InspectTopology {
        root: InspectNode {
            id: "device".into(),
            label: "整盘".into(),
            kind: InspectNodeKind::Device,
            range: InspectNodeRange::sectors(0, total),
            children: InspectChildren::Materialized(regions),
            decoder: None,
            status: SemanticStatus::Identified,
            region_semantic: None,
        },
    };

    if context.is_plain() {
        let Some(table) = &context.partition_table else {
            return root(vec![region_with_extent(
                "region.unknown.0",
                "分区布局不可用",
                0,
                total,
                None,
                SemanticStatus::Unknown,
                DiskRegionSemantic::Unknown,
            )]);
        };

        let mut regions = Vec::new();
        let mut claimed = Vec::new();
        for (index, extent) in table.table_extents.iter().enumerate() {
            if let Some((start, count)) = clip_range(extent.start_lba, extent.sector_count, total) {
                regions.push(region_with_extent(
                    format!("region.partition_table.{index}"),
                    extent.label.clone(),
                    start,
                    count,
                    None,
                    SemanticStatus::Identified,
                    DiskRegionSemantic::PartitionTable,
                ));
                claimed.push((start, count));
            }
        }
        for partition in &table.partitions {
            if let Some((start, count)) =
                clip_range(partition.start_lba, partition.sector_count, total)
            {
                regions.push(region_with_extent(
                    format!("region.plain_partition.{}", partition.index),
                    partition.display_label(),
                    start,
                    count,
                    Some(InspectDecoderKind::Partition),
                    SemanticStatus::Identified,
                    DiskRegionSemantic::PlainPartition,
                ));
                claimed.push((start, count));
            }
        }
        let claimed = merged_claimed_ranges(claimed, total);
        for (index, (start, count)) in unknown_gaps(&claimed, total).into_iter().enumerate() {
            regions.push(region_with_extent(
                format!("region.unallocated.{index}"),
                "空闲区域",
                start,
                count,
                None,
                SemanticStatus::Identified,
                DiskRegionSemantic::Unallocated,
            ));
        }
        regions.sort_by_key(|region| region.range.start_lba);
        return root(regions);
    }

    let Ok(model) =
        crate::application::disk_layout::DiskLayoutModel::canonical_inspect_context(context)
    else {
        return root(vec![region_with_extent(
            "region.unknown.0",
            "布局证据不足",
            0,
            total,
            None,
            SemanticStatus::Unknown,
            DiskRegionSemantic::Unknown,
        )]);
    };

    fn properties(
        context: &InspectDiskContext,
        segment: &crate::application::disk_layout::DiskLayoutSegment,
    ) -> (Option<InspectDecoderKind>, DiskRegionSemantic) {
        use crate::application::disk_layout::DiskRegionKind;
        match segment.kind {
            DiskRegionKind::Protocol => (
                Some(InspectDecoderKind::Protocol),
                DiskRegionSemantic::Protocol,
            ),
            DiskRegionKind::Free => (None, DiskRegionSemantic::Unallocated),
            DiskRegionKind::Lce => (Some(InspectDecoderKind::Lce), DiskRegionSemantic::Lce),
            DiskRegionKind::BackupMirror => (None, DiskRegionSemantic::TailMetadataMirror),
            DiskRegionKind::RestoreNode => (None, DiskRegionSemantic::TailRestoreNode),
            DiskRegionKind::Boot
            | DiskRegionKind::Share
            | DiskRegionKind::Combined
            | DiskRegionKind::Encrypt
            | DiskRegionKind::Compatibility => {
                let partition_type = context
                    .partitions
                    .iter()
                    .find(|partition| {
                        partition.start_sector == segment.start_lba
                            && partition.sector_count == segment.sector_count
                    })
                    .map_or(0, |partition| partition.partition_type);
                (
                    Some(InspectDecoderKind::Partition),
                    DiskRegionSemantic::Partition { partition_type },
                )
            }
            _ => (None, DiskRegionSemantic::Unknown),
        }
    }

    let mut regions = Vec::new();
    let mut free_index = 0usize;
    let tail_start = model.tail_group().map(|tail| tail.start_lba);
    for segment in &model.segments {
        if tail_start.is_some_and(|start| segment.start_lba >= start) {
            break;
        }
        let (decoder, semantic) = properties(context, segment);
        let id = match segment.kind {
            crate::application::disk_layout::DiskRegionKind::Protocol => "region.protocol".into(),
            crate::application::disk_layout::DiskRegionKind::Free => {
                let id = format!("region.unallocated.{free_index}");
                free_index += 1;
                id
            }
            crate::application::disk_layout::DiskRegionKind::Boot
            | crate::application::disk_layout::DiskRegionKind::Share
            | crate::application::disk_layout::DiskRegionKind::Combined
            | crate::application::disk_layout::DiskRegionKind::Encrypt
            | crate::application::disk_layout::DiskRegionKind::Compatibility => {
                let index = context
                    .partitions
                    .iter()
                    .position(|partition| {
                        partition.start_sector == segment.start_lba
                            && partition.sector_count == segment.sector_count
                    })
                    .expect("canonical partition must come from inspect context");
                format!("region.partition.{index}")
            }
            _ => format!("region.extent.{}", segment.start_lba),
        };
        regions.push(region_with_extent(
            id,
            segment.label.clone(),
            segment.start_lba,
            segment.sector_count,
            decoder,
            SemanticStatus::Identified,
            semantic,
        ));
    }

    if let Some(tail) = model.tail_group() {
        let children = tail
            .children
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                use crate::application::disk_layout::DiskRegionKind;
                let (decoder, semantic) = properties(context, segment);
                let id = match segment.kind {
                    DiskRegionKind::Lce => "region.lce".into(),
                    DiskRegionKind::BackupMirror => "region.tail.metadata_mirror".into(),
                    DiskRegionKind::RestoreNode => "region.tail.restore_node_end4".into(),
                    DiskRegionKind::Free => format!("region.tail.free.{index}"),
                    _ => format!("region.tail.extent.{index}"),
                };
                lazy_extent(
                    id,
                    segment.label.clone(),
                    segment.start_lba,
                    segment.sector_count,
                    decoder,
                    SemanticStatus::Identified,
                    Some(semantic),
                )
            })
            .collect();
        regions.push(InspectNode {
            id: "region.tail".into(),
            label: "尾部区域".into(),
            kind: InspectNodeKind::Region,
            range: InspectNodeRange::sectors(
                tail.start_lba,
                tail.end_exclusive.saturating_sub(tail.start_lba),
            ),
            children: InspectChildren::Materialized(children),
            decoder: None,
            status: SemanticStatus::Identified,
            region_semantic: Some(DiskRegionSemantic::Tail),
        });
    }

    regions.sort_by_key(|region| region.range.start_lba);
    root(regions)
}

pub fn find_sector_structured_path(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Option<Vec<String>> {
    find_sector_structured_paths(lba, decoder, status, fields, query)
        .into_iter()
        .next()
}

pub fn find_sector_structured_paths(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Vec<Vec<String>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let sector = sector_node_with_fields(lba, decoder, status, fields);
    let mut out = Vec::new();
    node_paths_match(
        &sector,
        &mut Vec::new(),
        &|node| {
            if node.label.to_lowercase().contains(&query) {
                return true;
            }
            node.range
                .byte_range
                .and_then(|range| fields.iter().find(|field| field.range == range))
                .is_some_and(|field| field.value.to_lowercase().contains(&query))
        },
        &mut out,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::partition_table::{
        PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
        PhysicalPartition,
    };
    use crate::backup_metadata::{Lba7CompatibilityGeometry, PartitionGeometry};
    use crate::common::{METADATA_IMAGE_LEN, SECTOR};
    use crate::inspect_adapter::{FieldChild, FieldStyle};

    fn partition(index: usize, start: u64, count: u64, partition_type: u32) -> PartitionGeometry {
        PartitionGeometry {
            index,
            partition_type,
            partition_count: 1,
            need_disturb: 0,
            need_encrypt: 0,
            start_sector: start,
            sector_size: SECTOR as u64,
            partition_size: count * SECTOR as u64,
            sector_count: count,
            user_key_crc: 0,
            file_key_crc: 0,
            encrypt_mode: 0,
        }
    }

    fn context(total: u64) -> InspectDiskContext {
        let lce_start = total.saturating_sub(2_000);
        InspectDiskContext {
            protocol_image: vec![0; METADATA_IMAGE_LEN],
            device_id: Some("disk&ven_test&prod_test".into()),
            total_sectors: total,
            provision_kind: Some(crate::provision::DiskProvisionKind::Mode0),
            partition_table: None,
            partitions: vec![
                partition(0, 63, 37, 1),
                partition(1, 100, 900, 2),
                partition(2, 1_000, 1_000, 4),
            ],
            lce: Some(Lba7CompatibilityGeometry {
                start_lba: lce_start,
                sector_count: 6,
                lba7_pointer_entries: Vec::new(),
                official_partition_mode: None,
                chs_expected_start_lba: None,
            }),
            context_issues: Vec::new(),
        }
    }

    #[test]
    fn topology_uses_canonical_protocol_free_partitions_and_tail() {
        let topology = build_inspect_topology(&context(5_000));
        assert_eq!(topology.root.range, InspectNodeRange::sectors(0, 5_000));
        assert_eq!(
            topology
                .primary_region_for_lba(0)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Protocol)
        );
        assert_eq!(
            topology
                .primary_region_for_lba(30)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Unallocated)
        );
        assert_eq!(
            topology
                .primary_region_for_lba(150)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Partition { partition_type: 2 })
        );
        assert_eq!(
            topology
                .primary_region_for_lba(4_999)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Tail)
        );
    }

    #[test]
    fn tail_starts_at_lce_and_children_are_ordered_disjoint_and_complete() {
        let total = 15_728_640;
        let ctx = context(total);
        let lce_start = ctx.lce.as_ref().unwrap().start_lba;
        let topology = build_inspect_topology(&ctx);
        let tail = topology
            .primary_region_for_lba(total - 1)
            .expect("tail region");
        assert_eq!(tail.id, "region.tail");
        assert_eq!(tail.range.start_lba, lce_start);
        let InspectChildren::Materialized(children) = &tail.children else {
            panic!("tail extents must be materialized");
        };
        assert_eq!(
            children.first().and_then(|child| child.region_semantic),
            Some(DiskRegionSemantic::Lce)
        );
        assert_eq!(children.first().unwrap().range.sector_count, 6);
        assert!(children.iter().any(|child| {
            child.region_semantic == Some(DiskRegionSemantic::TailMetadataMirror)
                && child.range.start_lba == total - 1024
                && child.range.sector_count == 9
        }));
        assert!(children.iter().any(|child| {
            child.region_semantic == Some(DiskRegionSemantic::TailRestoreNode)
                && child.range.start_lba == total - 4
                && child.range.sector_count == 1
        }));
        assert!(children
            .windows(2)
            .all(|pair| pair[0].range.end_lba_exclusive() == pair[1].range.start_lba));
        assert_eq!(
            children.first().unwrap().range.start_lba,
            tail.range.start_lba
        );
        assert_eq!(children.last().unwrap().range.end_lba_exclusive(), total);
    }

    #[test]
    fn topology_partition_is_lazy_and_free_gaps_are_explicit() {
        let topology = build_inspect_topology(&context(10_000));
        let partition_region = topology
            .regions_for_lba(150)
            .into_iter()
            .find(|node| {
                matches!(
                    node.region_semantic,
                    Some(DiskRegionSemantic::Partition { partition_type: 2 })
                )
            })
            .unwrap();
        let InspectChildren::Materialized(extents) = &partition_region.children else {
            panic!("partition region must own one extent");
        };
        let page = extents[0].materialize_sector_page(10, 3);
        assert_eq!(
            page.iter()
                .map(|node| node.range.start_lba)
                .collect::<Vec<_>>(),
            vec![110, 111, 112]
        );
        assert_eq!(
            topology
                .primary_region_for_lba(2_500)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Unallocated)
        );
    }

    #[test]
    fn region_semantics_do_not_depend_on_display_labels() {
        let mut topology = build_inspect_topology(&context(10_000));
        let InspectChildren::Materialized(regions) = &mut topology.root.children else {
            panic!("root regions must be materialized");
        };
        let protocol = regions
            .iter_mut()
            .find(|node| node.region_semantic == Some(DiskRegionSemantic::Protocol))
            .unwrap();
        protocol.label = "renamed".into();
        assert_eq!(protocol.region_semantic, Some(DiskRegionSemantic::Protocol));
        assert!(regions.iter().any(|node| {
            node.region_semantic == Some(DiskRegionSemantic::Partition { partition_type: 2 })
        }));
        assert!(regions
            .iter()
            .any(|node| node.region_semantic == Some(DiskRegionSemantic::Tail)));
    }

    #[test]
    fn root_regions_follow_physical_lba_order_without_unknown_gaps() {
        let topology = build_inspect_topology(&context(10_000));
        let InspectChildren::Materialized(regions) = topology.root.children else {
            panic!("root regions must be materialized");
        };
        assert!(regions
            .windows(2)
            .all(|pair| pair[0].range.end_lba_exclusive() == pair[1].range.start_lba));
        assert!(regions
            .iter()
            .all(|region| region.region_semantic != Some(DiskRegionSemantic::Unknown)));
        assert_eq!(regions.first().unwrap().range.start_lba, 0);
        assert_eq!(regions.last().unwrap().range.end_lba_exclusive(), 10_000);
    }

    #[test]
    fn ui_lba_ranges_are_closed_but_model_ranges_remain_half_open() {
        assert_eq!(format_lba_closed_range(12, 13).as_deref(), Some("[12..12]"));
        assert_eq!(format_lba_closed_range(13, 63).as_deref(), Some("[13..62]"));
        assert_eq!(format_lba_closed_range(0, 0), None);
        assert_eq!(format_lba_closed_range(8, 7), None);
        assert_eq!(
            format_lba_closed_range(u64::MAX - 1, u64::MAX).as_deref(),
            Some("[18446744073709551614..18446744073709551614]")
        );
        assert_eq!(InspectNodeRange::sectors(13, 50).end_lba_exclusive(), 63);
    }

    #[test]
    fn overlapping_known_evidence_fails_closed_instead_of_creating_overlap() {
        let mut ctx = context(10_000);
        ctx.lce.as_mut().unwrap().start_lba = 1_500;
        let topology = build_inspect_topology(&ctx);
        let InspectChildren::Materialized(regions) = topology.root.children else {
            panic!("root regions must be materialized");
        };
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].label, "布局证据不足");
        assert_eq!(
            regions[0].region_semantic,
            Some(DiskRegionSemantic::Unknown)
        );
    }

    #[test]
    fn plain_partition_table_uses_metadata_partition_and_free_segments() {
        let mut ctx = context(20_000);
        ctx.provision_kind = Some(crate::provision::DiskProvisionKind::Plain);
        ctx.device_id = None;
        ctx.partitions.clear();
        ctx.lce = None;
        ctx.partition_table = Some(PartitionTableSnapshot {
            kind: PartitionTableKind::Mbr,
            partitions: vec![PhysicalPartition {
                index: 1,
                start_lba: 2_048,
                sector_count: 10_000,
                source: PartitionSource::Mbr {
                    partition_type: 0x07,
                    primary_slot: Some(1),
                },
                filesystem: Some("exFAT".into()),
            }],
            table_extents: vec![PartitionTableExtent {
                label: "MBR".into(),
                start_lba: 0,
                sector_count: 1,
            }],
            issues: Vec::new(),
        });
        let topology = build_inspect_topology(&ctx);
        assert_eq!(
            topology
                .primary_region_for_lba(0)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::PartitionTable)
        );
        assert_eq!(
            topology
                .primary_region_for_lba(1_000)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::Unallocated)
        );
        assert_eq!(
            topology
                .primary_region_for_lba(3_000)
                .and_then(|node| node.region_semantic),
            Some(DiskRegionSemantic::PlainPartition)
        );
    }

    #[test]
    fn tail_mirror_and_end4_are_nested_under_lce_anchored_tail_region() {
        let topology = build_inspect_topology(&context(10_000));
        let tail = topology.primary_region_for_lba(9_999).unwrap();
        assert_eq!(tail.id, "region.tail");
        let InspectChildren::Materialized(children) = &tail.children else {
            panic!("tail must have extents");
        };
        assert!(children
            .iter()
            .any(|node| node.region_semantic == Some(DiskRegionSemantic::TailMetadataMirror)));
        assert!(children
            .iter()
            .any(|node| node.region_semantic == Some(DiskRegionSemantic::TailRestoreNode)));
    }

    #[test]
    fn protocol_lba0_sector_stub_exposes_partition_table_structure() {
        let topology = build_inspect_topology(&context(5_000));
        let protocol = topology.primary_region_for_lba(0).unwrap();
        let InspectChildren::Materialized(extents) = &protocol.children else {
            panic!("protocol region must own extent");
        };
        let sector = extents[0].materialize_sector_page(0, 1).remove(0);
        let InspectChildren::Materialized(children) = sector.children else {
            panic!("LBA0 must expose structure");
        };
        let table = &children[0];
        assert_eq!(table.kind, InspectNodeKind::Structure);
        assert_eq!(table.range.byte_range.unwrap().start, 0x1be);
        assert_eq!(table.range.byte_range.unwrap().end_exclusive, 0x1fe);
    }

    #[test]
    fn field_nodes_keep_cross_sector_absolute_ranges() {
        let field = InspectField {
            key: super::super::inspect::InspectFieldKey::Synthetic,
            range: AbsoluteByteRange {
                start: 100 * SECTOR as u64 + 0x1f0,
                end_exclusive: 101 * SECTOR as u64 + 0x30,
            },
            field_type: super::super::inspect::InspectFieldType::Identity,
            raw: vec![0; 64],
            decoded: vec![0; 64],
            field_logical: None,
            transform: None,
            status: super::super::inspect::InspectFieldStatus::Known,
            label: "跨扇区字段".into(),
            value: "value".into(),
            style: FieldStyle::Identity,
            group: Some("group".into()),
            children: vec![FieldChild {
                label: "child".into(),
                value: "value".into(),
                relative_range: None,
            }],
        };
        let node = field_node(0, &field);
        assert_eq!(node.kind, InspectNodeKind::Field);
        assert_eq!(node.range.start_lba, 100);
        assert_eq!(node.range.sector_count, 2);
        assert!(node.range.byte_range.unwrap().spans_sectors());
    }
}
