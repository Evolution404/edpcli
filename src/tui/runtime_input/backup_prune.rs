use super::*;

pub(super) fn handle_backup_prune_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    _terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if let Some(stage) = state.backup_prune().map(|prune| prune.stage) {
        use state::BackupPruneStage;
        match stage {
            BackupPruneStage::Input => {
                if let Some(action) = keys.map(state::InputMode::Insert, key) {
                    match action {
                        keymap::TuiAction::Text(ch) if ch.is_ascii_digit() => {
                            state.backup_prune_push_digit(ch);
                        }
                        keymap::TuiAction::Backspace => state.backup_prune_backspace(),
                        keymap::TuiAction::Submit => match state.backup_prune_start_plan() {
                            Ok(keep) => {
                                if let Err(message) =
                                    tasks.request_backup_prune_plan(backup_dir.to_path_buf(), keep)
                                {
                                    state.backup_prune_finish_plan(Err(message.to_string()));
                                }
                            }
                            Err(message) => {
                                if let Some(prune) = state.backup_prune_mut() {
                                    prune.message = Some(message);
                                }
                            }
                        },
                        keymap::TuiAction::Back => state.close_backup_prune(),
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupPruneStage::Planning => {
                if keys.map(state::InputMode::Normal, key) == Some(keymap::TuiAction::Back) {
                    state.set_notice("清理计划正在后台生成，请等待完成。");
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupPruneStage::Review => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    match action {
                        keymap::TuiAction::Activate => state.backup_prune_begin_confirm(),
                        keymap::TuiAction::Back => state.close_backup_prune(),
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupPruneStage::Confirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => {
                            state.backup_prune_push_confirmation(ch);
                        }
                        keymap::TuiAction::Backspace => state.backup_prune_backspace(),
                        keymap::TuiAction::Submit => {
                            if let Some(prepared) = state.backup_prune_take_for_execute() {
                                if let Err(message) = tasks.request_backup_prune_execute(
                                    prepared,
                                    backup_dir.to_path_buf(),
                                ) {
                                    state.backup_prune_finish_execute(Err(message.to_string()));
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            state.close_backup_prune()
                        }
                        keymap::TuiAction::Confirm => {
                            state.set_notice("备份清理仍需精确输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupPruneStage::Running => {}
            BackupPruneStage::Result => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    if matches!(
                        action,
                        keymap::TuiAction::Activate | keymap::TuiAction::Back
                    ) {
                        state.close_backup_prune();
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
        }
    }
    None
}
