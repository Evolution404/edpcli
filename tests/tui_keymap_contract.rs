use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use edpcli::tui::{
    keymap::{
        KeyMapper, TuiAction, WidgetRole, DEVICES_HELP, GLOBAL_HELP, INSPECT_HELP, TABLE_HELP,
    },
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
        Some(TuiAction::TableColumnLeft)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('l'))
        ),
        Some(TuiAction::TableColumnRight)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('<'))
        ),
        Some(TuiAction::TableMoveColumnLeft)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('>'))
        ),
        Some(TuiAction::TableMoveColumnRight)
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

    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('H'))
        ),
        Some(TuiAction::TableScrollLeft)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('L'))
        ),
        Some(TuiAction::TableScrollRight)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('s'))
        ),
        Some(TuiAction::TableSortToggle)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('S'))
        ),
        Some(TuiAction::TableSortClear)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('0'))
        ),
        Some(TuiAction::TableColumnFirst)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('$'))
        ),
        Some(TuiAction::TableColumnLast)
    );
}

#[test]
fn chapter_11_single_key_actions_and_exit_contract() {
    let mut mapper = KeyMapper::new();
    for (code, expected) in [
        (KeyCode::Char('b'), Some(TuiAction::BackupCreate)),
        (KeyCode::Char('a'), Some(TuiAction::Add)),
        (KeyCode::Char('p'), Some(TuiAction::Provision)),
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
fn g_prefix_keeps_vim_navigation_and_adds_standard_tab_switching() {
    let mut mapper = KeyMapper::new();
    for (second, expected) in [
        ('g', TuiAction::Top),
        ('l', TuiAction::InspectJump),
        ('t', TuiAction::WorkspaceNext),
        ('T', TuiAction::WorkspacePrevious),
    ] {
        assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
        assert_eq!(
            mapper.map(InputMode::Normal, key(KeyCode::Char(second))),
            Some(expected),
            "g{second}"
        );
    }
    for second in ['d', 'b', 'p', 'i'] {
        assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
        assert_eq!(
            mapper.map(InputMode::Normal, key(KeyCode::Char(second))),
            None,
            "g{second} must not become an unrelated function shortcut"
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
fn tab_is_context_focus_and_gt_owns_top_level_switching() {
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
        Some(TuiAction::FocusNext)
    );
    assert_eq!(
        mapper.map(
            InputMode::Normal,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
        ),
        Some(TuiAction::FocusPrevious)
    );
    assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('t'))),
        Some(TuiAction::WorkspaceNext)
    );
    assert_eq!(mapper.map(InputMode::Normal, key(KeyCode::Char('g'))), None);
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('T'))),
        Some(TuiAction::WorkspacePrevious)
    );
}

#[test]
fn tab_focus_actions_are_distinct_from_top_level_workspace_actions() {
    assert_ne!(TuiAction::FocusNext, TuiAction::WorkspaceNext);
    assert_ne!(TuiAction::FocusPrevious, TuiAction::WorkspacePrevious);

    let inspect = include_str!("../src/tui/runtime_input/inspect.rs");
    let provision = include_str!("../src/tui/runtime_input/provision.rs");
    assert!(inspect.contains("TuiAction::FocusNext"));
    assert!(provision.contains("TuiAction::FocusNext"));
}

#[test]
fn table_role_maps_y_to_cell_copy_and_shift_y_to_row_copy() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('y'))
        ),
        Some(TuiAction::TableCopyCell)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Table,
            key(KeyCode::Char('Y'))
        ),
        Some(TuiAction::TableCopyRow)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Other,
            key(KeyCode::Char('y'))
        ),
        Some(TuiAction::Yank)
    );
    assert_eq!(
        mapper.map_for_role(
            InputMode::Normal,
            WidgetRole::Other,
            key(KeyCode::Char('Y'))
        ),
        Some(TuiAction::YankRaw)
    );
}

