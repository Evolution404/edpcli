//! 备份/还原体系测试: 历史命名解析、按盘匹配(LBA4 终验)、备份落盘、免密打标。
//! 全部纯文件系统操作 + 注入 DiskFacts/FixedClock, 不碰真盘。

mod common;

use std::fs;

use common::*;
use edpcli::cli::{backup_list, backup_prune, backup_rm, backup_verify};
use edpcli::common::SECTOR;
use edpcli::diskio::Clock;
use edpcli::diskio::{
    backup_is_nopwd, backup_label_id, create_backup, find_backups, parse_backup_name,
    prune_candidates, scan_backup_dir, BackupEntry, BackupMeta, DiskFacts, Md5Status,
};

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
    fs::write(&p, data).unwrap();
    fs::write(format!("{}.md5", p.display()), format!("{}\n", md5(data))).unwrap();
    p
}

#[test]
fn real_backup_label_id() {
    // 备份文件名中的 onlyid 段 == 从其 LBA4 读出的值; 负 id 盘同理
    let Some(p) = fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    assert_eq!(backup_label_id(&p).as_deref(), Some("1402259934"));
    if let Some(neg) = neg_id_bin() {
        assert_eq!(backup_label_id(&neg).as_deref(), Some("-1833210541"));
    }
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
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin",
        &netac,
    );
    let _fake = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid9999999999_20260910_173000.bin",
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
fn backup_written_with_md5_and_onlyid() {
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
    assert_eq!(fs::read(&path).unwrap(), data); // LBA0-13 全量
    let md5_content = fs::read_to_string(format!("{}.md5", path.display())).unwrap();
    assert_eq!(md5_content.trim(), md5(&data));
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
    assert!(result.is_err(), "备份输入必须恰好为 LBA0-13 共 7168B");
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
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin",
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
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("bin"))
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
    let legacy = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin",
        &data,
    );
    let legacy_md5 = std::path::PathBuf::from(format!("{}.md5", legacy.display()));

    let (new_path, _) = create_backup(
        &netac_facts(),
        &data,
        "disk&ven_netac&prod_onlydisk",
        &tmp.0,
        &FixedClock,
    )
    .unwrap();

    assert!(legacy.exists(), "创建新备份不应重命名历史 .bin");
    assert!(legacy_md5.exists(), "创建新备份不应重命名历史 .md5");
    assert!(new_path.exists());
    assert_ne!(new_path, legacy);
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
    assert_eq!(
        fs::read_to_string(format!("{}.md5", path.display()))
            .unwrap()
            .trim(),
        md5(&first)
    );
}

#[test]
fn backup_tagging_by_content() {
    let Some((conv, did)) = converted_image("netac") else {
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
        std::path::Path::new("/nonexistent.bin"),
        &did
    ));
    // 短文件安全返回 false
    let short = tmp.0.join("short.bin");
    fs::write(&short, vec![0u8; 100]).unwrap();
    assert!(!backup_is_nopwd(&short, &did));
}

#[test]
fn parse_backup_name_modern_nopwd_legacy_and_invalid() {
    let modern = parse_backup_name(
        "disk26_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260917_224100.bin",
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
        "disk6_unknown_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid-1402259934_nopwd_20260910_172433.bin",
    )
    .unwrap();
    assert_eq!(nopwd.secs, None);
    assert_eq!(nopwd.onlyid.as_deref(), Some("-1402259934"));
    assert!(nopwd.tagged_nopwd);

    // 无 onlyid 的历史命名仍可解析；scan 只在内存中从 LBA4 补齐。
    let legacy = parse_backup_name(
        "disk4_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.bin",
    )
    .unwrap();
    assert_eq!(legacy.onlyid, None);
    assert_eq!(legacy.device_id, "disk&ven_aigo&prod_u320");

    let lid = parse_backup_name(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_lid1402259934_20250101_000000.bin",
    )
    .unwrap();
    assert_eq!(lid.onlyid.as_deref(), Some("1402259934"));
    assert_eq!(lid.device_id, "disk&ven_netac&prod_onlydisk");

    for bad in [
        "other.bin",
        "diskx_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.bin",
        "disk4_bad_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.bin",
        "disk4_61440000_vidzzzz_pid6300_disk&ven_aigo&prod_u320_20260827_172228.bin",
        "disk4_61440000_vid3535_pid6300_not-a-device_20260827_172228.bin",
        "disk4_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_badtime.bin",
    ] {
        assert!(parse_backup_name(bad).is_none(), "应拒绝: {bad}");
    }
}

