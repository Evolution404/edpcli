//! 备份/还原体系测试：命名解析、按盘匹配、备份落盘与保留策略。
//! 全部纯文件系统操作 + 注入 DiskFacts/FixedClock, 不碰真盘。

use crate::common;
use crate::gold_name::parse_fixture_backup_name;

use std::fs;

use common::*;
use edpcli::application::media_identity::{
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentitySnapshot,
    ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::application::support::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::cli::{backup_delete, backup_list, backup_prune, backup_verify};
use edpcli::edpb::{
    self, ArtifactCompleteness, ArtifactInput, CoreCapture, Extent, ManifestPartition,
    MetadataCapture, Region, RestorePolicy, SemanticStatus,
};
use edpcli::infrastructure::backup_store::catalog::{
    prune_candidates, scan_backup_dir_checked, BackupEntry, BackupIntegrityStatus, BackupMeta,
    DiskFacts,
};
use edpcli::infrastructure::backup_store::create::{create_metadata_backup, find_backups};
use edpcli::ports::Clock;

struct FixedClock;
impl Clock for FixedClock {
    fn now_epoch(&self) -> i64 {
        1789603200
    }
    fn fmt_ts(&self, _epoch: i64) -> String {
        "20260917_000000".into()
    }
    fn fmt_human(&self, _epoch: i64) -> String {
        "2026-09-17 00:00".into()
    }
}

fn netac_facts() -> DiskFacts {
    DiskFacts {
        disk: 99,
        total_sectors: Some(122880000),
        vid: "0dd8".into(),
        pid: "2005".into(),
    }
}

fn current_identity(data: &[u8], device_id: &str) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            serial: None,
            serial_sha256: None,
            serial_quality: SerialQuality::Missing,
            vendor: None,
            product: None,
            revision: None,
            transport: None,
            total_sectors: Some(122_880_000),
            logical_sector_size: Some(SECTOR as u32),
        },
        protocol: ProtocolIdentityEvidence {
            device_id: Some(device_id.to_string()),
            onlyid: edpcli::infrastructure::backup_store::catalog::lba4_label_id_from(
                &data[4 * SECTOR..5 * SECTOR],
            ),
            provision_kind: edpcli::provision::DiskProvisionKind::from_metadata(data, device_id),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    }
}

fn create_test_metadata_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    root: &std::path::Path,
    clock: &dyn Clock,
) -> Result<std::path::PathBuf, edpcli::application::support::EdpCliError> {
    create_metadata_backup(
        facts,
        data,
        device_id,
        Default::default(),
        &current_identity(data, device_id),
        root,
        clock,
    )
}

fn write_backup(dir: &std::path::Path, name: &str, data: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    let meta = parse_fixture_backup_name(name).expect("测试 EDPB 文件名必须可解析");
    let onlyid = edpcli::infrastructure::backup_store::catalog::lba4_label_id_from(
        &data[4 * SECTOR..5 * SECTOR],
    );
    let capture = CoreCapture {
        snapshot_id: format!("test-{name}"),
        created_epoch: named_test_capture_epoch(&p),
        disk_number: Some(meta.disk),
        vid: meta.vid.clone(),
        pid: meta.pid.clone(),
        device_id: meta.device_id.clone(),
        onlyid,
        total_sectors: meta.secs,
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "edp".into(),
        lba0_12: data,
    };
    edpb::write_core_backup(&p, &capture).unwrap();
    p
}

