mod common;

use std::fs;

use common::*;
use nopwd::backup_catalog::BackupCatalog;
use nopwd::md5::md5_hex;

fn copied_catalog() -> Option<(TmpDir, BackupCatalog)> {
    let Some(src) = fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return None;
    };
    let tmp = TmpDir::new("backup_catalog");
    let name = src.file_name().unwrap();
    let first = tmp.0.join(name);
    fs::copy(&src, &first).unwrap();
    let data = fs::read(&first).unwrap();
    fs::write(
        format!("{}.md5", first.display()),
        format!("{}\n", md5_hex(&data)),
    )
    .unwrap();

    // 同一 onlyid 制造第二份，并故意让 mtime 与文件名时间相反。
    // 备份真实创建时间来自文件名；复制/touch 不应改变 [1][2] 编号。
    let second_name = name
        .to_string_lossy()
        .replace("_20260910_172300.bin", "_20260911_172300.bin");
    let second = tmp.0.join(second_name);
    fs::copy(&first, &second).unwrap();
    fs::write(
        format!("{}.md5", second.display()),
        format!("{}\n", md5_hex(&data)),
    )
    .unwrap();
    set_mtime(&first, 1_789_100_000); // 文件名较旧，但 mtime 较新
    set_mtime(&second, 1_789_000_000); // 文件名较新，但 mtime 较旧

    let catalog = BackupCatalog::load(&tmp.0);
    Some((tmp, catalog))
}

#[test]
fn onlyid_index_is_one_based_and_newest_first() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    let group = catalog.onlyid_group("1402259934").unwrap();
    assert_eq!(group.len(), 2);
    assert!(group[0].path.to_string_lossy().contains("20260911_172300"));
    assert!(group[1].path.to_string_lossy().contains("20260910_172300"));
    assert_eq!(catalog.onlyid_index("1402259934", 1).unwrap().path, group[0].path);
    assert_eq!(catalog.onlyid_index("1402259934", 2).unwrap().path, group[1].path);
    assert!(catalog.onlyid_index("1402259934", 0).is_err());
    assert!(catalog.onlyid_index("1402259934", 3).is_err());
}

#[test]
fn target_resolution_is_confined_to_backup_root() {
    let Some((tmp, catalog)) = copied_catalog() else {
        return;
    };
    let name = catalog.entries()[0].path.file_name().unwrap().to_string_lossy();
    let entry = catalog.resolve_target(&name).unwrap();
    assert!(entry.path.starts_with(&tmp.0));

    let outside = tmp.0.parent().unwrap().join("outside.bin");
    fs::write(&outside, vec![0u8; 16]).unwrap();
    assert!(catalog.resolve_target(outside.to_str().unwrap()).is_err());
}

#[test]
fn onlyid_values_are_unique_and_sorted_by_latest_backup() {
    let Some((_tmp, catalog)) = copied_catalog() else {
        return;
    };
    assert_eq!(catalog.onlyid_values(), vec!["1402259934"]);
    assert!(catalog.onlyid_group("404").is_err());
}
