//! Background work coordinator for TUI read-side operations.
//!
//! Blocking device discovery is always executed on a worker thread. Generations make refreshes
//! race-safe: a slow old scan can never overwrite a newer request.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::application::BackupWorkspaceItem;
use crate::disk_scan::Row;
use crate::sysinfo::SysRunner;

#[path = "task_gate.rs"]
mod task_gate;
pub use task_gate::{GenerationGate, SingleFlightGate};
use task_gate::{LatestCompletion, LatestRequest, TaskSlot};

#[path = "backups/task.rs"]
mod backups_task;
#[path = "inspect/task.rs"]
mod inspect_task;
#[path = "provision/task.rs"]
mod provision_task;
#[path = "write_task.rs"]
mod write_task;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationId(u64);

impl OperationId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

enum WorkerResult {
    Devices {
        generation: u64,
        rows: Vec<Row>,
    },
    Backups {
        generation: u64,
        rows: Vec<BackupWorkspaceItem>,
    },
    Write {
        operation_id: OperationId,
        result: Result<(), String>,
    },
    Restore {
        operation_id: OperationId,
        result: Result<crate::application::post_restore::MetadataRestoreOutcome, String>,
    },
    PostRestoreFormat {
        operation_id: OperationId,
        result: crate::application::post_restore::PostRestoreFormatResult,
    },
    PostRestoreEncryptedFormat {
        operation_id: OperationId,
        result: crate::application::post_restore::EncryptedPostRestoreFormatResult,
    },
    PostRestoreReinitialize {
        operation_id: OperationId,
        result: crate::application::post_restore::EncryptedPartitionReinitializeResult,
    },
    WriteProgress {
        operation_id: OperationId,
        event: crate::application::WriteEvent,
    },
    AdvancedInspect {
        generation: u64,
        result: Result<crate::application::inspect::AdvancedInspectWorkspace, String>,
    },
    AdvancedInspectSector {
        generation: u64,
        lba: u64,
        result: Result<crate::application::inspect::AdvancedInspectItem, String>,
    },
    DeviceError {
        generation: u64,
        message: String,
    },
    BackupError {
        generation: u64,
        message: String,
    },
    BackupVerify {
        generation: u64,
        path: PathBuf,
        result: Result<(), String>,
    },
    BackupDelete {
        operation_id: OperationId,
        result: Result<(), String>,
    },
    BackupBatchDeletePlan {
        generation: u64,
        result: Result<crate::application::backup::DeletePlan, String>,
    },
    BackupBatchDeleteExecute {
        operation_id: OperationId,
        result: Result<usize, String>,
    },
    BackupPrunePlan {
        generation: u64,
        result: Result<crate::tui::state::BackupPrunePrepared, String>,
    },
    BackupPruneExecute {
        operation_id: OperationId,
        result: Result<usize, String>,
    },
    ProvisionKeyProbe {
        generation: u64,
        result: Result<crate::application::provision::ProvisionKeyProbe, String>,
    },
    ProvisionKeyVerify {
        revision: u64,
        domain: crate::provision::KeyDomainRole,
        result: Result<crate::provision::SourcePasswordKnowledge, String>,
    },
    ProvisionPlan {
        generation: u64,
        result: Result<crate::tui::state::ProvisionPrepared, String>,
    },
    ProvisionProgress {
        operation_id: OperationId,
        event: crate::application::progress::ProgressEvent,
    },
    ProvisionWrite {
        operation_id: OperationId,
        result: Result<crate::application::provision::ProvisionWriteOutcome, String>,
    },
    ProvisionExport {
        generation: u64,
        result: Result<PathBuf, String>,
    },
}

