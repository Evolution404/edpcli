//! 全盘 Inspect 的只读拓扑模型。
//!
//! 本模块只表达磁盘结构与 lazy children，不保存 TUI 展开/焦点等界面状态。
//! 大范围 sector 通过 `LazySectors` 延迟 materialize，避免按磁盘容量分配节点。

use super::inspect::{AbsoluteByteRange, InspectDecoderKind, InspectField};
use crate::backup_metadata::{
    DEVICE_TAIL_WINDOW_SECTORS, TAIL_END4_MIRROR_OFFSET_SECTORS,
    TAIL_METADATA_MIRROR_OFFSET_SECTORS, TAIL_METADATA_MIRROR_SECTORS,
};
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
                sector_stub(lba, self.decoder, self.status)
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

fn node_path_match<F>(
    node: &InspectNode,
    path: &mut Vec<String>,
    predicate: &F,
) -> Option<Vec<String>>
where
    F: Fn(&InspectNode) -> bool,
{
    path.push(node.id.clone());
    if predicate(node) {
        return Some(path.clone());
    }
    if let InspectChildren::Materialized(children) = &node.children {
        for child in children {
            if let Some(found) = node_path_match(child, path, predicate) {
                return Some(found);
            }
        }
    }
    path.pop();
    None
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
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }
        node_path_match(&self.root, &mut Vec::new(), &|node| {
            node.label.to_lowercase().contains(&query)
        })
    }
}

fn lazy_extent(
    id: impl Into<String>,
    label: impl Into<String>,
    start_lba: u64,
    sector_count: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    kind: InspectNodeKind,
) -> InspectNode {
    InspectNode {
        id: id.into(),
        label: label.into(),
        kind,
        range: InspectNodeRange::sectors(start_lba, sector_count),
        children: InspectChildren::LazySectors {
            start_lba,
            sector_count,
        },
        decoder,
        status,
    }
}

fn region_with_extent(
    id: impl Into<String>,
    label: impl Into<String>,
    start_lba: u64,
    sector_count: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    extent_kind: InspectNodeKind,
) -> InspectNode {
    let id = id.into();
    let extent = lazy_extent(
        format!("{id}.extent"),
        "扇区范围",
        start_lba,
        sector_count,
        decoder,
        status,
        extent_kind,
    );
    InspectNode {
        id,
        label: label.into(),
        kind: InspectNodeKind::Region,
        range: InspectNodeRange::sectors(start_lba, sector_count),
        children: InspectChildren::Materialized(vec![extent]),
        decoder,
        status,
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
    }
}