fn write_plain_metadata_v3(
    dir: &std::path::Path,
    name: &str,
    total_sectors: u64,
) -> std::path::PathBuf {
    let path = dir.join(name);
    let legacy_projection = "disk&ven_test&prod_plain";
    let protocol_placeholder = vec![0u8; METADATA_IMAGE_LEN];
    let region_id = "region.partition_table.mbr".to_string();
    let extent_id = "extent.partition_table.mbr".to_string();
    let capture = MetadataCapture {
        core: CoreCapture {
            snapshot_id: format!("plain-{name}"),
            created_epoch: 1_789_000_100,
            disk_number: Some(5),
            vid: "2bdf".into(),
            pid: "0300".into(),
            device_id: legacy_projection.into(),
            onlyid: None,
            total_sectors: Some(total_sectors),
            logical_sector_size: SECTOR as u32,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "plain".into(),
            lba0_12: &protocol_placeholder,
        },
        partitions: vec![ManifestPartition {
            index: 1,
            role: Some("mbr_primary".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba: 2_048,
            sector_count: total_sectors - 2_048,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        }],
        regions: vec![Region {
            id: region_id.clone(),
            role: "plain_partition_table".into(),
            start_lba: Some(0),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        }],
        extents: vec![Extent {
            id: extent_id.clone(),
            region_id,
            start_lba: 0,
            sector_count: 1,
            purpose: "mbr_partition_table".into(),
        }],
        artifacts: vec![ArtifactInput {
            id: "raw.partition_table.mbr".into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            data: {
                let mut mbr = vec![0u8; SECTOR];
                let entry = 0x1be;
                mbr[entry + 4] = 0x07;
                mbr[entry + 8..entry + 12].copy_from_slice(&2_048u32.to_le_bytes());
                mbr[entry + 12..entry + 16]
                    .copy_from_slice(&u32::try_from(total_sectors - 2_048).unwrap().to_le_bytes());
                mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
                mbr
            },
        }],
        notes: Vec::new(),
    };
    edpb::write_metadata_backup(&path, &capture).unwrap();
    path
}

#[test]
fn edpb_backup_label_id_comes_from_raw_lba4() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("edpb_label_id");
    let path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_20260910_172300.edpb",
        &data,
    );
    let raw = edpb::read_raw_protocol(&path).unwrap();
    assert_eq!(
        edpcli::infrastructure::backup_store::catalog::lba4_label_id_from(
            &raw[4 * SECTOR..5 * SECTOR]
        )
        .as_deref(),
        Some("1402259934")
    );
}

#[test]
fn find_backups_lba4_final_filter() {
    let (netac, lexar, real_bin) = (
        load_disk_image("netac"),
        load_disk_image("lexar"),
        fixture_bin("netac"),
    );
    let tmp = TmpDir::new("find");
    // 同型号模式但 LBA4 是他盘(lexar 内容)的备份 → 被 LBA4 终验剔除
    let real = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
        &netac,
    );
    let _fake = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_20260910_173000.edpb",
        &lexar,
    );
    let current = current_identity(&netac, "disk&ven_netac&prod_onlydisk");
    let found = find_backups(&tmp.0, &current).unwrap();
    assert_eq!(found.confirmed, vec![real.clone()]);
    assert!(found.possible.is_empty());

    // 空目录 → 空
    let empty = TmpDir::new("find_empty");
    let empty_matches = find_backups(&empty.0, &current).unwrap();
    assert!(empty_matches.confirmed.is_empty());
    assert!(empty_matches.possible.is_empty());
    let _ = real_bin;
}

#[test]
fn backup_written_as_single_edpb_with_internal_hashes_and_onlyid() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("backup");
    let path = create_test_metadata_backup(
        &netac_facts(),
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("_onlyid1402259934_"), "{}", name);
    assert!(!name.contains("_mode1"), "{}", name);
    assert_eq!(edpb::read_raw_protocol(&path).unwrap(), data); // LBA0-12 全量
    let verified = edpb::verify_file(&path).unwrap();
    assert_eq!(
        verified.manifest.device.onlyid.as_deref(),
        Some("1402259934")
    );
    assert!(!std::path::PathBuf::from(format!("{}.sha256", path.display())).exists());
    // 备份可被 typed affinity 找回
    let current = current_identity(&data, "disk&ven_netac&prod_onlydisk");
    let found = find_backups(&tmp.0, &current).unwrap();
    assert_eq!(found.confirmed, vec![path]);
    assert!(found.possible.is_empty());
}

#[test]
fn backup_rejects_incomplete_lba_image_before_creating_files() {
    let mut data = load_disk_image("netac");
    data.pop();
    let tmp = TmpDir::new("backup_short_image");
    let result = create_test_metadata_backup(
        &netac_facts(),
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    );
    assert!(
        result.is_err(),
        "备份输入必须恰好为 LBA0-12 共 {METADATA_IMAGE_LEN}B"
    );
    assert_eq!(fs::read_dir(&tmp.0).map(|it| it.count()).unwrap_or(0), 0);
}

#[test]
fn backup_filename_onlyid_is_derived_from_lba4_content() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("backup_content_onlyid");
    let facts = netac_facts();
    let path = create_test_metadata_backup(
        &facts,
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    assert!(name.contains("_onlyid1402259934_"), "{name}");
    assert!(!name.contains("_onlyid999999999_"), "{name}");
}

