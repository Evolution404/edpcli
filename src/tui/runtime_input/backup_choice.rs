use super::*;

pub(super) fn handle_backup_choice_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if state.backup_create_choice().is_some() {
        if let Some(action) = keys.map(state::InputMode::Normal, key) {
            match action {
                keymap::TuiAction::MoveUp => {
                    state.move_backup_create_choice(-1);
                }
                keymap::TuiAction::MoveDown => {
                    state.move_backup_create_choice(1);
                }
                keymap::TuiAction::Activate => {
                    if let Some(choice) = state.take_backup_create_choice() {
                        let command = match choice {
                            state::BackupCreateChoice::Metadata => NavCommand::BeginBackupCreate,
                            state::BackupCreateChoice::Deep => NavCommand::BeginBackupCreateDeep,
                        };
                        let viewport_height = terminal_size.height.saturating_sub(9) as usize;
                        let _ = dispatch_nav_command(
                            state,
                            tasks,
                            command,
                            backup_dir,
                            viewport_height,
                        );
                    }
                }
                keymap::TuiAction::Back => state.cancel_backup_create_choice(),
                _ => {}
            }
        }
        return Some(KeyOutcome::NextIteration);
    }

    None
}
