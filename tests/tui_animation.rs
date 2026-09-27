use edpcli::tui::{
    render,
    state::{AppState, WriteKind},
};
use ratatui::{backend::TestBackend, Terminal};

fn render_text(state: &AppState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, state))
        .expect("draw TUI");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn animation_tick_is_state_only_and_monotonic() {
    let mut state = AppState::new();
    assert_eq!(state.animation_frame(), 0);
    state.advance_animation();
    state.advance_animation();
    assert_eq!(state.animation_frame(), 2);
}

#[test]
fn wide_devices_keep_only_compact_core_indicator_during_normal_navigation() {
    let state = AppState::new();
    let text = render_text(&state, 160, 30);
    assert!(text.contains("CORE ◇ STABLE"), "{text}");
    assert!(!text.contains("EDP CORE · LIVE"), "{text}");
    assert!(text.replace(' ', "").contains("设备状态"), "{text}");
}

#[test]
fn wizard_keeps_compact_core_indicator_without_business_animation_sidebar() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::BackupCreate, 6, None);
    let text = render_text(&state, 160, 30);
    assert!(text.contains("Create Backup"), "{text}");
    assert!(text.contains("CORE ◇ ACTIVE"), "{text}");
    assert!(!text.contains("EDP CORE · LIVE"), "{text}");
    assert!(!text.contains("ACTIVITY  // USER FLOW"), "{text}");
}

#[test]
fn narrow_tui_keeps_operation_state_in_compact_indicator() {
    let mut state = AppState::new();
    state.begin_write_wizard(WriteKind::BackupCreate, 6, None);
    let text = render_text(&state, 80, 24);
    assert!(text.contains("CORE ◇ ACTIVE"), "{text}");
    assert!(!text.contains("EDP CORE · LIVE"), "{text}");
}

#[test]
fn narrow_tui_falls_back_to_compact_animated_core_indicator() {
    let mut state = AppState::new();
    let first = render_text(&state, 80, 24);
    assert!(first.contains("CORE ◇ STABLE"), "{first}");
    assert!(!first.contains("EDP CORE · LIVE"), "{first}");

    state.advance_animation();
    state.advance_animation();
    let next = render_text(&state, 80, 24);
    assert!(next.contains("CORE ◆ STABLE"), "{next}");
    assert_ne!(first, next);
}
