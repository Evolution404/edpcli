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
    let task = include_str!("../src/tui/backups/task.rs");
    assert!(task.contains("delete_backup_exact"));
    assert!(!task.contains("remove_file("));

    // 统一删除/保留策略服务：执行原语与保留底线必须住在共享服务里，
    // 前端不得自带副本；application 仅保留薄封装再导出。
    let service = include_str!("../src/application/backup.rs");
    assert!(service.contains("pub fn delete_backup_exact"));
    assert!(service.contains("delete_entry_verified"));
    assert!(service.contains("fn enforce_retention_floor"));
    assert!(service.contains("pub struct DeleteSession"));

    let application = include_str!("../src/application.rs");
    assert!(application.contains("pub mod backup;"));
    assert!(application.contains("pub use backup::delete_backup_exact;"));
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
