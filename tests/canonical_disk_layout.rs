use edpcli::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
use edpcli::application::partition_table::{
    PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
    PhysicalPartition,
};
use edpcli::tui::disk_layout::{DiskLayoutPresentation, DiskLayoutProfile, TailExpansion};

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
        Some((DiskRegionKind::Reserved, 63))
    );
    assert!(!model.segments.iter().any(|segment| {
        segment.kind == DiskRegionKind::Free
            && segment.start_lba < edpcli::provision::OFFICIAL_PARTITION_START_SECTOR
    }));

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
fn draft_edp_marks_partition_overlap_with_reserved_header_as_conflict() {
    let model = DiskLayoutModel::draft_edp(
        10_000,
        vec![segment("启动区", 60, 10, DiskRegionKind::Boot)],
        6_000,
        6,
    );

    let reserved = model
        .segments
        .iter()
        .find(|segment| segment.kind == DiskRegionKind::Reserved)
        .expect("LBA13-62 must remain explicitly reserved");
    assert_eq!(reserved.start_lba, 13);
    assert_eq!(reserved.end_exclusive().unwrap(), 60);

    let conflict = model
        .segments
        .iter()
        .find(|segment| segment.kind == DiskRegionKind::Conflict)
        .expect("partition entering the reserved header must render as a conflict");
    assert_eq!(conflict.start_lba, 60);
    assert_eq!(conflict.end_exclusive().unwrap(), 63);
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

#[test]
fn canonical_layout_rejects_unknown_physical_ownership() {
    let error = DiskLayoutModel::canonical_from_known(
        100,
        vec![segment("unverified", 0, 100, DiskRegionKind::Unknown)],
    )
    .unwrap_err();
    assert!(error.contains("unknown physical ownership"), "{error}");
}

#[test]
fn presentation_profiles_preserve_canonical_geometry_and_tail_expansion() {
    let model = DiskLayoutModel::canonical_edp(
        10_000,
        vec![segment("交换区", 63, 5_937, DiskRegionKind::Share)],
        6_000,
        6,
    )
    .unwrap();
    let canonical = model.segments.clone();

    for profile in [
        DiskLayoutProfile::CompactHuman,
        DiskLayoutProfile::DetailedExact,
        DiskLayoutProfile::EditorExact,
    ] {
        let collapsed = DiskLayoutPresentation::new(&model, profile, TailExpansion::Collapsed);
        let visible = collapsed.visible_model();
        visible.validate_complete().unwrap();
        assert_eq!(visible.segments.last().unwrap().kind, DiskRegionKind::Tail);
        assert_eq!(visible.segments.last().unwrap().start_lba, 6_000);

        let expanded = DiskLayoutPresentation::new(&model, profile, TailExpansion::Expanded);
        assert_eq!(expanded.visible_model().segments, canonical);
        assert!(expanded
            .legend_lines()
            .iter()
            .any(|line| line.contains("盘尾恢复节点")));
    }
    assert_eq!(model.segments, canonical);
}

#[test]
fn plain_presentation_never_invents_edp_tail() {
    let model = DiskLayoutModel::canonical_plain_plan(
        10_000,
        vec![segment("普通分区", 2_048, 5_000, DiskRegionKind::Plain)],
    )
    .unwrap();
    for expansion in [TailExpansion::Collapsed, TailExpansion::Expanded] {
        let visible =
            DiskLayoutPresentation::new(&model, DiskLayoutProfile::DetailedExact, expansion)
                .visible_model();
        assert_eq!(visible.segments, model.segments);
        assert!(!visible
            .segments
            .iter()
            .any(|segment| segment.kind == DiskRegionKind::Tail));
    }
}
