use super::*;

pub(super) fn handle_backup_wizard_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    _terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if let Some(stage) = state.backup_delete().map(|delete| delete.stage) {
        match stage {
            state::WizardStage::Confirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => {
                            state.push_backup_delete_confirmation(ch);
                        }
                        keymap::TuiAction::Backspace => {
                            state.backspace_backup_delete_confirmation();
                        }
                        keymap::TuiAction::Submit => {
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
                        keymap::TuiAction::Confirm => {
                            state.set_notice("删除备份仍需精确输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Running => {}
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
        }
    }

    if let Some(stage) = state.wizard().map(|wizard| wizard.stage) {
        match stage {
            state::WizardStage::Confirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => {
                            state.push_wizard_confirmation(ch);
                        }
                        keymap::TuiAction::Backspace => {
                            state.backspace_wizard_confirmation();
                        }
                        keymap::TuiAction::Submit => {
                            if let Some(intent) = state.submit_wizard_confirmation() {
                                if !crate::elevate::is_root() {
                                    return Some(KeyOutcome::Elevate(intent));
                                }
                                if matches!(
                                    intent.kind,
                                    state::WriteKind::BackupCreate
                                        | state::WriteKind::BackupCreateDeep
                                ) {
                                    if let Err(message) = tasks
                                        .request_backup_create(intent, backup_dir.to_path_buf())
                                    {
                                        state.finish_write(Err(message.to_string()));
                                    }
                                } else if let Err(message) =
                                    tasks.request_write(intent, backup_dir.to_path_buf())
                                {
                                    state.finish_write(Err(message.to_string()));
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        keymap::TuiAction::Confirm => {
                            state.set_notice("破坏性操作仍需精确输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Running => {}
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
        }
    }

    None
}
