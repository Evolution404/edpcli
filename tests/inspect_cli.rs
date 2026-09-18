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
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .arg("inspect")
        .arg(&path)
        .args(["--lba", "7"])
        .arg("--hex")
        .output()
        .expect("run edpcli inspect");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("来源"));
    assert!(stdout.contains("LBA7"));
    assert!(stdout.contains("EDPF magic"));
    assert!(stdout.contains("字段图例"));
    assert!(!stdout.contains("需要管理员权限"));
}

#[test]
fn inspect_backup_file_exports_selected_lbas() {
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
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .arg("inspect")
        .arg(&copied)
        .args(["--lba", "11,12", "--backup-dir"])
        .arg(&tmp.0)
        .arg("--export")
        .arg(&export)
        .output()
        .expect("run edpcli inspect backup");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("onlyid=1402259934"));
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

#[test]
fn known_lba_without_structure_does_not_dump_hex_unless_requested() {
    let (Some((name, _)), Some(data)) = (fixture("aigo"), load_disk_image("aigo")) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let tmp = TmpDir::new("inspect_no_auto_hex");
    let target = tmp.0.join(name);
    fs::write(&target, data).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["inspect", target.to_str().unwrap(), "--lba", "9"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("未检测到") || stdout.contains("未识别"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("+0x000:"),
        "未指定 --hex 时不应自动刷 hex: {stdout}"
    );

    let out_hex = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["inspect", target.to_str().unwrap(), "--lba", "9", "--hex"])
        .output()
        .unwrap();
    let stdout_hex = String::from_utf8_lossy(&out_hex.stdout);
    assert!(
        stdout_hex.contains("+0x000:"),
        "--hex 应明确展开: {stdout_hex}"
    );
}
