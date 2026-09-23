use edpcli::backup_deep::keys::{default_file_key, sm4_encrypt_block};
use edpcli::backup_metadata::parse_partition_geometry;
use edpcli::common::SECTOR;
use edpcli::crypto::{a6b0_full, a6b0_full_offset, a7f0_full, crc32_bare};
use edpcli::inspect_target::{
    FilesystemBootKind, InspectDiskContext, PhysicalDataState, SectorRegion,
};

const LEXAR_DEVICE_ID: &str = "disk&ven_lexar&prod_usb_flash_drive";
const LEXAR_TOTAL_SECTORS: u64 = 243_625_984;
const LEXAR_LCE_START: u64 = 243_623_933;
const LEXAR_PROTOCOL: &[u8; 13 * SECTOR] = include_bytes!(
    "fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin"
);
const LEXAR_LCE_CIPHER: &[u8; 3072] =
    include_bytes!("../audit/protocol/lba7_compatibility/gold/lexar_lba7_compat_lba243623933.bin");
const LCE_PLAIN: &[u8; 3072] =
    include_bytes!("../audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin");

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn valid_exfat_boot(partition_start: u64, sector_count: u64) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    let fat_offset = 24u64;
    let fat_length = 1024u64.min(sector_count.saturating_sub(fat_offset + 1).max(1));
    let heap_offset = fat_offset + fat_length;
    let sectors_per_cluster = 8u64;
    let cluster_count = sector_count
        .saturating_sub(heap_offset)
        .checked_div(sectors_per_cluster)
        .unwrap_or(0)
        .min(u32::MAX as u64);
    assert!(cluster_count > 0);
    put64(&mut boot, 64, partition_start);
    put64(&mut boot, 72, sector_count);
    put32(&mut boot, 80, fat_offset as u32);
    put32(&mut boot, 84, fat_length as u32);
    put32(&mut boot, 88, heap_offset as u32);
    put32(&mut boot, 92, cluster_count as u32);
    put32(&mut boot, 96, 2);
    put16(&mut boot, 104, 0x0100);
    boot[108] = 9;
    boot[109] = 3;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = 0xff;
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn valid_ntfs_boot(sector_count: u64) -> [u8; SECTOR] {
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    boot[3..11].copy_from_slice(b"NTFS    ");
    put16(&mut boot, 11, 512);
    boot[13] = 8;
    boot[21] = 0xf8;
    put64(&mut boot, 40, sector_count);
    put64(&mut boot, 48, 4);
    put64(&mut boot, 56, 8);
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn real_shape_fat12_boot(partition_start: u64, sector_count: u64) -> [u8; SECTOR] {
    assert!(sector_count <= u16::MAX as u64);
    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    put16(&mut boot, 11, 512);
    boot[13] = 8;
    put16(&mut boot, 14, 8);
    boot[16] = 2;
    put16(&mut boot, 17, 512);
    put16(&mut boot, 19, sector_count as u16);
    boot[21] = 0xf8;
    put16(&mut boot, 22, 8);
    put16(&mut boot, 24, 63);
    put16(&mut boot, 26, 255);
    put32(&mut boot, 28, partition_start as u32);
    boot[38] = 0x29;
    boot[43..54].copy_from_slice(b"NO NAME    ");
    boot[54..62].copy_from_slice(b"FAT12   ");
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn encrypt_mode2_sector(plain: &[u8; SECTOR], key: &[u8; 16]) -> [u8; SECTOR] {
    let mut out = [0u8; SECTOR];
    for (source, target) in plain.chunks_exact(16).zip(out.chunks_exact_mut(16)) {
        target.copy_from_slice(&sm4_encrypt_block(source.try_into().unwrap(), key));
    }
    out
}

#[test]
fn lce_u64_physical_offset_decrypts_real_3072_bytes_bit_exact() {
    let offset = LEXAR_LCE_START * SECTOR as u64;
    assert!(
        offset > u32::MAX as u64,
        "fixture must exercise the 64-bit tweak"
    );
    let plain = a6b0_full_offset(LEXAR_LCE_CIPHER, &[0u8; 8], offset);
    assert_eq!(plain.as_slice(), LCE_PLAIN);
}

#[test]
fn inspect_context_classifies_and_decodes_each_real_lce_sector() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let lce = context.lce.as_ref().expect("LCE geometry");
    assert_eq!(lce.start_lba, LEXAR_LCE_START);
    assert_eq!(lce.sector_count, 6);

    for index in 0..6usize {
        let lba = LEXAR_LCE_START + index as u64;
        assert!(context.regions(lba).iter().any(|region| {
            matches!(region, SectorRegion::Lce { index: found, .. } if *found == index as u64)
        }));
        let raw = &LEXAR_LCE_CIPHER[index * SECTOR..(index + 1) * SECTOR];
        let (decoded, method) = context.decode_non_protocol(lba, raw).unwrap();
        assert!(method.contains("zero8"));
        assert_eq!(
            decoded.as_slice(),
            &LCE_PLAIN[index * SECTOR..(index + 1) * SECTOR]
        );
    }
}

#[test]
fn plaintext_type1_with_valid_raw_filesystem_decodes_without_transform() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let partition = context
        .partitions
        .iter()
        .find(|partition| partition.partition_type == 1)
        .expect("real protocol fixture must contain type1");
    let raw = valid_exfat_boot(partition.start_sector, partition.sector_count);
    let state = context.partition_physical_state(partition, &raw);
    assert!(matches!(
        state,
        PhysicalDataState::PlaintextFilesystem { .. }
    ));
    let (decoded, method) = context
        .decode_non_protocol(partition.start_sector, &raw)
        .unwrap();
    assert_eq!(decoded.as_slice(), raw.as_slice());
    assert!(method.contains("type1"));
    assert!(method.contains("未执行 SM4"));
}

