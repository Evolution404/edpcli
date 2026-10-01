use super::*;
use crate::tui::clipboard::ClipboardOutcome;

struct NullClipboard;

impl ClipboardBackend for NullClipboard {
    fn copy(&mut self, _content: &str) -> ClipboardOutcome {
        ClipboardOutcome::Unsupported
    }
}

#[test]
fn backup_device_tree_does_not_swallow_panel_navigation_actions() {
    let mut state = AppState::new();
    let _ = state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupDevices);
    let mut clipboard = NullClipboard;

    let right = dispatch_action(
        &mut state,
        TuiAction::PanelRight,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(right.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);

    let left = dispatch_action(
        &mut state,
        TuiAction::PanelLeft,
        WidgetRole::Table,
        20,
        120,
        &mut clipboard,
    );
    assert!(left.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupDevices);

    let down = dispatch_action(
        &mut state,
        TuiAction::PanelDown,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(down.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupSummary);

    let up = dispatch_action(
        &mut state,
        TuiAction::PanelUp,
        WidgetRole::Other,
        20,
        120,
        &mut clipboard,
    );
    assert!(up.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);

    state.focus_backups_pane(PaneId::BackupDevices);
    let next = dispatch_action(
        &mut state,
        TuiAction::PanelNext,
        WidgetRole::Tree,
        20,
        120,
        &mut clipboard,
    );
    assert!(next.handled);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}
