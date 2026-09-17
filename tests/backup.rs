//! 备份/还原体系测试: 命名迁移、按盘匹配(LBA4 终验)、备份落盘、免密打标。
//! 全部纯文件系统操作 + 注入 DiskFacts/FixedClock, 不碰真盘。

mod common;

use std::fs;

use common::*;
use nopwd::diskio::{backup_disk, backup_is_nopwd, find_backups, migrate_backup_names,
                    backup_label_id, parse_backup_name, scan_backup_dir, BackupMeta,
                    Md5Status, DiskFacts};
use nopwd::diskio::Clock;

struct FixedClock;
impl Clock for FixedClock {
    fn now_epoch(&self) -> i64 { 1789603200 }
    fn fmt_ts(&self, _epoch: i64) -> String { "20260917_000000".into() }
    fn fmt_human(&self, _epoch: i64) -> String { "2026-09-17 00:00".into() }
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
fn migrate_old_lid_and_no_id_names() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("migrate");
    let old1 = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_lid1402259934_20250101_000000.bin",
        &data,
    );
    let old2 = write_backup(
        &tmp.0,
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_20250101_010101.bin",
        &data,
    );
    migrate_backup_names(&tmp.0);
    assert!(!old1.exists());
    assert!(!old2.exists());
    let bins: Vec<String> = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bin"))
        .collect();
    assert_eq!(bins.len(), 2);
    assert!(bins.iter().all(|n| n.contains("_onlyid1402259934_")), "{:?}", bins);
    // .md5 同步改名
    let md5s: Vec<String> = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md5"))
        .collect();
    assert_eq!(md5s.len(), 2);
    // 幂等: 第二次应无变化
    let before: Vec<String> = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    migrate_backup_names(&tmp.0);
    let after: Vec<String> = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut b = before;
    let mut a = after;
    b.sort();
    a.sort();
    assert_eq!(b, a);
}

#[test]
fn find_backups_lba4_final_filter() {
    let (Some(netac), Some(lexar), Some(real_bin)) =
        (load_disk_image("netac"), load_disk_image("lexar"), fixture_bin("netac"))
    else {
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
    let found = find_backups(&tmp.0, &netac_facts(), Some("disk&ven_netac&prod_onlydisk"), Some(my_tag));
    assert_eq!(found, vec![real.clone()]);

    // 空目录 → 空
    let empty = TmpDir::new("find_empty");
    assert!(find_backups(&empty.0, &netac_facts(), Some("disk&ven_netac&prod_onlydisk"), None).is_empty());
    let _ = real_bin;
}

#[test]
fn backup_written_with_md5_and_onlyid() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("backup");
    let (path, is_nopwd) =
        backup_disk(&netac_facts(), &data, "disk&ven_netac&prod_onlydisk", &tmp.0, &FixedClock).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("_onlyid1402259934_"), "{}", name);
    assert!(!is_nopwd); // 原盘备份不打 _nopwd
    assert!(!name.contains("_nopwd"), "{}", name);
    assert_eq!(fs::read(&path).unwrap(), data); // LBA0-13 全量
    let md5_content = fs::read_to_string(format!("{}.md5", path.display())).unwrap();
    assert_eq!(md5_content.trim(), md5(&data));
    // 备份可被 find_backups 找回
    let my_tag: [u8; 16] = data[4 * 512..4 * 512 + 16].try_into().unwrap();
    let found = find_backups(&tmp.0, &netac_facts(), Some("disk&ven_netac&prod_onlydisk"), Some(my_tag));
    assert_eq!(found, vec![path]);
}

#[test]
fn backup_tagging_by_content() {
    let Some((conv, did)) = converted_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("tagging");
    // 免密状态镜像 → 文件名含 _nopwd + 返回标记
    let (path, is_nopwd) = backup_disk(&netac_facts(), &conv, &did, &tmp.0, &FixedClock).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("_nopwd"), "{}", name);
    assert!(is_nopwd);
    // backup_is_nopwd 按内容检测(与文件名无关)
    assert!(backup_is_nopwd(&path, &did));
    assert!(!backup_is_nopwd(&path, "disk&ven_bogus&prod_x"));
    assert!(!backup_is_nopwd(std::path::Path::new("/nonexistent.bin"), &did));
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

    // 无 onlyid 的历史命名仍可解析；scan 时 migrate 会尽力从 LBA4 补齐。
    let legacy = parse_backup_name(
        "disk4_61440000_vid3535_pid6300_disk&ven_aigo&prod_u320_20260827_172228.bin",
    )
    .unwrap();
    assert_eq!(legacy.onlyid, None);
    assert_eq!(legacy.device_id, "disk&ven_aigo&prod_u320");

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
    fs::write(format!("{}.md5", damaged.display()), "00000000000000000000000000000000\n").unwrap();
    let missing = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_nopwd_20260910_172302.bin",
    );
    fs::write(&missing, &converted).unwrap();
    let odd = tmp.0.join("other.bin");
    fs::write(&odd, &original).unwrap();
    fs::write(format!("{}.md5", odd.display()), format!("{}\n", md5(&original))).unwrap();

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
