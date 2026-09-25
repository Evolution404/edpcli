use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use edpcli::tui::{
    keymap::{KeyMapper, TuiAction, INSPECT_HELP, NORMAL_HELP},
    state::InputMode,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(ch: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

#[test]
fn normal_navigation_uses_vim_semantics_without_workspace_side_effects() {
    let mut open_mapper = KeyMapper::new();
    assert_eq!(
        open_mapper.map(InputMode::Normal, key(KeyCode::Enter)),
        Some(TuiAction::Activate)
    );
    assert_eq!(
        open_mapper.map(InputMode::Normal, key(KeyCode::Char('o'))),
        Some(TuiAction::Open)
    );

    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('j'))),
        Some(TuiAction::MoveDown)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('k'))),
        Some(TuiAction::MoveUp)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('h'))),
        Some(TuiAction::MoveLeft)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('l'))),
        Some(TuiAction::MoveRight)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Left)),
        Some(TuiAction::MoveLeft)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Right)),
        Some(TuiAction::MoveRight)
    );
}

#[test]
fn g_prefix_owns_workspace_navigation_and_inspect_jump() {
    let mut mapper = KeyMapper::new();
    for (second, expected) in [
        ('g', TuiAction::Top),
        ('t', TuiAction::WorkspaceNext),
        ('T', TuiAction::WorkspacePrevious),
        ('d', TuiAction::WorkspaceDevices),
        ('b', TuiAction::WorkspaceBackups),
        ('p', TuiAction::WorkspaceProvision),
        ('i', TuiAction::WorkspaceInspect),
        ('l', TuiAction::InspectJump),
    ] {
        assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
        assert_eq!(
            mapper.map(InputMode::Normal, key(KeyCode::Char(second))),
            Some(expected),
            "g{second}"
        );
    }
}

#[test]
fn single_g_invalid_or_timed_out_prefix_never_executes_jump() {
    let mut mapper = KeyMapper::new();
    let now = Instant::now();
    assert_eq!(
        mapper.map_at(InputMode::Normal, key(KeyCode::Char('g')), now),
        None
    );
    assert_eq!(
        mapper.map_at(
            InputMode::Normal,
            key(KeyCode::Char('x')),
            now + Duration::from_millis(100)
        ),
        None
    );

    assert_eq!(
        mapper.map_at(InputMode::Normal, key(KeyCode::Char('g')), now),
        None
    );
    assert_eq!(
        mapper.map_at(
            InputMode::Normal,
            key(KeyCode::Char('l')),
            now + Duration::from_secs(2)
        ),
        Some(TuiAction::MoveRight),
        "timed-out g must be cancelled; l becomes ordinary right navigation"
    );
}

#[test]
fn ctrl_w_prefix_and_tab_are_panel_navigation_only() {
    let mut mapper = KeyMapper::new();
    for (second, expected) in [
        (KeyCode::Char('h'), TuiAction::PanelLeft),
        (KeyCode::Char('j'), TuiAction::PanelDown),
        (KeyCode::Char('k'), TuiAction::PanelUp),
        (KeyCode::Char('l'), TuiAction::PanelRight),
        (KeyCode::Char('w'), TuiAction::PanelNext),
        (KeyCode::Char('W'), TuiAction::PanelPrevious),
    ] {
        assert_eq!(mapper.map(InputMode::Normal, ctrl('w')), None);
        assert_eq!(mapper.map(InputMode::Normal, key(second)), Some(expected));
    }

    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Tab)),
        Some(TuiAction::PanelNext)
    );
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
        ),
        Some(TuiAction::PanelPrevious)
    );
}

#[test]
fn insert_mode_treats_vim_action_letters_as_text_and_arrows_as_cursor_motion() {
    let mut mapper = KeyMapper::new();
    for ch in ['h', 'j', 'k', 'l', 'g', 'd', 'r', 'f'] {
        assert_eq!(
            mapper.map(InputMode::Insert, key(KeyCode::Char(ch))),
            Some(TuiAction::Text(ch)),
            "{ch} must remain text in Insert"
        );
    }
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Left)),
        Some(TuiAction::CursorLeft)
    );
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Right)),
        Some(TuiAction::CursorRight)
    );
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Home)),
        Some(TuiAction::CursorHome)
    );
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Enter)),
        Some(TuiAction::Submit)
    );
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Esc)),
        Some(TuiAction::Back)
    );
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::End)),
        Some(TuiAction::CursorEnd)
    );
}

#[test]
fn search_and_command_modes_consume_text_before_normal_bindings() {
    let mut mapper = KeyMapper::new();
    for mode in [InputMode::Search, InputMode::Command] {
        assert_eq!(
            mapper.map(mode, key(KeyCode::Char('g'))),
            Some(TuiAction::Text('g'))
        );
        assert_eq!(
            mapper.map(mode, key(KeyCode::Char('r'))),
            Some(TuiAction::Text('r'))
        );
        assert_eq!(
            mapper.map(mode, key(KeyCode::Enter)),
            Some(TuiAction::Submit)
        );
        assert_eq!(mapper.map(mode, key(KeyCode::Esc)), Some(TuiAction::Back));
    }
}

#[test]
fn confirm_mode_has_uniform_yes_no_escape_contract_without_weakening_typed_yes() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(InputMode::Confirm, key(KeyCode::Char('y'))),
        Some(TuiAction::Confirm)
    );
    assert_eq!(
        mapper.map(InputMode::Confirm, key(KeyCode::Char('n'))),
        Some(TuiAction::Cancel)
    );
    assert_eq!(
        mapper.map(InputMode::Confirm, key(KeyCode::Esc)),
        Some(TuiAction::Cancel)
    );
    assert_eq!(
        mapper.map(InputMode::Confirm, key(KeyCode::Char('Y'))),
        Some(TuiAction::Text('Y'))
    );
    assert_eq!(
        mapper.map(InputMode::Confirm, key(KeyCode::Enter)),
        Some(TuiAction::Submit)
    );
}

#[test]
fn release_events_never_reach_keymap() {
    let event = KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(KeyMapper::new().map(InputMode::Normal, event), None);
}

#[test]
fn help_registry_is_the_same_metadata_source_for_core_and_inspect_hints() {
    assert!(NORMAL_HELP
        .iter()
        .any(|binding| binding.keys == "j/k" && binding.label == "Move"));
    assert!(NORMAL_HELP
        .iter()
        .any(|binding| binding.keys == "r" && binding.action == TuiAction::Refresh));
    assert!(INSPECT_HELP
        .iter()
        .any(|binding| binding.keys == "gl" && binding.action == TuiAction::InspectJump));
    assert!(INSPECT_HELP
        .iter()
        .any(|binding| binding.keys == "h/l" && binding.label == "Fold"));
    assert!(INSPECT_HELP
        .iter()
        .any(|binding| binding.keys == "o" && binding.action == TuiAction::Open));
}
