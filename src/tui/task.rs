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

#[derive(Debug, Default)]
pub struct GenerationGate {
    current: u64,
}

#[derive(Debug, Default)]
pub struct SingleFlightGate {
    running: bool,
}

impl SingleFlightGate {
    pub const fn new() -> Self {
        Self { running: false }
    }

    pub fn try_start(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    pub fn finish(&mut self) {
        self.running = false;
    }

    pub const fn is_running(&self) -> bool {
        self.running
    }
}

impl GenerationGate {
    pub const fn new() -> Self {
        Self { current: 0 }
    }

    pub fn begin(&mut self) -> u64 {
        self.current = self.current.wrapping_add(1);
        if self.current == 0 {
            self.current = 1;
        }
        self.current
    }

    pub const fn is_current(&self, generation: u64) -> bool {
        generation == self.current
    }

    pub const fn current(&self) -> u64 {
        self.current
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
    ProvisionPlan {
        generation: u64,
        result: Result<crate::tui::state::ProvisionPrepared, String>,
    },
    ProvisionBackup {
        operation_id: OperationId,
        result: Result<(), String>,
    },
    ProvisionProgress {
        operation_id: OperationId,
        message: String,
    },
    ProvisionWrite {
        operation_id: OperationId,
        result: Result<String, String>,
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
    pub write_progress: Option<(OperationId, crate::application::WriteEvent)>,
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
    pub provision_plan: Option<Result<crate::tui::state::ProvisionPrepared, String>>,
    pub provision_backup: Option<(OperationId, Result<(), String>)>,
    pub provision_progress: Option<(OperationId, String)>,
    pub provision_write: Option<(OperationId, Result<String, String>)>,
    pub provision_export: Option<Result<PathBuf, String>>,
}

impl TaskUpdates {
    pub fn has_updates(&self) -> bool {
        self.devices.is_some()
            || self.backups.is_some()
            || self.write.is_some()
            || self.write_progress.is_some()
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
            || self.provision_plan.is_some()
            || self.provision_backup.is_some()
            || self.provision_progress.is_some()
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
    device_generation: GenerationGate,
    backup_generation: GenerationGate,
    advanced_inspect_generation: GenerationGate,
    advanced_inspect_sector_generation: GenerationGate,
    verify_generation: GenerationGate,
    provision_generation: GenerationGate,
    provision_export_generation: GenerationGate,
    prune_generation: GenerationGate,
    batch_delete_generation: GenerationGate,
    device_single_flight: SingleFlightGate,
    backup_single_flight: SingleFlightGate,
    advanced_inspect_single_flight: SingleFlightGate,
    advanced_inspect_sector_single_flight: SingleFlightGate,
    verify_single_flight: SingleFlightGate,
    provision_single_flight: SingleFlightGate,
    provision_export_single_flight: SingleFlightGate,
    prune_single_flight: SingleFlightGate,
    batch_delete_single_flight: SingleFlightGate,
    pending_device_scan: Option<PathBuf>,
    pending_backup_scan: Option<PathBuf>,
    pending_verify: Option<(u64, PathBuf, PathBuf)>,
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
            device_generation: GenerationGate::new(),
            backup_generation: GenerationGate::new(),
            advanced_inspect_generation: GenerationGate::new(),
            advanced_inspect_sector_generation: GenerationGate::new(),
            verify_generation: GenerationGate::new(),
            provision_generation: GenerationGate::new(),
            provision_export_generation: GenerationGate::new(),
            prune_generation: GenerationGate::new(),
            batch_delete_generation: GenerationGate::new(),
            device_single_flight: SingleFlightGate::new(),
            backup_single_flight: SingleFlightGate::new(),
            advanced_inspect_single_flight: SingleFlightGate::new(),
            advanced_inspect_sector_single_flight: SingleFlightGate::new(),
            verify_single_flight: SingleFlightGate::new(),
            provision_single_flight: SingleFlightGate::new(),
            provision_export_single_flight: SingleFlightGate::new(),
            prune_single_flight: SingleFlightGate::new(),
            batch_delete_single_flight: SingleFlightGate::new(),
            pending_device_scan: None,
            pending_backup_scan: None,
            pending_verify: None,
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
        if !self.device_single_flight.try_start() {
            self.pending_device_scan = Some(backup_dir);
            return self.device_generation.current();
        }
        self.start_device_scan(backup_dir)
    }

    fn start_device_scan(&mut self, backup_dir: PathBuf) -> u64 {
        let generation = self.device_generation.begin();
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
        generation
    }

    pub fn request_backup_scan(&mut self, backup_dir: PathBuf) -> u64 {
        if !self.backup_single_flight.try_start() {
            self.pending_backup_scan = Some(backup_dir);
            return self.backup_generation.current();
        }
        self.start_backup_scan(backup_dir)
    }

    fn start_backup_scan(&mut self, backup_dir: PathBuf) -> u64 {
        let generation = self.backup_generation.begin();
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
        generation
    }

    /// Drain all ready messages exactly once so one result kind cannot consume another.
    pub fn poll(&mut self) -> TaskUpdates {
        let mut updates = TaskUpdates::default();
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Devices { generation, rows } => {
                    self.device_single_flight.finish();
                    if let Some(backup_dir) = self.pending_device_scan.take() {
                        let started = self.device_single_flight.try_start();
                        debug_assert!(started);
                        self.start_device_scan(backup_dir);
                    } else if self.device_generation.is_current(generation) {
                        updates.devices = Some(rows);
                    }
                }
                WorkerResult::Backups { generation, rows } => {
                    self.backup_single_flight.finish();
                    if let Some(backup_dir) = self.pending_backup_scan.take() {
                        let started = self.backup_single_flight.try_start();
                        debug_assert!(started);
                        self.start_backup_scan(backup_dir);
                    } else if self.backup_generation.is_current(generation) {
                        updates.backups = Some(rows);
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
                WorkerResult::WriteProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.write_progress = Some((operation_id, event));
                    }
                }
                WorkerResult::AdvancedInspect { generation, result } => {
                    self.advanced_inspect_single_flight.finish();
                    if self.advanced_inspect_generation.is_current(generation) {
                        updates.advanced_inspect = Some(result);
                    }
                }
                WorkerResult::AdvancedInspectSector {
                    generation,
                    lba,
                    result,
                } => {
                    self.advanced_inspect_sector_single_flight.finish();
                    if self
                        .advanced_inspect_sector_generation
                        .is_current(generation)
                    {
                        updates.advanced_inspect_sector = Some((lba, result));
                    }
                }
                WorkerResult::DeviceError {
                    generation,
                    message,
                } => {
                    self.device_single_flight.finish();
                    if let Some(backup_dir) = self.pending_device_scan.take() {
                        let started = self.device_single_flight.try_start();
                        debug_assert!(started);
                        self.start_device_scan(backup_dir);
                    } else if self.device_generation.is_current(generation) {
                        updates.device_error = Some(message);
                    }
                }
                WorkerResult::BackupError {
                    generation,
                    message,
                } => {
                    self.backup_single_flight.finish();
                    if let Some(backup_dir) = self.pending_backup_scan.take() {
                        let started = self.backup_single_flight.try_start();
                        debug_assert!(started);
                        self.start_backup_scan(backup_dir);
                    } else if self.backup_generation.is_current(generation) {
                        updates.backup_error = Some(message);
                    }
                }
                WorkerResult::BackupVerify {
                    generation,
                    path,
                    result,
                } => {
                    self.verify_single_flight.finish();
                    if let Some((next_generation, next_path, backup_dir)) =
                        self.pending_verify.take()
                    {
                        let started = self.verify_single_flight.try_start();
                        debug_assert!(started);
                        self.start_backup_verify(next_generation, next_path, backup_dir);
                    } else if self.verify_generation.is_current(generation) {
                        updates.backup_verify = Some((path, result));
                    }
                }
                WorkerResult::BackupDelete {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.backup_delete = Some((operation_id, result));
                    }
                }
                WorkerResult::BackupBatchDeletePlan { generation, result } => {
                    self.batch_delete_single_flight.finish();
                    if self.batch_delete_generation.is_current(generation) {
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
                    self.prune_single_flight.finish();
                    if self.prune_generation.is_current(generation) {
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
                WorkerResult::ProvisionPlan { generation, result } => {
                    self.provision_single_flight.finish();
                    if self.provision_generation.is_current(generation) {
                        updates.provision_plan = Some(result);
                    }
                }
                WorkerResult::ProvisionBackup {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.provision_backup = Some((operation_id, result));
                    }
                }
                WorkerResult::ProvisionProgress {
                    operation_id,
                    message,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.provision_progress = Some((operation_id, message));
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
                    self.provision_export_single_flight.finish();
                    if self.provision_export_generation.is_current(generation) {
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
        assert!(hub.device_single_flight.try_start());

        hub.request_device_scan(PathBuf::from("first"));
        hub.request_device_scan(PathBuf::from("latest"));

        assert_eq!(hub.pending_device_scan, Some(PathBuf::from("latest")));
        assert!(hub.device_single_flight.is_running());
    }

    #[test]
    fn verify_keeps_only_the_latest_queued_target() {
        let mut hub = TaskHub::new();
        assert!(hub.verify_single_flight.try_start());
        hub.request_backup_verify(PathBuf::from("one.bin"), PathBuf::from("backups"));
        hub.request_backup_verify(PathBuf::from("two.bin"), PathBuf::from("backups"));
        assert!(matches!(
            hub.pending_verify,
            Some((_, ref path, _)) if path == &PathBuf::from("two.bin")
        ));
    }
}
