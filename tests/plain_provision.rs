use edpcli::provision::{
    build_plain_provision_write_plan, max_plain_sector_count, OfficialFilesystemFormat,
    PlainCleanupExtent, PlainPartitionSpec, PlainProvisionPlan, PlainSectorOwner,
    DEFAULT_PLAIN_START_LBA,
};

fn part(start_lba: u64, sector_count: u64) -> PlainPartitionSpec {
    PlainPartitionSpec::new(
        start_lba,
        sector_count,
        OfficialFilesystemFormat::ExFat,
        "DATA",
    )
}

#[test]
fn default_plain_partition_starts_at_2048_and_fills_the_disk() {
    let plan = PlainProvisionPlan::default_for_disk(1_000_000).unwrap();
    assert_eq!(plan.partitions.len(), 1);
    assert_eq!(plan.partitions[0].start_lba, DEFAULT_PLAIN_START_LBA);
    assert_eq!(
        plan.partitions[0].sector_count,
        1_000_000 - DEFAULT_PLAIN_START_LBA
    );
    assert_eq!(plan.gaps.len(), 1);
    assert_eq!(plan.gaps[0].start_lba, 1);
    assert_eq!(plan.gaps[0].sector_count, DEFAULT_PLAIN_START_LBA - 1);
}

#[test]
fn plain_accepts_one_to_four_partitions_and_explicit_gaps() {
    let plan = PlainProvisionPlan::new(
        100_000,
        vec![
            part(2_048, 10_000),
            part(20_000, 5_000),
            part(30_000, 10_000),
            part(50_000, 20_000),
        ],
    )
    .unwrap();
    assert_eq!(plan.partitions.len(), 4);
    assert!(plan
        .gaps
        .iter()
        .any(|gap| gap.start_lba == 12_048 && gap.sector_count == 7_952));
    assert!(PlainProvisionPlan::new(
        100_000,
        vec![
            part(2_048, 1),
            part(3_000, 1),
            part(4_000, 1),
            part(5_000, 1),
            part(6_000, 1),
        ],
    )
    .is_err());
}

#[test]
fn editing_one_partition_never_moves_another_partition() {
    let mut parts = vec![part(2_048, 10_000), part(30_000, 20_000)];
    let second_before = parts[1].clone();
    parts[0].sector_count = 15_000;
    let plan = PlainProvisionPlan::new(100_000, parts).unwrap();
    assert_eq!(plan.partitions[1], second_before);
}

#[test]
fn overlap_and_disk_overflow_fail_closed() {
    assert!(
        PlainProvisionPlan::new(100_000, vec![part(2_048, 30_000), part(20_000, 5_000)])
            .unwrap_err()
            .contains("重叠")
    );
    assert!(PlainProvisionPlan::new(100_000, vec![part(90_000, 20_000)])
        .unwrap_err()
        .contains("越过磁盘末端"));
    assert!(PlainProvisionPlan::new(100_000, vec![part(0, 1)])
        .unwrap_err()
        .contains("LBA0"));
}

#[test]
fn fill_uses_the_next_partition_as_a_hard_boundary_without_moving_it() {
    let mut plan =
        PlainProvisionPlan::new(100_000, vec![part(2_048, 10_000), part(40_000, 10_000)]).unwrap();
    assert_eq!(
        max_plain_sector_count(100_000, &plan.partitions, 0).unwrap(),
        37_952
    );
    let second_before = plan.partitions[1].clone();
    plan.fill_partition(0).unwrap();
    assert_eq!(plan.partitions[0].sector_count, 37_952);
    assert_eq!(plan.partitions[1], second_before);
}

#[test]
fn mbr_32_bit_boundaries_fail_closed() {
    let too_far = u32::MAX as u64 + 1;
    assert!(PlainProvisionPlan::new(too_far + 10, vec![part(too_far, 1)]).is_err());
    assert!(PlainProvisionPlan::new(u32::MAX as u64 + 100, vec![part(1, u32::MAX as u64)]).is_ok());
    assert!(
        PlainProvisionPlan::new(u32::MAX as u64 + 101, vec![part(2, u32::MAX as u64)]).is_err()
    );
}

#[test]
fn write_plan_builds_plain_mbr_cleans_edp_and_preserves_lba3() {
    let plan = PlainProvisionPlan::new(100_000, vec![part(2_048, 20_000)]).unwrap();
    let write = build_plain_provision_write_plan(
        &plan,
        Some(PlainCleanupExtent::new(50_000, 6)),
        &[0x1234_5678],
    )
    .unwrap();

    assert_eq!(&write.mbr[510..512], &[0x55, 0xaa]);
    let entry = &write.mbr[0x1be..0x1ce];
    assert_eq!(entry[4], 0x07);
    assert_eq!(u32::from_le_bytes(entry[8..12].try_into().unwrap()), 2_048);
    assert_eq!(
        u32::from_le_bytes(entry[12..16].try_into().unwrap()),
        20_000
    );
    assert_eq!(write.preserved_lbas(), &[3]);
    assert!(write.writes.get(&3).is_none());
    for lba in [1u32, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
        let sector = write.writes.get(&lba).unwrap();
        assert_eq!(sector.owner, PlainSectorOwner::EdpMetadataCleanup);
        assert_eq!(sector.bytes, [0; 512]);
    }
    for lba in 50_000u32..50_006 {
        let sector = write.writes.get(&lba).unwrap();
        assert_eq!(sector.owner, PlainSectorOwner::LceCleanup);
        assert_eq!(sector.bytes, [0; 512]);
    }
    assert_eq!(
        write.writes.get(&2_048).unwrap().owner,
        PlainSectorOwner::Filesystem { partition_index: 0 }
    );
}

#[test]
fn filesystem_sector_owns_lba_when_old_lce_overlaps_it() {
    let plan = PlainProvisionPlan::new(100_000, vec![part(2_048, 20_000)]).unwrap();
    let write = build_plain_provision_write_plan(
        &plan,
        Some(PlainCleanupExtent::new(2_048, 6)),
        &[0x1234_5678],
    )
    .unwrap();
    let first = write.writes.get(&2_048).unwrap();
    assert_eq!(
        first.owner,
        PlainSectorOwner::Filesystem { partition_index: 0 }
    );
    assert_ne!(first.bytes, [0; 512]);
}

#[test]
fn plain_write_plan_rejects_a_partition_that_would_overwrite_lba3() {
    let plan = PlainProvisionPlan::new(100_000, vec![part(1, 10_000)]).unwrap();
    assert!(build_plain_provision_write_plan(&plan, None, &[1])
        .unwrap_err()
        .contains("LBA3"));
}

#[test]
fn plain_mbr_contains_all_primary_partition_entries() {
    let plan = PlainProvisionPlan::new(
        100_000,
        vec![
            part(2_048, 10_000),
            part(20_000, 5_000),
            part(40_000, 8_000),
        ],
    )
    .unwrap();
    let write = build_plain_provision_write_plan(&plan, None, &[1, 2, 3]).unwrap();
    for (index, expected) in [(2_048u32, 10_000u32), (20_000, 5_000), (40_000, 8_000)]
        .into_iter()
        .enumerate()
    {
        let offset = 0x1be + index * 16;
        let entry = &write.mbr[offset..offset + 16];
        assert_eq!(entry[4], 0x07);
        assert_eq!(
            u32::from_le_bytes(entry[8..12].try_into().unwrap()),
            expected.0
        );
        assert_eq!(
            u32::from_le_bytes(entry[12..16].try_into().unwrap()),
            expected.1
        );
    }
}