#[test]
fn find_backups_ignores_matching_non_bin_files() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("find_only_bin");
    let bin = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
        &data,
    );
    fs::write(
        tmp.0.join(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172301.txt",
        ),
        &data,
    )
    .unwrap();
    let current = current_identity(&data, "disk&ven_netac&prod_onlydisk");
    let found = find_backups(&tmp.0, &current).unwrap();
    assert_eq!(found.confirmed, vec![bin]);
    assert!(found.possible.is_empty());
}

#[test]
fn find_backups_prefers_device_id_tier_before_generic_fallback() {
    let tmp = TmpDir::new("find_tier_priority");
    let mut data = vec![0u8; METADATA_IMAGE_LEN];
    data[4 * SECTOR..4 * SECTOR + 7].copy_from_slice(b"$$$1$$$");

    let exact = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1_20260910_172300.edpb",
        &data,
    );
    let _generic_only = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_other&prod_other_onlyid1_20260910_172301.edpb",
        &data,
    );

    let current = current_identity(&data, "disk&ven_netac&prod_onlydisk");
    let found = find_backups(&tmp.0, &current).unwrap();
    assert_eq!(found.confirmed, vec![exact]);
    assert!(found.possible.is_empty());
}

#[test]
fn backup_rejects_device_id_with_path_separators_before_creating_files() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("backup_device_id_path_escape");
    let bak = tmp.0.join("bak");
    fs::create_dir_all(&bak).unwrap();
    // 旧实现会把 device_id 原样拼进文件名。预建第一层目录后，`/../../` 可逃出 bak。
    fs::create_dir_all(bak.join("disk99_122880000_vid0dd8_pid2005_disk&ven_bad")).unwrap();
    let result = create_test_metadata_backup(
        &netac_facts(),
        &data,
        "disk&ven_bad/../../escaped",
        &bak,
        &FixedClock,
    );
    assert!(result.is_err(), "不安全 device_id 必须在路径构造前拒绝");
    let escaped = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("edpb"))
        .collect::<Vec<_>>();
    assert!(escaped.is_empty(), "备份目录外不得生成文件: {escaped:?}");
}

#[test]
fn creating_new_backup_does_not_rename_existing_history() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("backup_no_implicit_migrate");
    let legacy = tmp
        .0
        .join("disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin");
    fs::write(&legacy, &data).unwrap();
    let legacy_sha256 = std::path::PathBuf::from(format!("{}.sha256", legacy.display()));
    fs::write(&legacy_sha256, format!("{}\n", sha256(&data))).unwrap();

    let new_path = create_test_metadata_backup(
        &netac_facts(),
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();

    assert!(legacy.exists(), "创建新备份不应隐式迁移或重命名历史 .bin");
    assert!(legacy_sha256.exists(), "创建新备份不应修改历史 .sha256");
    assert!(new_path.exists());
    assert_ne!(new_path, legacy);
    assert!(scan_backup_dir_checked(&tmp.0)
        .unwrap()
        .iter()
        .all(|entry| entry.path != legacy));
}

