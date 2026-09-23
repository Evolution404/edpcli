use edpcli::protocol::edpf::EdpPartitionType;
use edpcli::provision::{
    build_official_partition_layout, OfficialPartitionMode, OfficialPartitionSizes,
    OFFICIAL_PARTITION_START_SECTOR, WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};

fn types(mode: OfficialPartitionMode) -> Vec<u32> {
    build_official_partition_layout(mode, OfficialPartitionSizes::new(32, 64, 128), 512)
        .unwrap()
        .into_iter()
        .map(|entry| entry.partition_type.raw())
        .collect()
}

#[test]
fn official_four_modes_emit_the_verified_partition_type_sequences() {
    assert_eq!(
        types(OfficialPartitionMode::DefaultThreePartition),
        vec![1, 2, 4]
    );
    assert_eq!(types(OfficialPartitionMode::BootShareCombined), vec![2, 4]);
    assert_eq!(types(OfficialPartitionMode::WholeDiskEncrypted), vec![1, 4]);
    assert_eq!(
        types(OfficialPartitionMode::IntranetExtranetDualPartition),
        vec![1, 2]
    );
}

#[test]
fn official_layouts_are_contiguous_from_lba63() {
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let layout =
            build_official_partition_layout(mode, OfficialPartitionSizes::new(32, 64, 128), 512)
                .unwrap();
        assert_eq!(layout[0].start_sector, OFFICIAL_PARTITION_START_SECTOR);
        for pair in layout.windows(2) {
            assert_eq!(pair[0].end_sector_exclusive(), pair[1].start_sector);
        }
    }
}

#[test]
fn whole_disk_encrypted_keeps_the_verified_0x7e00_type1_compatibility_geometry() {
    let layout = build_official_partition_layout(
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionSizes::new(999, 999, 128),
        512,
    )
    .unwrap();
    assert_eq!(layout.len(), 2);
    assert_eq!(layout[0].partition_type, EdpPartitionType::Boot);
    assert_eq!(layout[0].size_bytes, WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES);
    assert_eq!(layout[0].sector_count(), 63);
    assert_eq!(layout[1].partition_type, EdpPartitionType::Encrypt);
    assert_eq!(
        layout[1].size_bytes,
        128 * 1024 * 1024 - WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES
    );
}

#[test]
fn mode_specific_unused_size_fields_do_not_change_the_layout() {
    let a = build_official_partition_layout(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(1, 64, 128),
        512,
    )
    .unwrap();
    let b = build_official_partition_layout(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(999, 64, 128),
        512,
    )
    .unwrap();
    assert_eq!(a, b);
}