#[test]
fn normal_mode_keeps_inspect_and_backup_as_single_key_actions() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('p'))),
        Some(TuiAction::Provision)
    );
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

    let controller = include_str!("../src/tui/controller.rs");
    assert!(!controller.contains("TuiAction::Plan if state.workspace() == Workspace::Devices"));
    assert!(
        !controller.contains("TuiAction::Activate | TuiAction::Open => match state.workspace()")
    );
    assert!(contains_tokens_in_order(
        controller,
        &[
            "TuiAction::Insert",
            "if matches!(state.workspace(), Workspace::Devices | Workspace::Backups)",
            "ActionRequest::Navigate(NavCommand::OpenInspect)",
        ],
    ));
    assert!(contains_tokens_in_order(
        controller,
        &[
            "TuiAction::BackupCreate",
            "if matches!(state.workspace(), Workspace::Devices | Workspace::Backups)",
            "state.begin_backup_create_choice()",
        ],
    ));
    assert!(contains_tokens_in_order(
        controller,
        &[
            "TuiAction::Restore",
            "state.workspace() == Workspace::Backups",
            "ActionRequest::Navigate(NavCommand::BeginRestore)",
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
    assert_eq!(
        mapper.map(InputMode::Insert, key(KeyCode::Tab)),
        Some(TuiAction::FocusNext)
    );
    assert_eq!(
        mapper.map(
            InputMode::Insert,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
        ),
        Some(TuiAction::FocusPrevious)
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
    let controller = include_str!("../src/tui/controller/provision.rs");
    let form = source_section(
        controller,
        "ProvisionStage::Form if state.input_mode()",
        "ProvisionStage::Review =>",
    );
    assert!(
        contains_tokens_in_order(
            form,
            &[
                "TuiAction::Insert",
                "state.provision_begin_insert()",
                "TuiAction::Activate",
                "ActionRequest::ProvisionPlan",
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
    let production = include_str!("../src/tui/dispatch.rs");
    assert!(
        contains_tokens_in_order(
            production,
            &[
                "ActionRequest::ProvisionPlan",
                "start_provision_plan(state, tasks)",
            ],
        ),
        "Provision Form plan request must execute through the production task adapter"
    );

    let keymap = include_str!("../src/tui/keymap/help.rs");
    assert!(
        !keymap.contains("i/Enter 进入 Insert"),
        "help/footer must not advertise the regressed Enter-to-edit behavior"
    );
    assert!(
        keymap.contains("进入下一阶段"),
        "Provision help registry must advertise Enter as the forward action"
    );
}

#[test]
fn user_visible_inspect_hints_point_to_full_disk_tree_entry() {
    let keymap = include_str!("../src/tui/keymap/help.rs");
    let shell = include_str!("../src/tui/shell/mod.rs");
    assert!(keymap.contains("检查当前设备"));
    assert!(shell.contains("? 帮助"));
    assert!(keymap.contains("打开设备信息 / 进入详情"));
    assert!(!keymap.contains("i Inspect"));
    assert!(!keymap.contains("gi Inspect"));

    let dispatch = include_str!("../src/tui/dispatch.rs");
    assert!(dispatch.contains("NavCommand::OpenInspect =>"));
    assert!(!dispatch.contains("OpenAdvancedInspect"));
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
    assert!(inspect_state.contains("pub struct InspectState"));
    assert!(inspect_state.contains("advanced: Option<AdvancedInspectState>"));
    assert!(!task.contains("WorkerResult::Inspect"));
    assert!(!task.contains("InspectRequest"));
    assert!(!inspect_task.contains("request_inspect_disk"));
    assert!(!inspect_task.contains("request_inspect_backup"));
    assert!(!render.contains("fn draw_inspect("));
}

#[test]
fn event_loop_does_not_parse_text_or_confirmation_chars_outside_keymap() {
    let source = concat!(
        include_str!("../src/tui/mod.rs"),
        include_str!("../src/tui/runtime_input/inspect.rs"),
        include_str!("../src/tui/runtime_input/provision.rs"),
        include_str!("../src/tui/runtime_input/backup_batch.rs"),
        include_str!("../src/tui/runtime_input/backup_prune.rs"),
        include_str!("../src/tui/runtime_input/backup_wizard.rs"),
        include_str!("../src/tui/runtime_input/shell.rs"),
    );
    assert!(
        !source.contains("ct_event::KeyCode::Char(ch)"),
        "text/confirmation character handling must go through KeyMapper"
    );
}

#[test]
fn sector_inspector_sector_navigation_is_separate_from_page_scroll() {
    let mut mapper = KeyMapper::new();
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char('['))),
        Some(TuiAction::SectorPrevious)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::Char(']'))),
        Some(TuiAction::SectorNext)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::PageUp)),
        Some(TuiAction::PageUp)
    );
    assert_eq!(
        mapper.map(InputMode::Normal, key(KeyCode::PageDown)),
        Some(TuiAction::PageDown)
    );

    let mut insert = KeyMapper::new();
    assert_eq!(
        insert.map(InputMode::Insert, key(KeyCode::Char('['))),
        Some(TuiAction::Text('['))
    );
    assert_eq!(
        insert.map(InputMode::Insert, key(KeyCode::Char(']'))),
        Some(TuiAction::Text(']'))
    );
}

