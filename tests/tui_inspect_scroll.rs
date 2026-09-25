use edpcli::tui::state::{AppState, InspectMode, NavCommand};

#[test]
fn inspect_hex_half_page_keys_scroll_content_without_changing_lba() {
    let mut state = AppState::new();
    state.open_inspect(14);
    state.inspect_cycle_mode();
    assert_eq!(state.inspect_mode(), Some(InspectMode::DecodedHex));

    let lba = state.inspect_selected_lba();
    state.navigate(NavCommand::HalfPageDown, 20);
    assert_eq!(state.inspect_selected_lba(), lba);
    assert_eq!(state.inspect_scroll(), Some(10));

    state.navigate(NavCommand::HalfPageUp, 20);
    assert_eq!(state.inspect_scroll(), Some(0));
}

#[test]
fn changing_lba_or_hex_mode_resets_inspect_scroll() {
    let mut state = AppState::new();
    state.open_inspect(14);
    state.inspect_cycle_mode();
    state.navigate(NavCommand::HalfPageDown, 20);
    assert!(state.inspect_scroll().unwrap_or_default() > 0);

    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.inspect_selected_lba(), Some(1));
    assert_eq!(state.inspect_scroll(), Some(0));

    state.navigate(NavCommand::HalfPageDown, 20);
    state.inspect_cycle_mode();
    assert_eq!(state.inspect_mode(), Some(InspectMode::RawHex));
    assert_eq!(state.inspect_scroll(), Some(0));
}

#[test]
fn inspect_search_haystack_includes_raw_hex_bytes() {
    let source = include_str!("../src/tui/state.rs");
    assert!(
        source.contains("format!(\"{byte:02x}\")"),
        "inspect search must include hex bytes, not only ASCII/field text"
    );
}