#[test]
fn plaintext_type1_with_real_fat12_shape_decodes_without_transform() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let partition = context
        .partitions
        .iter()
        .find(|partition| partition.partition_type == 1)
        .expect("real protocol fixture must contain type1");
    let raw = real_shape_fat12_boot(partition.start_sector, partition.sector_count);
    let state = context.partition_physical_state(partition, &raw);
    assert_eq!(
        state,
        PhysicalDataState::PlaintextFilesystem {
            filesystem: FilesystemBootKind::Fat12,
        }
    );
    let (decoded, method) = context
        .decode_non_protocol(partition.start_sector, &raw)
        .unwrap();
    assert_eq!(decoded.as_slice(), raw.as_slice());
    assert!(method.contains("FAT12"));
    assert!(method.contains("未执行 SM4"));
}

#[test]
fn need_encrypt_one_type2_with_valid_raw_exfat_stays_plaintext() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let partition = context
        .partitions
        .iter()
        .find(|partition| partition.partition_type == 2 && partition.need_encrypt != 0)
        .expect("real protocol fixture must contain need_encrypt=1 type2");
    assert_eq!(partition.encrypt_mode, 2);

    let raw = valid_exfat_boot(partition.start_sector, partition.sector_count);
    let state = context.partition_physical_state(partition, &raw);
    assert!(matches!(
        state,
        PhysicalDataState::PlaintextFilesystem { .. }
    ));
    let (decoded, method) = context
        .decode_non_protocol(partition.start_sector, &raw)
        .unwrap();
    assert_eq!(decoded.as_slice(), raw.as_slice());
    assert!(method.contains("物理盘面已为有效 exFAT 明文文件系统"));
    assert!(!method.contains("SM4-ECB"));
}

#[test]
fn encrypted_type4_requires_valid_file_key_and_decoded_boot_sector() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let partition = context
        .partitions
        .iter()
        .find(|partition| partition.partition_type == 4 && partition.need_encrypt != 0)
        .expect("real protocol fixture must contain encrypted type4");
    assert_eq!(partition.encrypt_mode, 2);

    let plain = valid_ntfs_boot(partition.sector_count);
    let key = default_file_key(LEXAR_PROTOCOL, LEXAR_DEVICE_ID, partition.index).unwrap();
    let raw = encrypt_mode2_sector(&plain, &key);
    assert_ne!(raw, plain);
    let state = context.partition_physical_state(partition, &raw);
    assert!(matches!(state, PhysicalDataState::EncryptedMode2 { .. }));
    let (decoded, method) = context
        .decode_non_protocol(partition.start_sector, &raw)
        .unwrap();
    assert_eq!(decoded.as_slice(), plain.as_slice());
    assert!(method.contains("SM4-ECB"));
    assert!(method.contains("FileKeyCRC=PASS"));
    assert!(method.contains("NTFS"));
}

#[test]
fn encrypted_partition_decode_fails_closed_when_wrapped_key_crc_breaks() {
    let mut image = LEXAR_PROTOCOL.to_vec();
    let crc = crc32_bare(LEXAR_DEVICE_ID.as_bytes());
    let mut plain = a6b0_full(&image[12 * SECTOR..13 * SECTOR], &crc.to_le_bytes(), 0);
    let partitions =
        parse_partition_geometry(&image, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    let partition = partitions
        .iter()
        .find(|partition| partition.partition_type == 2 && partition.need_encrypt != 0)
        .unwrap();
    plain[partition.index * 0x60 + 0x38] ^= 0x01;
    image[12 * SECTOR..13 * SECTOR].copy_from_slice(&a7f0_full(&plain, &crc.to_le_bytes(), 0));

    let context = InspectDiskContext::new(image, Some(LEXAR_DEVICE_ID.into()), LEXAR_TOTAL_SECTORS);
    let error = context
        .decode_non_protocol(partition.start_sector, &[0u8; SECTOR])
        .unwrap_err();
    assert!(error.contains("FileKeyCRC"), "{error}");
}

#[test]
fn non_start_partition_sector_requires_boot_evidence() {
    let context = InspectDiskContext::new(
        LEXAR_PROTOCOL.to_vec(),
        Some(LEXAR_DEVICE_ID.into()),
        LEXAR_TOTAL_SECTORS,
    );
    let partition = context
        .partitions
        .iter()
        .find(|partition| partition.partition_type == 2 && partition.need_encrypt != 0)
        .unwrap();
    let error = context
        .decode_non_protocol(partition.start_sector + 1, &[0u8; SECTOR])
        .unwrap_err();
    assert!(error.contains("缺少起始扇区证据"), "{error}");
}

#[test]
fn unknown_region_decode_is_fail_closed() {
    let context = InspectDiskContext::new(vec![0; 13 * SECTOR], None, 4096);
    assert_eq!(context.regions(1000), vec![SectorRegion::Unknown]);
    let error = context
        .decode_non_protocol(1000, &[0u8; SECTOR])
        .unwrap_err();
    assert!(error.contains("decode 拒绝猜测"), "{error}");
}
