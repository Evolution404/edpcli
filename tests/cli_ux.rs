use crate::common;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use common::*;
use edpcli::edpb::{self, CoreCapture};

fn two_netac_backups() -> Option<TmpDir> {
    let data = load_disk_image("netac")?;
    let tmp = TmpDir::new("cli_ux");
    for ts in ["20260910_172300", "20260911_172300"] {
        let name = format!(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_{ts}.edpb"
        );
        let dst = tmp.0.join(name);
        let capture = CoreCapture {
            snapshot_id: format!("ux-{ts}"),
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
            lba0_12: &data,
        };
        edpb::write_core_backup(&dst, &capture).unwrap();
    }
    Some(tmp)
}

#[test]
fn removed_inspect_onlyid_ui_is_rejected_with_focused_help() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "--onlyid", "1402259934", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .expect("run removed inspect syntax");
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--onlyid"));
    assert!(stdout.contains("用法: edpcli inspect"));
}

#[test]
fn bare_backup_defaults_to_global_list() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    for args in [
        vec!["backup", "--backup-dir"],
        vec!["backup", "list", "--backup-dir"],
    ] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_edpcli"));
        cmd.env("NO_COLOR", "1").args(args).arg(&tmp.0);
        let out = cmd.output().expect("run backup default list");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("备份目录"));
        assert!(stdout.contains("onlyid=1402259934"));
        assert!(stdout.contains("Dept"), "备份列表应直接显示 Dept: {stdout}");
        assert!(stdout.contains("User"), "备份列表应直接显示 User: {stdout}");
    }
}

#[test]
fn old_meta_commands_return_v2_migration_hint() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    for old in ["meta", "metainfo"] {
        let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .arg(old)
            .args(["--backup-dir"])
            .arg(&tmp.0)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("edpcli info"));
    }
}

#[test]
fn info_accepts_backup_file_directly() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let file = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("edpb"))
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .arg("info")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("设备"), "{stdout}");
    assert!(stdout.contains("身份"), "{stdout}");
    assert!(stdout.contains("状态"), "{stdout}");
    assert!(stdout.contains("备份"), "{stdout}");
    assert!(stdout.contains("Dept"), "{stdout}");
    assert!(stdout.contains("User"), "{stdout}");
}

#[test]
fn focused_help_works_inside_subcommands() {
    for args in [
        ["inspect", "--help"].as_slice(),
        ["backup", "--help"].as_slice(),
        ["completion", "--help"].as_slice(),
        ["info", "--help"].as_slice(),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("run focused help");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("用法:"));
        assert!(!stdout.contains("错误:"));
    }
}

#[test]
fn completion_scripts_and_dynamic_values_are_available() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    for shell in ["zsh", "bash", "fish"] {
        let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .args(["completion", shell])
            .output()
            .expect("render completion");
        assert_eq!(out.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("__complete"),
            "{shell} completion should use dynamic provider"
        );
        assert!(stdout.contains("inspect"));
        assert!(stdout.contains("backup"));
        assert!(stdout.contains("info"));
        assert!(stdout.contains("backup create"));
        assert!(stdout.contains("backup restore"));
        assert!(!stdout.contains("--onlyid"), "{shell} leaked v1 --onlyid");
        assert!(!stdout.contains("--index"), "{shell} leaked v1 --index");
        assert!(!stdout.contains("metainfo"), "{shell} leaked v1 metainfo");
    }

    let numbers = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["__complete", "backup-number", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(numbers.status.code(), Some(0));
    let lines: Vec<_> = String::from_utf8_lossy(&numbers.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, vec!["1", "2"]);

    let files = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["__complete", "backup-file", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(files.status.code(), Some(0));
    let file_lines: Vec<_> = String::from_utf8_lossy(&files.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(file_lines.len(), 2);
}

#[test]
fn positional_backup_path_and_numbered_verify_follow_same_ux() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let file = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("edpb"))
        .unwrap();

    let inspect = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "meta"])
        .arg(&file)
        .args(["--lba", "7"])
        .output()
        .unwrap();
    assert_eq!(
        inspect.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("EDPF magic"));

    let verify = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["backup", "verify"])
        .arg(&file)
        .args(["--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(
        verify.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert_eq!(stdout.lines().filter(|l| l.starts_with('✓')).count(), 1);

    let numbered = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["backup", "verify", "1", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(
        numbered.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&numbered.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&numbered.stdout)
            .lines()
            .filter(|line| line.starts_with('✓'))
            .count(),
        1,
        "编号 1 必须与 backup list 的全局编号指向同一份备份"
    );
}

#[test]
fn backup_delete_without_target_uses_global_interactive_selector() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let before = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("edpb"))
        .count();
    assert_eq!(before, 2);

    let mut child = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["backup", "delete", "--backup-dir"])
        .arg(&tmp.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn backup delete picker");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1\nYES\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let after = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("edpb"))
        .count();
    assert_eq!(after, 1, "交互删除应删除所选全局编号且保留至少一份");
}

#[test]
fn inspect_requires_one_of_the_three_modes() {
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "--lba", "7"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("raw / decode / meta") || stderr.contains("模式"));
    assert!(stdout.contains("用法: edpcli inspect"));
    assert!(!stdout.contains("cems 加密 U 盘"));
}

#[cfg(unix)]
#[test]
fn piping_output_to_head_does_not_panic_on_broken_pipe() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let file = fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("edpb"))
        .unwrap();
    let bin = env!("CARGO_BIN_EXE_edpcli");
    let script = format!(
        "\"{}\" inspect raw \"{}\" --lba 7 | head -n 1 >/dev/null",
        bin,
        file.display()
    );
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("run edpcli through head");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("Broken pipe") && !stderr.contains("panicked at"),
        "管道提前关闭不应触发 Rust panic: {stderr}"
    );
}

#[test]
fn plain_info_card_does_not_claim_edp_protocol_or_decode_filesystem_as_safe6() {
    let summary = edpcli::metainfo::MetaInfoSummary::plain(
        Some("3535".into()),
        Some("6300".into()),
        Some(8_053_063_680),
    );
    let rendered = edpcli::metainfo::render_with_source(&summary, Some("disk4"));
    assert!(rendered.contains("普通盘 (Plain)"));
    assert!(rendered.contains("3535:6300"));
    for bogus in ["device_id", "EDP/cems", "LBA8 未解析", "SAFE6", "onlyid"] {
        assert!(!rendered.contains(bogus), "Plain card contains {bogus}");
    }
}
