use super::*;

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

pub(super) fn sector_stub(
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