#[test]
fn sector_inspector_dispatches_the_documented_vim_actions() {
    let source = include_str!("../src/tui/runtime_input/inspect.rs");
    let page_section = source_section(
        source,
        "TuiAction::PageUp => {",
        "TuiAction::SectorPrevious",
    );
    assert!(page_section.contains("advanced_inspect_sector_page"));
    assert!(!page_section.contains("advanced_inspect_shift_sector"));
    for action in [
        "TuiAction::Toggle",
        "TuiAction::RowStart",
        "TuiAction::RowEnd",
        "TuiAction::Top",
        "TuiAction::Bottom",
        "TuiAction::HalfPageUp",
        "TuiAction::HalfPageDown",
        "TuiAction::PageUp",
        "TuiAction::PageDown",
        "TuiAction::SectorPrevious",
        "TuiAction::SectorNext",
        "TuiAction::NextMatch",
        "TuiAction::PreviousMatch",
    ] {
        assert!(
            source.contains(action),
            "Sector Inspector event loop missing {action}"
        );
    }
    let render = include_str!("../src/tui/inspect/sector_render.rs");
    assert!(render.contains("0/$"));
    assert!(render.contains("gg/G"));
    assert!(render.contains("Ctrl-u/d"));
    assert!(render.contains("[/]"));
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
    assert!(DEVICES_HELP
        .iter()
        .any(|binding| binding.keys.contains("j/k") && binding.action == TuiAction::MoveDown));
    assert!(GLOBAL_HELP.iter().any(|binding| {
        binding.keys == "Tab / Shift-Tab"
            && binding.label == "切换当前层级焦点 / 顶层标签"
            && binding.action == TuiAction::FocusNext
    }));
    assert!(GLOBAL_HELP.iter().any(|binding| {
        binding.keys == "gt / gT"
            && binding.label == "下一个 / 上一个顶层标签"
            && binding.action == TuiAction::WorkspaceNext
    }));
    assert!(TABLE_HELP
        .iter()
        .any(|binding| binding.keys == "y / Y" && binding.action == TuiAction::TableCopyCell));
    assert!(INSPECT_HELP.iter().any(|binding| {
        binding.keys == "Tab/Shift-Tab"
            && binding.label == "切换当前页 Pane"
            && binding.action == TuiAction::FocusNext
    }));
    assert!(!INSPECT_HELP.iter().any(|binding| binding.keys == "gt/gT"));
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

#[test]
fn chapter_13_p8_removes_legacy_page_level_detail_scroll_state() {
    let inspect = include_str!("../src/tui/inspect/state.rs");
    let navigation = include_str!("../src/tui/navigation.rs");
    let app_state = include_str!("../src/tui/state.rs");

    assert!(!inspect.contains("detail_scroll"));
    assert!(!inspect.contains("advanced_inspect_scroll_detail"));
    assert!(!navigation.contains("detail_scroll"));
    assert!(!app_state.contains("detail_scroll"));
}
