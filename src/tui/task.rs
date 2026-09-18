//! Background work coordinator for TUI read-side operations.
//!
//! Blocking device discovery is always executed on a worker thread. Generations make refreshes
//! race-safe: a slow old scan can never overwrite a newer request.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::application::{inspect::InspectWorkspace, BackupWorkspaceItem};
use crate::disk_scan::Row;
use crate::sysinfo::SysRunner;

#[derive(Debug, Default)]
pub struct GenerationGate {
    current: u64,
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
    Inspect {
        generation: u64,
        result: Result<InspectWorkspace, String>,
    },
}

#[derive(Default)]
pub struct TaskUpdates {
    pub devices: Option<Vec<Row>>,
    pub backups: Option<Vec<BackupWorkspaceItem>>,
    pub write: Option<Result<(), String>>,
    pub inspect: Option<Result<InspectWorkspace, String>>,
}

pub struct TaskHub {
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    device_generation: GenerationGate,
    backup_generation: GenerationGate,
    inspect_generation: GenerationGate,
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
        }
    }

    pub fn request_device_scan(&mut self, backup_dir: PathBuf) -> u64 {
        let generation = self.device_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let runner = SysRunner;
            let rows = crate::application::scan_device_dashboard(&runner, &backup_dir);
            let _ = tx.send(WorkerResult::Devices { generation, rows });
        });
        generation
    }

    pub fn request_backup_scan(&mut self, backup_dir: PathBuf) -> u64 {
        let generation = self.backup_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let rows = crate::application::scan_backup_workspace(&backup_dir);
            let _ = tx.send(WorkerResult::Backups { generation, rows });
        });
        generation
    }

    pub fn request_inspect_disk(&mut self, disk: u32) -> u64 {
        let generation = self.inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let runner = SysRunner;
            let result = crate::application::inspect::load_disk_inspect(&runner, disk);
            let _ = tx.send(WorkerResult::Inspect { generation, result });
        });
        generation
    }

    pub fn request_inspect_backup(&mut self, path: PathBuf) -> u64 {
        let generation = self.inspect_generation.begin();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = crate::application::inspect::load_backup_inspect(&path);
            let _ = tx.send(WorkerResult::Inspect { generation, result });
        });
        generation
    }

    pub fn request_write(
        &mut self,
        intent: crate::tui::state::WriteIntent,
        backup_dir: PathBuf,
    ) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            struct ConfirmedPrompter;
            impl crate::application::write::Prompter for ConfirmedPrompter {
                fn prompt_line(&mut self, _msg: &str) -> String {
                    String::new()
                }

                fn confirm_yes(&mut self, _msg: &str) -> bool {
                    true
                }
            }

            let result = (|| -> Result<(), String> {
                let runner = SysRunner;
                crate::application::write::guard_usb_disk(&runner, intent.disk)
                    .map_err(|error| error.msg)?;
                let path = crate::diskio::raw_path(intent.disk);
                let mut dev = crate::diskio::FileDev::open_rdonly(&path)
                    .map_err(|error| format!("错误: 无法只读打开 {path}: {error}"))?;
                let mut prompt = ConfirmedPrompter;
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
                }
            })();
            let _ = tx.send(WorkerResult::Write { result });
        });
    }

    /// Drain all ready messages exactly once so one result kind cannot consume another.
    pub fn poll(&mut self) -> TaskUpdates {
        let mut updates = TaskUpdates::default();
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Devices { generation, rows }
                    if self.device_generation.is_current(generation) =>
                {
                    updates.devices = Some(rows);
                }
                WorkerResult::Backups { generation, rows }
                    if self.backup_generation.is_current(generation) =>
                {
                    updates.backups = Some(rows);
                }
                WorkerResult::Write { result } => {
                    updates.write = Some(result);
                }
                WorkerResult::Inspect { generation, result }
                    if self.inspect_generation.is_current(generation) =>
                {
                    updates.inspect = Some(result);
                }
                WorkerResult::Inspect { .. }
                | WorkerResult::Devices { .. }
                | WorkerResult::Backups { .. } => {}
            }
        }
        updates
    }
}
