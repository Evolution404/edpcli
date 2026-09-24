use std::io;

use edpcli::{
    backup_deep::{analyze_partition, AnalysisStatus, PartitionReader},
    backup_metadata::PartitionGeometry,
    provision::{build_empty_exfat, build_empty_fat16, SparseFilesystemImage},
};

struct ImageReader<'a>(&'a SparseFilesystemImage);

impl PartitionReader for ImageReader<'_> {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.0
            .sector_or_zero(lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| io::Error::other("outside volume"))
    }
}

fn partition(sectors: u64) -> PartitionGeometry {
    PartitionGeometry {
        index: 0,
        partition_type: 1,
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: 63,
        sector_size: 512,
        partition_size: sectors * 512,
        sector_count: sectors,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    }
}

#[test]
fn official_20417_sector_fat16_has_complete_bpb_mirrored_fats_and_label() {
    let image = build_empty_fat16(63, 20_417, 0x1234_5678, "启动区").unwrap();
    let boot = image.sectors().get(&0).unwrap();
    assert_eq!(&boot[..3], &[0xeb, 0x3c, 0x90]);
    assert_eq!(u16::from_le_bytes(boot[11..13].try_into().unwrap()), 512);
    assert!(boot[13].is_power_of_two());
    assert_eq!(u16::from_le_bytes(boot[14..16].try_into().unwrap()), 1);
    assert_eq!(boot[16], 2);
    assert_eq!(u16::from_le_bytes(boot[17..19].try_into().unwrap()), 512);
    assert_eq!(u16::from_le_bytes(boot[19..21].try_into().unwrap()), 20_417);
    assert_eq!(boot[21], 0xf8);
    let fat_sectors = u16::from_le_bytes(boot[22..24].try_into().unwrap()) as u64;
    assert!(fat_sectors > 0);
    assert_eq!(u16::from_le_bytes(boot[24..26].try_into().unwrap()), 63);
    assert_eq!(u16::from_le_bytes(boot[26..28].try_into().unwrap()), 255);
    assert_eq!(u32::from_le_bytes(boot[28..32].try_into().unwrap()), 63);
    assert_eq!(
        u32::from_le_bytes(boot[39..43].try_into().unwrap()),
        0x1234_5678
    );
    assert_eq!(&boot[54..62], b"FAT16   ");
    assert_eq!(&boot[510..512], &[0x55, 0xaa]);
    for offset in 0..fat_sectors {
        assert_eq!(
            image.sectors().get(&(1 + offset)),
            image.sectors().get(&(1 + fat_sectors + offset))
        );
    }
    let root_lba = 1 + 2 * fat_sectors;
    let root = image.sectors().get(&root_lba).unwrap();
    assert_eq!(&boot[43..54], &root[..11]);
    assert_eq!(root[11], 0x08);
    let (label, _, errors) = encoding_rs::GBK.decode(&root[..11]);
    assert!(!errors);
    assert_eq!(label.trim_end_matches(' '), "启动区");
    for offset in 1..32 {
        assert_eq!(image.sectors().get(&(root_lba + offset)), Some(&[0; 512]));
    }
    let mut reader = ImageReader(&image);
    let report = analyze_partition(&partition(20_417), &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("fat16"));
    assert_eq!(report.file_count, Some(0));
}

#[test]
fn original_20417_sector_exfat_builder_remains_valid() {
    let image = build_empty_exfat(63, 20_417, 0x1234_5678, "启动区").unwrap();
    let boot = image.sectors().get(&0).unwrap();
    assert_eq!(&boot[3..11], b"EXFAT   ");
    assert_eq!(u64::from_le_bytes(boot[64..72].try_into().unwrap()), 63);
    assert_eq!(u64::from_le_bytes(boot[72..80].try_into().unwrap()), 20_417);
    let mut reader = ImageReader(&image);
    let report = analyze_partition(&partition(20_417), &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
}
