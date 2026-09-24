//! 备份/还原体系测试: 历史命名解析、按盘匹配(LBA4 终验)、备份落盘、免密打标。
//! 全部纯文件系统操作 + 注入 DiskFacts/FixedClock, 不碰真盘。

mod common;

use std::fs;

use common::*;
use edpcli::cli::{backup_delete, backup_list, backup_prune, backup_verify};
use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::diskio::Clock;
use edpcli::diskio::{
    backup_is_nopwd, backup_label_id, create_backup, find_backups, parse_backup_name,
    prune_candidates, scan_backup_dir, BackupEntry, BackupMeta, DiskFacts, Sha256Status,
};
use edpcli::edpb::{self, CoreCapture};

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
        label_id: Some("1402259934".into()),
    }
}

fn write_backup(dir: &std::path::Path, name: &str, data: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    let meta = parse_backup_name(name).expect("测试 EDPB 文件名必须可解析");
    let onlyid = edpcli::diskio::lba4_label_id_from(&data[4 * SECTOR..5 * SECTOR]);
    let is_nopwd = edpcli::diskio::image_is_nopwd(data, &meta.device_id);
    let capture = CoreCapture {
        snapshot_id: format!("test-{name}"),
        created_epoch: 1_789_000_000,
        disk_number: Some(meta.disk),
        vid: meta.vid.clone(),
        pid: meta.pid.clone(),
        device_id: meta.device_id.clone(),
        onlyid,
        total_sectors: meta.secs,
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: if is_nopwd {
            "passwordless".into()
        } else {
            "encrypted".into()
        },
        lba0_12: data,
    };
    edpb::write_core_backup(&p, &capture).unwrap();
    p
}

#[test]
fn edpb_backup_label_id_comes_from_raw_lba4() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("edpb_label_id");
    let path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_20260910_172300.edpb",
        &data,
    );
    assert_eq!(backup_label_id(&path).as_deref(), Some("1402259934"));
}

#[test]
fn find_backups_lba4_final_filter() {
    let (Some(netac), Some(lexar), Some(real_bin)) = (
        load_disk_image("netac"),
        load_disk_image("lexar"),
        fixture_bin("netac"),
    ) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let my_tag: [u8; 16] = netac[4 * 512..4 * 512 + 16].try_into().unwrap();
    let found = find_backups(
        &tmp.0,
        &netac_facts(),
        Some("disk&ven_netac&prod_onlydisk"),
        Some(my_tag),
    );
    assert_eq!(found, vec![real.clone()]);

    // 空目录 → 空
    let empty = TmpDir::new("find_empty");
    assert!(find_backups(
        &empty.0,
        &netac_facts(),
        Some("disk&ven_netac&prod_onlydisk"),
        None
    )
    .is_empty());
    let _ = real_bin;
}

#[test]
fn backup_written_as_single_edpb_with_internal_hashes_and_onlyid() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup");
    let (path, is_nopwd) = create_backup(
        &netac_facts(),
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("_onlyid1402259934_"), "{}", name);
    assert!(!is_nopwd); // 原盘备份不打 _nopwd
    assert!(!name.contains("_nopwd"), "{}", name);
    assert_eq!(edpb::read_raw_protocol(&path).unwrap(), data); // LBA0-12 全量
    let verified = edpb::verify_file(&path).unwrap();
    assert_eq!(
        verified.manifest.device.onlyid.as_deref(),
        Some("1402259934")
    );
    assert!(!std::path::PathBuf::from(format!("{}.sha256", path.display())).exists());
    // 备份可被 find_backups 找回
    let my_tag: [u8; 16] = data[4 * 512..4 * 512 + 16].try_into().unwrap();
    let found = find_backups(
        &tmp.0,
        &netac_facts(),
        Some("disk&ven_netac&prod_onlydisk"),
        Some(my_tag),
    );
    assert_eq!(found, vec![path]);
}

