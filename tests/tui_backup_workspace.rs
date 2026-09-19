mod common;

use edpcli::application::scan_backup_workspace;
use edpcli::tui::state::{AppState, NavCommand, Workspace};

#[test]
fn backup_workspace_uses_one_based_global_selector_indices() {
    let rows = scan_backup_workspace(std::path::Path::new(common::FIXTURE_DIR));
    for (offset, row) in rows.iter().enumerate() {
        assert_eq!(row.index, offset + 1);
    }
}

#[test]
fn vim_horizontal_navigation_switches_workspaces() {
    let mut state = AppState::new();
    assert_eq!(state.workspace(), Workspace::Devices);

    state.navigate(NavCommand::Right, 20);
    assert_eq!(state.workspace(), Workspace::Backups);

    state.navigate(NavCommand::Left, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
}

#[test]
fn backup_scans_are_background_tasks_not_redraw_work() {
    let task = include_str!("../src/tui/task.rs");
    let render = include_str!("../src/tui/render.rs");

    assert!(task.contains("request_backup_scan"));
    assert!(task.contains("scan_backup_workspace"));
    assert!(!render.contains("scan_backup_workspace"));
}
