mod common;

use std::fs;
use std::process::Command;

use common::*;
use nopwd::md5::md5_hex;

fn two_netac_backups() -> Option<TmpDir> {
    let src = fixture_bin("netac")?;
    let tmp = TmpDir::new("cli_ux");
    for ts in ["20260910_172300", "20260911_172300"] {
        let name = format!(
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_{ts}.bin"
        );
        let dst = tmp.0.join(name);
        fs::copy(&src, &dst).unwrap();
        let data = fs::read(&dst).unwrap();
        fs::write(format!("{}.md5", dst.display()), format!("{}\n", md5_hex(&data))).unwrap();
    }
    Some(tmp)
}

#[test]
fn inspect_onlyid_without_index_lists_choices_instead_of_usage_error() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .args([
            "inspect",
            "--onlyid",
            "1402259934",
            "--backup-dir",
        ])
        .arg(&tmp.0)
        .output()
        .expect("run inspect picker hint");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("onlyid=1402259934"));
    assert!(stdout.contains("[1]"));
    assert!(stdout.contains("[2]"));
    assert!(stdout.contains("--index N"));
    assert!(!stdout.contains("用法: nopwd <子命令>"));
}

#[test]
fn bare_backup_defaults_to_list_and_accepts_onlyid_without_action() {
    let Some(tmp) = two_netac_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    for args in [
        vec!["backup", "--backup-dir"],
        vec!["backup", "--onlyid", "1402259934", "--backup-dir"],
    ] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nopwd"));
        cmd.env("NO_COLOR", "1").args(args).arg(&tmp.0);
        let out = cmd.output().expect("run backup default list");
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("备份目录"));
        assert!(stdout.contains("onlyid=1402259934"));
    }
}

#[test]
fn focused_help_works_inside_subcommands() {
    for args in [
        ["inspect", "--help"].as_slice(),
        ["backup", "--help"].as_slice(),
        ["completion", "--help"].as_slice(),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("run focused help");
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
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
        let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
            .args(["completion", shell])
            .output()
            .expect("render completion");
        assert_eq!(out.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("__complete"), "{shell} completion should use dynamic provider");
        assert!(stdout.contains("inspect"));
        assert!(stdout.contains("backup"));
    }

    let onlyids = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .args(["__complete", "onlyid", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(onlyids.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&onlyids.stdout).lines().any(|s| s == "1402259934"));

    let indices = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .args([
            "__complete",
            "index",
            "--onlyid",
            "1402259934",
            "--backup-dir",
        ])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(indices.status.code(), Some(0));
    let lines: Vec<_> = String::from_utf8_lossy(&indices.stdout).lines().map(str::to_string).collect();
    assert_eq!(lines, vec!["1", "2"]);
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
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bin"))
        .unwrap();

    let inspect = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .arg("inspect")
        .arg(&file)
        .arg("7")
        .output()
        .unwrap();
    assert_eq!(inspect.status.code(), Some(0), "{}", String::from_utf8_lossy(&inspect.stderr));
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("EDPF magic"));

    let verify = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .args([
            "backup",
            "verify",
            "--onlyid",
            "1402259934",
            "--index",
            "1",
            "--backup-dir",
        ])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(verify.status.code(), Some(0), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert_eq!(stdout.lines().filter(|l| l.starts_with('✓')).count(), 1);
}

#[test]
fn contradictory_inspect_flags_fail_with_focused_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .env("NO_COLOR", "1")
        .args(["inspect", "7", "--raw", "--hex"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("不能同时使用"));
    assert!(stdout.contains("用法: nopwd inspect"));
    assert!(!stdout.contains("cems 加密 U 盘"));
}

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
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bin"))
        .unwrap();
    let bin = env!("CARGO_BIN_EXE_nopwd");
    let script = format!(
        "\"{}\" inspect \"{}\" --hex | head -n 1 >/dev/null",
        bin,
        file.display()
    );
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("run nopwd through head");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("Broken pipe") && !stderr.contains("panicked at"),
        "管道提前关闭不应触发 Rust panic: {stderr}"
    );
}
