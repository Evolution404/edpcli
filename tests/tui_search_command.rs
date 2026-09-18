use edpcli::tui::command::{parse_command, PaletteAction};
use edpcli::tui::state::{AppState, InputMode, NavCommand};
use edpcli::disk_scan::Row;

fn device(disk: u32, user: &str, dept: &str) -> Row {
    Row {
        disk,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some(format!("disk&ven_test&prod_{user}")),
        onlyid: Some(format!("{disk}001")),
        dept: Some(dept.into()),
        user: Some(user.into()),
        n_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        partitions: None,
    }
}

#[test]
fn slash_search_selects_matches_and_n_cycles_them() {
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
    assert_eq!(state.submit_search(), 2);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::NextMatch, 20);
    assert_eq!(state.selected(), 2);
    state.navigate(NavCommand::NextMatch, 20);
    assert_eq!(state.selected(), 0);
    state.navigate(NavCommand::PreviousMatch, 20);
    assert_eq!(state.selected(), 2);
}

#[test]
fn command_palette_has_task_semantics_not_shell_semantics() {
    assert_eq!(parse_command("devices").unwrap(), PaletteAction::Devices);
    assert_eq!(parse_command("backups").unwrap(), PaletteAction::Backups);
    assert_eq!(parse_command("inspect").unwrap(), PaletteAction::Inspect);
    assert_eq!(parse_command("apply").unwrap(), PaletteAction::Apply);
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
