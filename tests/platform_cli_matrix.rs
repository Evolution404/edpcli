use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use edpcli::common::{EXIT_OK, EXIT_TARGET, METADATA_IMAGE_LEN};
use edpcli::edpb::{self, CoreCapture};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .args(args)
        .output()
        .expect("run edpcli")
}

fn assert_invalid_target_is_rejected_before_write(args: &[&str]) {
    let output = run(args);
    assert_eq!(
        output.status.code(),
        Some(EXIT_TARGET),
        "args={args:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "edpcli-platform-matrix-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn synthetic_backup(dir: &TempDir) -> PathBuf {
    let path = dir.0.join(
        "disk6_123456_vid3535_pid6300_disk&ven_aigo&prod_u335_onlyid1987718388_20260918_120000.edpb",
    );
    let data = vec![0u8; METADATA_IMAGE_LEN];
    let capture = CoreCapture {
        snapshot_id: "platform-matrix".into(),
        created_epoch: 1_790_000_000,
        disk_number: Some(6),
        vid: "3535".into(),
        pid: "6300".into(),
        device_id: "disk&ven_aigo&prod_u335".into(),
        onlyid: Some("1987718388".into()),
        total_sectors: Some(123_456),
        logical_sector_size: 512,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: &data,
    };
    edpb::write_core_backup(&path, &capture).expect("write EDPB");
    path
}

fn assert_ok(output: std::process::Output, context: &str) {
    assert_eq!(
        output.status.code(),
        Some(EXIT_OK),
        "{context}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn list_is_read_only_and_available_on_every_platform() {
    // This subprocess matrix must stay hermetic even when the host has a real
    // external disk whose raw identity needs elevation. The internal sentinel
    // suppresses interactive re-elevation while still exercising the same
    // read-only list path; elevation policy itself has dedicated unit tests.
    assert_ok(run(&["list", "--_elevated"]), "list");
}

#[test]
fn detailed_version_reports_native_build_identity_on_every_platform() {
    let output = run(&["version"]);
    assert_eq!(output.status.code(), Some(EXIT_OK), "version failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("edpcli {}", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains(&format!("目标: {}", env!("EDPCLI_BUILD_TARGET"))));
    assert!(stdout.contains("构建时间:"));
    assert!(stdout.contains("Git:"));
    assert!(stdout.contains("Rust:"));
}

#[test]
fn dash_version_stays_single_line_for_script_compatibility() {
    let output = run(&["--version"]);
    assert_eq!(output.status.code(), Some(EXIT_OK), "--version failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("edpcli {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn info_inspect_and_backup_work_offline_on_every_platform() {
    let dir = TempDir::new("offline");
    let backup = synthetic_backup(&dir);
    let backup_s = backup.to_str().expect("utf8 backup path");
    let dir_s = dir.0.to_str().expect("utf8 backup dir");

    assert_ok(run(&["info", backup_s]), "info backup");
    assert_ok(
        run(&["inspect", "meta", backup_s, "--lba", "8"]),
        "inspect backup",
    );
    assert_ok(
        run(&["backup", "verify", backup_s, "--backup-dir", dir_s]),
        "backup verify",
    );
}

#[test]
fn provision_rejects_nonexistent_explicit_disk_on_every_platform() {
    assert_invalid_target_is_rejected_before_write(&[
        "provision",
        "plan",
        "--disk",
        "4294967295",
        "--mode",
        "0",
    ]);
}

#[test]
fn backup_create_rejects_nonexistent_explicit_disk_on_every_platform() {
    assert_invalid_target_is_rejected_before_write(&["backup", "create", "--disk", "4294967295"]);
}

#[test]
fn restore_rejects_nonexistent_explicit_disk_on_every_platform() {
    assert_invalid_target_is_rejected_before_write(&[
        "backup",
        "restore",
        "--disk",
        "4294967295",
        "--yes",
    ]);
}