#[test]
fn backup_collision_never_overwrites_existing_file() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("backup_collision");
    let path = create_test_metadata_backup(
        &netac_facts(),
        &original,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();
    let first = fs::read(&path).unwrap();

    // 保持同一时间戳和同一“加密原盘”状态，只改一个与识别无关的保留扇区字节。
    let mut second = original.clone();
    second[2 * 512 + 17] ^= 0x5A;
    let err = create_test_metadata_backup(
        &netac_facts(),
        &second,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap_err();

    assert!(
        err.msg.contains("已存在") || err.msg.contains("exists"),
        "{}",
        err.msg
    );
    assert_eq!(fs::read(&path).unwrap(), first, "同名备份绝不能被静默覆盖");
    assert!(edpb::verify_file(&path).is_ok());
}

#[test]
fn plain_v3_metadata_opens_as_evidence_without_protocol_core() {
    use edpcli::application::evidence::{EvidenceSource, SectorReader};

    let tmp = TmpDir::new("plain_v3_evidence");
    let total_sectors = 245_760_000u64;
    let path = write_plain_metadata_v3(
        &tmp.0,
        "disk5_245760000_vid2bdf_pid0300_plain_20260929_073104.edpb",
        total_sectors,
    );

    let mut evidence =
        EvidenceSource::open_backup(&path).expect("Plain v3 evidence source must open");
    assert_eq!(
        evidence.identity().provision_kind,
        Some(edpcli::provision::DiskProvisionKind::Plain)
    );
    let mbr = SectorReader::read_sector(&mut evidence, 0).expect("captured MBR");
    assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
    assert!(
        SectorReader::read_sector(&mut evidence, 4).is_err(),
        "uncaptured Plain protocol sectors must stay explicitly unavailable"
    );
}

#[test]
fn plain_fixture_filename_supplies_fixture_metadata() {
    let name = "disk5_245760000_vid2bdf_pid0300_plain_20260929_073104.edpb";
    let meta =
        parse_fixture_backup_name(name).expect("current Plain writer filename must be parseable");
    assert_eq!(meta.disk, 5);
    assert_eq!(meta.secs, Some(245_760_000));
    assert_eq!(meta.vid, "2bdf");
    assert_eq!(meta.pid, "0300");
    assert_eq!(meta.device_id, "plain");
    assert_eq!(meta.onlyid, None);
}

#[test]
fn plain_v3_metadata_without_protocol_core_is_healthy_and_keeps_plain_kind() {
    let tmp = TmpDir::new("plain_v3_catalog");
    let total_sectors = 245_760_000u64;
    let path = write_plain_metadata_v3(
        &tmp.0,
        "disk5_245760000_vid2bdf_pid0300_plain_20260929_073100.edpb",
        total_sectors,
    );

    let verified = edpb::verify_file(&path).expect("Plain v3 metadata container must verify");
    assert_eq!(verified.manifest.schema, "edpb.manifest.v3");
    assert_eq!(verified.manifest.snapshot.device_state, "plain");
    assert!(
        edpb::read_raw_protocol(&path).is_err(),
        "Plain v3 metadata intentionally has no fixed LBA0-12 protocol artifact"
    );

    let entries = scan_backup_dir_checked(&tmp.0).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .expect("catalog entry");
    assert!(
        entry.size_ok,
        "valid Plain v3 metadata must not be marked as 大小异常 merely because protocol core is absent"
    );
    assert_eq!(entry.integrity_status, BackupIntegrityStatus::Verified);
    assert_eq!(
        entry.provision_kind,
        Some(edpcli::provision::DiskProvisionKind::Plain)
    );

    let rows = edpcli::application::scan_backup_workspace_checked(&tmp.0).unwrap();
    let identity = edpcli::application::identity::WorkspaceIdentity::from_backup(&rows[0]);
    assert_eq!(
        identity.provision_kind,
        Some(edpcli::provision::DiskProvisionKind::Plain),
        "verified Plain v3 row must show 普通盘"
    );
    assert!(
        edpcli::infrastructure::backup_store::catalog::backup_group_key(entry).is_none(),
        "weak Plain identity must remain excluded from destructive prune grouping"
    );
    assert!(
        edpcli::infrastructure::backup_store::catalog::backup_list_group_key(entry).is_some(),
        "healthy tool-owned Plain v3 must still be recognized by non-destructive list grouping"
    );
}

#[test]
fn scan_backup_dir_reports_edpb_integrity_and_ignores_legacy_bin() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("scan_backup");

    let ok = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
        &original,
    );
    let damaged = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172301.edpb",
        &original,
    );
    let verified = edpb::verify_file(&damaged).unwrap();
    let data_offset = verified.manifest.artifacts[0].storage.data_offset as usize;
    let mut damaged_bytes = fs::read(&damaged).unwrap();
    damaged_bytes[data_offset + 17] ^= 0x5A;
    fs::write(&damaged, damaged_bytes).unwrap();

    let invalid = tmp.0.join("invalid.edpb");
    fs::write(&invalid, &original).unwrap();
    let legacy = tmp.0.join("legacy.bin");
    fs::write(&legacy, &original).unwrap();
    fs::write(
        format!("{}.sha256", legacy.display()),
        format!("{}\n", sha256(&original)),
    )
    .unwrap();

    let entries = scan_backup_dir_checked(&tmp.0).unwrap();
    assert_eq!(entries.len(), 3, "旧 .bin 必须被正式运行时完全忽略");
    assert!(entries.iter().all(|entry| entry.path != legacy));
    let by_path = |path: &std::path::Path| entries.iter().find(|entry| entry.path == path).unwrap();

    let ok_e = by_path(&ok);
    assert_eq!(ok_e.integrity_status, BackupIntegrityStatus::Verified);
    assert!(ok_e.size_ok);
    assert!(ok_e.meta.is_some());
    assert!(ok_e.provision_kind.is_some());

    let damaged_e = by_path(&damaged);
    assert_eq!(damaged_e.integrity_status, BackupIntegrityStatus::Invalid);
    assert!(!damaged_e.size_ok);
    assert!(damaged_e.meta.is_none());
    assert_eq!(damaged_e.provision_kind, None, "损坏备份不得回退成普通盘");

    let invalid_e = by_path(&invalid);
    assert_eq!(invalid_e.integrity_status, BackupIntegrityStatus::Invalid);
    assert!(!invalid_e.size_ok);
    assert!(invalid_e.meta.is_none());
    assert_eq!(invalid_e.provision_kind, None, "无法验证的备份盘型必须未知");
}