#[test]
fn scan_backup_dir_reports_ok_mismatch_missing_and_unrecognized() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let Some((converted, _)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("scan_backup");

    let ok = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin",
        &original,
    );
    let damaged = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172301.bin",
        &original,
    );
    fs::write(
        format!("{}.md5", damaged.display()),
        "00000000000000000000000000000000\n",
    )
    .unwrap();
    let missing = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_172302.bin",
    );
    fs::write(&missing, &converted).unwrap();
    let odd = tmp.0.join("other.bin");
    fs::write(&odd, &original).unwrap();
    fs::write(
        format!("{}.md5", odd.display()),
        format!("{}\n", md5(&original)),
    )
    .unwrap();

    let entries = scan_backup_dir(&tmp.0);
    assert_eq!(entries.len(), 4);
    let by_name = |name: &str| {
        entries
            .iter()
            .find(|e| e.path.file_name().unwrap().to_string_lossy() == name)
            .unwrap()
    };
    let ok_e = by_name(ok.file_name().unwrap().to_str().unwrap());
    assert_eq!(ok_e.md5_ok, Md5Status::Ok);
    assert!(ok_e.size_ok);
    assert!(!ok_e.is_nopwd);
    assert!(ok_e.meta.is_some());

    let damaged_e = by_name(damaged.file_name().unwrap().to_str().unwrap());
    assert_eq!(damaged_e.md5_ok, Md5Status::Mismatch);
    assert!(damaged_e.size_ok);

    let missing_e = by_name(missing.file_name().unwrap().to_str().unwrap());
    assert_eq!(missing_e.md5_ok, Md5Status::NoSidecar);
    assert!(missing_e.is_nopwd);

    let odd_e = by_name("other.bin");
    assert!(odd_e.meta.is_none());
    assert!(!odd_e.is_nopwd);
    assert_eq!(odd_e.md5_ok, Md5Status::Ok);
}

#[cfg(unix)]
#[test]
fn md5_sidecar_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;

    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("md5_symlink");
    let backup = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.bin",
        &data,
    );
    let sidecar = std::path::PathBuf::from(format!("{}.md5", backup.display()));
    fs::remove_file(&sidecar).unwrap();
    let outside = tmp.0.parent().unwrap().join(format!(
        "edpcli_outside_md5_{}_{}",
        std::process::id(),
        md5(&data)
    ));
    fs::write(&outside, format!("{}\n", md5(&data))).unwrap();
    symlink(&outside, &sidecar).unwrap();

    assert_eq!(backup_verify(&tmp.0, None, None), 5);
    assert!(outside.exists(), "校验不能修改符号链接目标");
    let _ = fs::remove_file(outside);
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
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin",
        &original,
    );
    let legacy_md5 = std::path::PathBuf::from(format!("{}.md5", legacy.display()));

    let entries = scan_backup_dir(&tmp.0);
    assert_eq!(entries.len(), 1);
    assert!(legacy.exists(), "扫描不应重命名 .bin");
    assert!(legacy_md5.exists(), "扫描不应重命名 .md5");
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
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid999999999_20260917_120000.bin",
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
        md5_ok: Md5Status::Ok,
        size_ok: true,
        lba8: None,
        content_md5: None,
    }
}

#[test]
fn prune_policy_keeps_originals_latest_snapshots_and_last_backup() {
    let entries = vec![
        fake_entry("a-original.bin", "A", 1, false),
        fake_entry("a-n1.bin", "A", 10, true),
        fake_entry("a-n2.bin", "A", 20, true),
        fake_entry("a-n3.bin", "A", 30, true),
        fake_entry("b-n1.bin", "B", 10, true),
        fake_entry("b-n2.bin", "B", 20, true),
        fake_entry("b-n3.bin", "B", 30, true),
    ];

    let keep2: Vec<String> = prune_candidates(&entries, 2)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(keep2, vec!["a-n1.bin", "b-n1.bin"]);

    let keep0: Vec<String> = prune_candidates(&entries, 0)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    // A 有原盘，可清光免密快照；B 没原盘，最老两份可删但最新一份强制保留。
    assert_eq!(
        keep0,
        vec!["a-n1.bin", "a-n2.bin", "a-n3.bin", "b-n1.bin", "b-n2.bin"]
    );
}

