use std::path::PathBuf;

use edpcli::tui::state::{WriteIntent, WriteKind};
use edpcli::tui::{parse_resume_args, resume_argv};

#[test]
fn elevation_resume_argv_pins_apply_disk() {
    let intent = WriteIntent {
        kind: WriteKind::Apply,
        disk: 6,
        backup: None,
    };
    let argv = resume_argv(&intent);
    assert_eq!(
        parse_resume_args(&argv).expect("resume apply"),
        Some(intent)
    );
}

#[test]
fn elevation_resume_argv_pins_restore_disk_and_backup() {
    let intent = WriteIntent {
        kind: WriteKind::Restore,
        disk: 9,
        backup: Some(PathBuf::from("backup/a b.bin")),
    };
    let argv = resume_argv(&intent);
    assert_eq!(
        parse_resume_args(&argv).expect("resume restore"),
        Some(intent)
    );
}

#[test]
fn malformed_resume_state_fails_closed() {
    let argv = vec![
        "tui".to_string(),
        "--_resume-kind".to_string(),
        "restore".to_string(),
        "--_resume-disk".to_string(),
        "7".to_string(),
    ];
    assert!(parse_resume_args(&argv).is_err());
}
