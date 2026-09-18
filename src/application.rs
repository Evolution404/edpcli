//! Application/service boundary shared by the CLI and interactive frontends.
//!
//! This layer owns task-oriented, UI-neutral operations. Frontends may schedule these
//! operations however they need, but must not reimplement device discovery or raw-disk
//! safety policy.

use std::cell::RefCell;
use std::io;
use std::path::Path;

use crate::disk_scan::{scan_disks, Row};
use crate::diskio::{self, raw_path, FileDev};
use crate::sysinfo::{CmdRunner, ReadProbeCache};

/// Build the device-dashboard model using the same read-only probing path for every frontend.
///
/// Raw devices are opened read-only and pooled for the duration of one scan; `disk_scan`
/// additionally caches individual LBAs. No write preparation or write-capable reopen is reachable
/// from this service.
pub fn scan_device_dashboard(runner: &dyn CmdRunner, backup_dir: &Path) -> Vec<Row> {
    let devices = RefCell::new(diskio::ReadOnlyDiskPool::new(|disk| {
        FileDev::open_rdonly(&raw_path(disk))
    }));
    let read_disk = |disk: u32, lba: u32| -> io::Result<Vec<u8>> {
        devices.borrow_mut().read_sector(disk, lba)
    };
    let probe = ReadProbeCache::new(runner);
    scan_disks(&probe, backup_dir, &read_disk)
}

/// Decide whether a completed read-only device scan needs elevation for identity metadata.
///
/// Kept as pure policy so CLI and TUI can present different UI while sharing the decision.
pub fn device_scan_needs_elevation(
    rows: &[Row],
    elevated: bool,
    has_elevation_sentinel: bool,
) -> bool {
    rows.iter().any(|row| row.denied) && !elevated && !has_elevation_sentinel
}
