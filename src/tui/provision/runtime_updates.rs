//! Provision owns delivery and state presentation of its task batch.
use super::*;
use std::path::Path;
pub(super) fn apply(
    state: &mut AppState,
    tasks: &mut TaskHub,
    updates: task::ProvisionUpdates,
    backup_dir: &Path,
) {
    if let Some(result) = updates.key_probe {
        state.provision_finish_key_probe(result.map_err(|error| error.to_string()));
    }
    for (session_id, domain, revision, result) in updates.key_verify {
        if session_id != state.provision().session_id {
            continue;
        }
        state.provision_finish_source_password_verify(
            domain,
            revision,
            result.map_err(|error| error.to_string()),
        );
    }
    if let Some(result) = updates.plan {
        state.provision_finish_plan(result.map_err(|error| error.to_string()));
    }
    for (_operation_id, event) in updates.progress {
        state.provision_push_progress(event);
    }
    if let Some((_operation_id, result)) = updates.write {
        let success = result.is_ok();
        state.provision_finish_write(result.map_err(|error| error.to_string()));
        if success {
            tasks.request_workspace_scan(backup_dir.to_path_buf());
            state.set_device_scan_pending(true);
            state.set_backup_scan_pending(true);
        }
    }
    if let Some(result) = updates.export {
        state.provision_finish_export(result.map_err(|error| error.to_string()));
    }
}
