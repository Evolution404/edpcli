mod common;

use std::fs;
use std::process::Command;

use common::*;
use edpcli::md5::md5_hex;

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
        fs::write(
            format!("{}.md5", dst.display()),
            format!("{}\n", md5_hex(&data)),
        )
        .unwrap();
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
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bin"))
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
        ["apply", "--help"].as_slice(),
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
        assert!(stdout.contains("metainfo") || stdout.contains("meta"));
    }

    let onlyids = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(["__complete", "onlyid", "--backup-dir"])
        .arg(&tmp.0)
        .output()
        .unwrap();
    assert_eq!(onlyids.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&onlyids.stdout)
        .lines()
        .any(|s| s == "1402259934"));

    let indices = Command::new(env!("CARGO_BIN_EXE_edpcli"))
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
    let lines: Vec<_> = String::from_utf8_lossy(&indices.stdout)
        .lines()
        .map(str::to_string)
        .collect();
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

    let inspect = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .arg("inspect")
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
}

#[test]
fn contradictory_inspect_flags_fail_with_focused_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .env("NO_COLOR", "1")
        .args(["inspect", "--lba", "7", "--raw", "--hex"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("不能同时使用"));
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
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bin"))
        .unwrap();
    let bin = env!("CARGO_BIN_EXE_edpcli");
    let script = format!(
        "\"{}\" inspect \"{}\" --hex | head -n 1 >/dev/null",
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
