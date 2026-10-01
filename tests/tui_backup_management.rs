use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edpcli::tui::keymap::{KeyMapper, TuiAction};
use edpcli::tui::state::InputMode;

#[test]
fn backup_workspace_has_explicit_verify_and_delete_actions() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)
        ),
        Some(TuiAction::ViewOrVerify)
    );
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)
        ),
        Some(TuiAction::Delete)
    );
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)
        ),
        None,
        "legacy D delete binding must not survive the unified keymap"
    );
}

#[test]
fn backup_workspace_uses_one_create_modal_and_only_keeps_explicit_single_key_actions() {
    let keymap = include_str!("../src/tui/keymap.rs");
    for legacy in [
        "KeyCode::Char('D')",
        "KeyCode::Char('X')",
        "KeyCode::Char('I')",
        "KeyCode::Char('B')",
        "KeyCode::Char('P')",
    ] {
        assert!(
            !keymap.contains(legacy),
            "legacy backup binding must stay removed: {legacy}"
        );
    }

    let state = include_str!("../src/tui/backups/state.rs");
    assert!(keymap.contains("KeyCode::Char('R') => Some(TuiAction::Restore)"));

    let render = include_str!("../src/tui/render.rs");
    let runtime_input = include_str!("../src/tui/runtime_input.rs");
    assert!(!state.contains("BackupCreateChoiceState"));
    assert!(!state.contains("begin_backup_create_choice"));
    assert!(!state.contains("create_choice:"));
    assert!(!runtime_input.contains("backup_choice"));
    assert!(render.contains("创建元数据备份"));
    assert!(render.contains("不会向目标设备写入"));
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
fn backup_help_registry_exposes_the_complete_management_shortcuts() {
    let keymap = include_str!("../src/tui/keymap/help.rs");
    for (key, label) in [
        ("v", "校验备份"),
        ("d", "删除备份"),
        ("R", "恢复备份"),
        ("b", "新建备份"),
        ("Enter / i", "检查备份"),
        ("Space", "勾选 / 取消勾选"),
    ] {
        assert!(
            keymap.contains(&format!("keys: \"{key}\""))
                && keymap.contains(&format!("label: \"{label}\"")),
            "missing backup shortcut binding: {key} -> {label}"
        );
    }
}
