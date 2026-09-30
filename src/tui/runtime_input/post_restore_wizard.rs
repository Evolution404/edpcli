use super::*;

pub(super) fn handle_post_restore_wizard_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    let stage = state.wizard().map(|wizard| wizard.stage)?;
    match stage {
        state::WizardStage::PostRestore => {
            let role = controller::active_widget_role(state);
            if let Some(action) = keys.map_for_role(state::InputMode::Normal, role, key) {
                let visible_rows = usize::from(terminal_size.height.saturating_sub(12)).max(1);
                if matches!(
                    action,
                    keymap::TuiAction::TableColumnLeft
                        | keymap::TuiAction::TableColumnRight
                        | keymap::TuiAction::TableMoveColumnLeft
                        | keymap::TuiAction::TableMoveColumnRight
                        | keymap::TuiAction::TableColumnFirst
                        | keymap::TuiAction::TableColumnLast
                        | keymap::TuiAction::TableScrollLeft
                        | keymap::TuiAction::TableScrollRight
                        | keymap::TuiAction::TableSortToggle
                        | keymap::TuiAction::TableSortClear
                        | keymap::TuiAction::TableCopyCell
                        | keymap::TuiAction::TableCopyRow
                ) {
                    match dispatch_tui_action(
                        state,
                        tasks,
                        action,
                        role,
                        backup_dir,
                        visible_rows,
                        terminal_size.width,
                    ) {
                        StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                        StateEffect::ExitDeferred | StateEffect::None => {}
                    }
                    return Some(KeyOutcome::NextIteration);
                }
                match action {
                    keymap::TuiAction::MoveDown => {
                        state.move_post_restore_result_selection(1, visible_rows)
                    }
                    keymap::TuiAction::MoveUp => {
                        state.move_post_restore_result_selection(-1, visible_rows)
                    }
                    keymap::TuiAction::PageUp => state
                        .move_post_restore_result_selection(-(visible_rows as isize), visible_rows),
                    keymap::TuiAction::PageDown => state
                        .move_post_restore_result_selection(visible_rows as isize, visible_rows),
                    keymap::TuiAction::Top => state.post_restore_result_top(visible_rows),
                    keymap::TuiAction::Bottom => state.post_restore_result_bottom(visible_rows),
                    keymap::TuiAction::PanelPrevious => state.post_restore_result_shift_pane(true),
                    keymap::TuiAction::PanelNext => state.post_restore_result_shift_pane(false),
                    keymap::TuiAction::PanelLeft => state.post_restore_result_spatial_focus(-1, 0),
                    keymap::TuiAction::PanelRight => state.post_restore_result_spatial_focus(1, 0),
                    keymap::TuiAction::PanelUp => state.post_restore_result_spatial_focus(0, -1),
                    keymap::TuiAction::PanelDown => state.post_restore_result_spatial_focus(0, 1),
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
            Some(KeyOutcome::NextIteration)
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
            Some(KeyOutcome::NextIteration)
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
            Some(KeyOutcome::NextIteration)
        }
        state::WizardStage::EncryptedFormatConfirm => {
            if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                match action {
                    keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                    keymap::TuiAction::Backspace => state.backspace_wizard_confirmation(),
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
                        state.set_warning_notice("加密格式化需要独立输入大写 YES 后按 Enter。")
                    }
                    _ => {}
                }
            }
            Some(KeyOutcome::NextIteration)
        }
        state::WizardStage::ReinitializeConfirm => {
            if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                match action {
                    keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                    keymap::TuiAction::Backspace => state.backspace_wizard_confirmation(),
                    keymap::TuiAction::Submit => {
                        if let Some(intent) = state.submit_reinitialize_confirmation() {
                            if let Err(message) = tasks.request_post_restore_reinitialize(intent) {
                                state.abort_post_restore_encrypted_action(message.to_string());
                            }
                        }
                    }
                    keymap::TuiAction::Cancel | keymap::TuiAction::Back => {
                        state.cancel_post_restore_secret_flow();
                    }
                    keymap::TuiAction::Confirm => {
                        state.set_warning_notice("重建密钥域需要独立输入大写 YES 后按 Enter。")
                    }
                    _ => {}
                }
            }
            Some(KeyOutcome::NextIteration)
        }
        state::WizardStage::FormatConfirm => {
            if let Some(action) = keys.map(state::InputMode::Confirm, key) {
                match action {
                    keymap::TuiAction::Text(ch) => state.push_wizard_confirmation(ch),
                    keymap::TuiAction::Backspace => state.backspace_wizard_confirmation(),
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
                        state.set_warning_notice("格式化需要第二次独立输入大写 YES 后按 Enter。")
                    }
                    _ => {}
                }
            }
            Some(KeyOutcome::NextIteration)
        }
        _ => None,
    }
}
