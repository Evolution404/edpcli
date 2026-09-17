//! 备份/还原体系测试: 命名迁移、按盘匹配(LBA4 终验)、备份落盘、免密打标。
//! 全部纯文件系统操作 + 注入 DiskFacts/FixedClock, 不碰真盘。

mod common;

use std::fs;

use common::*;
use nopwd::diskio::{backup_disk, backup_is_nopwd, find_backups, migrate_backup_names,
                    backup_label_id, DiskFacts};
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