#[test]
fn prune_uses_backup_name_time_before_filesystem_mtime() {
    let entries = vec![
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260910_120000.bin",
            "A",
            300,
            true,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260911_120000.bin",
            "A",
            200,
            true,
        ),
        fake_entry(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyidA_nopwd_20260912_120000.bin",
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
    let Some((converted, _)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("verify_prune");
    let original_path = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.bin",
        &original,
    );
    for (i, ts) in ["170001", "170002", "170003"].iter().enumerate() {
        let name = format!(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_{ts}.bin"
        );
        let p = write_backup(&tmp.0, &name, &converted);
        // 在不引入 filetime 依赖的前提下，文件名只用于断言预览不删除；策略本身的 mtime
        // 排序由上面的纯函数用例覆盖。
        assert!(p.exists(), "snapshot {i}");
    }

    assert_eq!(backup_verify(&tmp.0, None, None), 0);
    assert_eq!(
        backup_verify(
            &tmp.0,
            None,
            Some(original_path.file_name().unwrap().to_str().unwrap()),
        ),
        0
    );

    let bad = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170010.bin",
    );
    fs::write(&bad, &original).unwrap(); // 故意缺 .md5
    assert_eq!(backup_verify(&tmp.0, None, None), 5);

    let before = fs::read_dir(&tmp.0).unwrap().count();
    assert_eq!(backup_prune(&tmp.0, None, 2, false), 0);
    let after = fs::read_dir(&tmp.0).unwrap().count();
    assert_eq!(before, after, "prune 预览绝不能删除文件");
}

#[test]
fn rm_cancel_yes_missing_and_last_backup_guard() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("rm_backup");
    let first = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.bin",
        &original,
    );
    let second = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.bin",
        &original,
    );

    let mut deny = ScriptPrompter {
        inputs: vec!["NO".into()],
        idx: 0,
    };
    assert_eq!(
        backup_rm(
            &tmp.0,
            None,
            &[first.file_name().unwrap().to_string_lossy().into_owned()],
            false,
            &mut deny,
        ),
        130
    );
    assert!(first.exists());

    let mut unused = ScriptPrompter::yes();
    assert_eq!(
        backup_rm(
            &tmp.0,
            None,
            &[first.file_name().unwrap().to_string_lossy().into_owned()],
            true,
            &mut unused,
        ),
        0
    );
    assert!(!first.exists());
    assert!(!std::path::PathBuf::from(format!("{}.md5", first.display())).exists());

    // 安全底线：同盘只剩 second 时，手动 rm 也不能清到零份。
    assert_eq!(
        backup_rm(
            &tmp.0,
            None,
            &[second.file_name().unwrap().to_string_lossy().into_owned()],
            true,
            &mut unused,
        ),
        5
    );
    assert!(second.exists());
    assert_eq!(
        backup_rm(&tmp.0, None, &["missing.bin".into()], true, &mut unused),
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
fn rm_refuses_if_confirmed_backup_is_replaced_before_delete() {
    let (Some(original), Some(replacement)) = (load_disk_image("netac"), load_disk_image("lexar"))
    else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("rm_replaced_after_confirm_view");
    let victim = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170000.bin",
        &original,
    );
    let _keep = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.bin",
        &original,
    );
    let mut prompt = ReplaceBeforeConfirm {
        path: victim.clone(),
        replacement: replacement.clone(),
    };

    assert_eq!(
        backup_rm(
            &tmp.0,
            None,
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
fn onlyid_filter_and_numbered_rm_follow_newest_first_order() {
    let (Some(original), Some(other_disk)) = (load_disk_image("netac"), load_disk_image("lexar"))
    else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("onlyid_numbered_rm");
    let id = "1402259934";
    let names = [
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.bin",
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170002.bin",
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170003.bin",
    ];
    let mut paths = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let p = write_backup(&tmp.0, name, &original);
        set_mtime(&p, 1_700_000_001 + i as i64);
        paths.push(p);
    }
    // 另一个 onlyid 的备份不应被筛选或删除。
    let other = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid999999999_20260910_170004.bin",
        &other_disk,
    );
    set_mtime(&other, 1_700_000_004);

    assert_eq!(backup_list(&tmp.0, Some(id)), 0);
    assert_eq!(backup_list(&tmp.0, Some("404")), 5);
    assert_eq!(backup_verify(&tmp.0, Some(id), None), 0);
    assert_eq!(backup_verify(&tmp.0, Some("404"), None), 5);
    assert_eq!(backup_prune(&tmp.0, Some(id), 2, false), 0);

    // 编号按文件名创建时间新→旧，所以 [2] 是 paths[1]。
    let mut unused = ScriptPrompter::yes();
    assert_eq!(
        backup_rm(&tmp.0, Some(id), &["2".into()], true, &mut unused),
        0
    );
    assert!(paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[2].exists());
    assert!(other.exists());
}

#[test]
fn onlyid_rm_without_selector_enters_picker_then_confirms() {
    let Some(original) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("onlyid_picker_rm");
    let older = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170001.bin",
        &original,
    );
    let newer = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_170002.bin",
        &original,
    );
    set_mtime(&older, 1_700_000_001);
    set_mtime(&newer, 1_700_000_002);

    let mut prompt = ScriptPrompter {
        inputs: vec!["2".into(), "YES".into()],
        idx: 0,
    };
    assert_eq!(
        backup_rm(&tmp.0, Some("1402259934"), &[], false, &mut prompt),
        0
    );
    assert!(!older.exists());
    assert!(newer.exists());
}
