use super::*;

pub(super) fn handle_provision_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if state.provision_scheme_picker_open() || state.workspace() == state::Workspace::Provision {
        use keymap::TuiAction;
        use state::ProvisionStage;

        let role = controller::active_widget_role(state);
        let Some(action) = keys.map_for_role(state.input_mode(), role, key) else {
            return Some(KeyOutcome::NextIteration);
        };
        let viewport_height = terminal_size.height.saturating_sub(9) as usize;

        if matches!(
            action,
            TuiAction::Refresh
                | TuiAction::Help
                | TuiAction::Command
                | TuiAction::TableColumnLeft
                | TuiAction::TableColumnRight
                | TuiAction::TableMoveColumnLeft
                | TuiAction::TableMoveColumnRight
                | TuiAction::TableColumnFirst
                | TuiAction::TableColumnLast
                | TuiAction::TableScrollLeft
                | TuiAction::TableScrollRight
                | TuiAction::TableSortToggle
                | TuiAction::TableSortClear
                | TuiAction::TableCopyCell
                | TuiAction::TableCopyRow
                | TuiAction::FocusNext
                | TuiAction::FocusPrevious
                | TuiAction::WorkspaceNext
                | TuiAction::WorkspacePrevious
        ) {
            if matches!(action, TuiAction::FocusNext | TuiAction::FocusPrevious)
                && state.provision().stage == ProvisionStage::Form
                && state.input_mode() == state::InputMode::Insert
            {
                state.provision_end_insert();
            }
            match dispatch_tui_action(
                state,
                tasks,
                action,
                role,
                backup_dir,
                viewport_height,
                terminal_size.width,
            ) {
                StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                StateEffect::ExitDeferred | StateEffect::None => {}
            }
            return Some(KeyOutcome::NextIteration);
        }

        match state.provision().stage {
            ProvisionStage::Form if state.input_mode() == state::InputMode::Insert => {
                match action {
                    TuiAction::Text(ch) => state.provision_push_char(ch),
                    TuiAction::Backspace => state.provision_backspace(),
                    TuiAction::DeleteChar => state.provision_delete_char(),
                    TuiAction::CursorLeft => state.provision_move_cursor(-1),
                    TuiAction::CursorRight => state.provision_move_cursor(1),
                    TuiAction::CursorHome => state.provision_cursor_home(),
                    TuiAction::CursorEnd => state.provision_cursor_end(),
                    TuiAction::Submit | TuiAction::Back => state.provision_end_insert(),
                    _ => {}
                }
            }
            ProvisionStage::Form => {
                match dispatch_tui_action(
                    state,
                    tasks,
                    action,
                    role,
                    backup_dir,
                    viewport_height,
                    terminal_size.width,
                ) {
                    StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                    StateEffect::ExitDeferred | StateEffect::None => {}
                }
            }
            ProvisionStage::Planning => {
                if action == TuiAction::Back {
                    state.set_notice("制盘计划正在后台生成，请等待完成。");
                }
            }
            ProvisionStage::Review => {
                match dispatch_tui_action(
                    state,
                    tasks,
                    action,
                    role,
                    backup_dir,
                    viewport_height,
                    terminal_size.width,
                ) {
                    StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                    StateEffect::ExitDeferred | StateEffect::None => {}
                }
            }
            ProvisionStage::ExportPath => match action {
                TuiAction::Text(ch) => state.provision_export_push_char(ch),
                TuiAction::Backspace => state.provision_export_backspace(),
                TuiAction::Submit => {
                    if let Some((prepared, path)) = state.provision_take_export() {
                        if let Err(message) = tasks.request_provision_export(prepared, path) {
                            state.provision_finish_export(Err(message.to_string()));
                        }
                    }
                }
                TuiAction::Back => state.provision_cancel_export(),
                _ => {}
            },
            ProvisionStage::Exporting => {
                if action == TuiAction::Back {
                    state.set_notice("镜像正在后台导出，请等待完成。");
                }
            }
            ProvisionStage::Confirm => match action {
                TuiAction::Text(ch) => state.provision_push_confirmation(ch),
                TuiAction::Backspace => state.provision_backspace_confirmation(),
                TuiAction::Submit => {
                    if let Some(prepared) = state.provision_take_for_write() {
                        if let Err(message) =
                            tasks.request_provision_write(prepared, backup_dir.to_path_buf())
                        {
                            state.provision_finish_write(Err(message.to_string()));
                        }
                    }
                }
                TuiAction::Cancel | TuiAction::Back => {
                    let _ = state.navigate(NavCommand::Escape, viewport_height);
                }
                TuiAction::Confirm => {
                    state.set_notice("写入目标介质前必须精确输入大写 YES 后按 Enter。")
                }
                _ => {}
            },
            ProvisionStage::Running => {
                let log_visible = viewport_height.saturating_sub(12).max(1);
                let log_navigation = matches!(
                    action,
                    TuiAction::MoveUp
                        | TuiAction::MoveDown
                        | TuiAction::PageUp
                        | TuiAction::PageDown
                        | TuiAction::Bottom
                );
                match action {
                    TuiAction::MoveUp => state.provision_scroll_run_log(-1, log_visible),
                    TuiAction::MoveDown => state.provision_scroll_run_log(1, log_visible),
                    TuiAction::PageUp => {
                        state.provision_scroll_run_log(-(log_visible as isize), log_visible)
                    }
                    TuiAction::PageDown => {
                        state.provision_scroll_run_log(log_visible as isize, log_visible)
                    }
                    TuiAction::Bottom => state.provision_follow_run_log(log_visible),
                    _ => {}
                }
                if log_navigation {
                    return Some(KeyOutcome::NextIteration);
                }
                match dispatch_tui_action(
                    state,
                    tasks,
                    action,
                    role,
                    backup_dir,
                    viewport_height,
                    terminal_size.width,
                ) {
                    StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                    StateEffect::ExitDeferred | StateEffect::None => {}
                }
            }
            ProvisionStage::Result => {
                match action {
                    TuiAction::MoveUp => state.provision_result_move(-1, viewport_height),
                    TuiAction::MoveDown => state.provision_result_move(1, viewport_height),
                    TuiAction::PageUp => state
                        .provision_result_move(-(viewport_height.max(1) as isize), viewport_height),
                    TuiAction::PageDown => state
                        .provision_result_move(viewport_height.max(1) as isize, viewport_height),
                    TuiAction::Top => state.provision_result_top(viewport_height),
                    TuiAction::Bottom => state.provision_result_bottom(viewport_height),
                    TuiAction::MoveLeft => {
                        if !state.provision_result_shift_partition_column(true) {
                            state.provision_result_shift_pane(true);
                        }
                    }
                    TuiAction::MoveRight => {
                        if !state.provision_result_shift_partition_column(false) {
                            state.provision_result_shift_pane(false);
                        }
                    }
                    TuiAction::PanelPrevious => state.provision_result_shift_pane(true),
                    TuiAction::PanelNext => state.provision_result_shift_pane(false),
                    TuiAction::Activate | TuiAction::Submit | TuiAction::Back => {
                        let _ = state.navigate(NavCommand::Escape, viewport_height);
                    }
                    _ => {}
                }
            }
        }
        return Some(KeyOutcome::NextIteration);
    }
    None
}
