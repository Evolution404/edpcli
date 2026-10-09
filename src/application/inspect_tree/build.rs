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

pub(super) fn region_with_lazy_sectors(
    id: impl Into<String>,
    label: impl Into<String>,
    start_lba: u64,
    sector_count: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    region_semantic: DiskRegionSemantic,
) -> InspectNode {
    InspectNode {
        id: id.into(),
        label: label.into(),
        kind: if region_semantic == DiskRegionSemantic::Unknown {
            InspectNodeKind::UnknownRange
        } else {
            InspectNodeKind::Region
        },
        range: InspectNodeRange::sectors(start_lba, sector_count),
        children: InspectChildren::LazySectors {
            start_lba,
            sector_count,
        },
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
    field_node_with_sector_bytes(index, field, crate::common::SECTOR as u32)
}

pub fn field_node_with_sector_bytes(
    index: usize,
    field: &InspectField,
    native_bytes: u32,
) -> InspectNode {
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
                    range: InspectNodeRange::from_bytes_with_sector_bytes(
                        field.range,
                        native_bytes,
                    ),
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
        range: InspectNodeRange::from_bytes_with_sector_bytes(field.range, native_bytes),
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

/// Build a standalone sector tree when there is no canonical topology node to enrich.
///
/// Topology consumers must use enrich_sector_node so region identity/semantics survive
/// cached decode hydration.
pub fn standalone_sector_node_with_fields(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
) -> InspectNode {
    standalone_sector_node_with_fields_and_sector_bytes(
        lba,
        decoder,
        status,
        fields,
        crate::common::SECTOR as u32,
    )
}

pub fn standalone_sector_node_with_fields_and_sector_bytes(
    lba: u64,
    decoder: Option<InspectDecoderKind>,
    status: SemanticStatus,
    fields: &[InspectField],
    native_bytes: u32,
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
            .map(|(index, field)| field_node_with_sector_bytes(index, field, native_bytes)),
    );
    node.children = if children.is_empty() {
        InspectChildren::None
    } else {
        InspectChildren::Materialized(children)
    };
    node
}

/// Attach decoded fields to an existing topology sector without rebuilding its identity.
///
/// The topology owns id/label/kind/range/decoder/status/region_semantic. Decode hydration may
/// only replace Field children; structural children such as the LBA0 MBR table are preserved.
pub fn enrich_sector_node(node: InspectNode, fields: &[InspectField]) -> InspectNode {
    enrich_sector_node_with_sector_bytes(node, fields, crate::common::SECTOR as u32)
}

pub fn enrich_sector_node_with_sector_bytes(
    mut node: InspectNode,
    fields: &[InspectField],
    native_bytes: u32,
) -> InspectNode {
    if node.kind != InspectNodeKind::Sector {
        return node;
    }

    let existing = std::mem::replace(&mut node.children, InspectChildren::None);
    let mut children = match existing {
        InspectChildren::Materialized(children) => children
            .into_iter()
            .filter(|child| child.kind != InspectNodeKind::Field)
            .collect::<Vec<_>>(),
        InspectChildren::None => Vec::new(),
        lazy @ InspectChildren::LazySectors { .. } => {
            node.children = lazy;
            return node;
        }
    };
    children.extend(
        fields
            .iter()
            .enumerate()
            .map(|(index, field)| field_node_with_sector_bytes(index, field, native_bytes)),
    );
    node.children = if children.is_empty() {
        InspectChildren::None
    } else {
        InspectChildren::Materialized(children)
    };
    node
}
