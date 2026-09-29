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
    assert!(matches!(
        partition_region.children,
        InspectChildren::LazySectors { .. }
    ));
    let page = partition_region.materialize_sector_page(10, 3);
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
fn ordinary_regions_own_lazy_sectors_directly_without_singleton_extent_wrappers() {
    let topology = build_inspect_topology(&context(10_000));
    let InspectChildren::Materialized(regions) = &topology.root.children else {
        panic!("root regions must be materialized");
    };
    for region in regions {
        if region.region_semantic == Some(DiskRegionSemantic::Tail) {
            continue;
        }
        assert!(
            matches!(region.children, InspectChildren::LazySectors { .. }),
            "{} must expose sectors directly",
            region.id
        );
        assert!(!region.id.ends_with(".extent"));
        assert_ne!(region.label, "扇区范围");
    }
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
    assert!(matches!(
        protocol.children,
        InspectChildren::LazySectors { .. }
    ));
    let sector = protocol.materialize_sector_page(0, 1).remove(0);
    let InspectChildren::Materialized(children) = sector.children else {
        panic!("LBA0 must expose structure");
    };
    let table = &children[0];
    assert_eq!(table.kind, InspectNodeKind::Structure);
    assert_eq!(table.range.byte_range.unwrap().start, 0x1be);
    assert_eq!(table.range.byte_range.unwrap().end_exclusive, 0x1fe);
}

#[test]
fn cached_sector_enrichment_preserves_canonical_topology_identity_and_semantic() {
    let topology = build_inspect_topology(&context(10_000));
    let region = topology
        .primary_region_for_lba(30)
        .expect("free region containing LBA30");
    assert_eq!(
        region.region_semantic,
        Some(DiskRegionSemantic::Unallocated)
    );
    let offset = 30 - region.range.start_lba;
    let base = region
        .materialize_sector_page(offset, 1)
        .into_iter()
        .next()
        .expect("LBA30");
    let identity = (
        base.id.clone(),
        base.label.clone(),
        base.kind,
        base.range,
        base.decoder,
        base.status,
        base.region_semantic,
    );
    let field = InspectField {
        key: super::super::inspect::InspectFieldKey::Synthetic,
        range: AbsoluteByteRange {
            start: 30 * SECTOR as u64,
            end_exclusive: 30 * SECTOR as u64 + 4,
        },
        field_type: super::super::inspect::InspectFieldType::Identity,
        raw: vec![1, 2, 3, 4],
        decoded: vec![1, 2, 3, 4],
        field_logical: None,
        transform: None,
        status: super::super::inspect::InspectFieldStatus::Known,
        label: "cached".into(),
        value: "value".into(),
        style: FieldStyle::Identity,
        group: None,
        children: Vec::new(),
    };

    let enriched = enrich_sector_node(base, &[field]);
    assert_eq!(
        (
            enriched.id.clone(),
            enriched.label.clone(),
            enriched.kind,
            enriched.range,
            enriched.decoder,
            enriched.status,
            enriched.region_semantic,
        ),
        identity
    );
    assert_eq!(
        enriched.region_semantic,
        Some(DiskRegionSemantic::Unallocated),
        "decode hydration must not change the region color/semantic"
    );
    let InspectChildren::Materialized(children) = enriched.children else {
        panic!("enriched sector must expose decoded field children");
    };
    assert!(children
        .iter()
        .any(|child| child.kind == InspectNodeKind::Field));
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
