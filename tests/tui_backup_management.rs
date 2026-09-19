use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edpcli::tui::event::KeyMapper;
use edpcli::tui::state::NavCommand;

#[test]
fn backup_workspace_has_explicit_verify_and_delete_actions() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)),
        Some(NavCommand::VerifyBackup)
    );
    assert_eq!(
        mapper.map(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)),
        Some(NavCommand::BeginBackupDelete)
    );
}

#[test]
fn backup_delete_uses_shared_application_service_not_direct_filesystem_removal() {
    let task = include_str!("../src/tui/task.rs");
    assert!(task.contains("delete_backup_exact"));
    assert!(!task.contains("remove_file("));

    let application = include_str!("../src/application.rs");
    assert!(application.contains("pub fn delete_backup_exact"));
    assert!(application.contains("delete_entry_verified"));
}

#[test]
fn backup_page_exposes_the_complete_management_shortcuts() {
    let render = include_str!("../src/tui/render.rs");
    for label in ["v 校验", "D 删除", "R 恢复", "b 新建", "i 查看"] {
        assert!(
            render.contains(label),
            "missing backup shortcut hint: {label}"
        );
    }
}
