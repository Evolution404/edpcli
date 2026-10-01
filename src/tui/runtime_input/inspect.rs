use super::*;

pub(super) fn handle_inspect_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> Option<KeyOutcome> {
    if let Some(stage) = state
        .advanced_inspect()
        .filter(|_| state.workspace() == state::Workspace::Inspect)
        .map(|advanced| advanced.stage)
    {
        use state::AdvancedInspectStage;
        match stage {
            AdvancedInspectStage::Running => {
                match key.code {
                    ct_event::KeyCode::Esc => {
                        state.set_progress_notice("全盘检查正在后台读取结构，请等待完成。");
                    }
                    ct_event::KeyCode::Char('J') => {
                        state.set_warning_notice("全盘检查数据尚未准备好，完成后才能跳转 LBA。");
                    }
                    _ => {}
                }
                return Some(KeyOutcome::NextIteration);
            }
            AdvancedInspectStage::Failed => {
                return Some(KeyOutcome::NextIteration);
            }
            AdvancedInspectStage::Browser => {
                if let Some(prompt) = state.advanced_inspect_prompt() {
                    let mode = if matches!(prompt, state::AdvancedInspectPrompt::Jump { .. }) {
                        state::InputMode::Command
                    } else {
                        state::InputMode::Search
                    };
                    if let Some(action) = keys.map(mode, key) {
                        match action {
                            keymap::TuiAction::Back => {
                                state.advanced_inspect_cancel_prompt();
                            }
                            keymap::TuiAction::Backspace => {
                                state.advanced_inspect_prompt_backspace();
                            }
                            keymap::TuiAction::Submit => {
                                match state.advanced_inspect_submit_prompt() {
                                    Ok(Some((source, lba))) => {
                                        if let Err(message) =
                                            tasks.request_advanced_inspect_sector(source, lba)
                                        {
                                            if message == "已有扇区读取正在执行" {
                                                state.advanced_inspect_mark_decode_pending(
                                                    lba, false,
                                                );
                                            } else {
                                                state.advanced_inspect_sector_finish(
                                                    lba,
                                                    Err(message.to_string()),
                                                );
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(message)
                                        if !matches!(
                                            state.advanced_inspect_prompt(),
                                            Some(state::AdvancedInspectPrompt::Jump { .. })
                                        ) =>
                                    {
                                        state.set_warning_notice(message);
                                    }
                                    Err(_) => {}
                                }
                            }
                            keymap::TuiAction::Text(ch) => {
                                state.advanced_inspect_prompt_push(ch);
                            }
                            _ => {}
                        }
                    }
                    return Some(KeyOutcome::NextIteration);
                }

                let sector_detail = state.advanced_inspect().is_some_and(|advanced| {
                    advanced.view_mode == state::InspectViewMode::Hex
                        && advanced.sector.is_some()
                        && advanced.panel == state::AdvancedInspectPanel::Bytes
                });
                if sector_detail {
                    let Some(action) = keys.map(state::InputMode::Normal, key) else {
                        return Some(KeyOutcome::NextIteration);
                    };
                    use keymap::TuiAction;
                    match action {
                        TuiAction::FocusNext | TuiAction::FocusPrevious => {}
                        TuiAction::MoveLeft => {
                            state.advanced_inspect_sector_move_cursor(-1);
                        }
                        TuiAction::MoveRight => {
                            state.advanced_inspect_sector_move_cursor(1);
                        }
                        TuiAction::MoveUp => {
                            state.advanced_inspect_sector_move_cursor(-16);
                        }
                        TuiAction::MoveDown => {
                            state.advanced_inspect_sector_move_cursor(16);
                        }
                        TuiAction::ViewOrVerify => {
                            state.advanced_inspect_sector_cycle_mode();
                        }
                        TuiAction::Open | TuiAction::Toggle => {
                            state.advanced_inspect_sector_toggle_field();
                        }
                        TuiAction::RowStart => {
                            state.advanced_inspect_sector_row_start();
                        }
                        TuiAction::RowEnd => {
                            state.advanced_inspect_sector_row_end();
                        }
                        TuiAction::Top => {
                            state.advanced_inspect_sector_top();
                        }
                        TuiAction::Bottom => {
                            state.advanced_inspect_sector_bottom();
                        }
                        TuiAction::HalfPageUp => {
                            state.advanced_inspect_sector_half_page(true);
                        }
                        TuiAction::HalfPageDown => {
                            state.advanced_inspect_sector_half_page(false);
                        }
                        TuiAction::Yank => {
                            let _ = state.advanced_inspect_sector_yank(false);
                        }
                        TuiAction::YankRaw => {
                            let _ = state.advanced_inspect_sector_yank(true);
                        }
                        TuiAction::PageUp => {
                            state.advanced_inspect_sector_page(true);
                        }
                        TuiAction::PageDown => {
                            state.advanced_inspect_sector_page(false);
                        }
                        TuiAction::SectorPrevious | TuiAction::SectorNext => {
                            let delta = if action == TuiAction::SectorPrevious {
                                -1
                            } else {
                                1
                            };
                            if let Some((source, lba)) = state.advanced_inspect_shift_sector(delta)
                            {
                                if let Err(message) =
                                    tasks.request_advanced_inspect_sector(source, lba)
                                {
                                    if message == "已有扇区读取正在执行" {
                                        state.advanced_inspect_mark_decode_pending(lba, false);
                                    } else {
                                        state.advanced_inspect_sector_finish(
                                            lba,
                                            Err(message.to_string()),
                                        );
                                    }
                                }
                            }
                        }
                        TuiAction::PanelNext | TuiAction::PanelPrevious => {
                            state.advanced_inspect_shift_panel(action == TuiAction::PanelPrevious);
                        }
                        TuiAction::PanelLeft => state.advanced_inspect_spatial_focus(-1, 0),
                        TuiAction::PanelRight => state.advanced_inspect_spatial_focus(1, 0),
                        TuiAction::PanelUp => state.advanced_inspect_spatial_focus(0, -1),
                        TuiAction::PanelDown => state.advanced_inspect_spatial_focus(0, 1),
                        TuiAction::Back => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                        }
                        TuiAction::InspectJump => {
                            state.advanced_inspect_begin_jump();
                        }
                        TuiAction::Search => {
                            state.advanced_inspect_begin_search();
                        }
                        TuiAction::NextMatch | TuiAction::PreviousMatch => {
                            if let Err(message) = state
                                .advanced_inspect_search_next(action == TuiAction::PreviousMatch)
                            {
                                state.set_warning_notice(message);
                            }
                        }
                        _ => {}
                    }
                    return Some(KeyOutcome::NextIteration);
                }

                let role = controller::active_widget_role(state);
                let Some(action) = keys.map_for_role(state::InputMode::Normal, role, key) else {
                    return Some(KeyOutcome::NextIteration);
                };
                let size = terminal_size;
                let viewport_height = size.height.saturating_sub(9) as usize;
                match dispatch_tui_action(
                    state,
                    tasks,
                    action,
                    role,
                    backup_dir,
                    viewport_height,
                    size.width,
                ) {
                    StateEffect::ExitRequested => return Some(KeyOutcome::Exit),
                    StateEffect::ExitDeferred | StateEffect::None => {}
                }
                return Some(KeyOutcome::NextIteration);
            }
        }
    }
    None
}
