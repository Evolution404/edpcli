use crate::common;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use common::load_disk_image;
use edpcli::backup_metadata::{
    acquire_metadata, parse_lba7_compatibility_geometry, parse_partition_geometry, FilesystemKind,
    DEVICE_TAIL_WINDOW_SECTORS, LBA7_COMPAT_EXTENT_SECTORS, PARTITION_PREFIX_SECTORS,
};
use edpcli::common::SECTOR;
use edpcli::crypto::{crc32_bare, xor_rolling};
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
fn metadata_capture_reads_complete_lba7_compatibility_extent_and_separate_tail_window() {
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

    let tail = acquired
        .regions
        .iter()
        .find(|region| region.id == "region.device_tail_window")
        .expect("device tail forensic window");
    assert_eq!(tail.semantic_status, SemanticStatus::Unknown);
    assert_eq!(tail.sector_count, Some(DEVICE_TAIL_WINDOW_SECTORS));
    assert_eq!(
        tail.start_lba,
        Some(LEXAR_TOTAL_SECTORS - DEVICE_TAIL_WINDOW_SECTORS)
    );
    assert_ne!(tail.start_lba, compat.start_lba);
    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.device_tail_window"
            && artifact.restore_policy == RestorePolicy::EvidenceOnly));

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
fn metadata_capture_reads_partition_key_sectors_and_tail_evidence_without_writes() {
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
    assert!(!dev.reads.is_empty());

    let tail = acquired
        .regions
        .iter()
        .find(|region| region.id == "region.device_tail_window")
        .expect("device tail forensic window");
    assert_eq!(tail.semantic_status, SemanticStatus::Unknown);
    assert_eq!(tail.sector_count, Some(DEVICE_TAIL_WINDOW_SECTORS));
    assert_eq!(
        tail.start_lba,
        Some(NETAC_TOTAL_SECTORS - DEVICE_TAIL_WINDOW_SECTORS)
    );
    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.device_tail_window"
            && artifact.restore_policy == RestorePolicy::EvidenceOnly));
    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.tail.metadata_mirror_512k"));
    assert!(acquired
        .artifacts
        .iter()
        .any(|artifact| artifact.id == "raw.tail.restore_node_end4"));

    let prefix = acquired
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
    let probe_artifact = acquired
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
        vec![edpb::RAW_PROTOCOL_ARTIFACT_ID, "raw.lba7_compatibility"]
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
