use super::*;

pub(super) fn lazy_extent(
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

pub(super) fn region_with_extent(
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