#[test]
fn scan_backup_dir_rejects_raw_7168_bytes_disguised_as_edpb() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("scan_backup_reject_7168");
    let mut legacy = original;
    legacy.extend_from_slice(&[0u8; SECTOR]);
    let path = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172399.edpb",
    );
    fs::write(&path, &legacy).unwrap();

    let entries = scan_backup_dir_checked(&tmp.0).unwrap();
    let entry = entries.iter().find(|entry| entry.path == path).unwrap();
    assert_eq!(legacy.len(), METADATA_IMAGE_LEN + SECTOR);
    assert_eq!(entry.integrity_status, BackupIntegrityStatus::Invalid);
    assert!(!entry.size_ok);
    assert!(entry.meta.is_none());
}

#[test]
fn legacy_bin_is_not_a_runtime_backup_even_with_valid_sidecar() {
    let data = load_disk_image("netac");
    let tmp = TmpDir::new("legacy_bin_rejected");
    let legacy = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.bin",
    );
    fs::write(&legacy, &data).unwrap();
    fs::write(
        format!("{}.sha256", legacy.display()),
        format!("{}\n", sha256(&data)),
    )
    .unwrap();

    assert!(scan_backup_dir_checked(&tmp.0).unwrap().is_empty());
    assert_eq!(backup_verify(&tmp.0, None), 0);
    assert_eq!(
        backup_verify(&tmp.0, Some(legacy.file_name().unwrap().to_str().unwrap())),
        5
    );
}

#[test]
fn scan_is_read_only_and_infers_missing_onlyid_in_memory() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("scan_read_only");
    let legacy = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.edpb",
        &original,
    );
    let sidecar = std::path::PathBuf::from(format!("{}.sha256", legacy.display()));

    let entries = scan_backup_dir_checked(&tmp.0).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(legacy.exists(), "扫描不应重命名 .edpb");
    assert!(!sidecar.exists(), "EDPB 不应创建外部 .sha256 sidecar");
    assert_eq!(entries[0].path, legacy);
    assert_eq!(
        entries[0].meta.as_ref().and_then(|m| m.onlyid.as_deref()),
        Some("1402259934")
    );
    assert!(
        fs::read_dir(&tmp.0).unwrap().flatten().all(|e| !e
            .file_name()
            .to_string_lossy()
            .contains("_onlyid1402259934_")),
        "只读扫描不能产生迁移后的新文件名"
    );
}

#[test]
fn scan_prefers_lba4_identity_over_filename_onlyid() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("scan_onlyid_content_wins");
    let path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid999999999_20260917_120000.edpb",
        &original,
    );

    let entries = scan_backup_dir_checked(&tmp.0).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .expect("应扫描到测试备份");
    assert_eq!(
        entry.meta.as_ref().and_then(|meta| meta.onlyid.as_deref()),
        Some("1402259934"),
        "备份归属必须以自身 LBA4 为准，不能信任被改过的文件名"
    );
}

fn grouped_identity(onlyid: &str) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            total_sectors: Some(122_880_000),
            logical_sector_size: Some(SECTOR as u32),
            ..HardwareIdentityEvidence::default()
        },
        protocol: ProtocolIdentityEvidence {
            device_id: Some("disk&ven_netac&prod_onlydisk".into()),
            onlyid: Some(onlyid.into()),
            provision_kind: Some(edpcli::provision::DiskProvisionKind::Mode0),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    }
}

fn fake_entry(name: &str, onlyid: &str, mtime: i64) -> BackupEntry {
    BackupEntry {
        created_epoch: Some(mtime),
        display_cached: false,
        meta: Some(BackupMeta {
            disk: 6,
            secs: Some(122880000),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some(onlyid.into()),
            identity: Some(grouped_identity(onlyid)),
        }),
        path: std::path::PathBuf::from(name),
        mtime,
        provision_kind: Some(edpcli::provision::DiskProvisionKind::Plain),
        integrity_status: BackupIntegrityStatus::Verified,
        size_ok: true,
        lba8: None,
        verification_error: None,
        content_sha256: None,
        coverage: None,
        manifest: None,
        restore_preview: None,
    }
}

