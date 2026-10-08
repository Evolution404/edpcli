//! Background work coordinator for TUI read-side operations.
//!
//! Blocking device discovery is always executed on a worker thread. Generations make refreshes
//! race-safe: a slow old scan can never overwrite a newer request.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::application::{system_runner, BackupWorkspaceItem};
use crate::disk_scan::Row;

#[path = "task_gate.rs"]
mod task_gate;
pub use task_gate::{GenerationGate, SingleFlightGate};
use task_gate::{LatestCompletion, LatestRequest, TaskSlot};

#[path = "backups/task.rs"]
mod backups_task;
#[path = "inspect/task.rs"]
mod inspect_task;
#[path = "provision/task_model.rs"]
mod provision_model;
#[path = "provision/task.rs"]
mod provision_task;
use provision_model::{
    password_domain_index, PasswordVerifyRequest, ProvisionTaskState, ProvisionWorkerResult,
};
pub use provision_model::{KeyProbeContext, ProvisionUpdates};
#[path = "provision/task_updates.rs"]
mod provision_updates;
#[path = "task_progress.rs"]
mod task_progress;
use task_progress::{ProgressKind, ProgressPublisher, ProgressSlot};

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
        result: Result<(), crate::application::error::OperationError>,
    },
    Restore {
        operation_id: OperationId,
        result: Result<
            crate::application::post_restore::MetadataRestoreOutcome,
            crate::application::error::OperationError,
        >,
    },
    PostRestoreFormat {
        operation_id: OperationId,
        assessment: Option<crate::application::post_restore::PostRestoreAssessment>,
        result: crate::application::post_restore::PostRestoreFormatResult,
    },
    PostRestoreFormatProgress {
        operation_id: OperationId,
        event: crate::application::progress::ProgressEvent,
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
    Provision(ProvisionWorkerResult),
}

#[derive(Default)]
pub struct TaskUpdates {
    pub devices: Option<Vec<Row>>,
    pub backups: Option<Vec<BackupWorkspaceItem>>,
    pub write: Option<(
        OperationId,
        Result<(), crate::application::error::OperationError>,
    )>,
    pub restore: Option<(
        OperationId,
        Result<
            crate::application::post_restore::MetadataRestoreOutcome,
            crate::application::error::OperationError,
        >,
    )>,
    pub post_restore_assessment: Option<crate::application::post_restore::PostRestoreAssessment>,
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
    pub post_restore_format_progress:
        Vec<(OperationId, crate::application::progress::ProgressEvent)>,
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
    pub provision: ProvisionUpdates,
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
            || !self.post_restore_format_progress.is_empty()
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
            || self.provision.has_updates()
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

#[derive(Debug)]
struct ScanRequest {
    root: PathBuf,
    snapshot: Option<Arc<crate::application::catalog_snapshot::CatalogSnapshot>>,
}

pub struct TaskHub {
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    device_slot: TaskSlot<ScanRequest>,
    device_display_scan: Option<Arc<crate::application::catalog_snapshot::CatalogSnapshot>>,
    backup_display_scan: Option<Arc<crate::application::catalog_snapshot::CatalogSnapshot>>,
    backup_slot: TaskSlot<ScanRequest>,
    advanced_inspect_slot: TaskSlot<()>,
    advanced_inspect_sector_slot: TaskSlot<()>,
    verify_slot: TaskSlot<(PathBuf, PathBuf)>,
    provision: ProvisionTaskState,
    prune_slot: TaskSlot<()>,
    batch_delete_slot: TaskSlot<()>,
    progress_slot: ProgressSlot,
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
            device_display_scan: None,
            backup_display_scan: None,
            backup_slot: TaskSlot::new(),
            advanced_inspect_slot: TaskSlot::new(),
            advanced_inspect_sector_slot: TaskSlot::new(),
            verify_slot: TaskSlot::new(),
            provision: ProvisionTaskState::default(),
            prune_slot: TaskSlot::new(),
            batch_delete_slot: TaskSlot::new(),
            progress_slot: Arc::new(Mutex::new(None)),
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

