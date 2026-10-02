use super::*;

pub(super) fn handle_backup_wizard_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if let Some(stage) = state.backup_delete().map(|delete| delete.stage) {
        match stage {
            state::WizardStage::Confirm => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    match action {
                        keymap::TuiAction::Activate | keymap::TuiAction::Submit => {
                            if let Some((path, expected_sha256)) =
                                state.submit_backup_delete_confirmation()
                            {
                                if let Err(message) = tasks.request_backup_delete(
                                    path,
                                    expected_sha256,
                                    backup_dir.to_path_buf(),
                                ) {
                                    state.finish_backup_delete(Err(message.to_string()));
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Running => {}
            _ => {}
        }
    }

    if let Some(outcome) = super::post_restore_wizard::handle_post_restore_wizard_key(
        state,
        tasks,
        keys,
        key,
        backup_dir,
        terminal_size,
    ) {
        return Some(outcome);
    }

    if let Some(stage) = state.wizard().map(|wizard| wizard.stage) {
        match stage {
            state::WizardStage::Confirm => {
                let kind = state.wizard().map(|wizard| wizard.kind);
                if kind == Some(state::WriteKind::BackupCreate) {
                    if let Some(action) = keys.map(state::InputMode::Normal, key) {
                        match action {
                            keymap::TuiAction::Activate | keymap::TuiAction::Submit => {
                                if let Some(intent) = state.confirm_backup_create() {
                                    if let Err(message) = tasks
                                        .request_backup_create(intent, backup_dir.to_path_buf())
                                    {
                                        state.finish_write(Err(message.to_string()));
                                    }
                                }
                            }
                            keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                                let _ = state.navigate(NavCommand::Escape, 1);
                            }
                            _ => {}
                        }
                    }
                } else if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                        keymap::TuiAction::Backspace => state.backspace_wizard_confirmation(),
                        keymap::TuiAction::Submit => {
                            if let Some(intent) = state.submit_wizard_confirmation() {
                                if !crate::elevate::is_root() {
                                    return Some(KeyOutcome::Elevate(intent));
                                }
                                if let Err(message) =
                                    tasks.request_write(intent, backup_dir.to_path_buf())
                                {
                                    state.finish_restore(Err(message.to_string()));
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        keymap::TuiAction::Confirm => state
                            .set_warning_notice("写入目标介质前必须精确输入大写 YES 后按 Enter。"),
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Running
            | state::WizardStage::Formatting
            | state::WizardStage::Reinitializing => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    match action {
                        keymap::TuiAction::Help => {
                            let _ = state.navigate(NavCommand::Help, 1);
                        }
                        keymap::TuiAction::Quit => {
                            let _ = state.navigate(NavCommand::Quit, 1);
                        }
                        keymap::TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Result => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    if matches!(
                        action,
                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                    ) {
                        let _ = state.navigate(NavCommand::Escape, 1);
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            _ => {}
        }
    }

    None
}