#[test]
fn prune_policy_keeps_latest_backups_and_last_backup() {
    let entries = vec![
        fake_entry("a-original.edpb", "A", 1),
        fake_entry("a-n1.edpb", "A", 10),
        fake_entry("a-n2.edpb", "A", 20),
        fake_entry("a-n3.edpb", "A", 30),
        fake_entry("b-n1.edpb", "B", 10),
        fake_entry("b-n2.edpb", "B", 20),
        fake_entry("b-n3.edpb", "B", 30),
    ];

    let keep2: Vec<String> = prune_candidates(&entries, 2)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(keep2, vec!["a-original.edpb", "a-n1.edpb", "b-n1.edpb"]);

    let keep0: Vec<String> = prune_candidates(&entries, 0)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        keep0,
        vec![
            "a-original.edpb",
            "a-n1.edpb",
            "a-n2.edpb",
            "b-n1.edpb",
            "b-n2.edpb"
        ]
    );
}

#[test]
fn prune_uses_verified_capture_time_before_filesystem_mtime() {
    let mut entries = vec![
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_20260910_120000.edpb",
            "A",
            300,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_20260911_120000.edpb",
            "A",
            200,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_20260912_120000.edpb",
            "A",
            100,
        ),
    ];

    for (index, entry) in entries.iter_mut().enumerate() {
        entry.created_epoch = Some(index as i64 + 1);
    }
    let candidates = prune_candidates(&entries, 2);
    assert_eq!(candidates.len(), 1);
    assert!(
        candidates[0].to_string_lossy().contains("20260910_120000"),
        "应删除文件名时间最旧的一份，而不是 mtime 最旧的一份: {:?}",
        candidates
    );
}

#[test]
fn verify_and_prune_preview_exit_contract() {
    let original = load_disk_image("netac");
    let Some((mode1, _)) = mode1_fixture_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("verify_prune");
    let original_path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.edpb",
        &original,
    );
    for (i, ts) in ["170001", "170002", "170003"].iter().enumerate() {
        let name = format!(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_{ts}.edpb"
        );
        let p = write_backup(&tmp.0, &name, &mode1);
        // 在不引入 filetime 依赖的前提下，文件名只用于断言预览不删除；策略本身的 mtime
        // 排序由上面的纯函数用例覆盖。
        assert!(p.exists(), "mode1 backup {i}");
    }

    assert_eq!(backup_verify(&tmp.0, None), 0);
    assert_eq!(
        backup_verify(
            &tmp.0,
            Some(original_path.file_name().unwrap().to_str().unwrap()),
        ),
        0
    );

    let bad = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170010.edpb",
    );
    fs::write(&bad, &original).unwrap(); // 故意缺 .sha256
    assert_eq!(backup_verify(&tmp.0, None), 5);

    let before = fs::read_dir(&tmp.0).unwrap().count();
    assert_eq!(backup_prune(&tmp.0, 2, false), 0);
    let after = fs::read_dir(&tmp.0).unwrap().count();
    assert_eq!(before, after, "prune 预览绝不能删除文件");
}

#[test]
fn delete_cancel_yes_missing_and_last_backup_guard() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("rm_backup");
    let first = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.edpb",
        &original,
    );
    let second = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb",
        &original,
    );

    let mut deny = ScriptPrompter {
        inputs: vec!["NO".into()],
        idx: 0,
    };
    assert_eq!(
        backup_delete(
            &tmp.0,
            &[first.file_name().unwrap().to_string_lossy().into_owned()],
            false,
            &mut deny,
        ),
        130
    );
    assert!(first.exists());

    let mut unused = ScriptPrompter::yes();
    assert_eq!(
        backup_delete(
            &tmp.0,
            &[first.file_name().unwrap().to_string_lossy().into_owned()],
            true,
            &mut unused,
        ),
        0
    );
    assert!(!first.exists());
    assert!(!std::path::PathBuf::from(format!("{}.sha256", first.display())).exists());

    // 安全底线：同盘只剩 second 时，手动 delete 也不能清到零份。
    assert_eq!(
        backup_delete(
            &tmp.0,
            &[second.file_name().unwrap().to_string_lossy().into_owned()],
            true,
            &mut unused,
        ),
        5
    );
    assert!(second.exists());
    assert_eq!(
        backup_delete(&tmp.0, &["missing.edpb".into()], true, &mut unused),
        5
    );
}

