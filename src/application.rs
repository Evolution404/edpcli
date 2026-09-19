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
    pub content_md5: Option<String>,
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
                content_md5: entry.content_md5.clone(),
            }
        })
        .collect()
}

/// Convert a resolved disk number into the platform-native selector before crossing a
/// privilege/re-exec boundary. Frontends must not encode platform device paths themselves.
pub fn pin_disk_selector(disk: u32) -> String {
    crate::platform::disk_selector_value(disk)
}

/// Parse a selector previously pinned by `pin_disk_selector`.
pub fn parse_pinned_disk_selector(value: &str) -> Result<u32, String> {
    crate::platform::parse_disk_selector(value)
        .map_err(|error| format!("错误: resume disk {value}: {error}"))
}


fn scanned_backup_by_path<'a>(
    selector: &'a crate::selectors::BackupSelector,
    path: &Path,
) -> Result<&'a crate::diskio::BackupEntry, String> {
    let target = crate::backup_catalog::canonical_entry_path(path);
    selector
        .catalog()
        .entries()
        .iter()
        .find(|entry| crate::backup_catalog::canonical_entry_path(&entry.path) == target)
        .ok_or_else(|| format!("备份已不存在或不再属于当前备份目录: {}", path.display()))
}

/// Verify exactly one backup selected by the TUI against the canonical backup catalog.
pub fn verify_backup_exact(root: &Path, path: &Path) -> Result<(), String> {
    let selector = load_backup_selector(root);
    let entry = scanned_backup_by_path(&selector, path)?;
    if crate::backup_catalog::is_healthy(entry) {
        Ok(())
    } else {
        Err(format!(
            "备份校验失败: {}（大小或 MD5 异常）",
            entry.path.display()
        ))
    }
}

/// Delete one exact backup selected from a prior TUI scan.
///
/// The expected MD5 pins the exact bytes that the user selected before confirmation. If the file is
/// replaced or changed while the confirmation dialog is open, deletion fails closed.
pub fn delete_backup_exact(
    root: &Path,
    path: &Path,
    expected_md5: &str,
) -> Result<(), String> {
    let selector = load_backup_selector(root);
    let entry = scanned_backup_by_path(&selector, path)?;
    if entry.content_md5.as_deref() != Some(expected_md5) {
        return Err(format!(
            "备份在选择/确认期间已变化，拒绝删除: {}",
            path.display()
        ));
    }

    if let Some(group) = diskio::backup_group_key(entry) {
        let remaining_in_group = selector
            .catalog()
            .entries()
            .iter()
            .filter(|candidate| diskio::backup_group_key(candidate).as_deref() == Some(group.as_str()))
            .count();
        if remaining_in_group <= 1 {
            return Err("安全保护拒绝删除——该盘将被清到零份备份；至少保留 1 份。".into());
        }
    }

    crate::backup_catalog::delete_entry_verified(entry)
}
