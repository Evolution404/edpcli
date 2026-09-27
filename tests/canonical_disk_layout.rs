use edpcli::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
use edpcli::application::partition_table::{
    PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
    PhysicalPartition,
};

fn segment(
    label: &str,
    start_lba: u64,
    sector_count: u64,
    kind: DiskRegionKind,
) -> DiskLayoutSegment {
    DiskLayoutSegment {
        label: label.into(),
        start_lba,
        sector_count,
        kind,
    }
}

#[test]
fn canonical_edp_layout_is_complete_disjoint_and_groups_tail_from_lce() {
    let model = DiskLayoutModel::canonical_edp(
        10_000,
        vec![
            segment("启动区", 63, 37, DiskRegionKind::Boot),
            segment("交换区", 100, 4_900, DiskRegionKind::Share),
            segment("保密区", 5_000, 1_000, DiskRegionKind::Encrypt),
        ],
        6_000,
        6,
    )
    .unwrap();

    model.validate_complete().unwrap();
    assert!(model
        .segments
        .iter()
        .all(|segment| segment.kind != DiskRegionKind::Unknown));
    assert_eq!(
        model
            .segments
            .iter()
            .find(|segment| segment.start_lba == 13)
            .map(|segment| (segment.kind, segment.end_exclusive().unwrap())),
        Some((DiskRegionKind::Free, 63))
    );

    let tail = model.tail_group().expect("EDP LCE must anchor tail group");
    assert_eq!(tail.start_lba, 6_000);
    assert_eq!(tail.end_exclusive, 10_000);
    assert_eq!(tail.children.first().unwrap().kind, DiskRegionKind::Lce);
    assert_eq!(tail.children.first().unwrap().sector_count, 6);
    assert!(tail
        .children
        .iter()
        .any(|segment| segment.kind == DiskRegionKind::BackupMirror
            && segment.start_lba == 8_976
            && segment.sector_count == 9));
    assert!(tail
        .children
        .iter()
        .any(|segment| segment.kind == DiskRegionKind::RestoreNode
            && segment.start_lba == 9_996
            && segment.sector_count == 1));
    assert_eq!(
        tail.children
            .iter()
            .map(|segment| segment.sector_count)
            .sum::<u64>(),
        4_000
    );
}

#[test]
fn canonical_plain_layout_uses_partition_table_metadata_and_real_gaps_only() {
    let table = PartitionTableSnapshot {
        kind: PartitionTableKind::Mbr,
        partitions: vec![PhysicalPartition {
            index: 1,
            start_lba: 2_048,
            sector_count: 5_000,
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
    };

    let model = DiskLayoutModel::canonical_plain(10_000, &table).unwrap();
    model.validate_complete().unwrap();
    assert_eq!(model.segments[0].kind, DiskRegionKind::Metadata);
    assert_eq!(model.segments[0].start_lba, 0);
    assert_eq!(model.segments[1].kind, DiskRegionKind::Free);
    assert_eq!(model.segments[1].start_lba, 1);
    assert_eq!(model.segments[1].sector_count, 2_047);
    assert_eq!(model.segments[2].kind, DiskRegionKind::Plain);
    assert_eq!(model.segments[2].start_lba, 2_048);
    assert!(model
        .segments
        .iter()
        .all(|segment| !matches!(segment.kind, DiskRegionKind::Unknown | DiskRegionKind::Tail)));
    assert!(model.tail_group().is_none());
}

#[test]
fn canonical_layout_rejects_overlapping_physical_ownership() {
    let error = DiskLayoutModel::canonical_from_known(
        100,
        vec![
            segment("A", 10, 20, DiskRegionKind::Plain),
            segment("B", 20, 20, DiskRegionKind::Plain),
        ],
    )
    .unwrap_err();
    assert!(error.contains("overlap"), "{error}");
}