#[test]
fn backup_rejects_incomplete_lba_image_before_creating_files() {
    let Some(mut data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    data.pop();
    let tmp = TmpDir::new("backup_short_image");
    let result = create_backup(
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
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup_content_onlyid");
    let mut facts = netac_facts();
    facts.label_id = Some("999999999".into());
    let (path, _) = create_backup(
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
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let tag: [u8; 16] = data[4 * SECTOR..4 * SECTOR + 16].try_into().unwrap();
    let found = find_backups(
        &tmp.0,
        &netac_facts(),
        Some("disk&ven_netac&prod_onlydisk"),
        Some(tag),
    );
    assert_eq!(found, vec![bin]);
}

#[test]
fn find_backups_prefers_device_id_tier_before_generic_fallback() {
    let tmp = TmpDir::new("find_tier_priority");
    let mut data = vec![0u8; METADATA_IMAGE_LEN];
    let tag: [u8; 16] = *b"0123456789ABCDEF";
    data[4 * SECTOR..4 * SECTOR + 16].copy_from_slice(&tag);

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

    let found = find_backups(
        &tmp.0,
        &netac_facts(),
        Some("disk&ven_netac&prod_onlydisk"),
        Some(tag),
    );
    assert_eq!(found, vec![exact]);
}

#[test]
fn backup_rejects_device_id_with_path_separators_before_creating_files() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup_device_id_path_escape");
    let bak = tmp.0.join("bak");
    fs::create_dir_all(&bak).unwrap();
    // 旧实现会把 device_id 原样拼进文件名。预建第一层目录后，`/../../` 可逃出 bak。
    fs::create_dir_all(bak.join("disk99_122880000_vid0dd8_pid2005_disk&ven_bad")).unwrap();
    let result = create_backup(
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
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup_no_implicit_migrate");
    let legacy = tmp
        .0
        .join("disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin");
    fs::write(&legacy, &data).unwrap();
    let legacy_sha256 = std::path::PathBuf::from(format!("{}.sha256", legacy.display()));
    fs::write(&legacy_sha256, format!("{}\n", sha256(&data))).unwrap();

    let (new_path, _) = create_backup(
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
    assert!(scan_backup_dir(&tmp.0)
        .iter()
        .all(|entry| entry.path != legacy));
}

#[test]
fn backup_collision_never_overwrites_existing_file() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup_collision");
    let (path, _) = create_backup(
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
    let err = create_backup(
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
fn backup_tagging_by_content() {
    let Some((conv, did)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("tagging");
    // 免密状态镜像 → 文件名含 _nopwd + 返回标记
    let (path, is_nopwd) = create_backup(&netac_facts(), &conv, &did, &tmp.0, &FixedClock).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("_nopwd"), "{}", name);
    assert!(is_nopwd);
    // backup_is_nopwd 按内容检测(与文件名无关)
    assert!(backup_is_nopwd(&path, &did));
    assert!(!backup_is_nopwd(&path, "disk&ven_bogus&prod_x"));
    assert!(!backup_is_nopwd(
        std::path::Path::new("/nonexistent.edpb"),
        &did
    ));
    // 短文件安全返回 false
    let short = tmp.0.join("short.edpb");
    fs::write(&short, vec![0u8; 100]).unwrap();
    assert!(!backup_is_nopwd(&short, &did));
}

#[test]
fn parse_backup_name_modern_nopwd_legacy_and_invalid() {
    let modern = parse_backup_name(
        "disk26_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260917_224100.edpb",
    )
    .unwrap();
    assert_eq!(
        modern,
        BackupMeta {
            disk: 26,
            secs: Some(245760000),
            vid: "3535".into(),
            pid: "6300".into(),
            device_id: "disk&ven_aigo&prod_u335&rev_pmap".into(),
            onlyid: Some("1987718388".into()),
            tagged_nopwd: false,
        }
    );

    let nopwd = parse_backup_name(
        "disk6_unknown_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid-1402259934_nopwd_20260910_172433.edpb",
    )
    .unwrap();
    assert_eq!(nopwd.secs, None);
    assert_eq!(nopwd.onlyid.as_deref(), Some("-1402259934"));
    assert!(nopwd.tagged_nopwd);

    // 无 onlyid 的历史命名仍可解析；scan 只在内存中从 LBA4 补齐。
    let legacy = parse_backup_name(
        "disk4_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.edpb",
    )
    .unwrap();
    assert_eq!(legacy.onlyid, None);
    assert_eq!(legacy.device_id, "disk&ven_aigo&prod_u320");

    let lid = parse_backup_name(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_lid1402259934_20250101_000000.edpb",
    )
    .unwrap();
    assert_eq!(lid.onlyid.as_deref(), Some("1402259934"));
    assert_eq!(lid.device_id, "disk&ven_netac&prod_onlydisk");

    for bad in [
        "other.edpb",
        "diskx_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.edpb",
        "disk4_bad_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.edpb",
        "disk4_61440000_vidzzzz_pid6300_disk&ven_aigo&prod_u320_20260827_172228.edpb",
        "disk4_61440000_vid3535_pid6300_not-a-device_20260827_172228.edpb",
        "disk4_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_badtime.edpb",
    ] {
        assert!(parse_backup_name(bad).is_none(), "应拒绝: {bad}");
    }
}

#[test]
fn scan_backup_dir_reports_edpb_integrity_and_ignores_legacy_bin() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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

    let entries = scan_backup_dir(&tmp.0);
    assert_eq!(entries.len(), 3, "旧 .bin 必须被正式运行时完全忽略");
    assert!(entries.iter().all(|entry| entry.path != legacy));
    let by_path = |path: &std::path::Path| entries.iter().find(|entry| entry.path == path).unwrap();

    let ok_e = by_path(&ok);
    assert_eq!(ok_e.sha256_ok, Sha256Status::Ok);
    assert!(ok_e.size_ok);
    assert!(!ok_e.is_nopwd);
    assert!(ok_e.meta.is_some());

    let damaged_e = by_path(&damaged);
    assert_eq!(damaged_e.sha256_ok, Sha256Status::Mismatch);
    assert!(!damaged_e.size_ok);
    assert!(damaged_e.meta.is_none());

    let invalid_e = by_path(&invalid);
    assert_eq!(invalid_e.sha256_ok, Sha256Status::Mismatch);
    assert!(!invalid_e.size_ok);
    assert!(invalid_e.meta.is_none());
}

#[test]
fn scan_backup_dir_rejects_raw_7168_bytes_disguised_as_edpb() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("scan_backup_reject_7168");
    let mut legacy = original;
    legacy.extend_from_slice(&[0u8; SECTOR]);
    let path = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172399.edpb",
    );
    fs::write(&path, &legacy).unwrap();

    let entries = scan_backup_dir(&tmp.0);
    let entry = entries.iter().find(|entry| entry.path == path).unwrap();
    assert_eq!(legacy.len(), METADATA_IMAGE_LEN + SECTOR);
    assert_eq!(entry.sha256_ok, Sha256Status::Mismatch);
    assert!(!entry.size_ok);
    assert!(entry.meta.is_none());
}

#[test]
fn legacy_bin_is_not_a_runtime_backup_even_with_valid_sidecar() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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

    assert!(scan_backup_dir(&tmp.0).is_empty());
    assert_eq!(backup_verify(&tmp.0, None), 0);
    assert_eq!(
        backup_verify(&tmp.0, Some(legacy.file_name().unwrap().to_str().unwrap())),
        5
    );
}

#[test]
fn scan_is_read_only_and_infers_missing_onlyid_in_memory() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("scan_read_only");
    let legacy = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.edpb",
        &original,
    );
    let sidecar = std::path::PathBuf::from(format!("{}.sha256", legacy.display()));

    let entries = scan_backup_dir(&tmp.0);
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
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("scan_onlyid_content_wins");
    let path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid999999999_20260917_120000.edpb",
        &original,
    );

    let entries = scan_backup_dir(&tmp.0);
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

