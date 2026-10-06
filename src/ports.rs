//! Platform-neutral capability definitions; stable facades re-export these contracts.
use crate::domain::hardware::{HardwareProbe, ObservedDeviceGeometry};
use std::io;
use std::time::Duration;

pub trait CmdRunner {
    fn check_output(&self, cmd: &[&str], timeout: Duration) -> io::Result<String>;
    fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
        None
    }
    fn hardware_serial(&self, _disk: u32) -> Option<String> {
        None
    }
    fn device_geometry(&self, _disk: u32) -> Option<ObservedDeviceGeometry> {
        None
    }
}

pub trait SectorDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>>;
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()>;
    /// Complete durable synchronization, or return an error.
    fn sync(&mut self) -> io::Result<()>;
    /// Reopen with write access, or return an error.
    fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()>;
}

pub trait Clock {
    fn now_epoch(&self) -> i64;
    fn fmt_ts(&self, epoch: i64) -> String;
    fn fmt_human(&self, epoch: i64) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandCompletion {
    Exited { success: bool, code: Option<i32> },
    TimedOut,
    OutputLimit,
}
#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub completion: CommandCompletion,
    pub elapsed: Duration,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// Shared read-only scan cancellation and cumulative I/O/deadline budget.
/// Never used to interrupt a critical write transaction.
#[derive(Clone, Debug)]
pub struct ReadControl {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
    max_bytes: u64,
    deadline: std::time::Instant,
}
impl ReadControl {
    pub fn new(max_bytes: u64, timeout: Duration) -> Self {
        Self {
            cancelled: Default::default(),
            bytes: Default::default(),
            max_bytes,
            deadline: std::time::Instant::now() + timeout,
        }
    }
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn bytes_read(&self) -> u64 {
        self.bytes.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn check(&self) -> io::Result<()> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(io::Error::other("目录扫描已取消"));
        }
        if std::time::Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "目录扫描超过耗时预算",
            ));
        }
        if self.bytes_read() > self.max_bytes {
            return Err(io::Error::other("目录扫描超过累计读取字节预算"));
        }
        Ok(())
    }
    pub(crate) fn remaining(&self) -> u64 {
        self.max_bytes.saturating_sub(self.bytes_read())
    }
    pub(crate) fn record_read(&self, bytes: usize) -> io::Result<()> {
        self.bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
        self.check()
    }
}
