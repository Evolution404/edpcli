//! Platform-neutral capability definitions; stable facades re-export these contracts.
use crate::domain::hardware::{HardwareProbe, ObservedDeviceGeometry};
use std::io;
use std::time::Duration;

/// Command execution carries no device authorization or write lease.
pub trait CommandExecutor {
    fn run_command(&self, cmd: &[&str], timeout: Duration) -> io::Result<CommandOutcome>;
}

/// Fresh device facts, independent of command execution and raw write access.
pub trait DeviceObserver {
    fn is_system_disk(&self, disk: u32) -> bool;
    fn is_external_usb_whole(&self, disk: u32) -> bool;
    fn device_geometry(&self, disk: u32) -> Option<ObservedDeviceGeometry>;
    fn total_sectors(&self, disk: u32) -> Option<u64>;
    fn hardware_probe(&self, disk: u32) -> Option<HardwareProbe>;
    fn hardware_serial(&self, disk: u32) -> Option<String>;
}

/// Owned write exclusion; releasing it must release the platform guard.
/// Providers must reject overlapping leases for the same target.
pub trait WriteLease {}

pub trait WriteLeaseProvider {
    fn acquire_write_lease(&self, disk: u32) -> io::Result<Box<dyn WriteLease>>;
}

/// Transitional host probe adapter. New safety sessions use DeviceObserver and
/// WriteLeaseProvider separately; legacy read-side consumers still use this contract.
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
    /// Read one full sector into caller-owned storage. The default preserves
    /// existing custom device/fault-injection implementations; concrete devices
    /// may override it to avoid a heap allocation in the transaction hot path.
    fn read_sector_into(
        &mut self,
        lba: u32,
        out: &mut [u8; crate::common::SECTOR],
    ) -> io::Result<()> {
        let bytes = self.read_sector(lba)?;
        if bytes.len() != out.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("LBA{lba} 返回 {}B，预期 {}B", bytes.len(), out.len()),
            ));
        }
        out.copy_from_slice(&bytes);
        Ok(())
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()>;
    /// Optional contiguous I/O hint. Defaults to one sector to preserve all
    /// physical-device and fault-injection implementations unmodified.
    fn max_contiguous_sectors(&self) -> usize {
        1
    }
    /// Read adjacent sectors to a bounded reusable caller-owned buffer.
    fn read_contiguous_sectors_into(
        &mut self,
        first_lba: u32,
        sectors: &mut [[u8; crate::common::SECTOR]],
    ) -> io::Result<()> {
        for (index, sector) in sectors.iter_mut().enumerate() {
            let lba = first_lba
                .checked_add(index as u32)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "连续读取 LBA 溢出"))?;
            self.read_sector_into(lba, sector)?;
        }
        Ok(())
    }
    /// All-or-error adjacent write. A provider may partially mutate a failed
    /// batch; the transaction still rolls back its entire touched-sector set.
    fn write_contiguous_sectors(
        &mut self,
        first_lba: u32,
        sectors: &[[u8; crate::common::SECTOR]],
    ) -> io::Result<()> {
        for (index, sector) in sectors.iter().enumerate() {
            let lba = first_lba
                .checked_add(index as u32)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "连续写入 LBA 溢出"))?;
            self.write_sector(lba, sector)?;
        }
        Ok(())
    }
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