struct ReplaceBeforeConfirm {
    path: std::path::PathBuf,
    replacement: Vec<u8>,
}

impl edpcli::cli::Prompter for ReplaceBeforeConfirm {
    fn prompt_line(&mut self, _msg: &str) -> String {
        String::new()
    }

    fn prompt_secret(&mut self, _msg: &str) -> edpcli::provision::SecretBytes {
        panic!("unexpected secret prompt")
    }

    fn confirm_yes(&mut self, _msg: &str) -> bool {
        fs::write(&self.path, &self.replacement).unwrap();
        true
    }
}

#[test]
fn delete_refuses_if_confirmed_backup_is_replaced_before_delete() {
    let (original, replacement) = (load_disk_image("netac"), load_disk_image("lexar"));
    let tmp = TmpDir::new("rm_replaced_after_confirm_view");
    let victim = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.edpb",
        &original,
    );
    let _keep = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb",
        &original,
    );
    let mut prompt = ReplaceBeforeConfirm {
        path: victim.clone(),
        replacement: replacement.clone(),
    };

    assert_eq!(
        backup_delete(
            &tmp.0,
            &[victim.file_name().unwrap().to_string_lossy().into_owned()],
            false,
            &mut prompt,
        ),
        5
    );
    assert!(victim.exists(), "确认后被替换的同名文件不得删除");
    assert_eq!(fs::read(&victim).unwrap(), replacement);
}

#[test]
fn global_numbered_delete_follows_backup_list_order() {
    let (original, other_disk) = (load_disk_image("netac"), load_disk_image("lexar"));
    let tmp = TmpDir::new("onlyid_numbered_rm");
    let names = [
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb",
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170002.edpb",
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170003.edpb",
    ];
    let mut paths = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let p = write_backup(&tmp.0, name, &original);
        set_mtime(&p, 1_700_000_001 + i as i64);
        paths.push(p);
    }
    // 另一个盘的备份参与同一全局编号。
    let other = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid999999999_20260910_170004.edpb",
        &other_disk,
    );
    set_mtime(&other, 1_700_000_004);

    assert_eq!(backup_list(&tmp.0), 0);
    assert_eq!(backup_verify(&tmp.0, Some("3")), 0);
    assert_eq!(backup_prune(&tmp.0, 2, false), 0);

    // 全局编号按创建时间新→旧：other=[1], paths[2]=[2], paths[1]=[3], paths[0]=[4]。
    let mut unused = ScriptPrompter::yes();
    assert_eq!(backup_delete(&tmp.0, &["3".into()], true, &mut unused), 0);
    assert!(paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[2].exists());
    assert!(other.exists());
}

#[test]
fn delete_without_target_enters_global_picker_then_confirms() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("onlyid_picker_rm");
    let older = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb",
        &original,
    );
    let newer = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170002.edpb",
        &original,
    );
    set_mtime(&older, 1_700_000_001);
    set_mtime(&newer, 1_700_000_002);

    let mut prompt = ScriptPrompter {
        inputs: vec!["2".into(), "YES".into()],
        idx: 0,
    };
    assert_eq!(backup_delete(&tmp.0, &[], false, &mut prompt), 0);
    assert!(!older.exists());
    assert!(newer.exists());
}

// ══════════════════════════════════════════════════════════════════
// CLI 与 TUI 共用删除/保留策略服务 (application::backup)
// ══════════════════════════════════════════════════════════════════
const SHARED_GROUP_A: &str = "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.edpb";
const SHARED_GROUP_B: &str = "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb";

