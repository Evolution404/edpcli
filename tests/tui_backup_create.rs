use edpcli::tui::command::{parse_command, PaletteAction};
use edpcli::tui::state::{WriteIntent, WriteKind};
use edpcli::tui::{parse_resume_args, resume_argv};

#[test]
fn backup_create_has_a_tui_navigation_and_palette_action() {
    assert_eq!(
        parse_command("backup-create").expect("backup-create palette command"),
        PaletteAction::BackupCreate
    );
    let keymap = include_str!("../src/tui/keymap.rs");
    assert!(keymap.contains("KeyCode::Char('b') => Some(TuiAction::BackupCreate)"));
}

#[test]
fn backup_create_resume_pins_only_the_device() {
    let intent = WriteIntent {
        kind: WriteKind::BackupCreate,
        disk: 6,
        backup: None,
        expected_identity: None,
    };
    let argv = resume_argv(&intent);
    assert!(argv
        .windows(2)
        .any(|pair| { pair[0] == "--_resume-kind" && pair[1] == "backup-create" }));
    assert!(!argv.iter().any(|arg| arg == "--_resume-backup"));
}

#[test]
fn numeric_backup_create_resume_parses_without_becoming_a_write_wizard() {
    let argv = vec![
        "tui".to_string(),
        "--_resume-kind".to_string(),
        "backup-create".to_string(),
        "--_resume-disk".to_string(),
        "6".to_string(),
    ];
    assert_eq!(
        parse_resume_args(&argv).expect("backup create resume"),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: None,
        })
    );
}

#[test]
fn tui_backup_create_reuses_the_existing_read_only_application_flow() {
    let task = include_str!("../src/tui/backups/task.rs");
    assert!(task.contains("request_backup_create"));
    assert!(task.contains("backup_create_on_disk"));
    assert!(!task.contains("crate::diskio"));
    assert!(!task.contains("FileDev::open_rdonly"));
    assert!(!task.contains("raw_path("));
}
