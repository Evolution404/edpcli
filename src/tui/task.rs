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

#[derive(Debug)]
struct TaskSlot<P> {
    generation: GenerationGate,
    single_flight: SingleFlightGate,
    pending_latest: Option<(u64, P)>,
}

#[derive(Debug)]
enum LatestRequest<P> {
    Started { generation: u64, request: P },
    Queued { generation: u64 },
}

#[derive(Debug)]
enum LatestCompletion<P> {
    Restart { generation: u64, request: P },
    Deliver(bool),
}

impl<P> Default for TaskSlot<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> TaskSlot<P> {
    const fn new() -> Self {
        Self {
            generation: GenerationGate::new(),
            single_flight: SingleFlightGate::new(),
            pending_latest: None,
        }
    }

    fn try_begin(&mut self) -> Option<u64> {
        self.single_flight
            .try_start()
            .then(|| self.generation.begin())
    }

    fn request_latest(&mut self, request: P) -> LatestRequest<P> {
        let generation = self.generation.begin();
        if self.single_flight.try_start() {
            LatestRequest::Started {
                generation,
                request,
            }
        } else {
            self.pending_latest = Some((generation, request));
            LatestRequest::Queued { generation }
        }
    }

    fn finish(&mut self, generation: u64) -> bool {
        self.single_flight.finish();
        self.generation.is_current(generation)
    }

    fn finish_latest(&mut self, generation: u64) -> LatestCompletion<P> {
        self.single_flight.finish();
        if let Some((next_generation, request)) = self.pending_latest.take() {
            let started = self.single_flight.try_start();
            debug_assert!(started);
            LatestCompletion::Restart {
                generation: next_generation,
                request,
            }
        } else {
            LatestCompletion::Deliver(self.generation.is_current(generation))
        }
    }
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
    ProvisionKeyProbe {
        generation: u64,
        result: Result<crate::application::provision::ProvisionKeyProbe, String>,
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
    pub provision_key_probe:
        Option<Result<crate::application::provision::ProvisionKeyProbe, String>>,
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
            || self.provision_key_probe.is_some()
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
                WorkerResult::WriteProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.write_progress = Some((operation_id, event));
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
                WorkerResult::ProvisionPlan { generation, result } => {
                    if self.provision_slot.finish(generation) {
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
