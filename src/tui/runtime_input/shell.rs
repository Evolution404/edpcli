use super::*;

pub(super) fn handle_shell_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> KeyOutcome {
    if matches!(
        state.input_mode(),
        state::InputMode::Search | state::InputMode::Command
    ) {
        if let Some(action) = keys.map(state.input_mode(), key) {
            match action {
                keymap::TuiAction::Text(ch) => {
                    state.push_input_char(ch);
                }
                keymap::TuiAction::Backspace => {
                    state.backspace_input();
                }
                keymap::TuiAction::Back => {
                    let _ = state.navigate(NavCommand::Escape, 1);
                }
                keymap::TuiAction::Submit => {
                    if state.input_mode() == state::InputMode::Search {
                        state.submit_search();
                    } else {
                        let input = state.take_input();
                        state.cancel_input();
                        match command::parse_command(&input) {
                            Ok(action) => {
                                let viewport_height =
                                    terminal_size.height.saturating_sub(9) as usize;
                                if action == command::PaletteAction::Provision {
                                    if state.workspace() != state::Workspace::Devices {
                                        let _ = state.navigate(
                                            NavCommand::WorkspaceDevices,
                                            viewport_height,
                                        );
                                        state.set_warning_notice(
                                            "请在设备列表选定 USB 盘后按 p 选择制盘方案。",
                                        );
                                    } else if let Err(message) =
                                        state.begin_provision_for_selected_device()
                                    {
                                        state.set_error_notice(message);
                                    }
                                } else {
                                    let effect = dispatch_nav_command(
                                        state,
                                        tasks,
                                        palette_action_to_nav(action),
                                        backup_dir,
                                        viewport_height,
                                    );
                                    if effect == StateEffect::ExitRequested {
                                        return KeyOutcome::Exit;
                                    }
                                }
                            }
                            Err(message) => state.set_warning_notice(message),
                        }
                    }
                }
                _ => {}
            }
        }
        return KeyOutcome::NextIteration;
    }

    if let Some(action) = keys.map_for_role(state.input_mode(), keymap::WidgetRole::Table, key) {
        let viewport_height = terminal_size.height.saturating_sub(9) as usize;
        match dispatch_tui_action(
            state,
            tasks,
            action,
            keymap::WidgetRole::Table,
            backup_dir,
            viewport_height,
            terminal_size.width,
        ) {
            StateEffect::ExitRequested => return KeyOutcome::Exit,
            StateEffect::ExitDeferred | StateEffect::None => {}
        }
    }
    KeyOutcome::Handled
}