#[test]
fn cli_and_tui_delete_share_retention_floor_and_execution() {
    let original = load_disk_image("netac");
    let cli_tmp = TmpDir::new("shared_floor_cli");
    let tui_tmp = TmpDir::new("shared_floor_tui");
    for dir in [&cli_tmp.0, &tui_tmp.0] {
        write_backup(dir, SHARED_GROUP_A, &original);
        write_backup(dir, SHARED_GROUP_B, &original);
    }

    // 同盘两份时删一份: 两个入口都允许且都只删除单个 EDPB。
    let mut unused = ScriptPrompter::yes();
    assert_eq!(
        backup_delete(&cli_tmp.0, &[SHARED_GROUP_A.to_string()], true, &mut unused),
        0
    );
    assert!(!cli_tmp.0.join(SHARED_GROUP_A).exists());

    let first_path = tui_tmp.0.join(SHARED_GROUP_A);
    let sha = edpcli::edpb::sha256_hex(&fs::read(&first_path).unwrap());
    assert_eq!(
        edpcli::application::delete_backup_exact(&tui_tmp.0, &first_path, &sha),
        Ok(())
    );
    assert!(!first_path.exists());

    // 同盘仅剩一份: 两个入口都以同一保留底线拒绝，盘上文件保留。
    assert_eq!(
        backup_delete(&cli_tmp.0, &[SHARED_GROUP_B.to_string()], true, &mut unused),
        5
    );
    let second_path = tui_tmp.0.join(SHARED_GROUP_B);
    let second_sha = edpcli::edpb::sha256_hex(&fs::read(&second_path).unwrap());
    let refusal = edpcli::application::delete_backup_exact(&tui_tmp.0, &second_path, &second_sha)
        .unwrap_err();
    assert!(matches!(
        refusal,
        edpcli::application::backup::BackupDeleteError::Plan(
            edpcli::application::backup::DeletePlanError::RetentionFloor
        )
    ));
    assert!(cli_tmp.0.join(SHARED_GROUP_B).exists());
    assert!(tui_tmp.0.join(SHARED_GROUP_B).exists());
}

#[test]
fn batch_delete_plan_pins_each_path_and_sha_before_execution() {
    let original = load_disk_image("netac");
    let tmp = TmpDir::new("batch_delete_plan");
    let first = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.edpb",
        &original,
    );
    let second = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.edpb",
        &original,
    );
    let keep = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170002.edpb",
        &original,
    );
    let first_sha = edpcli::edpb::sha256_hex(&fs::read(&first).unwrap());
    let second_sha = edpcli::edpb::sha256_hex(&fs::read(&second).unwrap());

    let session = edpcli::application::backup::DeleteSession::open(&tmp.0);
    match session.plan_exact_many(&[(first.clone(), "00".repeat(32))]) {
        Err(edpcli::application::backup::DeletePlanError::Changed { .. }) => {}
        Err(other) => panic!("摘要不匹配应返回 Changed，实际: {}", other.message()),
        Ok(_) => panic!("摘要不匹配必须拒绝"),
    }

    let plan = session
        .plan_exact_many(&[
            (first.clone(), first_sha),
            (second.clone(), second_sha),
            (first.clone(), "ignored duplicate".into()),
        ])
        .expect("同盘三份中批删两份应允许");
    assert_eq!(plan.targets.len(), 2, "重复路径必须去重");
    let results = session.execute(&plan);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|(_, result)| result.is_ok()));
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(keep.exists(), "批量删除仍必须保留同盘至少一份备份");
}

#[test]
fn catalog_time_is_independent_of_filename_and_observation_mtime() {
    let mut newer = fake_entry("old-looking_19990101_000000.edpb", "A", 1);
    let mut older = fake_entry("future-looking_20990101_000000.edpb", "A", 9_999_999);
    newer.created_epoch = Some(200);
    older.created_epoch = Some(100);
    assert!(
        edpcli::infrastructure::backup_store::catalog::cmp_backup_newest_first(&newer, &older)
            .is_lt()
    );
    newer.path = std::path::PathBuf::from("renamed.edpb");
    newer.mtime = 0;
    assert!(
        edpcli::infrastructure::backup_store::catalog::cmp_backup_newest_first(&newer, &older)
            .is_lt()
    );
    let original_display =
        edpcli::infrastructure::backup_store::catalog::backup_display_time(&newer);
    newer.mtime = i64::MAX;
    assert_eq!(
        original_display,
        edpcli::infrastructure::backup_store::catalog::backup_display_time(&newer)
    );
    older.created_epoch = None;
    assert!(
        edpcli::infrastructure::backup_store::catalog::backup_display_time(&older)
            .contains("时间未知")
    );
}
#[test]
fn catalog_lookup_propagates_unreadable_directory_errors() {
    let tmp = TmpDir::new("catalog_error");
    let file = tmp.0.join("file");
    fs::write(&file, b"not a directory").unwrap();
    assert!(find_backups(&file, &grouped_identity("A")).is_err());
    assert!(find_backups(&tmp.0.join("absent"), &grouped_identity("A"))
        .unwrap()
        .confirmed
        .is_empty());
}
