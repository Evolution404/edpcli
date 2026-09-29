use edpcli::common::SECTOR;
use edpcli::inspect_target::{detect_plain_filesystem, FilesystemBootKind};
use edpcli::provision::{build_empty_exfat, build_empty_fat16};

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn fat12_boot(total: u16) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    put16(&mut boot, 11, 512);
    boot[13] = 8;
    put16(&mut boot, 14, 8);
    boot[16] = 2;
    put16(&mut boot, 17, 512);
    put16(&mut boot, 19, total);
    boot[21] = 0xf8;
    put16(&mut boot, 22, 8);
    boot[38] = 0x29;
    boot[43..54].copy_from_slice(b"NO NAME    ");
    boot[54..62].copy_from_slice(b"FAT12   ");
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn fat32_boot(total: u32) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    put16(&mut boot, 11, 512);
    boot[13] = 8;
    put16(&mut boot, 14, 32);
    boot[16] = 2;
    put16(&mut boot, 17, 0);
    put16(&mut boot, 19, 0);
    boot[21] = 0xf8;
    put16(&mut boot, 22, 0);
    put32(&mut boot, 32, total);
    put32(&mut boot, 36, 1_000);
    put32(&mut boot, 44, 2);
    boot[66] = 0x29;
    boot[82..90].copy_from_slice(b"FAT32   ");
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn ntfs_boot(total: u64) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    boot[3..11].copy_from_slice(b"NTFS    ");
    put16(&mut boot, 11, 512);
    boot[13] = 8;
    boot[21] = 0xf8;
    put64(&mut boot, 40, total);
    put64(&mut boot, 48, 4);
    put64(&mut boot, 56, 8);
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

#[test]
fn filesystem_detection_matrix_is_frozen_before_domain_refactor() {
    let fat12_total = 16_000u64;
    assert_eq!(
        detect_plain_filesystem(fat12_total, &fat12_boot(fat12_total as u16)),
        Some(FilesystemBootKind::Fat12)
    );

    let fat16_total = 20_417u64;
    let fat16 = build_empty_fat16(63, fat16_total, 0x1234_5678, "BOOT").unwrap();
    assert_eq!(
        detect_plain_filesystem(fat16_total, &fat16.sector_or_zero(0).unwrap()),
        Some(FilesystemBootKind::Fat16)
    );

    let fat32_total = 1_000_000u64;
    assert_eq!(
        detect_plain_filesystem(fat32_total, &fat32_boot(fat32_total as u32)),
        Some(FilesystemBootKind::Fat32)
    );

    let exfat_total = 100_000u64;
    let exfat = build_empty_exfat(2_048, exfat_total, 0x8765_4321, "DATA").unwrap();
    assert_eq!(
        detect_plain_filesystem(exfat_total, &exfat.sector_or_zero(0).unwrap()),
        Some(FilesystemBootKind::Exfat)
    );

    let ntfs_total = 100_000u64;
    assert_eq!(
        detect_plain_filesystem(ntfs_total, &ntfs_boot(ntfs_total)),
        Some(FilesystemBootKind::Ntfs)
    );

    assert_eq!(detect_plain_filesystem(100_000, &[0u8; SECTOR]), None);
}

#[test]
fn fat16_empty_label_has_no_root_label_entry() {
    let image = build_empty_fat16(63, 20_417, 0x1234_5678, "").unwrap();
    let boot = image.sector_or_zero(0).unwrap();
    assert_eq!(&boot[43..54], b"NO NAME    ");
    let fat_sectors = u16::from_le_bytes(boot[22..24].try_into().unwrap()) as u64;
    let root_start = 1 + 2 * fat_sectors;
    let root = image.sector_or_zero(root_start).unwrap();
    assert_ne!(
        root[11], 0x08,
        "empty label must not create a FAT root label entry"
    );
}

#[test]
fn exfat_label_entry_is_present_only_for_a_user_label() {
    for (label, expected_entry) in [("DATA", true), ("", false)] {
        let image = build_empty_exfat(2_048, 100_000, 0x8765_4321, label).unwrap();
        let boot = image.sector_or_zero(0).unwrap();
        let heap_offset = u32::from_le_bytes(boot[88..92].try_into().unwrap()) as u64;
        let root = image.sector_or_zero(heap_offset).unwrap();
        assert_eq!(root[64] == 0x83, expected_entry, "label={label:?}");
        if expected_entry {
            assert_eq!(root[65], 4);
            let units = (0..4)
                .map(|index| u16::from_le_bytes([root[66 + index * 2], root[67 + index * 2]]))
                .collect::<Vec<_>>();
            assert_eq!(String::from_utf16(&units).unwrap(), "DATA");
        }
    }
}
