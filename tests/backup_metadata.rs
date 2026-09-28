use crate::common;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use common::load_disk_image;
use edpcli::backup_metadata::{
    acquire_metadata, acquire_plain_metadata, parse_lba7_compatibility_geometry,
    parse_partition_geometry, FilesystemKind, LBA7_COMPAT_EXTENT_SECTORS, PARTITION_PREFIX_SECTORS,
};
use edpcli::common::SECTOR;
use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare, xor_rolling};
use edpcli::diskio::SectorDev;
use edpcli::edpb::{
    self, CaptureLevel, CoreCapture, MetadataCapture, RestorePolicy, SemanticStatus,
};

const NETAC_DEVICE_ID: &str = "disk&ven_netac&prod_onlydisk";
const NETAC_TOTAL_SECTORS: u64 = 122_880_000;
const LEXAR_DEVICE_ID: &str = "disk&ven_lexar&prod_usb_flash_drive";
const LEXAR_TOTAL_SECTORS: u64 = 243_625_984;
const LEXAR_LBA7_COMPAT_START: u64 = 243_623_933;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("edpcli_{tag}_{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ReadOnlySparseDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    reads: Vec<u32>,
    writes: usize,
}

impl ReadOnlySparseDev {
    fn new() -> Self {
        Self {
            sectors: BTreeMap::new(),
            reads: Vec::new(),
            writes: 0,
        }
    }

    fn insert(&mut self, lba: u64, bytes: Vec<u8>) {
        assert_eq!(bytes.len(), SECTOR);
        self.sectors.insert(u32::try_from(lba).unwrap(), bytes);
    }
}

impl SectorDev for ReadOnlySparseDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.reads.push(lba);
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0u8; SECTOR]))
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
        self.writes += 1;
        Err(io::Error::other(
            "Metadata capture must never write the source device",
        ))
    }
}

