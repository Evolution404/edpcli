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
