//! Background work coordinator for TUI read-side operations.
//!
//! Blocking device discovery is always executed on a worker thread. Generations make refreshes
//! race-safe: a slow old scan can never overwrite a newer request.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::application::{inspect::InspectWorkspace, BackupWorkspaceItem};
use crate::disk_scan::Row;
use crate::sysinfo::SysRunner;

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
        message: String,
    },
    Inspect {
        generation: u64,
        result: Result<InspectWorkspace, String>,
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
}

enum InspectRequest {
    Disk(u32),
    Backup(PathBuf),
}

#[derive(Default)]
pub struct TaskUpdates {
    pub devices: Option<Vec<Row>>,
    pub backups: Option<Vec<BackupWorkspaceItem>>,
    pub write: Option<(OperationId, Result<(), String>)>,
    pub write_progress: Option<(OperationId, String)>,
    pub inspect: Option<Result<InspectWorkspace, String>>,
    pub device_error: Option<String>,
    pub backup_error: Option<String>,
    pub backup_verify: Option<(PathBuf, Result<(), String>)>,
    pub backup_delete: Option<(OperationId, Result<(), String>)>,
}

impl TaskUpdates {
    pub fn has_updates(&self) -> bool {
        self.devices.is_some()
            || self.backups.is_some()
            || self.write.is_some()
            || self.write_progress.is_some()
            || self.inspect.is_some()
            || self.device_error.is_some()
            || self.backup_error.is_some()
            || self.backup_verify.is_some()
            || self.backup_delete.is_some()
    }
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn progress_summary(input: &str) -> String {
    let plain = strip_ansi(input);
    plain
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("写盘安全链执行中")
        .trim()
        .chars()
        .take(160)
        .collect()
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
    inspect_generation: GenerationGate,
    verify_generation: GenerationGate,
    device_single_flight: SingleFlightGate,
    backup_single_flight: SingleFlightGate,
    inspect_single_flight: SingleFlightGate,
    verify_single_flight: SingleFlightGate,
    pending_device_scan: Option<PathBuf>,
    pending_backup_scan: Option<PathBuf>,
    pending_inspect: Option<(u64, InspectRequest)>,
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
            inspect_generation: GenerationGate::new(),
            verify_generation: GenerationGate::new(),
            device_single_flight: SingleFlightGate::new(),
            backup_single_flight: SingleFlightGate::new(),
            inspect_single_flight: SingleFlightGate::new(),
            verify_single_flight: SingleFlightGate::new(),
            pending_device_scan: None,
            pending_backup_scan: None,
            pending_inspect: None,
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

    pub fn request_inspect_disk(&mut self, disk: u32) -> u64 {
        self.request_inspect(InspectRequest::Disk(disk))
    }

    pub fn request_inspect_backup(&mut self, path: PathBuf) -> u64 {
        self.request_inspect(InspectRequest::Backup(path))
    }

    fn request_inspect(&mut self, request: InspectRequest) -> u64 {
        let generation = self.inspect_generation.begin();
        if !self.inspect_single_flight.try_start() {
            self.pending_inspect = Some((generation, request));
            return generation;
        }
        self.start_inspect(generation, request);
        generation
    }

    fn start_inspect(&mut self, generation: u64, request: InspectRequest) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| match request {
                InspectRequest::Disk(disk) => {
                    let runner = SysRunner;
                    crate::application::inspect::load_disk_inspect(&runner, disk)
                }
                InspectRequest::Backup(path) => {
                    crate::application::inspect::load_backup_inspect(&path)
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "Inspect worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::Inspect { generation, result });
        });
    }

    pub fn request_backup_verify(&mut self, path: PathBuf, backup_dir: PathBuf) -> u64 {
        let generation = self.verify_generation.begin();
        if !self.verify_single_flight.try_start() {
            self.pending_verify = Some((generation, path, backup_dir));
            return generation;
        }
        self.start_backup_verify(generation, path, backup_dir);
        generation
    }

