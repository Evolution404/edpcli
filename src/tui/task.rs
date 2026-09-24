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
        event: crate::application::WriteEvent,
    },
    ApplyPreview {
        generation: u64,
        result: Result<Vec<crate::application::WriteEvent>, String>,
    },
    ApplyProgress {
        operation_id: OperationId,
        event: crate::application::WriteEvent,
    },
    ApplyWrite {
        operation_id: OperationId,
        result: Result<(), String>,
    },
    Inspect {
        generation: u64,
        result: Result<InspectWorkspace, String>,
    },
    AdvancedInspect {
        generation: u64,
        result: Result<crate::application::inspect::AdvancedInspectWorkspace, String>,
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
    OfflineConvert {
        generation: u64,
        result: Result<crate::tui::state::OfflineConvertView, String>,
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
    pub write_progress: Option<(OperationId, crate::application::WriteEvent)>,
    pub apply_preview: Option<Result<Vec<crate::application::WriteEvent>, String>>,
    pub apply_progress: Option<(OperationId, crate::application::WriteEvent)>,
    pub apply_write: Option<(OperationId, Result<(), String>)>,
    pub inspect: Option<Result<InspectWorkspace, String>>,
    pub advanced_inspect:
        Option<Result<crate::application::inspect::AdvancedInspectWorkspace, String>>,
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
    pub offline_convert: Option<Result<crate::tui::state::OfflineConvertView, String>>,
}

impl TaskUpdates {
    pub fn has_updates(&self) -> bool {
        self.devices.is_some()
            || self.backups.is_some()
            || self.write.is_some()
            || self.write_progress.is_some()
            || self.apply_preview.is_some()
            || self.apply_progress.is_some()
            || self.apply_write.is_some()
            || self.inspect.is_some()
            || self.advanced_inspect.is_some()
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
            || self.offline_convert.is_some()
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
    inspect_generation: GenerationGate,
    advanced_inspect_generation: GenerationGate,
    verify_generation: GenerationGate,
    provision_generation: GenerationGate,
    provision_export_generation: GenerationGate,
    offline_convert_generation: GenerationGate,
    apply_generation: GenerationGate,
    prune_generation: GenerationGate,
    batch_delete_generation: GenerationGate,
    device_single_flight: SingleFlightGate,
    backup_single_flight: SingleFlightGate,
    inspect_single_flight: SingleFlightGate,
    advanced_inspect_single_flight: SingleFlightGate,
    verify_single_flight: SingleFlightGate,
    provision_single_flight: SingleFlightGate,
    provision_export_single_flight: SingleFlightGate,
    offline_convert_single_flight: SingleFlightGate,
    apply_single_flight: SingleFlightGate,
    prune_single_flight: SingleFlightGate,
    batch_delete_single_flight: SingleFlightGate,
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
            advanced_inspect_generation: GenerationGate::new(),
            verify_generation: GenerationGate::new(),
            provision_generation: GenerationGate::new(),
            provision_export_generation: GenerationGate::new(),
            offline_convert_generation: GenerationGate::new(),
            apply_generation: GenerationGate::new(),
            prune_generation: GenerationGate::new(),
            batch_delete_generation: GenerationGate::new(),
            device_single_flight: SingleFlightGate::new(),
            backup_single_flight: SingleFlightGate::new(),
            inspect_single_flight: SingleFlightGate::new(),
            advanced_inspect_single_flight: SingleFlightGate::new(),
            verify_single_flight: SingleFlightGate::new(),
            provision_single_flight: SingleFlightGate::new(),
            provision_export_single_flight: SingleFlightGate::new(),
            offline_convert_single_flight: SingleFlightGate::new(),
            apply_single_flight: SingleFlightGate::new(),
            prune_single_flight: SingleFlightGate::new(),
            batch_delete_single_flight: SingleFlightGate::new(),
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

    pub fn request_advanced_inspect(
        &mut self,
        source: crate::tui::state::AdvancedInspectSource,
        request: crate::application::inspect::AdvancedInspectRequest,
    ) -> Result<u64, &'static str> {
        if !self.advanced_inspect_single_flight.try_start() {
            return Err("已有高级检查正在执行");
        }
        let generation = self.advanced_inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| match source {
                crate::tui::state::AdvancedInspectSource::Disk(disk) => {
                    let runner = SysRunner;
                    crate::application::inspect::load_disk_advanced_inspect(&runner, disk, &request)
                }
                crate::tui::state::AdvancedInspectSource::Backup(path) => {
                    crate::application::inspect::load_backup_advanced_inspect(&path, &request)
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "高级检查 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::AdvancedInspect { generation, result });
        });
        Ok(generation)
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

    pub fn request_backup_batch_delete_plan(
        &mut self,
        targets: Vec<(PathBuf, String)>,
        backup_dir: PathBuf,
    ) -> Result<u64, &'static str> {
        if !self.batch_delete_single_flight.try_start() {
            return Err("已有批量删除计划正在生成");
        }
        let generation = self.batch_delete_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                session
                    .plan_exact_many(&targets)
                    .map_err(|error| error.message())
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "批量删除计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupBatchDeletePlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_backup_batch_delete_execute(
        &mut self,
        plan: crate::application::backup::DeletePlan,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let expected = plan.targets.len();
                let results = session.execute(&plan);
                let failures = results
                    .iter()
                    .filter_map(|(path, result)| {
                        result
                            .as_ref()
                            .err()
                            .map(|message| format!("{}: {message}", path.display()))
                    })
                    .collect::<Vec<_>>();
                if failures.is_empty() {
                    Ok(expected)
                } else {
                    Err(format!(
                        "批量删除未完全成功：{} / {} 项失败；未通过复核的文件未删除。{}",
                        failures.len(),
                        expected,
                        failures.join("；")
                    ))
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "批量删除 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupBatchDeleteExecute {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_backup_prune_plan(
        &mut self,
        backup_dir: PathBuf,
        keep: usize,
    ) -> Result<u64, &'static str> {
        if !self.prune_single_flight.try_start() {
            return Err("已有备份清理计划正在生成");
        }
        let generation = self.prune_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let plan = session.plan_prune(keep).map_err(|error| error.message())?;
                let (originals, retained_snapshots) = plan
                    .prune_stats
                    .as_ref()
                    .map(|stats| (stats.originals, stats.retained_snapshots))
                    .unwrap_or((0, 0));
                Ok(crate::tui::state::BackupPrunePrepared {
                    plan,
                    keep,
                    originals,
                    retained_snapshots,
                })
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份清理计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupPrunePlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_backup_prune_execute(
        &mut self,
        prepared: crate::tui::state::BackupPrunePrepared,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let session = crate::application::backup::DeleteSession::open(&backup_dir);
                let expected = prepared.plan.targets.len();
                let results = session.execute(&prepared.plan);
                let failures = results
                    .iter()
                    .filter_map(|(path, result)| {
                        result
                            .as_ref()
                            .err()
                            .map(|message| format!("{}: {message}", path.display()))
                    })
                    .collect::<Vec<_>>();
                if failures.is_empty() {
                    Ok(expected)
                } else {
                    Err(format!(
                        "清理未完全成功：{} / {} 项失败；未通过摘要复核的文件未删除。{}",
                        failures.len(),
                        expected,
                        failures.join("；")
                    ))
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "备份清理 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::BackupPruneExecute {
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

                    fn write_event(&mut self, event: crate::application::WriteEvent) {
                        let _ = self.tx.send(WorkerResult::WriteProgress {
                            operation_id: self.operation_id,
                            event,
                        });
                    }

                    // 写流程只允许经 write_event 上报；兜底丢弃，防止文本直接写进备用屏。
                    fn output(&mut self, _msg: &str) {}
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
                let deep = intent.kind == crate::tui::state::WriteKind::BackupCreateDeep;
                crate::application::write::backup_create_level_flow(disk, &mut ctx, &mut dev, deep)
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

    pub fn request_provision_backup(
        &mut self,
        disk: u32,
        expected_identity: crate::tui::state::ExpectedIdentity,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                crate::application::write::verify_expected_identity(
                    &runner,
                    disk,
                    expected_identity.onlyid.as_deref(),
                    expected_identity.device_id.as_deref(),
                    &mut dev,
                )
                .map_err(|error| error.msg)?;

                struct ProvisionBackupPrompter;
                impl crate::application::write::Prompter for ProvisionBackupPrompter {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }
                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }
                    fn output(&mut self, _msg: &str) {}
                }

                let mut prompt = ProvisionBackupPrompter;
                let mut ctx = crate::application::write::Ctx {
                    runner: &runner,
                    clock: &crate::diskio::SystemClock,
                    prompt: &mut prompt,
                    backup_dir,
                };
                crate::application::write::backup_create_level_flow(disk, &mut ctx, &mut dev, false)
                    .map(|_| ())
                    .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "制盘前保存 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ProvisionBackup {
                operation_id,
                result,
            });
        }));
        Ok(operation_id)
    }

    pub fn request_apply_preview(
        &mut self,
        disk: u32,
        expected_identity: crate::tui::state::ExpectedIdentity,
        size_gb: Option<f64>,
        backup_dir: PathBuf,
    ) -> Result<u64, &'static str> {
        if !self.apply_single_flight.try_start() {
            return Err("已有 Apply 预览正在生成");
        }
        let generation = self.apply_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct Collector {
                    events: Vec<crate::application::WriteEvent>,
                }
                impl crate::application::write::Prompter for Collector {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }
                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        false
                    }
                    fn write_event(&mut self, event: crate::application::WriteEvent) {
                        self.events.push(event);
                    }
                    fn output(&mut self, _msg: &str) {}
                }

                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                crate::application::write::verify_expected_identity(
                    &runner,
                    disk,
                    expected_identity.onlyid.as_deref(),
                    expected_identity.device_id.as_deref(),
                    &mut dev,
                )
                .map_err(|error| error.msg)?;
                let mut prompt = Collector { events: Vec::new() };
                let mut ctx = crate::application::write::Ctx {
                    runner: &runner,
                    clock: &crate::diskio::SystemClock,
                    prompt: &mut prompt,
                    backup_dir,
                };
                crate::application::write::apply_flow(
                    crate::application::write::ApplyMode::DryRun,
                    disk,
                    size_gb,
                    &mut ctx,
                    &mut dev,
                )
                .map_err(|error| error.msg)?;
                Ok(prompt.events)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "Apply 预览 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ApplyPreview { generation, result });
        });
        Ok(generation)
    }

    pub fn request_apply_write(
        &mut self,
        disk: u32,
        expected_identity: crate::tui::state::ExpectedIdentity,
        size_gb: Option<f64>,
        force: bool,
        backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct Progress {
                    tx: Sender<WorkerResult>,
                    operation_id: OperationId,
                }
                impl crate::application::write::Prompter for Progress {
                    fn prompt_line(&mut self, _msg: &str) -> String {
                        String::new()
                    }
                    fn confirm_yes(&mut self, _msg: &str) -> bool {
                        true
                    }
                    fn write_event(&mut self, event: crate::application::WriteEvent) {
                        let _ = self.tx.send(WorkerResult::ApplyProgress {
                            operation_id: self.operation_id,
                            event,
                        });
                    }
                    fn output(&mut self, _msg: &str) {}
                }

                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                crate::application::write::verify_expected_identity(
                    &runner,
                    disk,
                    expected_identity.onlyid.as_deref(),
                    expected_identity.device_id.as_deref(),
                    &mut dev,
                )
                .map_err(|error| error.msg)?;
                let mut prompt = Progress {
                    tx: tx.clone(),
                    operation_id,
                };
                let mut ctx = crate::application::write::Ctx {
                    runner: &runner,
                    clock: &crate::diskio::SystemClock,
                    prompt: &mut prompt,
                    backup_dir,
                };
                crate::application::write::apply_flow(
                    crate::application::write::ApplyMode::Write { force },
                    disk,
                    size_gb,
                    &mut ctx,
                    &mut dev,
                )
                .map(|_| ())
                .map_err(|error| error.msg)
            }))
            .unwrap_or_else(|payload| {
                Err(format!("Apply worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::ApplyWrite {
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

                    fn write_event(&mut self, event: crate::application::WriteEvent) {
                        let _ = self.tx.send(WorkerResult::WriteProgress {
                            operation_id: self.operation_id,
                            event,
                        });
                    }

                    // 写流程只允许经 write_event 上报；兜底丢弃，防止文本直接写进备用屏。
                    fn output(&mut self, _msg: &str) {}
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
                        crate::tui::state::WriteKind::BackupCreateDeep => {
                            Err("错误: deep backup 必须走只读备份 worker".to_string())
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

    pub fn request_provision_plan(
        &mut self,
        disk: u32,
        kind: crate::tui::state::ProvisionKind,
        request: Option<crate::application::provision::NewProvisionRequest>,
    ) -> Result<u64, &'static str> {
        if !self.provision_single_flight.try_start() {
            return Err("已有制盘计划正在生成");
        }
        let generation = self.provision_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                let path = crate::diskio::raw_path(disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                let _ = kind;
                let request = request.ok_or_else(|| "错误: 新盘制盘缺少表单参数".to_string())?;
                let mut prepared =
                    crate::application::provision::prepare_new_provision(&runner, disk, &request)
                        .map_err(|error| error.msg)?;
                crate::application::provision::capture_manufacturer_lba3(&mut dev, &mut prepared)
                    .map_err(|error| error.msg)?;
                Ok(crate::tui::state::ProvisionPrepared::New(Box::new(
                    prepared,
                )))
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "制盘计划 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ProvisionPlan { generation, result });
        });
        Ok(generation)
    }

    pub fn request_provision_export(
        &mut self,
        prepared: crate::application::provision::PreparedNewProvision,
        path: PathBuf,
    ) -> Result<u64, &'static str> {
        if !self.provision_export_single_flight.try_start() {
            return Err("已有制盘镜像正在导出");
        }
        let generation = self.provision_export_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::provision::export_sparse_provision_image(&path, &prepared)
                    .map_err(|error| error.msg)?;
                Ok(path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "制盘镜像导出 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::ProvisionExport { generation, result });
        });
        Ok(generation)
    }

    pub fn request_offline_convert(
        &mut self,
        request: crate::application::offline_convert::OfflineConvertRequest,
    ) -> Result<u64, &'static str> {
        if !self.offline_convert_single_flight.try_start() {
            return Err("已有离线转换正在执行");
        }
        let generation = self.offline_convert_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let output = crate::application::offline_convert::run(&request)
                    .map_err(|error| error.msg)?;
                Ok(crate::tui::state::OfflineConvertView {
                    reports: output.reports,
                    share: output.result.share,
                    enc_start: output.result.enc_start,
                    enc_size: output.result.enc_size,
                    crc: output.result.crc,
                    k0: output.result.k0,
                    output_dir: output.output_dir,
                })
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "离线转换 worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::OfflineConvert { generation, result });
        });
        Ok(generation)
    }

    pub fn request_provision_write(
        &mut self,
        prepared: crate::tui::state::ProvisionPrepared,
        _backup_dir: PathBuf,
    ) -> Result<OperationId, &'static str> {
        let operation_id = self.begin_operation()?;
        let tx = self.tx.clone();
        self.critical_worker = Some(std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                match prepared {
                    crate::tui::state::ProvisionPrepared::New(prepared) => {
                        let _ = tx.send(WorkerResult::ProvisionProgress {
                            operation_id,
                            message: "正在写入 LCE/协议元数据，并逐扇区读回校验…".into(),
                        });
                        let path = crate::diskio::raw_path(prepared.disk);
                        let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                            .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                        crate::application::provision::commit_new_provision(
                            &runner, &mut dev, &prepared,
                        )
                        .map(|report| {
                            let mut lines =
                                vec!["制盘：成功，协议与几何读回验证通过。".to_string()];
                            if report.formats.is_empty() {
                                lines.push("格式化：未选择任何分区".into());
                            }
                            for item in report.formats {
                                lines.push(match item.result {
                                    Ok(()) => {
                                        format!("格式化：✓ {}，读回验证通过", item.role.label())
                                    }
                                    Err(message) => {
                                        format!("格式化：✗ {}：{message}", item.role.label())
                                    }
                                });
                            }
                            lines.join("\n")
                        })
                        .map_err(|error| error.msg)
                    }
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!("制盘 worker 异常终止: {}", panic_message(payload)))
            });
            let _ = tx.send(WorkerResult::ProvisionWrite {
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
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.write_progress = Some((operation_id, event));
                    }
                }
                WorkerResult::ApplyPreview { generation, result } => {
                    self.apply_single_flight.finish();
                    if self.apply_generation.is_current(generation) {
                        updates.apply_preview = Some(result);
                    }
                }
                WorkerResult::ApplyProgress {
                    operation_id,
                    event,
                } => {
                    if self.active_operation == Some(operation_id) {
                        updates.apply_progress = Some((operation_id, event));
                    }
                }
                WorkerResult::ApplyWrite {
                    operation_id,
                    result,
                } => {
                    if self.finish_operation(operation_id) {
                        updates.apply_write = Some((operation_id, result));
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
                WorkerResult::AdvancedInspect { generation, result } => {
                    self.advanced_inspect_single_flight.finish();
                    if self.advanced_inspect_generation.is_current(generation) {
                        updates.advanced_inspect = Some(result);
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
                WorkerResult::OfflineConvert { generation, result } => {
                    self.offline_convert_single_flight.finish();
                    if self.offline_convert_generation.is_current(generation) {
                        updates.offline_convert = Some(result);
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
