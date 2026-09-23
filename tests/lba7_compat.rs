use edpcli::protocol::lba7_compat::{
    locate_lba7_compatibility_extent_from_geometry, LBA7_COMPAT_EXTENT_TOTAL_SIZE,
};

#[test]
fn official_geometry_locator_matches_three_physical_lba7_compatibility_pointers() {
    // (cylinders, tracks/cylinder, sectors/track, bytes/sector, LBA7 StartSector)
    let samples = [
        (15_165u64, 255u32, 63u32, 512u32, 243_623_933u64), // Lexar
        (15_297u64, 255u32, 63u32, 512u32, 245_744_513u64), // aigo
        (14_955u64, 255u32, 63u32, 512u32, 240_250_283u64), // SanDisk
    ];

    for (cylinders, tracks, sectors, bytes, expected_lba) in samples {
        let layout =
            locate_lba7_compatibility_extent_from_geometry(cylinders, tracks, sectors, bytes)
                .expect("valid geometry");
        assert_eq!(layout.start_lba, expected_lba);
        assert_eq!(layout.size_bytes, LBA7_COMPAT_EXTENT_TOTAL_SIZE as u64);
        assert_eq!(layout.size_sectors, 6);
        assert_eq!(
            layout.start_byte_offset,
            layout.chs_bytes - 0xE0000,
            "official locator is CHS bytes minus 0xE0000"
        );
    }
}

#[test]
fn geometry_locator_is_chs_based_not_physical_total_sector_based() {
    let lexar = locate_lba7_compatibility_extent_from_geometry(15_165, 255, 63, 512)
        .expect("Lexar geometry");
    assert_eq!(lexar.chs_bytes / 512, 243_625_725);
    assert_eq!(lexar.start_lba, 243_623_933);

    // The physical device has 243,625,984 sectors, but the official locator
    // deliberately uses DISK_GEOMETRY's CHS product instead.
    assert_ne!(243_625_984u64 - lexar.start_lba, 0x700);
    assert_eq!(243_625_725u64 - lexar.start_lba, 0x700);
}
