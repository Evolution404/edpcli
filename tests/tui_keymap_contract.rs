use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use edpcli::tui::{
    keymap::{KeyMapper, TuiAction, WidgetRole, INSPECT_HELP, NORMAL_HELP},
    state::InputMode,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn h_l_follow_widget_role_without_changing_insert_text() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('h'))
        ),
        Some(TuiAction::TableScrollLeft)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('l'))
        ),
        Some(TuiAction::TableScrollRight)
    );
    assert_eq!(
        mapper.map_for_role(InputMode::Normal, WidgetRole::Tree, key(KeyCode::Char('h'))),
        Some(TuiAction::MoveLeft)
    );
    assert_eq!(
        mapper.map_for_role(InputMode::Normal, WidgetRole::Tree, key(KeyCode::Char('l'))),
        Some(TuiAction::MoveRight)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Insert,
            WidgetRole::Input,
            key(KeyCode::Char('h'))
        ),
        Some(TuiAction::Text('h'))
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Insert,
            WidgetRole::Input,
            key(KeyCode::Char('l'))
        ),
        Some(TuiAction::Text('l'))
    );
}

#[test]
fn chapter_11_single_key_actions_and_exit_contract() {
    let mut mapper = KeyMapper::new();
    for (code, expected) in [
        (KeyCode::Char('b'), Some(TuiAction::BackupCreate)),
        (KeyCode::Char('a'), Some(TuiAction::Add)),
        (KeyCode::Char('p'), None),
        (KeyCode::Char('w'), None),
        (KeyCode::Char('q'), Some(TuiAction::Quit)),
        (KeyCode::Esc, Some(TuiAction::Back)),
    ] {
        assert_eq!(mapper.map(InputMode::Normal, key(code)), expected);
    }
}

fn ctrl(ch: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

fn source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source
        .find(start)
        .unwrap_or_else(|| panic!("missing source section start: {start}"));
    let tail = &source[start_index..];
    let end_index = tail
        .find(end)
        .unwrap_or_else(|| panic!("missing source section end: {end}"));
    &tail[..end_index]
}

fn contains_tokens_in_order(mut source: &str, tokens: &[&str]) -> bool {
    for token in tokens {
        let Some(index) = source.find(token) else {
            return false;
        };
        source = &source[index + token.len()..];
    }
    true
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
fn g_prefix_is_reserved_for_vim_top_and_inspect_jump_only() {
    let mut mapper = KeyMapper::new();
    for (second, expected) in [('g', TuiAction::Top), ('l', TuiAction::InspectJump)] {
        assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
        assert_eq!(
            mapper.map(InputMode::Normal, key(KeyCode::Char(second))),
            Some(expected),
            "g{second}"
        );
    }
    for second in ['t', 'T', 'd', 'b', 'p', 'i'] {
        assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
        assert_eq!(
            mapper.map(InputMode::Normal, key(KeyCode::Char(second))),
            None,
            "g{second} must not remain a workspace/function shortcut"
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
fn tab_switches_top_level_tabs_and_ctrl_w_owns_panel_navigation() {
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
        Some(TuiAction::WorkspaceNext)
    );
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
        ),
        Some(TuiAction::WorkspacePrevious)
    );
}

#[test]
fn normal_mode_keeps_inspect_and_backup_as_single_key_actions() {
    let mut mapper = KeyMapper::new();
    assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('p'))), None);
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('i'))),
        Some(TuiAction::Insert)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('b'))),
        Some(TuiAction::BackupCreate)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('R'))),
        Some(TuiAction::Restore)
    );

    let event_loop = include_str!("../src/tui/mod.rs");
    let dispatch = source_section(
        event_loop,
        "fn dispatch_tui_action(",
        "fn open_advanced_inspect_selection(",
    );
    assert!(!dispatch.contains("TuiAction::Plan if state.workspace() == state::Workspace::Devices"));
    assert!(!dispatch.contains("TuiAction::Activate | TuiAction::Open => match state.workspace()"));
    assert!(contains_tokens_in_order(
        dispatch,
        &[
            "TuiAction::Insert",
            "if matches!(",
            "state.workspace()",
            "state::Workspace::Devices | state::Workspace::Backups",
            "NavCommand::OpenInspect",
        ],
    ));
    assert!(contains_tokens_in_order(
        dispatch,
        &[
            "TuiAction::BackupCreate",
            "if matches!(",
            "state.workspace()",
            "state::Workspace::Devices | state::Workspace::Backups",
            "state.begin_backup_create_choice()",
        ],
    ));
    assert!(contains_tokens_in_order(
        dispatch,
        &[
            "TuiAction::Restore",
            "state.workspace() == state::Workspace::Backups",
            "NavCommand::BeginRestore",
        ],
    ));
}

