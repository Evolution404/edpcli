//! Background work coordinator for TUI read-side operations.
//!
//! Blocking device discovery is always executed on a worker thread. Generations make refreshes
//! race-safe: a slow old scan can never overwrite a newer request.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

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
}

pub struct TaskHub {
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    device_generation: GenerationGate,
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

    /// Drain ready worker messages without waiting. Only the current generation is accepted.
    pub fn poll_devices(&mut self) -> Option<Vec<Row>> {
        let mut latest = None;
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Devices { generation, rows }
                    if self.device_generation.is_current(generation) =>
                {
                    latest = Some(rows);
                }
                WorkerResult::Devices { .. } => {}
            }
        }
        latest
    }
}
