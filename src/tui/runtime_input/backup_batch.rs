use super::*;

pub(super) fn handle_backup_batch_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    _terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if let Some(stage) = state.backup_batch_delete().map(|batch| batch.stage) {
        use state::BackupBatchDeleteStage;
        match stage {
            BackupBatchDeleteStage::Planning => {
                if keys.map(state::InputMode::Normal, key) == Some(keymap::TuiAction::Back) {
                    state.set_progress_notice("批量删除计划正在后台生成，请等待完成。");
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupBatchDeleteStage::Confirm => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    match action {
                        keymap::TuiAction::Activate | keymap::TuiAction::Submit => {
                            if let Some(plan) = state.backup_batch_delete_take_for_execute() {
                                if let Err(message) = tasks.request_backup_batch_delete_execute(
                                    plan,
                                    backup_dir.to_path_buf(),
                                ) {
                                    state.backup_batch_delete_finish_execute(Err(
                                        message.to_string()
                                    ));
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            state.close_backup_batch_delete();
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            BackupBatchDeleteStage::Running => {}
        }
    }
    None
}