#[test]
fn insert_mode_treats_vim_action_letters_as_text_and_arrows_as_cursor_motion() {
    let mut mapper = KeyMapper::new();
    for ch in ['h', 'j', 'k', 'l', 'g', 'd', 'r', 'f', 'p', 'i', 'a', 'R'] {
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
    assert_eq!(mapper.map(InputMode::Insert, key(KeyCode::Tab)), None);
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
        assert_eq!(mapper.map(mode, key(KeyCode::Tab)), None);
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
fn provision_form_enter_generates_plan_instead_of_editing_or_toggling() {
    let event_loop = include_str!("../src/tui/mod.rs");
    let form = source_section(
        event_loop,
        "ProvisionStage::Form => match action {",
        "ProvisionStage::Review =>",
    );
    assert!(
        contains_tokens_in_order(
            form,
            &[
                "TuiAction::Insert",
                "state.provision_begin_insert()",
                "TuiAction::Activate",
                "start_provision_plan(&mut state, &mut tasks)",
            ],
        ),
        "Provision Form Insert must edit while Enter/Activate generates the plan"
    );
    assert!(
        !contains_tokens_in_order(
            form,
            &["TuiAction::Activate", "state.provision_begin_insert()"],
        ),
        "Provision Form Enter must not enter Insert mode or toggle checkbox state"
    );

    let render = include_str!("../src/tui/render.rs");
    assert!(
        !render.contains("i/Enter 进入 Insert"),
        "help/footer must not advertise the regressed Enter-to-edit behavior"
    );
    assert!(
        render.contains("Enter 生成计划"),
        "Provision Form footer must advertise Enter as generate-plan"
    );
}

#[test]
fn user_visible_inspect_hints_point_to_full_disk_tree_entry() {
    let devices = include_str!("../src/tui/devices/render.rs");
    assert!(devices.contains("Span::styled(\"i\", accent())"));
    assert!(devices.contains("Span::styled(\"Enter\", accent())"));
    assert!(!devices.contains("gi"));

    let event_loop = include_str!("../src/tui/mod.rs");
    assert!(event_loop.contains("NavCommand::OpenInspect =>"));
    assert!(!event_loop.contains("OpenAdvancedInspect"));
}

#[test]
fn legacy_flat_inspect_state_worker_and_renderer_are_removed() {
    let state = include_str!("../src/tui/state.rs");
    let inspect_state = include_str!("../src/tui/inspect/state.rs");
    let task = include_str!("../src/tui/task.rs");
    let inspect_task = include_str!("../src/tui/inspect/task.rs");
    let render = include_str!("../src/tui/inspect/render.rs");

    assert!(!state.contains("inspect_data:"));
    assert!(!state.contains("inspect_pending:"));
    assert!(!inspect_state.contains("struct InspectState"));
    assert!(!task.contains("WorkerResult::Inspect"));
    assert!(!task.contains("InspectRequest"));
    assert!(!inspect_task.contains("request_inspect_disk"));
    assert!(!inspect_task.contains("request_inspect_backup"));
    assert!(!render.contains("fn draw_inspect("));
}

#[test]
fn event_loop_does_not_parse_text_or_confirmation_chars_outside_keymap() {
    let source = include_str!("../src/tui/mod.rs");
    assert!(
        !source.contains("ct_event::KeyCode::Char(ch)"),
        "text/confirmation character handling must go through KeyMapper"
    );
}

#[test]
fn sector_inspector_dispatches_the_documented_vim_actions() {
    let source = include_str!("../src/tui/mod.rs");
    for action in [
        "TuiAction::Toggle",
        "TuiAction::RowStart",
        "TuiAction::RowEnd",
        "TuiAction::Top",
        "TuiAction::Bottom",
        "TuiAction::HalfPageUp",
        "TuiAction::HalfPageDown",
        "TuiAction::NextMatch",
        "TuiAction::PreviousMatch",
    ] {
        assert!(
            source.contains(action),
            "Sector Inspector event loop missing {action}"
        );
    }
    let render = include_str!("../src/tui/inspect/render.rs");
    assert!(render.contains("0/$"));
    assert!(render.contains("gg/G"));
    assert!(render.contains("Ctrl-u/d"));
    assert!(render.contains("/ n/N"));
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
    assert!(NORMAL_HELP.iter().any(|binding| {
        binding.keys == "Tab/Shift-Tab" && binding.action == TuiAction::WorkspaceNext
    }));
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
