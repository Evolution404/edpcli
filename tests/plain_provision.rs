use edpcli::provision::{
    max_plain_sector_count, OfficialFilesystemFormat, PlainPartitionSpec, PlainProvisionPlan,
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
