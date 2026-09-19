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
        result: Result<(), String>,
    },
    WriteProgress {
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
}

#[derive(Default)]
pub struct TaskUpdates {
    pub devices: Option<Vec<Row>>,
    pub backups: Option<Vec<BackupWorkspaceItem>>,
    pub write: Option<Result<(), String>>,
    pub write_progress: Option<String>,
    pub inspect: Option<Result<InspectWorkspace, String>>,
    pub device_error: Option<String>,
    pub backup_error: Option<String>,
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
    device_single_flight: SingleFlightGate,
    backup_single_flight: SingleFlightGate,
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
            device_single_flight: SingleFlightGate::new(),
            backup_single_flight: SingleFlightGate::new(),
        }
    }

    pub fn request_device_scan(&mut self, backup_dir: PathBuf) -> u64 {
        if !self.device_single_flight.try_start() {
            return self.device_generation.current();
        }
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
            return self.backup_generation.current();
        }
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
        let generation = self.inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let runner = SysRunner;
                crate::application::inspect::load_disk_inspect(&runner, disk)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "Inspect worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::Inspect { generation, result });
        });
        generation
    }

    pub fn request_inspect_backup(&mut self, path: PathBuf) -> u64 {
        let generation = self.inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                crate::application::inspect::load_backup_inspect(&path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "Inspect worker 异常终止: {}",
                    panic_message(payload)
                ))
            });
            let _ = tx.send(WorkerResult::Inspect { generation, result });
        });
        generation
    }

    pub fn request_backup_create(&mut self, disk: u32, backup_dir: PathBuf) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct BackupPrompter {
                    tx: Sender<WorkerResult>,
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
                let mut prompt = BackupPrompter { tx: tx.clone() };
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
            let _ = tx.send(WorkerResult::Write { result });
        });
    }

    pub fn request_write(&mut self, intent: crate::tui::state::WriteIntent, backup_dir: PathBuf) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                struct ConfirmedPrompter {
                    tx: Sender<WorkerResult>,
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
                    let mut prompt = ConfirmedPrompter { tx: tx.clone() };
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
            let _ = tx.send(WorkerResult::Write { result });
        });
    }

    /// Drain all ready messages exactly once so one result kind cannot consume another.
    pub fn poll(&mut self) -> TaskUpdates {
        let mut updates = TaskUpdates::default();
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Devices { generation, rows } => {
                    self.device_single_flight.finish();
                    if self.device_generation.is_current(generation) {
                        updates.devices = Some(rows);
                    }
                }
                WorkerResult::Backups { generation, rows } => {
                    self.backup_single_flight.finish();
                    if self.backup_generation.is_current(generation) {
                        updates.backups = Some(rows);
                    }
                }
                WorkerResult::Write { result } => {
                    updates.write = Some(result);
                }
                WorkerResult::WriteProgress { message } => {
                    updates.write_progress = Some(message);
                }
                WorkerResult::Inspect { generation, result }
                    if self.inspect_generation.is_current(generation) =>
                {
                    updates.inspect = Some(result);
                }
                WorkerResult::DeviceError {
                    generation,
                    message,
                } => {
                    self.device_single_flight.finish();
                    if self.device_generation.is_current(generation) {
                        updates.device_error = Some(message);
                    }
                }
                WorkerResult::BackupError {
                    generation,
                    message,
                } => {
                    self.backup_single_flight.finish();
                    if self.backup_generation.is_current(generation) {
                        updates.backup_error = Some(message);
                    }
                }
                WorkerResult::Inspect { .. } => {}
            }
        }
        updates
    }
}
