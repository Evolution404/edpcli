//! Application/service boundary shared by the CLI and interactive frontends.
//!
//! This layer owns task-oriented, UI-neutral operations. Frontends may schedule these
//! operations however they need, but must not reimplement device discovery or raw-disk
//! safety policy.


pub mod inspect;
pub mod write;
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


/// Stable backup row shared by CLI/TUI read-side views.
#[derive(Debug, Clone)]
pub struct BackupWorkspaceItem {
    pub index: usize,
    pub path: std::path::PathBuf,
    pub file_name: String,
    pub display_time: String,
    pub onlyid: Option<String>,
    pub user: Option<String>,
    pub dept: Option<String>,
    pub is_nopwd: bool,
    pub md5_status: crate::diskio::Md5Status,
    pub size_ok: bool,
}

/// Load the canonical selector used by every backup frontend.
pub fn load_backup_selector(root: &Path) -> crate::selectors::BackupSelector {
    crate::selectors::BackupSelector::load(root)
}

/// Build the backup-workspace rows from the exact same global numbering used by CLI restore,
/// verify and delete. No frontend invents its own index.
pub fn scan_backup_workspace(root: &Path) -> Vec<BackupWorkspaceItem> {
    let selector = load_backup_selector(root);
    selector
        .numbered_with_indices()
        .into_iter()
        .map(|(index, entry)| {
            let ownership = crate::metainfo::backup_ownership(entry);
            BackupWorkspaceItem {
                index,
                path: entry.path.clone(),
                file_name: entry
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<无效文件名>")
                    .to_string(),
                display_time: crate::diskio::backup_display_time(&entry.path, entry.mtime),
                onlyid: entry.meta.as_ref().and_then(|meta| meta.onlyid.clone()),
                user: ownership.as_ref().and_then(|value| value.user.clone()),
                dept: ownership.as_ref().and_then(|value| value.dept.clone()),
                is_nopwd: entry.is_nopwd,
                md5_status: entry.md5_ok,
                size_ok: entry.size_ok,
            }
        })
        .collect()
}
