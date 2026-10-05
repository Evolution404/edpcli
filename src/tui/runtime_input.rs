use super::*;
use std::path::Path;

#[path = "runtime_input/backup_batch.rs"]
mod backup_batch;
#[path = "runtime_input/backup_prune.rs"]
mod backup_prune;
#[path = "runtime_input/backup_wizard.rs"]
mod backup_wizard;
#[path = "runtime_input/inspect.rs"]
mod inspect;
#[path = "runtime_input/post_restore_wizard.rs"]
mod post_restore_wizard;
#[path = "runtime_input/provision.rs"]
mod provision;
#[path = "runtime_input/shell.rs"]
mod shell;

pub(super) enum KeyOutcome {
    Handled,
    NextIteration,
    Exit,
    Elevate(state::WriteIntent),
}

pub(super) fn handle_key(
    state: &mut AppState,
    tasks: &mut TaskHub,
    keys: &mut KeyMapper,
    key: ct_event::KeyEvent,
    backup_dir: &Path,
    terminal_size: ratatui::layout::Size,
) -> KeyOutcome {
    state.set_viewport_size(terminal_size);
    if key.code == ct_event::KeyCode::F(2) && key.modifiers.is_empty() {
        // Reset any multi-key prefix without changing the underlying input mode.
        let _ = keys.map(state.input_mode(), key);
        if state.notice_details().is_some() {
            state.close_notice_details();
        } else {
            state.open_notice_details();
        }
        return KeyOutcome::NextIteration;
    }
    if state.notice_details().is_some() {
        if let Some(action) = keys.map(state::InputMode::Normal, key) {
            match action {
                keymap::TuiAction::Back => state.close_notice_details(),
                keymap::TuiAction::Quit => {
                    if state.navigate(NavCommand::Quit, usize::from(terminal_size.height))
                        == StateEffect::ExitRequested
                    {
                        return KeyOutcome::Exit;
                    }
                }
                action => state.scroll_notice_details(action),
            }
        }
        return KeyOutcome::NextIteration;
    }
    if state.help_open() {
        if let Some(action) = keys.map(state::InputMode::Normal, key) {
            let effect = dispatch_tui_action(
                state,
                tasks,
                action,
                controller::active_widget_role(state),
                backup_dir,
                usize::from(terminal_size.height),
                terminal_size.width,
            );
            if effect == StateEffect::ExitRequested {
                return KeyOutcome::Exit;
            }
        }
        return KeyOutcome::NextIteration;
    }
    if !state.help_open()
        && (state.provision().stage == state::ProvisionStage::Confirm
            || state.wizard().is_some_and(|wizard| {
                matches!(
                    wizard.stage,
                    state::WizardStage::Confirm
                        | state::WizardStage::FormatConfirm
                        | state::WizardStage::EncryptedFormatConfirm
                        | state::WizardStage::ReinitializeConfirm
                )
            }))
    {
        match key.code {
            ct_event::KeyCode::PageUp => {
                state.scroll_confirmation_details(true);
                return KeyOutcome::NextIteration;
            }
            ct_event::KeyCode::PageDown => {
                state.scroll_confirmation_details(false);
                return KeyOutcome::NextIteration;
            }
            _ => {}
        }
    }

    if let Some(outcome) =
        inspect::handle_inspect_key(state, tasks, keys, key, backup_dir, terminal_size)
    {
        return outcome;
    }
    if let Some(outcome) =
        provision::handle_provision_key(state, tasks, keys, key, backup_dir, terminal_size)
    {
        return outcome;
    }
    if let Some(outcome) =
        backup_batch::handle_backup_batch_key(state, tasks, keys, key, backup_dir, terminal_size)
    {
        return outcome;
    }
    if let Some(outcome) =
        backup_prune::handle_backup_prune_key(state, tasks, keys, key, backup_dir, terminal_size)
    {
        return outcome;
    }
    if let Some(outcome) =
        backup_wizard::handle_backup_wizard_key(state, tasks, keys, key, backup_dir, terminal_size)
    {
        return outcome;
    }
    shell::handle_shell_key(state, tasks, keys, key, backup_dir, terminal_size)
}