    fn start_backup_verify(&mut self, generation: u64, path: PathBuf, backup_dir: PathBuf) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::verify_backup_exact(&backup_dir, &path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份校验 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupVerify {
                generation,
                path,
                result,
            });
        });
    }

    pub fn request_backup_delete(
        &mut self,
        path: PathBuf,
        expected_sha256: String,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::delete_backup_exact(&backup_dir, &path, &expected_sha256)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份删除 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupDelete {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_backup_create(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let disk = intent.disk;
                struct BackupPrompter {
                    tx: Sender<WorkerResult>,
                    operation_id: OperationId,
                }

                impl crate::application::write::Prompter for BackupPrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }

                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }

                    fn output(&mut self, msg: &str) {
                        let _ = self.tx.send(WorkerResult::WriteProgress {
                            operation_id: self.operation_id,
                            message: progress_summary(msg),
                        });
                    }
                }

                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                let expected = intent.expected_identity.as_ref();
                crate::application::write::verify_expected_identity(
                    &runner,
                    disk,
                    expected.and_then(|value| value.onlyid.as_deref()),
                    expected.and_then(|value| value.device_id.as_deref()),
                    &mut dev,
                )
                .map_err(|error| error.msg)?;
                let mut prompt = BackupPrompter {
                    tx: tx.clone(),
                    operation_id,
                };
                let mut ctx = crate::application::write::Ctx {
                    runner: &runner,
                    clock: &crate::diskio::SystemClock,
                    prompt: &mut prompt,
                    backup_dir,
                };
                crate::application::write::backup_create_flow(disk, &mut ctx, &mut dev)
                    .map(|_| ())
                    .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("备份 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::Write {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_write(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct ConfirmedPrompter {
                    tx: Sender<WorkerResult>,
                    operation_id: OperationId,
                }
                impl crate::application::write::Prompter for ConfirmedPrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }

                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }

                    fn output(&mut self, msg: &str) {
                        let _ = self.tx.send(WorkerResult::WriteProgress {
                            operation_id: self.operation_id,
                            message: progress_summary(msg),
                        });
                    }
                }

                let result = (|| -> Result<(), String> {
                    let runner = SysRunner;
                    crate::application::write::guard_usb_disk(&runner, intent.disk)
                        .map_err(|error| error.msg)?;
                    let path = crate::diskio::raw_path(intent.disk);
                    let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                        .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                    let expected = intent.expected_identity.as_ref();
                    crate::application::write::verify_expected_identity(
                        &runner,
                        intent.disk,
                        expected.and_then(|value| value.onlyid.as_deref()),
                        expected.and_then(|value| value.device_id.as_deref()),
                        &mut dev,
                    )
                    .map_err(|error| error.msg)?;
                    let mut prompt = ConfirmedPrompter {
                        tx: tx.clone(),
                        operation_id,
                    };
                    let mut ctx = crate::application::write::Ctx {
                        runner: &runner,
                        clock: &crate::diskio::SystemClock,
                        prompt: &mut prompt,
                        backup_dir,
                    };
                    match intent.kind {
                        crate::tui::state::WriteKind::Apply => {
                            crate::application::write::apply_flow(
                                crate::application::write::ApplyMode::Write { force: false },
                                intent.disk,
                                None,
                                &mut ctx,
                                &mut dev,
                            )
                            .map(|_| ())
                            .map_err(|error| error.msg)
                        }
                        crate::tui::state::WriteKind::Restore => {
                            let backup = intent
                                .backup
                                .as_ref()
                                .ok_or_else(|| "错误: restore 缺少固定备份路径".to_string())?;
                            crate::application::write::restore_flow(
                                Some(backup.to_string_lossy().into_owned()),
                                intent.disk,
                                &mut ctx,
                                &mut dev,
                            )
                            .map(|_| ())
                            .map_err(|error| error.msg)
                        }
                        crate::tui::state::WriteKind::BackupCreate => {
                            Err("错误: backup create 必须走只读备份 worker".to_string())
                        }
                    }
                })();
                result
            }))
            .unwrap_or_else(|payload| {
                Err(format!("写盘 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::Write {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
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
                    message,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.write_progress = Some((operation_id, message));
                    }
                }
                WorkerResult::Inspect { generation, result } => {
                    self.inspect_single_flight.finish();
                    if let Some((next_generation, request)) = self.pending_inspect.take() {
                        let started = self.inspect_single_flight.try_start();
                        debug_assert!(started);
                        self.start_inspect(next_generation, request);
                    } else if self.inspect_generation.is_current(generation) {
                        updates.inspect = Some(result);
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
    fn inspect_and_verify_keep_only_the_latest_queued_target() {
        let mut hub = TaskHub::new();
        assert!(hub.inspect_single_flight.try_start());
        hub.request_inspect_disk(6);
        hub.request_inspect_disk(7);
        assert!(matches!(
            hub.pending_inspect,
            Some((_, InspectRequest::Disk(7)))
        ));

        assert!(hub.verify_single_flight.try_start());
        hub.request_backup_verify(PathBuf::from("one.bin"), PathBuf::from("backups"));
        hub.request_backup_verify(PathBuf::from("two.bin"), PathBuf::from("backups"));
        assert!(matches!(
            hub.pending_verify,
            Some((_, ref path, _)) if path == &PathBuf::from("two.bin")
        ));
    }
}