#[derive(Default)]
pub struct TaskUpdates {
    pub devices: Option<Vec<Row>>,
    pub backups: Option<Vec<BackupWorkspaceItem>>,
    pub write: Option<(OperationId, Result<(), String>)>,
    pub restore: Option<(
        OperationId,
        Result<crate::application::post_restore::MetadataRestoreOutcome, String>,
    )>,
    pub post_restore_format: Option<(
        OperationId,
        crate::application::post_restore::PostRestoreFormatResult,
    )>,
    pub post_restore_encrypted_format: Option<(
        OperationId,
        crate::application::post_restore::EncryptedPostRestoreFormatResult,
    )>,
    pub post_restore_reinitialize: Option<(
        OperationId,
        crate::application::post_restore::EncryptedPartitionReinitializeResult,
    )>,
    pub write_progress: Vec<(OperationId, crate::application::WriteEvent)>,
    pub advanced_inspect:
        Option<Result<crate::application::inspect::AdvancedInspectWorkspace, String>>,
    pub advanced_inspect_sector: Option<(
        u64,
        Result<crate::application::inspect::AdvancedInspectItem, String>,
    )>,
    pub device_error: Option<String>,
    pub backup_error: Option<String>,
    pub backup_verify: Option<(PathBuf, Result<(), String>)>,
    pub backup_delete: Option<(OperationId, Result<(), String>)>,
    pub backup_batch_delete_plan: Option<Result<crate::application::backup::DeletePlan, String>>,
    pub backup_batch_delete_execute: Option<(OperationId, Result<usize, String>)>,
    pub backup_prune_plan: Option<Result<crate::tui::state::BackupPrunePrepared, String>>,
    pub backup_prune_execute: Option<(OperationId, Result<usize, String>)>,
    pub provision_key_probe:
        Option<Result<crate::application::provision::ProvisionKeyProbe, String>>,
    pub provision_key_verify: Vec<(
        crate::provision::KeyDomainRole,
        u64,
        Result<crate::provision::SourcePasswordKnowledge, String>,
    )>,
    pub provision_plan: Option<Result<crate::tui::state::ProvisionPrepared, String>>,
    pub provision_progress: Vec<(OperationId, crate::application::progress::ProgressEvent)>,
    pub provision_write: Option<(
        OperationId,
        Result<crate::application::provision::ProvisionWriteOutcome, String>,
    )>,
    pub provision_export: Option<Result<PathBuf, String>>,
}

impl TaskUpdates {
    pub fn has_updates(&self) -> bool {
        self.devices.is_some()
            || self.backups.is_some()
            || self.write.is_some()
            || self.restore.is_some()
            || self.post_restore_format.is_some()
            || self.post_restore_encrypted_format.is_some()
            || self.post_restore_reinitialize.is_some()
            || !self.write_progress.is_empty()
            || self.advanced_inspect.is_some()
            || self.advanced_inspect_sector.is_some()
            || self.device_error.is_some()
            || self.backup_error.is_some()
            || self.backup_verify.is_some()
            || self.backup_delete.is_some()
            || self.backup_batch_delete_plan.is_some()
            || self.backup_batch_delete_execute.is_some()
            || self.backup_prune_plan.is_some()
            || self.backup_prune_execute.is_some()
            || self.provision_key_probe.is_some()
            || !self.provision_key_verify.is_empty()
            || self.provision_plan.is_some()
            || !self.provision_progress.is_empty()
            || self.provision_write.is_some()
            || self.provision_export.is_some()
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "未知 panic payload".to_string()
    }
}

pub struct TaskHub {
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    device_slot: TaskSlot<PathBuf>,
    backup_slot: TaskSlot<PathBuf>,
    advanced_inspect_slot: TaskSlot<()>,
    advanced_inspect_sector_slot: TaskSlot<()>,
    verify_slot: TaskSlot<(PathBuf, PathBuf)>,
    provision_key_probe_slot: TaskSlot<()>,
    provision_slot: TaskSlot<()>,
    provision_export_slot: TaskSlot<()>,
    prune_slot: TaskSlot<()>,
    batch_delete_slot: TaskSlot<()>,
    next_operation_id: u64,
    active_operation: Option<OperationId>,
    critical_worker: Option<std::thread::JoinHandle<()>>,
}

