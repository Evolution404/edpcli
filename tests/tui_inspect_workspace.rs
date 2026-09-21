mod common;

use edpcli::application::inspect::load_backup_inspect;
use edpcli::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use edpcli::tui::state::{AppState, InspectMode, NavCommand};

#[test]
fn backup_inspect_reuses_domain_analyzer_for_all_metadata_lbas() {
    let Some(path) = common::fixture_bin("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    assert_eq!(workspace.views.len(), METADATA_SECTOR_COUNT);
    for (lba, view) in workspace.views.iter().enumerate() {
        assert_eq!(view.lba, lba as u32);
        assert_eq!(view.raw.len(), 512);
        assert_eq!(view.decoded.len(), 512);
    }
}

#[test]
fn backup_inspect_rejects_legacy_7168_byte_images() {
    let tmp = common::TmpDir::new("inspect_reject_7168");
    let path = tmp.0.join("legacy-lba0-13.bin");
    std::fs::write(&path, vec![0u8; METADATA_IMAGE_LEN + SECTOR]).unwrap();

    let err = load_backup_inspect(&path).expect_err("7168B legacy image must be rejected");
    assert!(err.contains("LBA0-12"), "{err}");
    assert!(err.contains(&METADATA_IMAGE_LEN.to_string()), "{err}");
}

#[test]
fn inspect_overlay_has_vim_lba_navigation_and_hex_mode_switching() {
    let mut state = AppState::new();
    state.open_inspect(METADATA_SECTOR_COUNT);

    assert_eq!(state.inspect_selected_lba(), Some(0));
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.inspect_selected_lba(), Some(1));
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(
        state.inspect_selected_lba(),
        Some((METADATA_SECTOR_COUNT - 1) as u32)
    );

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
    state.open_inspect(METADATA_SECTOR_COUNT);
    let effect = state.navigate(NavCommand::Escape, 20);
    assert_eq!(effect, edpcli::tui::state::StateEffect::None);
    assert!(state.inspect_selected_lba().is_none());
}