fn ntfs_boot(mft_lcn: u64, mftmirr_lcn: u64) -> Vec<u8> {
    let mut boot = vec![0u8; SECTOR];
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    boot[13] = 8;
    boot[48..56].copy_from_slice(&mft_lcn.to_le_bytes());
    boot[56..64].copy_from_slice(&mftmirr_lcn.to_le_bytes());
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn strict_ntfs_boot(sector_count: u64) -> Vec<u8> {
    let mut boot = vec![0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    boot[13] = 8;
    boot[21] = 0xf8;
    boot[40..48].copy_from_slice(&sector_count.to_le_bytes());
    boot[48..56].copy_from_slice(&4u64.to_le_bytes());
    boot[56..64].copy_from_slice(&8u64.to_le_bytes());
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    boot
}

fn mutate_lba12_u32(
    image: &[u8],
    device_id: &str,
    entry: usize,
    field_offset: usize,
    value: u32,
) -> Vec<u8> {
    let mut out = image.to_vec();
    let crc = crc32_bare(device_id.as_bytes()).to_le_bytes();
    let mut plain = a6b0_full(&out[12 * SECTOR..13 * SECTOR], &crc, 0);
    let offset = entry * 0x60 + field_offset;
    plain[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    out[12 * SECTOR..13 * SECTOR].copy_from_slice(&a7f0_full(&plain, &crc, 0));
    out
}

fn insert_protocol(dev: &mut ReadOnlySparseDev, image: &[u8]) {
    for lba in 0..13u64 {
        let start = lba as usize * SECTOR;
        dev.insert(lba, image[start..start + SECTOR].to_vec());
    }
}

fn patterned_sector(byte: u8) -> Vec<u8> {
    vec![byte; SECTOR]
}

fn rewrite_lexar_lba7_compatibility_pointer(image: &[u8], start_lba: u64) -> Vec<u8> {
    let mut out = image.to_vec();
    let crc = crc32_bare(LEXAR_DEVICE_ID.as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let mut plain = xor_rolling(&out[7 * SECTOR..8 * SECTOR], k0);
    for entry in [1usize, 2usize] {
        let offset = entry * 0x40 + 0x18;
        plain[offset..offset + 8].copy_from_slice(&start_lba.to_le_bytes());
    }
    let wire = xor_rolling(&plain, k0);
    out[7 * SECTOR..8 * SECTOR].copy_from_slice(&wire);
    out
}

fn core<'a>(image: &'a [u8]) -> CoreCapture<'a> {
    CoreCapture {
        snapshot_id: "metadata-test".into(),
        created_epoch: 1_790_000_000,
        disk_number: Some(6),
        vid: "0dd8".into(),
        pid: "2005".into(),
        device_id: NETAC_DEVICE_ID.into(),
        onlyid: Some("1402259934".into()),
        total_sectors: Some(NETAC_TOTAL_SECTORS),
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: image,
    }
}

fn test_mbr(partition_type: u8, start_lba: u32, sector_count: u32) -> Vec<u8> {
    let mut raw = vec![0u8; SECTOR];
    raw[446 + 4] = partition_type;
    raw[446 + 8..446 + 12].copy_from_slice(&start_lba.to_le_bytes());
    raw[446 + 12..446 + 16].copy_from_slice(&sector_count.to_le_bytes());
    raw[510..512].copy_from_slice(&[0x55, 0xaa]);
    raw
}

fn test_crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn test_gpt_header(
    current_lba: u64,
    backup_lba: u64,
    entries_lba: u64,
    total: u64,
    entries_crc: u32,
) -> Vec<u8> {
    let mut raw = vec![0u8; SECTOR];
    raw[..8].copy_from_slice(b"EFI PART");
    raw[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    raw[12..16].copy_from_slice(&92u32.to_le_bytes());
    raw[24..32].copy_from_slice(&current_lba.to_le_bytes());
    raw[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    raw[40..48].copy_from_slice(&34u64.to_le_bytes());
    raw[48..56].copy_from_slice(&(total - 34).to_le_bytes());
    raw[56..72].copy_from_slice(&[0x44; 16]);
    raw[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    raw[80..84].copy_from_slice(&4u32.to_le_bytes());
    raw[84..88].copy_from_slice(&128u32.to_le_bytes());
    raw[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let crc = test_crc32_ieee(&raw[..92]);
    raw[16..20].copy_from_slice(&crc.to_le_bytes());
    raw
}

#[test]
fn chapter_18_b2_plain_mbr_capture_reads_only_partition_metadata() {
    let total = 100_000u64;
    let mut dev = ReadOnlySparseDev::new();
    dev.insert(0, test_mbr(0x07, 2_048, 50_000));
    let mut exfat = patterned_sector(0xEE);
    exfat[3..11].copy_from_slice(b"EXFAT   ");
    dev.insert(2_048, exfat);

    let capture = acquire_plain_metadata(&mut dev, total).unwrap();

    assert_eq!(dev.writes, 0);
    assert!(
        !dev.reads.contains(&2_048),
        "filesystem boot sector must not be read"
    );
    assert_eq!(capture.partitions.len(), 1);
    assert_eq!(capture.partitions[0].start_lba, 2_048);
    assert_eq!(capture.partitions[0].sector_count, 50_000);
    assert_eq!(capture.artifacts.len(), 1);
    assert_eq!(
        capture.artifacts[0].restore_policy,
        RestorePolicy::Restorable
    );
    assert_eq!(capture.artifacts[0].data, dev.sectors[&0]);
    assert!(capture
        .artifacts
        .iter()
        .all(|artifact| !artifact.id.contains("filesystem")));
}

#[test]
fn chapter_18_b2_plain_gpt_capture_requires_primary_and_backup_metadata_only() {
    let total = 100_000u64;
    let mut entries = vec![0u8; SECTOR];
    entries[..16].copy_from_slice(&[
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ]);
    entries[16..32].copy_from_slice(&[0x11; 16]);
    entries[32..40].copy_from_slice(&2_048u64.to_le_bytes());
    entries[40..48].copy_from_slice(&80_000u64.to_le_bytes());
    let entries_crc = test_crc32_ieee(&entries);

    let mut dev = ReadOnlySparseDev::new();
    dev.insert(0, test_mbr(0xee, 1, (total - 1) as u32));
    dev.insert(1, test_gpt_header(1, total - 1, 2, total, entries_crc));
    dev.insert(2, entries.clone());
    dev.insert(total - 2, entries.clone());
    dev.insert(
        total - 1,
        test_gpt_header(total - 1, 1, total - 2, total, entries_crc),
    );
    let mut exfat = patterned_sector(0xEE);
    exfat[3..11].copy_from_slice(b"EXFAT   ");
    dev.insert(2_048, exfat);

    let capture = acquire_plain_metadata(&mut dev, total).unwrap();

    assert_eq!(dev.writes, 0);
    assert!(
        !dev.reads.contains(&2_048),
        "filesystem boot sector must not be read"
    );
    assert_eq!(capture.partitions.len(), 1);
    assert_eq!(capture.artifacts.len(), 5);
    for lba in [0u32, 1, 2, (total - 2) as u32, (total - 1) as u32] {
        assert!(
            dev.reads.contains(&lba),
            "missing GPT metadata read at LBA{lba}"
        );
    }
    assert!(capture
        .artifacts
        .iter()
        .all(|artifact| artifact.restore_policy == RestorePolicy::Restorable));
}

#[test]
fn authentic_lba12_yields_bounded_partition_geometry() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let partitions =
        parse_partition_geometry(&image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();

    assert!((1..=3).contains(&partitions.len()));
    assert!(partitions.iter().all(|partition| {
        partition.sector_size == SECTOR as u64
            && partition.sector_count > 0
            && partition.start_sector + partition.sector_count <= NETAC_TOTAL_SECTORS
    }));
    let type4 = partitions
        .iter()
        .find(|partition| partition.partition_type == 4)
        .expect("authentic Netac image must describe type4");
    assert_eq!(type4.start_sector, 116_707_328);
    assert_eq!(type4.partition_size, 3_143_761_920);
}

#[test]
fn authentic_lexar_lba7_points_to_six_sector_compatibility_extent() {
    let Some(image) = load_disk_image("lexar") else {
        eprintln!("跳过: 真实 Lexar 协议夹具不可用");
        return;
    };
    let compat =
        parse_lba7_compatibility_geometry(&image, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    assert_eq!(compat.start_lba, LEXAR_LBA7_COMPAT_START);
    assert_eq!(compat.sector_count, LBA7_COMPAT_EXTENT_SECTORS);
    assert_eq!(compat.chs_expected_start_lba, Some(LEXAR_LBA7_COMPAT_START));
    assert_eq!(
        compat.official_partition_mode.as_deref(),
        Some("0 (缺省三分区)")
    );
    assert_eq!(compat.lba7_pointer_entries.len(), 2);
    assert_eq!(compat.lba7_pointer_entries[0].entry_index, 1);
    assert_eq!(compat.lba7_pointer_entries[0].partition_type, 2);
    assert_eq!(
        compat.lba7_pointer_entries[0].partition_role.as_deref(),
        Some("share")
    );
    assert_eq!(compat.lba7_pointer_entries[1].entry_index, 2);
    assert_eq!(compat.lba7_pointer_entries[1].partition_type, 4);
    assert_eq!(
        compat.lba7_pointer_entries[1].partition_role.as_deref(),
        Some("encrypt")
    );
}

#[test]
fn metadata_capture_reads_complete_lba7_compatibility_extent_without_magic_tail_window() {
    let Some(image) = load_disk_image("lexar") else {
        eprintln!("跳过: 真实 Lexar 协议夹具不可用");
        return;
    };
    let mut dev = ReadOnlySparseDev::new();
    let mut expected = Vec::new();
    for offset in 0..LBA7_COMPAT_EXTENT_SECTORS {
        let sector = patterned_sector(0x40 + offset as u8);
        expected.extend_from_slice(&sector);
        dev.insert(LEXAR_LBA7_COMPAT_START + offset, sector);
    }

    let acquired =
        acquire_metadata(&mut dev, &image, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    let compat = acquired
        .regions
        .iter()
        .find(|region| region.id == "region.lba7_compatibility_extent")
        .expect("LBA7 compatibility extent must be modeled explicitly");
    assert_eq!(compat.semantic_status, SemanticStatus::Identified);
    assert_eq!(compat.start_lba, Some(LEXAR_LBA7_COMPAT_START));
    assert_eq!(compat.sector_count, Some(LBA7_COMPAT_EXTENT_SECTORS));

    let raw_compat = acquired
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "raw.lba7_compatibility")
        .expect("raw LBA7 compatibility extent artifact");
    assert_eq!(raw_compat.restore_policy, RestorePolicy::Restorable);
    assert_eq!(raw_compat.data, expected);

    let layout = acquired
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "derived.lba7_compatibility.layout")
        .expect("LBA7 compatibility extent layout artifact");
    let layout_json: serde_json::Value = serde_json::from_slice(&layout.data).unwrap();
    assert_eq!(layout_json["wire_semantics"]["offset"], 0);
    assert_eq!(layout_json["wire_semantics"]["length"], 3072);
    assert_eq!(
        layout_json["wire_semantics"]["classification"],
        "fixed_fat16_compatibility_image_encrypted"
    );
    assert_eq!(layout_json["official_partition_mode"], "0 (缺省三分区)");
    assert_eq!(
        layout_json["pointer_entries"],
        serde_json::json!([
            {
                "entry_index": 1,
                "partition_type": 2,
                "partition_role": "share"
            },
            {
                "entry_index": 2,
                "partition_type": 4,
                "partition_role": "encrypt"
            }
        ])
    );
    assert_eq!(
        layout_json["wire_semantics"]["plaintext_sha256"],
        "386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c"
    );
    assert!(layout_json.get("iir_binding").is_none());
}

#[test]
fn lba7_compatibility_pointer_remains_authoritative_when_chs_cross_check_differs() {
    let Some(image) = load_disk_image("lexar") else {
        eprintln!("跳过: 真实 Lexar 协议夹具不可用");
        return;
    };
    let altered_start = LEXAR_LBA7_COMPAT_START - 1;
    let altered = rewrite_lexar_lba7_compatibility_pointer(&image, altered_start);
    let parsed =
        parse_lba7_compatibility_geometry(&altered, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    assert_eq!(parsed.start_lba, altered_start);
    assert_eq!(parsed.chs_expected_start_lba, Some(LEXAR_LBA7_COMPAT_START));

    let mut dev = ReadOnlySparseDev::new();
    let acquired =
        acquire_metadata(&mut dev, &altered, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    let compat = acquired
        .regions
        .iter()
        .find(|region| region.id == "region.lba7_compatibility_extent")
        .unwrap();
    assert_eq!(compat.start_lba, Some(altered_start));
    assert!(acquired.issues.iter().any(|issue| {
        issue.region_id == "region.lba7_compatibility_extent"
            && issue.start_lba == altered_start
            && issue.error.contains("differs from CHS-1792 expected")
    }));
}

#[test]
fn chapter_18_b5_plain_invalid_boot_is_typed_needs_format_without_writes() {
    use edpcli::application::post_restore::{
        assess_partitions_readonly, PostRestorePartitionState,
    };
    use edpcli::edpb::ManifestPartition;

    let mut dev = ReadOnlySparseDev::new();
    dev.insert(2_048, vec![0u8; SECTOR]);
    let partitions = vec![ManifestPartition {
        index: 1,
        role: Some("mbr_primary".into()),
        partition_type: Some("mbr:0x07".into()),
        start_lba: 2_048,
        sector_count: 50_000,
        filesystem_hint: Some("exfat".into()),
        volume_label_hint: None,
    }];

    let assessment =
        assess_partitions_readonly(&mut dev, "plain", "", 100_000, &partitions).unwrap();

    assert_eq!(dev.writes, 0);
    assert_eq!(assessment.partitions.len(), 1);
    assert_eq!(
        assessment.partitions[0].state,
        PostRestorePartitionState::NeedsFormat
    );
    assert_eq!(
        assessment.partitions[0].filesystem_hint.as_deref(),
        Some("exfat")
    );
}

#[test]
fn chapter_18_b5_plain_valid_boot_is_typed_usable_without_writes() {
    use edpcli::application::post_restore::{
        assess_partitions_readonly, PostRestorePartitionState,
    };
    use edpcli::edpb::ManifestPartition;

    let mut dev = ReadOnlySparseDev::new();
    dev.insert(2_048, strict_ntfs_boot(50_000));
    let partitions = vec![ManifestPartition {
        index: 1,
        role: Some("mbr_primary".into()),
        partition_type: Some("mbr:0x07".into()),
        start_lba: 2_048,
        sector_count: 50_000,
        filesystem_hint: Some("ntfs".into()),
        volume_label_hint: None,
    }];

    let assessment =
        assess_partitions_readonly(&mut dev, "plain", "", 100_000, &partitions).unwrap();

    assert_eq!(dev.writes, 0);
    assert_eq!(
        assessment.partitions[0].state,
        PostRestorePartitionState::Usable
    );
    assert_eq!(
        assessment.partitions[0]
            .detected_filesystem
            .map(|value| value.label()),
        Some("NTFS")
    );
}

#[test]
fn chapter_18_b5_edp_nondefault_password_is_typed_password_required() {
    use edpcli::application::post_restore::{
        assess_partitions_readonly, PostRestorePartitionState,
    };

    let Some(image) = load_disk_image("lexar") else {
        eprintln!("跳过: Lexar 协议夹具不可用");
        return;
    };
    let geometry = parse_partition_geometry(&image, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    let encrypted_index = geometry
        .iter()
        .position(|partition| partition.need_encrypt != 0)
        .expect("Lexar fixture must contain encrypted partition");
    let mutated = mutate_lba12_u32(
        &image,
        LEXAR_DEVICE_ID,
        encrypted_index,
        0x30,
        crc32_bare(b"original-password"),
    );
    let acquisition = acquire_metadata(
        &mut ReadOnlySparseDev::new(),
        &mutated,
        LEXAR_DEVICE_ID,
        LEXAR_TOTAL_SECTORS,
    )
    .unwrap();
    let mut dev = ReadOnlySparseDev::new();
    insert_protocol(&mut dev, &mutated);

    let assessment = assess_partitions_readonly(
        &mut dev,
        "edp",
        LEXAR_DEVICE_ID,
        LEXAR_TOTAL_SECTORS,
        &acquisition.partitions,
    )
    .unwrap();

    assert_eq!(dev.writes, 0);
    let encrypted_start = geometry[encrypted_index].start_sector;
    let encrypted = assessment
        .partitions
        .iter()
        .find(|partition| partition.start_lba == encrypted_start)
        .expect("encrypted partition assessment");
    assert_eq!(encrypted.state, PostRestorePartitionState::PasswordRequired);
}

#[test]
fn chapter_18_b5_edp_bad_file_key_crc_is_typed_crypto_metadata_invalid() {
    use edpcli::application::post_restore::{
        assess_partitions_readonly, PostRestorePartitionState,
    };

    let Some(image) = load_disk_image("lexar") else {
        eprintln!("跳过: Lexar 协议夹具不可用");
        return;
    };
    let geometry = parse_partition_geometry(&image, LEXAR_DEVICE_ID, LEXAR_TOTAL_SECTORS).unwrap();
    let encrypted_index = geometry
        .iter()
        .position(|partition| partition.need_encrypt != 0)
        .expect("Lexar fixture must contain encrypted partition");
    let bad_crc = geometry[encrypted_index].file_key_crc ^ 1;
    let mutated = mutate_lba12_u32(&image, LEXAR_DEVICE_ID, encrypted_index, 0x34, bad_crc);
    let acquisition = acquire_metadata(
        &mut ReadOnlySparseDev::new(),
        &mutated,
        LEXAR_DEVICE_ID,
        LEXAR_TOTAL_SECTORS,
    )
    .unwrap();
    let mut dev = ReadOnlySparseDev::new();
    insert_protocol(&mut dev, &mutated);

    let assessment = assess_partitions_readonly(
        &mut dev,
        "edp",
        LEXAR_DEVICE_ID,
        LEXAR_TOTAL_SECTORS,
        &acquisition.partitions,
    )
    .unwrap();

    let encrypted_start = geometry[encrypted_index].start_sector;
    let encrypted = assessment
        .partitions
        .iter()
        .find(|partition| partition.start_lba == encrypted_start)
        .expect("encrypted partition assessment");
    assert_eq!(
        encrypted.state,
        PostRestorePartitionState::CryptoMetadataInvalid
    );
}

#[test]
fn chapter_18_b5_edp_without_device_id_is_typed_unsupported() {
    use edpcli::application::post_restore::{
        assess_partitions_readonly, PostRestorePartitionState,
    };
    use edpcli::edpb::ManifestPartition;

    let partitions = vec![ManifestPartition {
        index: 1,
        role: Some("encrypt".into()),
        partition_type: Some("edp:4".into()),
        start_lba: 20_417,
        sector_count: 100_000,
        filesystem_hint: None,
        volume_label_hint: None,
    }];
    let mut dev = ReadOnlySparseDev::new();
    let assessment = assess_partitions_readonly(&mut dev, "edp", "", 200_000, &partitions).unwrap();

    assert_eq!(dev.writes, 0);
    assert_eq!(
        assessment.partitions[0].state,
        PostRestorePartitionState::Unsupported
    );
}

#[test]
fn chapter_18_b3_edp_metadata_capture_excludes_filesystem_and_keeps_protocol_extents() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let partitions =
        parse_partition_geometry(&image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();
    let sample_partition = &partitions[0];

    let mut dev = ReadOnlySparseDev::new();
    dev.insert(sample_partition.start_sector, ntfs_boot(4, 8));

    let acquired =
        acquire_metadata(&mut dev, &image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();

    assert_eq!(dev.writes, 0);
    assert!(
        !dev.reads
            .contains(&u32::try_from(sample_partition.start_sector).unwrap()),
        "default EDP metadata backup must not read partition filesystem boot sectors"
    );
    assert_eq!(acquired.partitions.len(), partitions.len());
    assert!(acquired.artifacts.iter().all(|artifact| {
        !artifact.id.contains("filesystem")
            && !artifact.id.contains("prefix")
            && !artifact.id.contains("suffix")
            && !artifact.id.contains("fskey")
            && !artifact.kind.contains("filesystem")
    }));

    for id in [
        "raw.lba7_compatibility",
        "raw.tail.metadata_mirror_512k",
        "raw.tail.restore_node_end4",
    ] {
        let artifact = acquired
            .artifacts
            .iter()
            .find(|artifact| artifact.id == id)
            .unwrap_or_else(|| panic!("missing protocol/recovery artifact {id}"));
        assert_eq!(
            artifact.restore_policy,
            RestorePolicy::Restorable,
            "{id} must remain exact restorable protocol/recovery metadata"
        );
    }
}

#[test]
fn legacy_deep_filesystem_evidence_is_only_read_when_explicitly_requested() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let partitions =
        parse_partition_geometry(&image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();
    let fs_partition = partitions
        .iter()
        .find(|partition| matches!(partition.partition_type, 1 | 2))
        .unwrap_or(&partitions[0]);

    let mut dev = ReadOnlySparseDev::new();
    dev.insert(fs_partition.start_sector, ntfs_boot(4, 8));

    let acquired =
        acquire_metadata(&mut dev, &image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();
    assert_eq!(dev.writes, 0);
    assert!(
        !dev.reads
            .contains(&u32::try_from(fs_partition.start_sector).unwrap()),
        "metadata-only path must not read the filesystem"
    );
    let deep =
        edpcli::backup_deep::acquire_deep(&mut dev, &image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS)
            .unwrap();
    assert!(
        dev.reads
            .contains(&u32::try_from(fs_partition.start_sector).unwrap()),
        "explicit Deep compatibility capture may read filesystem evidence"
    );

    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.tail.metadata_mirror_512k"));
    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.tail.restore_node_end4"));

    let prefix = deep
        .extents
        .iter()
        .find(|extent| {
            extent.start_lba == fs_partition.start_sector
                && extent.purpose == "partition_metadata_prefix"
        })
        .unwrap();
    assert_eq!(
        prefix.sector_count,
        fs_partition.sector_count.min(PARTITION_PREFIX_SECTORS)
    );
    let probe_artifact = deep
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.id == format!("derived.partition.{}.filesystem_probe", fs_partition.index)
        })
        .unwrap();
    let probe: serde_json::Value = serde_json::from_slice(&probe_artifact.data).unwrap();
    assert_eq!(probe["kind"], "ntfs");
    assert!(probe["key_lbas"].as_array().unwrap().len() >= 2);
}

#[test]
fn metadata_container_only_marks_protocol_and_validated_lce_restorable() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let mut dev = ReadOnlySparseDev::new();
    let acquisition =
        acquire_metadata(&mut dev, &image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();
    let tmp = TempDir::new("metadata_edpb");
    let path = tmp.0.join("metadata.edpb");
    let capture = MetadataCapture {
        core: core(&image),
        partitions: acquisition.partitions,
        regions: acquisition.regions,
        extents: acquisition.extents,
        artifacts: acquisition.artifacts,
        notes: acquisition.notes,
    };
    edpb::write_metadata_backup(&path, &capture).unwrap();

    let verified = edpb::verify_file(&path).unwrap();
    assert_eq!(
        verified.manifest.snapshot.capture_level,
        CaptureLevel::Metadata
    );
    assert!(verified.manifest.artifacts.len() > 5);
    let raw_protocol = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == edpb::RAW_PROTOCOL_ARTIFACT_ID)
        .unwrap();
    assert_eq!(raw_protocol.restore_policy, RestorePolicy::Restorable);
    let restorable_ids = verified
        .manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.restore_policy == RestorePolicy::Restorable)
        .map(|artifact| artifact.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        restorable_ids,
        vec![
            edpb::RAW_PROTOCOL_ARTIFACT_ID,
            "raw.lba7_compatibility",
            "raw.tail.metadata_mirror_512k",
            "raw.tail.restore_node_end4",
        ]
    );
    assert_eq!(edpb::read_raw_protocol(&path).unwrap(), image);
}

#[test]
fn metadata_container_detects_corruption_in_non_protocol_artifact() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let mut dev = ReadOnlySparseDev::new();
    let acquisition =
        acquire_metadata(&mut dev, &image, NETAC_DEVICE_ID, NETAC_TOTAL_SECTORS).unwrap();
    let tmp = TempDir::new("metadata_corrupt");
    let path = tmp.0.join("metadata.edpb");
    let capture = MetadataCapture {
        core: core(&image),
        partitions: acquisition.partitions,
        regions: acquisition.regions,
        extents: acquisition.extents,
        artifacts: acquisition.artifacts,
        notes: acquisition.notes,
    };
    let manifest = edpb::write_metadata_backup(&path, &capture).unwrap();
    let extra = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "raw.lba7_compatibility")
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes[extra.storage.data_offset as usize + 7] ^= 0x5a;
    fs::write(&path, bytes).unwrap();
    let error = edpb::verify_file(&path).unwrap_err();
    assert!(error.contains("raw.lba7_compatibility"));
    assert!(error.contains("SHA-256"));
}

#[test]
fn filesystem_kind_serialization_is_stable() {
    assert_eq!(
        serde_json::to_value(&FilesystemKind::Ntfs).unwrap(),
        serde_json::Value::String("ntfs".into())
    );
}
