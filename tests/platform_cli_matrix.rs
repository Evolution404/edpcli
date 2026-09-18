use std::process::Command;

use edpcli::common::EXIT_TARGET;

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

#[test]
fn apply_rejects_nonexistent_explicit_disk_on_every_platform() {
    assert_invalid_target_is_rejected_before_write(&[
        "apply",
        "--disk",
        "4294967295",
        "--force",
        "--yes",
    ]);
}

#[test]
fn restore_rejects_nonexistent_explicit_disk_on_every_platform() {
    assert_invalid_target_is_rejected_before_write(&[
        "restore",
        "--disk",
        "4294967295",
        "--yes",
    ]);
}
