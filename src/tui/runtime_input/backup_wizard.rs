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
                        keymap::TuiAction::Confirm => {
                            state.set_notice("写入目标介质前必须精确输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::Running
            | state::WizardStage::Formatting
            | state::WizardStage::Reinitializing => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    if matches!(action, keymap::TuiAction::Open) {
                        state.toggle_wizard_detail();
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::PostRestore => {
                if let Some(action) = keys.map(state::InputMode::Normal, key) {
                    let visible_rows = usize::from(terminal_size.height.saturating_sub(12)).max(1);
                    match action {
                        keymap::TuiAction::MoveDown => {
                            state.move_post_restore_result_selection(1, visible_rows)
                        }
                        keymap::TuiAction::MoveUp => {
                            state.move_post_restore_result_selection(-1, visible_rows)
                        }
                        keymap::TuiAction::PageUp => state.move_post_restore_result_selection(
                            -(visible_rows as isize),
                            visible_rows,
                        ),
                        keymap::TuiAction::PageDown => state.move_post_restore_result_selection(
                            visible_rows as isize,
                            visible_rows,
                        ),
                        keymap::TuiAction::Top => state.post_restore_result_top(visible_rows),
                        keymap::TuiAction::Bottom => state.post_restore_result_bottom(visible_rows),
                        keymap::TuiAction::MoveLeft => {
                            let _ = state.post_restore_result_shift_partition_column(true);
                        }
                        keymap::TuiAction::MoveRight => {
                            let _ = state.post_restore_result_shift_partition_column(false);
                        }
                        keymap::TuiAction::PanelPrevious => {
                            state.post_restore_result_shift_pane(true)
                        }
                        keymap::TuiAction::PanelNext => state.post_restore_result_shift_pane(false),
                        keymap::TuiAction::PanelLeft => {
                            state.post_restore_result_spatial_focus(-1, 0)
                        }
                        keymap::TuiAction::PanelRight => {
                            state.post_restore_result_spatial_focus(1, 0)
                        }
                        keymap::TuiAction::PanelUp => {
                            state.post_restore_result_spatial_focus(0, -1)
                        }
                        keymap::TuiAction::PanelDown => {
                            state.post_restore_result_spatial_focus(0, 1)
                        }
                        keymap::TuiAction::Activate => {
                            if state.post_restore_result_focused_pane()
                                == crate::tui::pane::PaneId::ResultPartitions
                            {
                                state.begin_selected_post_restore_action();
                            }
                        }
                        keymap::TuiAction::Open => state.toggle_wizard_detail(),
                        keymap::TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::VolumeLabelInput => {
                if let Some(action) = keys.map(state::InputMode::Insert, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_volume_label_char(ch),
                        keymap::TuiAction::Backspace => state.backspace_wizard_volume_label(),
                        keymap::TuiAction::Submit => state.submit_wizard_volume_label(),
                        keymap::TuiAction::Back | keymap::TuiAction::Cancel => {
                            state.cancel_post_restore_volume_label();
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::PasswordInput
            | state::WizardStage::ReinitializePassword
            | state::WizardStage::ReinitializePasswordConfirm => {
                if let Some(action) = keys.map(state::InputMode::Insert, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_secret_char(ch),
                        keymap::TuiAction::Backspace => state.backspace_wizard_secret(),
                        keymap::TuiAction::Submit => state.submit_wizard_secret(),
                        keymap::TuiAction::Back | keymap::TuiAction::Cancel => {
                            state.cancel_post_restore_secret_flow();
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::EncryptedFormatConfirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                        keymap::TuiAction::Backspace => {
                            state.backspace_wizard_confirmation();
                        }
                        keymap::TuiAction::Submit => {
                            if let Some(intent) = state.submit_encrypted_format_confirmation() {
                                if let Err(message) =
                                    tasks.request_post_restore_encrypted_format(intent)
                                {
                                    state.abort_post_restore_encrypted_action(message.to_string());
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            state.cancel_post_restore_secret_flow();
                        }
                        keymap::TuiAction::Confirm => {
                            state.set_notice("加密格式化需要独立输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::ReinitializeConfirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                        keymap::TuiAction::Backspace => {
                            state.backspace_wizard_confirmation();
                        }
                        keymap::TuiAction::Submit => {
                            if let Some(intent) = state.submit_reinitialize_confirmation() {
                                if let Err(message) =
                                    tasks.request_post_restore_reinitialize(intent)
                                {
                                    state.abort_post_restore_encrypted_action(message.to_string());
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            state.cancel_post_restore_secret_flow();
                        }
                        keymap::TuiAction::Confirm => {
                            state.set_notice("重建密钥域需要独立输入大写 YES 后按 Enter。")
                        }
                        _ => {}
                    }
                }
                return Some(KeyOutcome::NextIteration);
            }
            state::WizardStage::FormatConfirm => {
                if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                    match action {
                        keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                        keymap::TuiAction::Backspace => {
                            state.backspace_wizard_confirmation();
                        }
                        keymap::TuiAction::Submit => {
                            if let Some(intent) = state.submit_post_restore_format_confirmation() {
                                if let Err(message) = tasks.request_post_restore_format(intent) {
                                    state.abort_post_restore_format(message.to_string());
                                }
                            }
                        }
                        keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                            state.cancel_post_restore_format();
                        }
                        keymap::TuiAction::Confirm => {
                            state.set_notice("格式化需要第二次独立输入大写 YES 后按 Enter。")
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
        }
    }

    None
}