fn fake_entry(name: &str, onlyid: &str, mtime: i64, is_nopwd: bool) -> BackupEntry {
    BackupEntry {
        meta: Some(BackupMeta {
            disk: 6,
            secs: Some(122880000),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some(onlyid.into()),
            tagged_nopwd: is_nopwd,
        }),
        path: std::path::PathBuf::from(name),
        mtime,
        is_nopwd,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        sha256_ok: Sha256Status::Ok,
        size_ok: true,
        lba8: None,
        content_sha256: None,
    }
}

#[test]
fn prune_policy_keeps_originals_latest_snapshots_and_last_backup() {
    let entries = vec![
        fake_entry("a-original.edpb", "A", 1, false),
        fake_entry("a-n1.edpb", "A", 10, true),
        fake_entry("a-n2.edpb", "A", 20, true),
        fake_entry("a-n3.edpb", "A", 30, true),
        fake_entry("b-n1.edpb", "B", 10, true),
        fake_entry("b-n2.edpb", "B", 20, true),
        fake_entry("b-n3.edpb", "B", 30, true),
    ];

    let keep2: Vec<String> = prune_candidates(&entries, 2)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(keep2, vec!["a-n1.edpb", "b-n1.edpb"]);

    let keep0: Vec<String> = prune_candidates(&entries, 0)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    // A 有原盘，可清光免密快照；B 没原盘，最老两份可删但最新一份强制保留。
    assert_eq!(
        keep0,
        vec![
            "a-n1.edpb",
            "a-n2.edpb",
            "a-n3.edpb",
            "b-n1.edpb",
            "b-n2.edpb"
        ]
    );
}

