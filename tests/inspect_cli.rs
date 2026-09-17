mod common;

use std::fs;
use std::process::Command;

use common::*;

#[test]
fn inspect_backup_file_is_offline_and_renders_structured_hex() {
    let Some(path) = fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .args(["inspect", "7", "--backup"])
        .arg(&path)
        .arg("--hex")
        .output()
        .expect("run nopwd inspect");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("来源"));
    assert!(stdout.contains("LBA7"));
    assert!(stdout.contains("EDPF magic"));
    assert!(stdout.contains("字段图例"));
    assert!(!stdout.contains("需要管理员权限"));
}

#[test]
fn inspect_onlyid_index_matches_backup_list_and_exports() {
    let Some(path) = fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("inspect_cli");
    let name = path.file_name().unwrap();
    let copied = tmp.0.join(name);
    fs::copy(&path, &copied).unwrap();
    let src_md5 = format!("{}.md5", path.display());
    if std::path::Path::new(&src_md5).exists() {
        fs::copy(src_md5, format!("{}.md5", copied.display())).unwrap();
    }
    let export = tmp.0.join("out");
    let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .args([
            "inspect",
            "11",
            "12",
            "--onlyid",
            "1402259934",
            "--index",
            "1",
            "--backup-dir",
        ])
        .arg(&tmp.0)
        .arg("--export")
        .arg(&export)
        .output()
        .expect("run nopwd inspect onlyid");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("onlyid=1402259934 [1]"));
    assert!(stdout.contains("PDKB"));
    assert!(stdout.contains("尾部 144B RAW"));
    for name in [
        "LBA11_raw.bin",
        "LBA11_decoded.bin",
        "LBA11_raw.hex",
        "LBA11_decoded.hex",
        "LBA12_raw.bin",
        "LBA12_decoded.bin",
    ] {
        assert!(export.join(name).exists(), "missing export {name}");
    }
}
