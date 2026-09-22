use edpcli::protocol::region_a::{
    analyze_iir_plain, locate_region_a_from_geometry, stored_iir_crc, IIR_CRC_SEGMENTS,
    IIR_MAIN_CRC_BODY_LEN, IIR_MAIN_CRC_OFFSET, IIR_MAIN_SIZE, REGION_A_TOTAL_SIZE,
};

fn synthetic_valid_iir() -> Vec<u8> {
    let mut data = (0..IIR_MAIN_SIZE)
        .map(|index| ((index * 73 + 0x31) & 0xff) as u8)
        .collect::<Vec<_>>();

    data[0x020..0x024].copy_from_slice(&0x0012_3456u32.to_be_bytes());
    data[0x024..0x028].copy_from_slice(&0x0023_4567u32.to_be_bytes());
    data[0x028..0x02C].copy_from_slice(&0x0034_5678u32.to_be_bytes());
    data[0x02C..0x030].copy_from_slice(&0x0045_6789u32.to_be_bytes());

    for segment in IIR_CRC_SEGMENTS {
        let crc =
            stored_iir_crc(&data[segment.body_offset..segment.body_offset + segment.body_len]);
        data[segment.crc_offset..segment.crc_offset + 2].copy_from_slice(&crc.to_le_bytes());
    }
    let main_crc = stored_iir_crc(&data[..IIR_MAIN_CRC_BODY_LEN]);
    data[IIR_MAIN_CRC_OFFSET..IIR_MAIN_CRC_OFFSET + 2].copy_from_slice(&main_crc.to_le_bytes());
    data
}

#[test]
fn iir_plain_parser_closes_main_and_all_segment_crcs() {
    let data = synthetic_valid_iir();
    let analysis = analyze_iir_plain(&data).unwrap();

    assert!(analysis.main_crc.ok);
    assert_eq!(analysis.segment_crcs.len(), 19);
    assert!(analysis.segment_crcs.iter().all(|(_, check)| check.ok));
    assert!(analysis.all_crc_ok());
    assert_eq!(analysis.partition_sizes.log_be_sectors, 0x0012_3456);
    assert_eq!(analysis.partition_sizes.public_be_sectors, 0x0023_4567);
    assert_eq!(analysis.partition_sizes.share_be_sectors, 0x0034_5678);
    assert_eq!(analysis.partition_sizes.private_be_sectors, 0x0045_6789);
}

#[test]
fn single_byte_tamper_breaks_segment_and_main_crc() {
    let mut data = synthetic_valid_iir();
    data[0x114] ^= 0x80;

    let analysis = analyze_iir_plain(&data).unwrap();
    assert!(!analysis.main_crc.ok);
    let data_key = analysis
        .segment_crcs
        .iter()
        .find(|(name, _)| *name == "data_key_slot")
        .unwrap();
    assert!(!data_key.1.ok);
    assert!(!analysis.all_crc_ok());
}

#[test]
fn parser_rejects_non_0x800_plaintext() {
    let error = analyze_iir_plain(&vec![0u8; IIR_MAIN_SIZE - 1]).unwrap_err();
    assert!(error.to_string().contains("2048"));
}

#[test]
fn official_geometry_locator_matches_three_physical_lba7_region_a_pointers() {
    // (cylinders, tracks/cylinder, sectors/track, bytes/sector, LBA7 StartSector)
    let samples = [
        (15_165u64, 255u32, 63u32, 512u32, 243_623_933u64), // Lexar
        (15_297u64, 255u32, 63u32, 512u32, 245_744_513u64), // aigo
        (14_955u64, 255u32, 63u32, 512u32, 240_250_283u64), // SanDisk
    ];

    for (cylinders, tracks, sectors, bytes, expected_lba) in samples {
        let layout = locate_region_a_from_geometry(cylinders, tracks, sectors, bytes)
            .expect("valid geometry");
        assert_eq!(layout.start_lba, expected_lba);
        assert_eq!(layout.size_bytes, REGION_A_TOTAL_SIZE as u64);
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
    let lexar = locate_region_a_from_geometry(15_165, 255, 63, 512).expect("Lexar geometry");
    assert_eq!(lexar.chs_bytes / 512, 243_625_725);
    assert_eq!(lexar.start_lba, 243_623_933);

    // The physical device has 243,625,984 sectors, but the official locator
    // deliberately uses DISK_GEOMETRY's CHS product instead.
    assert_ne!(243_625_984u64 - lexar.start_lba, 0x700);
    assert_eq!(243_625_725u64 - lexar.start_lba, 0x700);
}
