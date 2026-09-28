use super::*;
use std::path::Path;

pub(super) fn apply_task_updates(
    state: &mut AppState,
    tasks: &mut TaskHub,
    updates: task::TaskUpdates,
    backup_dir: &Path,
) {
    if let Some(rows) = updates.devices {
        state.replace_devices(rows);
    }
    if let Some(rows) = updates.backups {
        state.replace_backups(rows);
    }
    if let Some(message) = updates.device_error {
        state.set_device_scan_pending(false);
        state.set_notice(message);
    }
    if let Some(message) = updates.backup_error {
        state.set_backup_scan_pending(false);
        state.set_notice(message);
    }
    for (_operation_id, event) in updates.write_progress {
        state.set_write_progress(event);
    }
    if let Some((_operation_id, result)) = updates.write {
        let refresh_backups = result.is_ok()
            && state
                .wizard()
                .is_some_and(|wizard| matches!(wizard.kind, state::WriteKind::BackupCreate));
        state.finish_write(result);
        if refresh_backups {
            tasks.request_backup_scan(backup_dir.to_path_buf());
            state.set_backup_scan_pending(true);
        }
    }
    if let Some((path, result)) = updates.backup_verify {
        state.set_backup_verify_run(None);
        match result {
            Ok(()) => state.set_notice(format!(
                "备份 {} 校验通过：大小与 SHA-256 正常。",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<无效文件名>")
            )),
            Err(message) => state.set_notice(message),
        }
    }
    if let Some((_operation_id, result)) = updates.backup_delete {
        let refresh_backups = result.is_ok();
        state.finish_backup_delete(result);
        if refresh_backups {
            tasks.request_backup_scan(backup_dir.to_path_buf());
            state.set_backup_scan_pending(true);
        }
    }
    if let Some(result) = updates.backup_batch_delete_plan {
        state.backup_batch_delete_finish_plan(result);
    }
    if let Some((_operation_id, result)) = updates.backup_batch_delete_execute {
        let refresh_backups = result.is_ok();
        state.backup_batch_delete_finish_execute(result);
        if refresh_backups {
            tasks.request_backup_scan(backup_dir.to_path_buf());
            state.set_backup_scan_pending(true);
        }
    }
    if let Some(result) = updates.backup_prune_plan {
        state.backup_prune_finish_plan(result);
    }
    if let Some((_operation_id, result)) = updates.backup_prune_execute {
        let refresh_backups = result.is_ok();
        state.backup_prune_finish_execute(result);
        if refresh_backups {
            tasks.request_backup_scan(backup_dir.to_path_buf());
            state.set_backup_scan_pending(true);
        }
    }
    if let Some(result) = updates.provision_key_probe {
        state.provision_finish_key_probe(result);
    }
    if let Some((domain, result)) = updates.provision_key_verify {
        state.provision_finish_source_password_verify(domain, result);
    }
    if let Some(result) = updates.provision_plan {
        state.provision_finish_plan(result);
    }
    for (_operation_id, event) in updates.provision_progress {
        state.provision_push_progress(event);
    }
    if let Some((_operation_id, result)) = updates.provision_write {
        let success = result.is_ok();
        state.provision_finish_write(result);
        if success {
            tasks.request_device_scan(backup_dir.to_path_buf());
            tasks.request_backup_scan(backup_dir.to_path_buf());
            state.set_device_scan_pending(true);
            state.set_backup_scan_pending(true);
        }
    }
    if let Some(result) = updates.provision_export {
        state.provision_finish_export(result);
    }
    if let Some(result) = updates.advanced_inspect {
        state.advanced_inspect_finish(result);
    }
    if let Some((lba, result)) = updates.advanced_inspect_sector {
        state.advanced_inspect_sector_finish(lba, result);
    }
    if let Some((source, lba)) = state.advanced_inspect_decode_request() {
        if tasks.request_advanced_inspect_sector(source, lba).is_ok() {
            state.advanced_inspect_mark_decode_pending(lba, true);
        }
    }
    if let Some((source, lba)) = state.advanced_inspect_preview_request() {
        if tasks.request_advanced_inspect_preview(source, lba).is_ok() {
            state.advanced_inspect_mark_preview_pending(lba);
        }
    }
}
