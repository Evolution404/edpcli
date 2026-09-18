mod common;

use edpcli::application::inspect::load_backup_inspect;
use edpcli::tui::state::{AppState, InspectMode, NavCommand};

#[test]
fn backup_inspect_reuses_domain_analyzer_for_all_metadata_lbas() {
    let Some(path) = common::fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    assert_eq!(workspace.views.len(), 14);
    for (lba, view) in workspace.views.iter().enumerate() {
        assert_eq!(view.lba, lba as u32);
        assert_eq!(view.raw.len(), 512);
        assert_eq!(view.decoded.len(), 512);
    }
}

#[test]
fn inspect_overlay_has_vim_lba_navigation_and_hex_mode_switching() {
    let mut state = AppState::new();
    state.open_inspect(14);

    assert_eq!(state.inspect_selected_lba(), Some(0));
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.inspect_selected_lba(), Some(1));
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.inspect_selected_lba(), Some(13));

    assert_eq!(state.inspect_mode(), Some(InspectMode::Fields));
    state.navigate(NavCommand::Right, 20);
    assert_eq!(state.inspect_mode(), Some(InspectMode::DecodedHex));
    state.navigate(NavCommand::Right, 20);
    assert_eq!(state.inspect_mode(), Some(InspectMode::RawHex));
    state.navigate(NavCommand::Left, 20);
    assert_eq!(state.inspect_mode(), Some(InspectMode::DecodedHex));
}

#[test]
fn inspect_overlay_escape_returns_to_workspace_instead_of_exiting() {
    let mut state = AppState::new();
    state.open_inspect(14);
    let effect = state.navigate(NavCommand::Escape, 20);
    assert_eq!(effect, edpcli::tui::state::StateEffect::None);
    assert!(state.inspect_selected_lba().is_none());
}