#[test]
fn prune_uses_backup_name_time_before_filesystem_mtime() {
    let entries = vec![
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260910_120000.edpb",
            "A",
            300,
            true,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260911_120000.edpb",
            "A",
            200,
            true,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260912_120000.edpb",
            "A",
            100,
            true,
        ),
    ];

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
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let Some((converted, _)) = passwordless_image("netac") else {
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
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_{ts}.edpb"
        );
        let p = write_backup(&tmp.0, &name, &converted);
        // 在不引入 filetime 依赖的前提下，文件名只用于断言预览不删除；策略本身的 mtime
        // 排序由上面的纯函数用例覆盖。
        assert!(p.exists(), "snapshot {i}");
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
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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

    fn confirm_yes(&mut self, _msg: &str) -> bool {
        fs::write(&self.path, &self.replacement).unwrap();
        true
    }
}

#[test]
fn delete_refuses_if_confirmed_backup_is_replaced_before_delete() {
    let (Some(original), Some(replacement)) = (load_disk_image("netac"), load_disk_image("lexar"))
    else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let (Some(original), Some(other_disk)) = (load_disk_image("netac"), load_disk_image("lexar"))
    else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let sha = edpcli::sha256::sha256_hex(&fs::read(&first_path).unwrap());
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
    let second_sha = edpcli::sha256::sha256_hex(&fs::read(&second_path).unwrap());
    let refusal = edpcli::application::delete_backup_exact(&tui_tmp.0, &second_path, &second_sha)
        .unwrap_err();
    assert!(refusal.contains("至少保留 1 份"), "{refusal}");
    assert!(cli_tmp.0.join(SHARED_GROUP_B).exists());
    assert!(tui_tmp.0.join(SHARED_GROUP_B).exists());
}

#[test]
fn batch_delete_plan_pins_each_path_and_sha_before_execution() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    let first_sha = edpcli::sha256::sha256_hex(&fs::read(&first).unwrap());
    let second_sha = edpcli::sha256::sha256_hex(&fs::read(&second).unwrap());

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
fn prune_plan_composition_deletes_only_snapshots_beyond_keep() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let Some((converted, _)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("prune_service");
    write_backup(&tmp.0, SHARED_GROUP_A, &original);
    for ts in ["170001", "170002", "170003"] {
        let name = format!(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_{ts}.edpb"
        );
        write_backup(&tmp.0, &name, &converted);
    }

    let session = edpcli::application::backup::DeleteSession::open(&tmp.0);
    let plan = session.plan_prune(0).expect("原盘在场, keep=0 可清光快照");
    assert_eq!(plan.targets.len(), 3);
    let stats = plan.prune_stats.as_ref().expect("plan_prune 携带统计");
    assert_eq!(stats.originals, 1);
    assert_eq!(stats.retained_snapshots, 0);
    for (_, result) in session.execute(&plan) {
        assert_eq!(result, Ok(()), "逐条执行不得阻断后续");
    }
    assert!(tmp.0.join(SHARED_GROUP_A).exists(), "加密原盘永不自动删除");
    for ts in ["170001", "170002", "170003"] {
        assert!(!tmp
            .0
            .join(format!(
                "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_{ts}.edpb"
            ))
            .exists());
    }

    // 无原盘的组即使 keep=0 也强制保留最新一份(prune_candidates 的 keep.max(1))。
    let lone = TmpDir::new("prune_service_lone");
    write_backup(
        &lone.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_nopwd_20260910_170000.edpb",
        &converted,
    );
    let session = edpcli::application::backup::DeleteSession::open(&lone.0);
    let plan = session
        .plan_prune(0)
        .expect("无原盘组强制保留, 不触发底线拒绝");
    assert!(plan.targets.is_empty());
}