pub fn field_node(index: usize, field: &InspectField) -> InspectNode {
    InspectNode {
        id: format!("field.{}.{}", field.range.start, index),
        label: field.label.clone(),
        kind: InspectNodeKind::Field,
        range: InspectNodeRange::from_bytes(field.range),
        children: InspectChildren::None,
        decoder: None,
        status: SemanticStatus::Identified,
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

fn tail_region(total_sectors: u64) -> Option<InspectNode> {
    let tail_count = total_sectors.min(DEVICE_TAIL_WINDOW_SECTORS);
    if tail_count == 0 {
        return None;
    }
    let tail_start = total_sectors - tail_count;
    let mut children = vec![lazy_extent(
        "region.tail.extent",
        "盘尾取证窗口",
        tail_start,
        tail_count,
        None,
        SemanticStatus::Unknown,
        InspectNodeKind::Extent,
    )];

    if total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS + TAIL_METADATA_MIRROR_SECTORS {
        let start = total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS;
        children.push(lazy_extent(
            "region.tail.metadata_mirror",
            "盘尾历史 9 扇区镜像",
            start,
            TAIL_METADATA_MIRROR_SECTORS,
            None,
            SemanticStatus::Identified,
            InspectNodeKind::Extent,
        ));
    }
    if total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS {
        let start = total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS;
        children.push(lazy_extent(
            "region.tail.restore_node_end4",
            "盘尾 end-4 restore-node",
            start,
            1,
            None,
            SemanticStatus::Identified,
            InspectNodeKind::Extent,
        ));
    }

    Some(InspectNode {
        id: "region.tail".into(),
        label: "盘尾区域".into(),
        kind: InspectNodeKind::Region,
        range: InspectNodeRange::sectors(tail_start, tail_count),
        children: InspectChildren::Materialized(children),
        decoder: None,
        status: SemanticStatus::Unknown,
    })
}

pub fn build_inspect_topology(context: &InspectDiskContext) -> InspectTopology {
    let total = context.total_sectors;
    let mut regions = Vec::new();
    let mut claimed = Vec::new();

    if let Some((start, count)) = clip_range(0, 13, total) {
        regions.push(region_with_extent(
            "region.protocol",
            "EDP 主协议区 LBA0-12",
            start,
            count,
            Some(InspectDecoderKind::Protocol),
            SemanticStatus::Identified,
            InspectNodeKind::Extent,
        ));
        claimed.push((start, count));
    }

    if let Some(lce) = context.lce.as_ref() {
        if let Some((start, count)) = clip_range(lce.start_lba, lce.sector_count, total) {
            regions.push(region_with_extent(
                "region.lce",
                "LCE legacy compatibility extent",
                start,
                count,
                Some(InspectDecoderKind::Lce),
                SemanticStatus::Identified,
                InspectNodeKind::Extent,
            ));
            claimed.push((start, count));
        }
    }

    for partition in &context.partitions {
        if let Some((start, count)) =
            clip_range(partition.start_sector, partition.sector_count, total)
        {
            regions.push(region_with_extent(
                format!("region.partition.{}", partition.index),
                format!("分区[{}] type{}", partition.index, partition.partition_type),
                start,
                count,
                Some(InspectDecoderKind::Partition),
                SemanticStatus::Identified,
                InspectNodeKind::Partition,
            ));
            claimed.push((start, count));
        }
    }

    if let Some(tail) = tail_region(total) {
        claimed.push((tail.range.start_lba, tail.range.sector_count));
        regions.push(tail);
    }

    let claimed = merged_claimed_ranges(claimed, total);
    for (index, (start, count)) in unknown_gaps(&claimed, total).into_iter().enumerate() {
        regions.push(region_with_extent(
            format!("region.unknown.{index}"),
            format!("未知区域 LBA{start}..{}", start + count - 1),
            start,
            count,
            None,
            SemanticStatus::Unknown,
            InspectNodeKind::UnknownRange,
        ));
    }

    InspectTopology {
        root: InspectNode {
            id: "device".into(),
            label: "整盘".into(),
            kind: InspectNodeKind::Device,
            range: InspectNodeRange::sectors(0, total),
            children: InspectChildren::Materialized(regions),
            decoder: None,
            status: SemanticStatus::Identified,
        },
    }
}

pub fn find_sector_structured_path(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    query: &str,
) -> Option<Vec<String>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }
    let sector = sector_node_with_fields(lba, decoder, status, fields);
    node_path_match(&sector, &mut Vec::new(), &|node| {
        if node.label.to_lowercase().contains(&query) {
            return true;
        }
        node.range
            .byte_range
            .and_then(|range| fields.iter().find(|field| field.range == range))
            .is_some_and(|field| field.value.to_lowercase().contains(&query))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup_metadata::{Lba7CompatibilityGeometry, PartitionGeometry};
    use crate::common::{METADATA_IMAGE_LEN, SECTOR};
    use crate::inspect::{FieldChild, FieldStyle};

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
        InspectDiskContext {
            protocol_image: vec![0; METADATA_IMAGE_LEN],
            device_id: None,
            total_sectors: total,
            partitions: Vec::new(),
            lce: None,
            context_issues: Vec::new(),
        }
    }

    #[test]
    fn topology_covers_protocol_tail_and_unknown_complement() {
        let topology = build_inspect_topology(&context(5_000));
        assert_eq!(topology.root.range, InspectNodeRange::sectors(0, 5_000));
        assert_eq!(
            topology
                .primary_region_for_lba(0)
                .map(|node| node.id.as_str()),
            Some("region.protocol")
        );
        assert!(topology
            .primary_region_for_lba(100)
            .is_some_and(|node| node.id.starts_with("region.unknown.")));
        assert_eq!(
            topology
                .primary_region_for_lba(4_999)
                .map(|node| node.id.as_str()),
            Some("region.tail")
        );
    }

    #[test]
    fn topology_expresses_lce_partitions_and_unknown_gaps_without_eager_sectors() {
        let mut ctx = context(10_000);
        ctx.lce = Some(Lba7CompatibilityGeometry {
            start_lba: 1_500,
            sector_count: 6,
            lba7_pointer_entries: Vec::new(),
            official_partition_mode: None,
            chs_expected_start_lba: None,
        });
        ctx.partitions.push(partition(0, 2_048, 2_000, 2));
        let topology = build_inspect_topology(&ctx);

        assert_eq!(
            topology
                .primary_region_for_lba(1_502)
                .map(|node| node.id.as_str()),
            Some("region.lce")
        );
        assert_eq!(
            topology
                .primary_region_for_lba(2_100)
                .map(|node| node.id.as_str()),
            Some("region.partition.0")
        );

        let partition_region = topology
            .regions_for_lba(2_100)
            .into_iter()
            .find(|node| node.id == "region.partition.0")
            .unwrap();
        let InspectChildren::Materialized(extents) = &partition_region.children else {
            panic!("partition region must own one extent");
        };
        let page = extents[0].materialize_sector_page(1_000, 3);
        assert_eq!(
            page.iter()
                .map(|node| node.range.start_lba)
                .collect::<Vec<_>>(),
            vec![3_048, 3_049, 3_050]
        );
        assert!(page.iter().all(|node| node.kind == InspectNodeKind::Sector));
    }

    #[test]
    fn tail_mirror_and_end4_are_nested_under_tail_region() {
        let topology = build_inspect_topology(&context(10_000));
        let tail = topology.primary_region_for_lba(9_999).unwrap();
        assert_eq!(tail.id, "region.tail");
        let InspectChildren::Materialized(children) = &tail.children else {
            panic!("tail must have extents");
        };
        assert!(children
            .iter()
            .any(|node| node.id == "region.tail.metadata_mirror"));
        assert!(children
            .iter()
            .any(|node| node.id == "region.tail.restore_node_end4"));
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
            range: AbsoluteByteRange {
                start: 100 * SECTOR as u64 + 0x1f0,
                end_exclusive: 101 * SECTOR as u64 + 0x30,
            },
            field_type: super::super::inspect::InspectFieldType::Identity,
            raw: vec![0; 64],
            decoded: vec![0; 64],
            status: super::super::inspect::InspectFieldStatus::Known,
            label: "跨扇区字段".into(),
            value: "value".into(),
            style: FieldStyle::Identity,
            group: Some("group".into()),
            children: vec![FieldChild {
                label: "child".into(),
                value: "value".into(),
            }],
        };
        let node = field_node(0, &field);
        assert_eq!(node.kind, InspectNodeKind::Field);
        assert_eq!(node.range.start_lba, 100);
        assert_eq!(node.range.sector_count, 2);
        assert!(node.range.byte_range.unwrap().spans_sectors());
    }
}