impl Default for TaskHub {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskHub {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            device_slot: TaskSlot::new(),
            backup_slot: TaskSlot::new(),
            advanced_inspect_slot: TaskSlot::new(),
            advanced_inspect_sector_slot: TaskSlot::new(),
            verify_slot: TaskSlot::new(),
            provision_key_probe_slot: TaskSlot::new(),
            provision_slot: TaskSlot::new(),
            provision_export_slot: TaskSlot::new(),
            prune_slot: TaskSlot::new(),
            batch_delete_slot: TaskSlot::new(),
            next_operation_id: 0,
            active_operation: None,
            critical_worker: None,
        }
    }

    fn begin_operation(&mut self) -> Result<OperationId, &'static str> {
        if self.active_operation.is_some() {
            return Err("已有关键操作正在执行");
        }
        self.next_operation_id = self.next_operation_id.wrapping_add(1);
        if self.next_operation_id == 0 {
            self.next_operation_id = 1;
        }
        let operation_id = OperationId(self.next_operation_id);
        self.active_operation = Some(operation_id);
        Ok(operation_id)
    }

    pub const fn active_operation(&self) -> Option<OperationId> {
        self.active_operation
    }

    /// Wait for the current critical worker to finish its safety chain. This is
    /// used after terminal I/O fails so losing the UI cannot terminate a raw-disk
    /// transaction halfway through its rollback/readback path.
    pub fn wait_for_critical_operation(&mut self) {
        if let Some(worker) = self.critical_worker.take() {
            let _ = worker.join();
        }
        self.active_operation = None;
    }

    fn finish_operation(&mut self, operation_id: OperationId) -> bool {
        if self.active_operation != Some(operation_id) {
            return false;
        }
        if let Some(worker) = self.critical_worker.take() {
            let _ = worker.join();
        }
        self.active_operation = None;
        true
    }

    pub fn request_device_scan(&mut self, backup_dir: PathBuf) -> u64 {
        match self.device_slot.request_latest(backup_dir) {
            LatestRequest::Started {
                generation,
                request,
            } => {
                self.start_device_scan(generation, request);
                generation
            }
            LatestRequest::Queued { generation } => generation,
        }
    }

    fn start_device_scan(&mut self, generation: u64, backup_dir: PathBuf) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::scan_device_dashboard(&runner, &backup_dir)
            }));
            let message = match outcome {
                Ok(rows) => WorkerResult::Devices { generation, rows },
                Err(payload) => WorkerResult::DeviceError {
                    generation,
                    message: format!("设备扫描异常终止: {}", panic_message(payload)),
                },
            };
            let _ = tx.send(message);
        });
    }

    pub fn request_backup_scan(&mut self, backup_dir: PathBuf) -> u64 {
        match self.backup_slot.request_latest(backup_dir) {
            LatestRequest::Started {
                generation,
                request,
            } => {
                self.start_backup_scan(generation, request);
                generation
            }
            LatestRequest::Queued { generation } => generation,
        }
    }

    fn start_backup_scan(&mut self, generation: u64, backup_dir: PathBuf) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                crate::application::scan_backup_workspace(&backup_dir)
            }));
            let message = match outcome {
                Ok(rows) => WorkerResult::Backups { generation, rows },
                Err(payload) => WorkerResult::BackupError {
                    generation,
                    message: format!("备份扫描异常终止: {}", panic_message(payload)),
                },
            };
            let _ = tx.send(message);
        });
    }

    /// Drain all ready messages exactly once so one result kind cannot consume another.
    pub fn poll(&mut self) -> TaskUpdates {
        let mut updates = TaskUpdates::default();
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Devices { generation, rows } => {
                    match self.device_slot.finish_latest(generation) {
                        LatestCompletion::Restart {
                            generation,
                            request,
                        } => self.start_device_scan(generation, request),
                        LatestCompletion::Deliver(true) => updates.devices = Some(rows),
                        LatestCompletion::Deliver(false) => {}
                    }
                }
                WorkerResult::Backups { generation, rows } => {
                    match self.backup_slot.finish_latest(generation) {
                        LatestCompletion::Restart {
                            generation,
                            request,
                        } => self.start_backup_scan(generation, request),
                        LatestCompletion::Deliver(true) => updates.backups = Some(rows),
                        LatestCompletion::Deliver(false) => {}
                    }
                }
                WorkerResult::Write {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.write = Some((operation_id, result));
                    }
                }
                WorkerResult::Restore {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.restore = Some((operation_id, result));
                    }
                }
                WorkerResult::PostRestoreFormat {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.post_restore_format = Some((operation_id, result));
                    }
                }
                WorkerResult::PostRestoreEncryptedFormat {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.post_restore_encrypted_format = Some((operation_id, result));
                    }
                }
                WorkerResult::PostRestoreReinitialize {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.post_restore_reinitialize = Some((operation_id, result));
                    }
                }
                WorkerResult::WriteProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.write_progress.push((operation_id, event));
                    }
                }
                WorkerResult::AdvancedInspect { generation, result } => {
                    if self.advanced_inspect_slot.finish(generation) {
                        updates.advanced_inspect = Some(result);
                    }
                }
                WorkerResult::AdvancedInspectSector {
                    generation,
                    lba,
                    result,
                } => {
                    if self.advanced_inspect_sector_slot.finish(generation) {
                        updates.advanced_inspect_sector = Some((lba, result));
                    }
                }
                WorkerResult::DeviceError {
                    generation,
                    message,
                } => match self.device_slot.finish_latest(generation) {
                    LatestCompletion::Restart {
                        generation,
                        request,
                    } => self.start_device_scan(generation, request),
                    LatestCompletion::Deliver(true) => updates.device_error = Some(message),
                    LatestCompletion::Deliver(false) => {}
                },
                WorkerResult::BackupError {
                    generation,
                    message,
                } => match self.backup_slot.finish_latest(generation) {
                    LatestCompletion::Restart {
                        generation,
                        request,
                    } => self.start_backup_scan(generation, request),
                    LatestCompletion::Deliver(true) => updates.backup_error = Some(message),
                    LatestCompletion::Deliver(false) => {}
                },
                WorkerResult::BackupVerify {
                    generation,
                    path,
                    result,
                } => match self.verify_slot.finish_latest(generation) {
                    LatestCompletion::Restart {
                        generation,
                        request: (next_path, backup_dir),
                    } => self.start_backup_verify(generation, next_path, backup_dir),
                    LatestCompletion::Deliver(true) => {
                        updates.backup_verify = Some((path, result));
                    }
                    LatestCompletion::Deliver(false) => {}
                },
                WorkerResult::BackupDelete {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.backup_delete = Some((operation_id, result));
                    }
                }
                WorkerResult::BackupBatchDeletePlan { generation, result } => {
                    if self.batch_delete_slot.finish(generation) {
                        updates.backup_batch_delete_plan = Some(result);
                    }
                }
                WorkerResult::BackupBatchDeleteExecute {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.backup_batch_delete_execute = Some((operation_id, result));
                    }
                }
                WorkerResult::BackupPrunePlan { generation, result } => {
                    if self.prune_slot.finish(generation) {
                        updates.backup_prune_plan = Some(result);
                    }
                }
                WorkerResult::BackupPruneExecute {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.backup_prune_execute = Some((operation_id, result));
                    }
                }
                WorkerResult::ProvisionKeyProbe { generation, result } => {
                    if self.provision_key_probe_slot.finish(generation) {
                        updates.provision_key_probe = Some(result);
                    }
                }
                WorkerResult::ProvisionKeyVerify {
                    revision,
                    domain,
                    result,
                } => {
                    updates
                        .provision_key_verify
                        .push((domain, revision, result));
                }
                WorkerResult::ProvisionPlan { generation, result } => {
                    if self.provision_slot.finish(generation) {
                        updates.provision_plan = Some(result);
                    }
                }
                WorkerResult::ProvisionProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        super::progress_transport::push_progress_coalesced(
                            &mut updates.provision_progress,
                            operation_id,
                            event,
                        );
                    }
                }
                WorkerResult::ProvisionWrite {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.provision_write = Some((operation_id, result));
                    }
                }
                WorkerResult::ProvisionExport { generation, result } => {
                    if self.provision_export_slot.finish(generation) {
                        updates.provision_export = Some(result);
                    }
                }
            }
        }
        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_operation_slot_is_exclusive_and_ids_do_not_repeat() {
        let mut hub = TaskHub::new();
        let first = hub.begin_operation().expect("first operation");
        assert_eq!(hub.active_operation(), Some(first));
        assert!(hub.begin_operation().is_err());
        assert!(hub.finish_operation(first));

        let second = hub.begin_operation().expect("second operation");
        assert_ne!(first, second);
    }

    #[test]
    fn progress_batch_retains_every_worker_event_in_order() {
        let mut hub = TaskHub::new();
        let operation_id = hub.begin_operation().unwrap();
        for (index, phase) in [
            crate::application::progress::Phase::Backup,
            crate::application::progress::Phase::Metadata,
            crate::application::progress::Phase::Readback,
        ]
        .into_iter()
        .enumerate()
        {
            hub.tx
                .send(WorkerResult::ProvisionProgress {
                    operation_id,
                    event: crate::application::progress::ProgressEvent::new(
                        phase,
                        crate::application::progress::Step::ProtocolReadback,
                        index as u64,
                        3,
                    ),
                })
                .unwrap();
        }
        let updates = hub.poll();
        assert_eq!(
            updates
                .provision_progress
                .into_iter()
                .map(|(_, event)| event.phase)
                .collect::<Vec<_>>(),
            [
                crate::application::progress::Phase::Backup,
                crate::application::progress::Phase::Metadata,
                crate::application::progress::Phase::Readback,
            ]
        );
    }

    #[test]
    fn backup_restore_progress_batch_retains_every_event_in_order() {
        let mut hub = TaskHub::new();
        let operation_id = hub.begin_operation().unwrap();
        for event in [
            crate::application::WriteEvent::BackupCreated {
                path: std::path::PathBuf::from("a.edpb"),
            },
            crate::application::WriteEvent::RestoreWriteCompleted,
        ] {
            hub.tx
                .send(WorkerResult::WriteProgress {
                    operation_id,
                    event,
                })
                .unwrap();
        }
        let updates = hub.poll();
        assert_eq!(updates.write_progress.len(), 2);
        assert!(matches!(
            updates.write_progress[0].1,
            crate::application::WriteEvent::BackupCreated { .. }
        ));
        assert!(matches!(
            updates.write_progress[1].1,
            crate::application::WriteEvent::RestoreWriteCompleted
        ));
    }

    #[test]
    fn critical_worker_can_be_joined_after_ui_failure() {
        let mut hub = TaskHub::new();
        let operation_id = hub.begin_operation().expect("operation");
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_completed = completed.clone();
        hub.critical_worker = Some(std::thread::spawn(move || {
            worker_completed.store(true, std::sync::atomic::Ordering::SeqCst);
        }));

        hub.wait_for_critical_operation();
        assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(hub.active_operation(), None);
        assert!(operation_id.get() > 0);
    }

    #[test]
    fn refresh_while_scanning_keeps_one_latest_follow_up() {
        let mut hub = TaskHub::new();
        assert!(hub.device_slot.single_flight.try_start());

        hub.request_device_scan(PathBuf::from("first"));
        hub.request_device_scan(PathBuf::from("latest"));

        assert_eq!(
            hub.device_slot
                .pending_latest
                .as_ref()
                .map(|(_, path)| path.as_path()),
            Some(std::path::Path::new("latest"))
        );
        assert!(hub.device_slot.single_flight.is_running());
    }

    #[test]
    fn verify_keeps_only_the_latest_queued_target() {
        let mut hub = TaskHub::new();
        assert!(hub.verify_slot.single_flight.try_start());
        hub.request_backup_verify(PathBuf::from("one.bin"), PathBuf::from("backups"));
        hub.request_backup_verify(PathBuf::from("two.bin"), PathBuf::from("backups"));
        assert!(matches!(
            hub.verify_slot.pending_latest.as_ref(),
            Some((_, (path, _))) if path == &PathBuf::from("two.bin")
        ));
    }
}
