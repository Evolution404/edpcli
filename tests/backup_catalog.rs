mod common;

use std::fs;

use common::*;
use edpcli::backup_catalog::BackupCatalog;
use edpcli::edpb::{self, CoreCapture};
use edpcli::metainfo::backup_ownership;

fn write_edpb(path: &std::path::Path, data: &[u8], snapshot_id: &str) {
    let capture = CoreCapture {
        snapshot_id: snapshot_id.into(),
        created_epoch: 1_789_000_000,
        disk_number: Some(6),
        vid: "0dd8".into(),
        pid: "2005".into(),
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        onlyid: Some("1402259934".into()),
        total_sectors: Some(122_880_000),
        logical_sector_size: 512,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: data,
    };
    edpb::write_core_backup(path, &capture).unwrap();
}

fn copied_catalog() -> Option<(TmpDir, BackupCatalog)> {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return None;
    };
    let tmp = TmpDir::new("backup_catalog");
    let first = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.edpb",
    );
    write_edpb(&first, &data, "catalog-first");

    // 同一 onlyid 制造第二份，并故意让 mtime 与文件名时间相反。
    // 列表创建时间排序来自文件名；复制/touch 不应改变 [1][2] 编号。
    let second = tmp.0.join(
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260911_172300.edpb",
    );
    write_edpb(&second, &data, "catalog-second");
    set_mtime(&first, 1_789_100_000); // 文件名较旧，但 mtime 较新
    set_mtime(&second, 1_789_000_000); // 文件名较新，但 mtime 较旧

    let catalog = BackupCatalog::load(&tmp.0);
    Some((tmp, catalog))
}

#[test]
fn catalog_order_prefers_backup_name_time_over_filesystem_mtime() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    assert_eq!(catalog.entries().len(), 2);
    assert!(catalog.entries()[0]
        .path
        .to_string_lossy()
        .contains("20260911_172300"));
    assert!(catalog.entries()[1]
        .path
        .to_string_lossy()
        .contains("20260910_172300"));
}

#[test]
fn target_resolution_is_confined_to_backup_root() {
    let Some((tmp, catalog)) = copied_catalog() else {
        return;
    };
    let name = catalog.entries()[0]
        .path
        .file_name()
        .unwrap()
        .to_string_lossy();
    let entry = catalog.resolve_target(&name).unwrap();
    assert!(entry.path.starts_with(&tmp.0));

    let outside = tmp.0.parent().unwrap().join("outside.edpb");
    fs::write(&outside, vec![0u8; 16]).unwrap();
    assert!(catalog.resolve_target(outside.to_str().unwrap()).is_err());
}

#[test]
fn ownership_uses_lba8_cached_during_catalog_scan() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    let entry = &catalog.entries()[0];
    assert!(entry.lba8.is_some());

    // 扫描完成后移除源文件；归属信息仍应从 BackupEntry 的内存 LBA8 得到，
    // 证明 backup list 不会为了 Dept/User 再次打开同一个 .edpb。
    fs::remove_file(&entry.path).unwrap();
    let ownership = backup_ownership(entry).expect("缓存 LBA8 应可解析归属信息");
    assert!(ownership
        .dept
        .as_deref()
        .unwrap_or_default()
        .contains("泰州供电公司"));
    assert_eq!(ownership.user.as_deref(), Some("宋旭琳"));
}
