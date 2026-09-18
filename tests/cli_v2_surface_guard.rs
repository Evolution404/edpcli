use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn manifest_file(path: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn assert_no_v1_grammar(surface: &str, name: &str) {
    for removed in [
        "edpcli run",
        "edpcli restore",
        "edpcli meta",
        "edpcli metainfo",
        "backup rm",
        "--onlyid",
        "--index",
    ] {
        assert!(
            !surface.contains(removed),
            "{name} 重新暴露已删除的 v1 grammar: {removed}"
        );
    }
}

#[test]
fn public_docs_do_not_teach_removed_v1_grammar() {
    assert_no_v1_grammar(&manifest_file("README.md"), "README.md");
    assert_no_v1_grammar(&manifest_file("docs/USAGE.md"), "docs/USAGE.md");
}

#[test]
fn help_and_completion_expose_only_v2_surface() {
    for args in [
        vec!["help"],
        vec!["backup", "--help"],
        vec!["inspect", "--help"],
        vec!["info", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .env("NO_COLOR", "1")
            .args(&args)
            .output()
            .expect("run edpcli help");
        assert_eq!(
            output.status.code(),
            Some(0),
            "args={args:?}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_no_v1_grammar(&String::from_utf8_lossy(&output.stdout), "help");
    }

    for shell in ["zsh", "bash", "fish"] {
        let output = Command::new(env!("CARGO_BIN_EXE_edpcli"))
            .args(["completion", shell])
            .output()
            .expect("render completion");
        assert_eq!(output.status.code(), Some(0), "{shell}");
        assert_no_v1_grammar(
            &String::from_utf8_lossy(&output.stdout),
            &format!("{shell} completion"),
        );
    }
}

#[test]
fn v2_surface_contains_required_task_commands() {
    let readme = manifest_file("README.md");
    let usage = manifest_file("docs/USAGE.md");
    for required in [
        "edpcli list",
        "edpcli info",
        "edpcli apply --dry-run",
        "edpcli backup create",
        "edpcli backup restore",
        "edpcli backup verify",
        "edpcli backup delete",
        "edpcli backup prune",
        "edpcli inspect --lba",
    ] {
        assert!(readme.contains(required), "README missing {required}");
        assert!(usage.contains(required), "USAGE missing {required}");
    }
}

#[test]
fn package_version_is_cli_v2_major() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "2.0.0");
}

#[test]
fn completion_does_not_load_full_backup_catalog() {
    let source = manifest_file("src/completion.rs");
    for heavyweight in ["BackupCatalog", "BackupSelector"] {
        assert!(
            !source.contains(heavyweight),
            "completion 不应为了 Tab 补全加载完整备份目录模型: {heavyweight}"
        );
    }
}
