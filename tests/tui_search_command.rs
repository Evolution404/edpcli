use edpcli::disk_scan::Row;
use edpcli::tui::command::{parse_command, PaletteAction};
use edpcli::tui::{
    render,
    state::{AppState, InputMode, NavCommand},
};
use ratatui::{backend::TestBackend, Terminal};

fn device(disk: u32, user: &str, dept: &str) -> Row {
    Row {
        disk,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some(format!("disk&ven_test&prod_{user}")),
        identity_pin: None,
        onlyid: Some(format!("{disk}001")),
        dept: Some(dept.into()),
        user: Some(user.into()),
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
        n_possible_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
    }
}

#[test]
fn slash_search_filters_the_visible_list_while_typing() {
    let mut state = AppState::new();
    state.replace_devices(vec![
        device(6, "Alice", "输电一班"),
        device(7, "Bob", "输电二班"),
        device(8, "Alice-2", "输电三班"),
    ]);

    state.navigate(NavCommand::Search, 20);
    assert_eq!(state.input_mode(), InputMode::Search);
    for ch in "alice".chars() {
        state.push_input_char(ch);
    }
    assert_eq!(state.item_count(), 2);
    assert_eq!(state.visible_device_indices(), vec![0, 2]);
    assert_eq!(state.selected_device_disk(), Some(6));

    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected(), 1);
    assert_eq!(state.selected_device_disk(), Some(8));

    assert_eq!(state.submit_search(), 2);
    assert_eq!(state.item_count(), 2);
    assert_eq!(state.selected_device_disk(), Some(6));

    state.navigate(NavCommand::NextMatch, 20);
    assert_eq!(state.selected(), 1);
    assert_eq!(state.selected_device_disk(), Some(8));
    state.navigate(NavCommand::NextMatch, 20);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::Search, 20);
    for _ in 0..5 {
        state.backspace_input();
    }
    assert_eq!(state.item_count(), 3);
    assert_eq!(state.visible_device_indices(), vec![0, 1, 2]);
    assert_eq!(state.submit_search(), 0);
}

#[test]
fn filtered_devices_are_the_only_rows_rendered() {
    let mut state = AppState::new();
    state.replace_devices(vec![
        device(6, "Alice", "输电一班"),
        device(7, "Bob", "输电二班"),
        device(8, "Alice-2", "输电三班"),
    ]);
    state.navigate(NavCommand::Search, 20);
    for ch in "alice".chars() {
        state.push_input_char(ch);
    }

    let backend = TestBackend::new(140, 32);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, &state))
        .expect("draw filtered devices");
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Alice"), "{text}");
    assert!(text.contains("Alice-2"), "{text}");
    assert!(!text.contains("Bob"), "{text}");
}

#[test]
fn command_palette_has_task_semantics_not_shell_semantics() {
    assert_eq!(parse_command("devices").unwrap(), PaletteAction::Devices);
    assert_eq!(parse_command("backups").unwrap(), PaletteAction::Backups);
    assert_eq!(parse_command("inspect").unwrap(), PaletteAction::Inspect);
    assert!(parse_command("apply").is_err());
    assert_eq!(parse_command("restore").unwrap(), PaletteAction::Restore);
    assert_eq!(parse_command("refresh").unwrap(), PaletteAction::Refresh);
    assert_eq!(parse_command("help").unwrap(), PaletteAction::Help);
    assert_eq!(parse_command("q").unwrap(), PaletteAction::Quit);
    assert!(parse_command("rm -rf /").is_err());

    let source = include_str!("../src/tui/command.rs");
    assert!(!source.contains("Command::new"));
    assert!(!source.contains("std::process"));
}

#[test]
fn inspect_palette_has_one_user_visible_full_disk_action() {
    assert_eq!(parse_command("inspect").unwrap(), PaletteAction::Inspect);
    assert!(parse_command("advanced-inspect").is_err());
    assert!(parse_command("inspect-advanced").is_err());
    assert!(parse_command("ai").is_err());
    let source = include_str!("../src/tui/mod.rs");
    assert!(source.contains("NavCommand::OpenInspect =>"));
    assert!(!source.contains("OpenAdvancedInspect"));
    assert!(source.contains("state.begin_advanced_inspect(source)"));
}

#[test]
fn escape_clears_palette_input_without_exiting() {
    let mut state = AppState::new();
    state.navigate(NavCommand::CommandPalette, 20);
    state.push_input_char('q');
    assert_eq!(state.input_buffer(), "q");
    let effect = state.navigate(NavCommand::Escape, 20);
    assert_eq!(effect, edpcli::tui::state::StateEffect::None);
    assert_eq!(state.input_mode(), InputMode::Normal);
    assert_eq!(state.input_buffer(), "");
}
