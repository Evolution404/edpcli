mod common;

use std::path::PathBuf;
use std::process::Command;

use common::*;
use edpcli::edpb::{self, CoreCapture};

fn fixture_edpb(key: &str, tag: &str) -> Option<(TmpDir, PathBuf)> {
    let data = load_disk_image(key)?;
    let (disk, sectors, vid, pid, device_id, onlyid) = match key {
        "netac" => (
            6,
            122_880_000u64,
            "0dd8",
            "2005",
            "disk&ven_netac&prod_onlydisk",
            "1402259934",
        ),
        "aigo" => (
            4,
            245_760_000u64,
            "3535",
            "6300",
            "disk&ven_aigo&prod_u335&rev_pmap",
            "1987718388",
        ),
        _ => return None,
    };
    let tmp = TmpDir::new(tag);
    let path = tmp.0.join(format!("{key}.edpb"));
    let capture = CoreCapture {
        snapshot_id: format!("inspect-cli-{key}"),
        created_epoch: 1_789_000_000,
        disk_number: Some(disk),
        vid: vid.into(),
        pid: pid.into(),
        device_id: device_id.into(),
        onlyid: Some(onlyid.into()),
        total_sectors: Some(sectors),
        logical_sector_size: 512,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: &data,
    };
    edpb::write_core_backup(&path, &capture).unwrap();
    Some((tmp, path))
}

#[test]
fn inspect_backup_file_is_offline_and_renders_structured_hex() {
    let Some((_tmp, path)) = fixture_edpb("netac", "inspect_cli_offline") else {
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
    let Some((tmp, copied)) = fixture_edpb("netac", "inspect_cli") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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
    assert!(stdout.contains("A6B0 整扇 512B"));
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
    let Some((_tmp, target)) = fixture_edpb("aigo", "inspect_no_auto_hex") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };

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
