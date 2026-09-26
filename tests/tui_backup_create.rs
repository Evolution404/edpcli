use edpcli::tui::command::{parse_command, PaletteAction};
use edpcli::tui::state::{WriteIntent, WriteKind};
use edpcli::tui::{parse_resume_args, resume_argv};

fn pin() -> edpcli::tui::state::ExpectedIdentity {
    edpcli::tui::state::ExpectedIdentity {
        serial_sha256: Some("a".repeat(64)),
        serial_quality: edpcli::application::media_identity::SerialQuality::Usable,
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        total_sectors: Some(122_880_000),
        logical_sector_size: Some(512),
        device_id: Some("disk&ven_netac&prod_onlydisk".into()),
        onlyid: Some("1402259934".into()),
        protocol_image_sha256: "b".repeat(64),
    }
}

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
        expected_identity: Some(pin()),
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
        "--_resume-identity-pin".to_string(),
        serde_json::to_string(&pin()).unwrap(),
    ];
    assert_eq!(
        parse_resume_args(&argv).expect("backup create resume"),
        Some(WriteIntent {
            kind: WriteKind::BackupCreate,
            disk: 6,
            backup: None,
            expected_identity: Some(pin()),
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
