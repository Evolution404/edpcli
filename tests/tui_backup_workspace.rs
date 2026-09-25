mod common;

use edpcli::application::scan_backup_workspace;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, Workspace},
};
use ratatui::{backend::TestBackend, Terminal};

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
fn tab_navigation_switches_workspaces_without_removing_vim_navigation() {
    let mut state = AppState::new();
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
}

fn highlighted_text(state: &AppState) -> String {
    let backend = TestBackend::new(140, 32);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, state))
        .expect("draw TUI");
    let selection = edpcli::tui::theme::current().palette().selection;
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.style().bg == Some(selection))
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn workspace_tabs_are_always_visible_and_active_page_is_highlighted() {
    let mut state = AppState::new();
    let highlighted = highlighted_text(&state);
    assert!(highlighted.contains('设'), "{highlighted}");

    state.navigate(NavCommand::NextWorkspace, 20);
    let highlighted = highlighted_text(&state);
    assert!(highlighted.contains('份'), "{highlighted}");
}

#[test]
fn backup_scans_are_background_tasks_not_redraw_work() {
    let task = include_str!("../src/tui/task.rs");
    let render = include_str!("../src/tui/render.rs");

    assert!(task.contains("request_backup_scan"));
    assert!(task.contains("scan_backup_workspace"));
    assert!(!render.contains("scan_backup_workspace"));
}
