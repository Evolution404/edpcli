//! Application/service boundary shared by the CLI and interactive frontends.
//!
//! This layer owns task-oriented, UI-neutral operations. Frontends may schedule these
//! operations however they need, but must not reimplement device discovery or raw-disk
//! safety policy.

pub mod backup;
pub mod device;
pub mod evidence;
pub mod inspect;
pub mod inspect_tree;
pub mod provision;
pub mod target_session;
pub mod write;
pub use crate::diskio::BackupIntegrityStatus;
pub use backup::delete_backup_exact;
use std::cell::RefCell;
use std::io;
use std::path::Path;
pub use write::WriteEvent;

use crate::disk_scan::{scan_disks, Row};
use crate::diskio::{self, raw_path, FileDev};
use crate::sysinfo::{CmdRunner, ReadProbeCache};

/// Frontend interaction boundary shared by CLI selectors and write services.
pub trait Prompter {
    fn prompt_line(&mut self, msg: &str) -> String;
    fn confirm_yes(&mut self, msg: &str) -> bool;
    fn confirm_write_yes(&mut self, msg: &str) -> bool {
        self.confirm_yes(msg)
    }

    /// UI-neutral typed progress event. Frontends decide how to render or store it.
    fn write_event(&mut self, _event: WriteEvent) {}
}

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
    pub provision_kind: crate::provision::DiskProvisionKind,
    pub integrity_status: BackupIntegrityStatus,
    pub size_ok: bool,
    pub content_sha256: Option<String>,
}

/// Load the canonical selector used by every backup frontend.
pub fn load_backup_selector(root: &Path) -> crate::selectors::BackupSelector {
    crate::selectors::BackupSelector::load(root)
}

pub fn resolve_backup_dir(flag: Option<&str>) -> std::path::PathBuf {
    crate::diskio::resolve_backup_dir(flag)
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
                provision_kind: entry.provision_kind,
                integrity_status: entry.integrity_status,
                size_ok: entry.size_ok,
                content_sha256: entry.content_sha256.clone(),
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

// 备份删除/保留策略的统一入口已迁至 application::backup(DeleteSession/plan/execute)；
// delete_backup_exact 保留为 TUI worker 的薄封装再导出。

/// Verify exactly one backup selected by the TUI against the canonical backup catalog.
pub fn verify_backup_exact(root: &Path, path: &Path) -> Result<(), String> {
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|error| format!("备份目录不可访问 {}: {error}", root.display()))?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|error| format!("备份文件不存在或不可访问 {}: {error}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "拒绝校验备份目录之外的路径: {}",
            canonical_path.display()
        ));
    }
    let entry = crate::diskio::scan_backup_file(&canonical_path)
        .ok_or_else(|| format!("目标不是可读取的 .edpb 备份: {}", canonical_path.display()))?;
    if crate::backup_catalog::is_healthy(&entry) {
        Ok(())
    } else {
        Err(format!(
            "EDPB 校验失败: {}（容器结构、Manifest 或 Artifact 完整性异常）",
            canonical_path.display()
        ))
    }
}
