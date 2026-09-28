use crate::common;

use edpcli::application::scan_backup_workspace;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, Workspace},
};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

#[test]
fn backup_workspace_uses_one_based_global_selector_indices() {
    let rows = scan_backup_workspace(std::path::Path::new(common::FIXTURE_DIR));
    for (offset, row) in rows.iter().enumerate() {
        assert_eq!(row.index, offset + 1);
    }
}

#[test]
fn legacy_horizontal_workspace_navigation_is_removed_from_state_commands() {
    let state_source = include_str!("../src/tui/state.rs");
    assert!(!state_source.contains("NavCommand::Left"));
    assert!(!state_source.contains("NavCommand::Right"));
}

#[test]
fn top_level_workspace_navigation_cycles_devices_and_backups_only() {
    let mut state = AppState::new();
    assert_eq!(
        Workspace::TOP_LEVEL,
        [Workspace::Devices, Workspace::Backups]
    );
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
}

fn active_tab_text(state: &AppState) -> String {
    let backend = TestBackend::new(140, 32);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, state))
        .expect("draw TUI");
    let accent = edpcli::tui::theme::current().palette().accent;
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| {
            cell.style().fg == Some(accent)
                && cell.style().add_modifier.contains(Modifier::UNDERLINED)
        })
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn top_level_tabs_are_always_visible_and_active_page_is_highlighted() {
    let mut state = AppState::new();
    let active = active_tab_text(&state);
    assert!(active.contains('设'), "{active}");

    state.navigate(NavCommand::NextWorkspace, 20);
    let active = active_tab_text(&state);
    assert!(active.contains('份'), "{active}");
    state.navigate(NavCommand::NextWorkspace, 20);
    let active = active_tab_text(&state);
    assert!(active.contains('设'), "{active}");
}

#[test]
fn nested_workflows_keep_their_parent_top_level_tab_highlighted() {
    use edpcli::tui::state::AdvancedInspectSource;

    let mut provision = AppState::new();
    provision.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(provision.workspace(), Workspace::Provision);
    let active = active_tab_text(&provision);
    assert!(active.contains('设'), "{active}");
    assert!(!active.contains('份'), "{active}");

    let mut backup_inspect = AppState::new();
    backup_inspect.navigate(NavCommand::WorkspaceBackups, 20);
    assert!(
        backup_inspect.begin_advanced_inspect(AdvancedInspectSource::Backup(
            std::path::PathBuf::from("demo.edpb")
        ))
    );
    assert_eq!(backup_inspect.workspace(), Workspace::Inspect);
    let active = active_tab_text(&backup_inspect);
    assert!(active.contains('份'), "{active}");
    assert!(!active.contains('设'), "{active}");
}

#[test]
fn backup_scans_are_background_tasks_not_redraw_work() {
    let task = include_str!("../src/tui/task.rs");
    let render = include_str!("../src/tui/render.rs");

    assert!(task.contains("request_backup_scan"));
    assert!(task.contains("scan_backup_workspace"));
    assert!(!render.contains("scan_backup_workspace"));
}