    /// A worker can complete between terminal events. Bound event wait only
    /// while jobs run; an idle TUI retains its low-poll-frequency behavior.
    pub fn has_pending_work(&self) -> bool {
        self.active_operation.is_some()
            || self.device_slot.single_flight.is_running()
            || self.backup_slot.single_flight.is_running()
            || self.advanced_inspect_slot.single_flight.is_running()
            || self.advanced_inspect_sector_slot.single_flight.is_running()
            || self.verify_slot.single_flight.is_running()
            || self.prune_slot.single_flight.is_running()
            || self.batch_delete_slot.single_flight.is_running()
            || self.provision.key_probe_slot.single_flight.is_running()
            || self.provision.plan_slot.single_flight.is_running()
            || self.provision.export_slot.single_flight.is_running()
            || self
                .provision
                .password_verify_slots
                .iter()
                .any(|slot| slot.single_flight.is_running())
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

    pub fn request_workspace_scan(&mut self, root: PathBuf) -> (u64, u64) {
        if let Some(previous) = self.device_display_scan.take() {
            previous.cancel();
        }
        if let Some(previous) = self.backup_display_scan.take() {
            previous.cancel();
        }
        let snapshot = Arc::new(crate::application::catalog_snapshot::CatalogSnapshot::new(
            &root,
        ));
        self.device_display_scan = Some(snapshot.clone());
        self.backup_display_scan = Some(snapshot.clone());
        let devices = self.request_device_scan_model(ScanRequest {
            root: root.clone(),
            snapshot: Some(snapshot.clone()),
        });
        let backups = self.request_backup_scan_model(ScanRequest {
            root,
            snapshot: Some(snapshot),
        });
        (devices, backups)
    }

    pub fn request_device_scan(&mut self, backup_dir: PathBuf) -> u64 {
        let snapshot = Arc::new(crate::application::catalog_snapshot::CatalogSnapshot::new(
            &backup_dir,
        ));
        if let Some(previous) = self.device_display_scan.replace(snapshot.clone()) {
            if !self
                .backup_display_scan
                .as_ref()
                .is_some_and(|other| Arc::ptr_eq(&previous, other))
            {
                previous.cancel();
            }
        }
        self.request_device_scan_model(ScanRequest {
            root: backup_dir,
            snapshot: Some(snapshot),
        })
    }

    fn request_device_scan_model(&mut self, request: ScanRequest) -> u64 {
        match self.device_slot.request_latest(request) {
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

    fn start_device_scan(&mut self, generation: u64, request: ScanRequest) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                let runner = system_runner();
                if let Some(snapshot) = request.snapshot {
                    crate::application::scan_dashboard_snapshot(&runner, &snapshot)
                } else {
                    crate::application::scan_device_dashboard(&runner, &request.root)
                }
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
        let snapshot = Arc::new(crate::application::catalog_snapshot::CatalogSnapshot::new(
            &backup_dir,
        ));
        if let Some(previous) = self.backup_display_scan.replace(snapshot.clone()) {
            if !self
                .device_display_scan
                .as_ref()
                .is_some_and(|other| Arc::ptr_eq(&previous, other))
            {
                previous.cancel();
            }
        }
        self.request_backup_scan_model(ScanRequest {
            root: backup_dir,
            snapshot: Some(snapshot),
        })
    }

    fn request_backup_scan_model(&mut self, request: ScanRequest) -> u64 {
        match self.backup_slot.request_latest(request) {
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

    fn start_backup_scan(&mut self, generation: u64, request: ScanRequest) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                if let Some(snapshot) = request.snapshot {
                    crate::application::workspace_from_snapshot(&snapshot)
                } else {
                    crate::application::scan_backup_workspace_checked(&request.root)
                }
            }));
            let message = match outcome {
                Ok(Ok(rows)) => WorkerResult::Backups { generation, rows },
                Ok(Err(message)) => WorkerResult::BackupError {
                    generation,
                    message,
                },
                Err(payload) => WorkerResult::BackupError {
                    generation,
                    message: format!("备份扫描异常终止: {}", panic_message(payload)),
                },
            };
            let _ = tx.send(message);
        });
    }

    /// Limit each frame to 512 queued boundaries/results; snapshots use a single slot.
    pub fn poll(&mut self) -> TaskUpdates {
        let mut updates = TaskUpdates::default();
        let slot = self.progress_slot.clone();
        // Publishing and draining share the lock so a newer snapshot cannot
        // overtake an older queued boundary. Critical workers never wait for UI consumption.
        let mut pending = slot.lock().unwrap_or_else(|error| error.into_inner());
        let mut exhausted = false;
        for _ in 0..512 {
            let Ok(message) = self.rx.try_recv() else {
                exhausted = true;
                break;
            };
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
                    assessment,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.post_restore_assessment = assessment;
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
                WorkerResult::PostRestoreFormatProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        super::progress_transport::push_progress_coalesced(
                            &mut updates.post_restore_format_progress,
                            operation_id,
                            event,
                        );
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
                WorkerResult::Provision(result) => {
                    self.route_provision_result(result, &mut updates.provision)
                }
            }
        }
        if exhausted {
            if let Some((operation_id, kind, event)) = pending.take() {
                if self.active_operation == Some(operation_id) {
                    let target = match kind {
                        ProgressKind::Format => &mut updates.post_restore_format_progress,
                        ProgressKind::Provision => &mut updates.provision.progress,
                    };
                    super::progress_transport::push_progress_coalesced(target, operation_id, event);
                }
            }
        }
        updates
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
