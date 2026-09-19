use edpcli::tui::state::{AppState, InputMode, NavCommand, StateEffect};

#[test]
fn vim_vertical_navigation_is_bounded() {
    let mut state = AppState::new();
    state.set_item_count(5);

    assert_eq!(state.selected(), 0);
    assert_eq!(state.navigate(NavCommand::Up, 10), StateEffect::None);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::Down, 10);
    state.navigate(NavCommand::Down, 10);
    assert_eq!(state.selected(), 2);

    for _ in 0..10 {
        state.navigate(NavCommand::Down, 10);
    }
    assert_eq!(state.selected(), 4);
}

#[test]
fn vim_top_bottom_and_half_page_navigation_are_deterministic() {
    let mut state = AppState::new();
    state.set_item_count(100);

    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.selected(), 99);

    state.navigate(NavCommand::Top, 20);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::HalfPageDown, 20);
    assert_eq!(state.selected(), 10);

    state.navigate(NavCommand::HalfPageUp, 20);
    assert_eq!(state.selected(), 0);
}

#[test]
fn search_command_and_help_modes_return_to_normal_with_escape() {
    let mut state = AppState::new();

    state.navigate(NavCommand::Search, 10);
    assert_eq!(state.input_mode(), InputMode::Search);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);

    state.navigate(NavCommand::CommandPalette, 10);
    assert_eq!(state.input_mode(), InputMode::Command);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);

    state.navigate(NavCommand::Help, 10);
    assert_eq!(state.input_mode(), InputMode::Help);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);
}

#[test]
fn quit_is_deferred_during_critical_write_phase() {
    let mut state = AppState::new();

    assert_eq!(
        state.navigate(NavCommand::Quit, 10),
        StateEffect::ExitRequested
    );

    state.set_critical_operation(true);
    assert_eq!(
        state.navigate(NavCommand::Quit, 10),
        StateEffect::ExitDeferred
    );
    assert_eq!(
        state.navigate(NavCommand::Escape, 10),
        StateEffect::ExitDeferred
    );
    assert!(state.exit_pending());

    state.set_critical_operation(false);
    assert_eq!(state.take_deferred_exit(), StateEffect::ExitRequested);
    assert!(!state.exit_pending());
}

#[test]
fn empty_lists_never_underflow_selection() {
    let mut state = AppState::new();
    state.set_item_count(0);

    for cmd in [
        NavCommand::Down,
        NavCommand::Up,
        NavCommand::Top,
        NavCommand::Bottom,
        NavCommand::HalfPageDown,
        NavCommand::HalfPageUp,
    ] {
        state.navigate(cmd, 20);
        assert_eq!(state.selected(), 0);
    }
}
