//! Application/service boundary shared by the CLI and interactive frontends.
//!
//! This layer owns task-oriented, UI-neutral operations. Frontends may schedule these
//! operations however they need, but must not reimplement device discovery or raw-disk
//! safety policy.

pub mod backup;
pub mod backup_coverage;
pub mod backup_restore_preview;
pub(crate) mod catalog_snapshot;
pub mod device;
pub mod disk_layout;
pub mod error;
pub mod evidence;
pub(crate) mod filesystem_format;
pub mod identity;
pub mod inspect;
pub mod inspect_summary;
pub(crate) mod inspect_text;
pub mod inspect_tree;
pub mod media_identity;
pub mod media_identity_observer;
pub mod partition_table;
pub mod post_restore;
pub mod progress;
pub mod provision;
pub mod provision_geometry;
pub mod target_session;
pub mod write;

/// 稳定外部接口下的共享容量/退出语义兼容门面；实现仍由 crate 内部 `common` 持有。
pub mod support {
    pub use crate::common::{
        fmt_capacity, fmt_capacity_sectors, fmt_capacity_with_system, group_digits,
        CapacityUnitSystem, EdpCliError, EdpCliResult, CAPACITY_UNIT_SYSTEM, EXIT_BACKUP,
        EXIT_CANCELLED, EXIT_INTERMEDIATE, EXIT_IO, EXIT_OK, EXIT_ROLLED_BACK, EXIT_TARGET,
        EXIT_USAGE, METADATA_IMAGE_LEN, METADATA_LAST_LBA, METADATA_SECTOR_COUNT, SECTOR,
    };
}

/// 稳定 application 门面下的文件系统纯模型与解析/格式化能力。
pub mod filesystem {
    pub use crate::filesystem::{
        analysis, build_empty_exfat, build_empty_fat16, build_empty_fat32, build_empty_filesystem,
        build_empty_filesystem_typed, default_registry, detect_boot_sector,
        detect_boot_sector_with_geometry, estimate_format_resources, is_writable_filesystem,
        registry, shift_writable_filesystem, validate_volume_label, validate_volume_label_typed,
        validate_writable_filesystem, BootSectorReader, DetectedFilesystem, DetectionConfidence,
        DetectionResult, DriverRegistry, ExFatDriver, Fat12Driver, Fat16Driver, Fat32Driver,
        FilesystemCapabilities, FilesystemDriver, FilesystemError, FilesystemErrorKind,
        FilesystemGeometry, FilesystemKind, FilesystemMetadata, FilesystemReader, FilesystemWrite,
        FormatPlan, FormatRequest, FormatResourceBudget, FormatResourceEstimate,
        FormatVerification, NtfsDriver, SparseFilesystemImage, EXFAT_DRIVER, FAT12_DRIVER,
        FAT16_DRIVER, FAT32_DRIVER, NTFS_DRIVER, WRITABLE_FILESYSTEMS,
    };
}

/// 稳定 application 门面下的元信息汇总能力。
pub mod metadata {
    pub use crate::metainfo::{
        backup_ownership, ownership_from_lba8, render, render_with_source, safe6_label_from_lba6,
        summarize, MetaInfoSummary, OwnershipInfo, PartitionInfo,
    };
}
pub use crate::infrastructure::backup_store::catalog::{BackupHealth, BackupIntegrityStatus};
/// Partition values carried by device-dashboard presentation rows.
pub use crate::protocol::sectors::EdpfPartition;
pub use crate::selectors::{BackupSelector, DeviceSelector};
pub use backup::delete_backup_exact;
use std::cell::RefCell;
use std::io;
use std::path::Path;
pub use write::WriteEvent;

use crate::disk_scan::Row;
use crate::diskio::{self, raw_path, FileDev};
use crate::platform::system::ReadProbeCache;
use crate::ports::CmdRunner;

/// Frontend interaction boundary shared by CLI selectors and write services.
pub trait Prompter {
    fn prompt_line(&mut self, msg: &str) -> String;
    /// Read through a secret-aware channel; never delegate to ordinary line input.
    /// Noninteractive implementations must return an empty secret to cancel input.
    ///
    /// A frontend must explicitly implement secret handling:
    /// ```compile_fail,E0046
    /// use edpcli::application::Prompter;
    /// struct TextOnly;
    /// impl Prompter for TextOnly {
    ///     fn prompt_line(&mut self, _: &str) -> String { String::new() }
    ///     fn confirm_yes(&mut self, _: &str) -> bool { false }
    /// }
    /// ```
    fn prompt_secret(&mut self, msg: &str) -> crate::provision::SecretBytes;
    fn confirm_yes(&mut self, msg: &str) -> bool;
    fn confirm_write_yes(&mut self, msg: &str) -> bool {
        self.confirm_yes(msg)
    }
    fn confirm_post_restore_format_yes(&mut self, msg: &str) -> bool {
        self.confirm_write_yes(msg)
    }
    fn confirm_reinitialize_yes(&mut self, msg: &str) -> bool {
        self.confirm_write_yes(msg)
    }

    /// UI-neutral typed progress event. Frontends decide how to render or store it.
    fn write_event(&mut self, _event: WriteEvent) {}

    /// Optional progress sink; operations isolate sink panics from device writes.
    fn operation_progress(&mut self, _event: progress::ProgressEvent) {}
}

/// Build the device-dashboard model using the same read-only probing path for every frontend.
///
/// Raw devices are opened read-only and pooled for the duration of one scan; `disk_scan`
/// additionally caches individual LBAs. No write preparation or write-capable reopen is reachable
/// from this service.
pub fn scan_device_dashboard(runner: &dyn CmdRunner, backup_dir: &Path) -> Vec<Row> {
    scan_dashboard_catalog(runner, || {
        crate::infrastructure::backup_store::catalog::scan_backup_dir_checked(backup_dir)
    })
}

/// Construct the runtime capability used by application workers.
pub(crate) fn system_runner() -> impl crate::ports::CmdRunner {
    crate::platform::system::SysRunner
}

pub(crate) fn scan_dashboard_snapshot(
    runner: &dyn CmdRunner,
    snapshot: &catalog_snapshot::CatalogSnapshot,
) -> Vec<Row> {
    scan_dashboard_catalog(runner, || {
        let catalog = snapshot.catalog();
        if let Some(error) = catalog.scan_error() {
            return Err(error.to_owned());
        }
        Ok(catalog.entries().to_vec())
    })
}

fn scan_dashboard_catalog(
    runner: &dyn CmdRunner,
    load: impl FnOnce()
        -> Result<Vec<crate::infrastructure::backup_store::catalog::BackupEntry>, String>,
) -> Vec<Row> {
    let devices = RefCell::new(diskio::ReadOnlyDiskPool::new(|disk| {
        FileDev::open_rdonly(&raw_path(disk))
    }));
    let read_disk = |disk: u32, lba: u32| -> io::Result<Vec<u8>> {
        devices.borrow_mut().read_sector(disk, lba)
    };
    let probe = ReadProbeCache::new(runner);
    crate::disk_scan::scan_disks_with_catalog(&probe, &read_disk, load)
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
    pub display_cached: bool,
    pub index: usize,
    pub path: std::path::PathBuf,
    pub file_name: String,
    pub display_time: String,
    pub size_bytes: Option<u64>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub identity: Option<crate::application::media_identity::MediaIdentitySnapshot>,
    pub user: Option<String>,
    pub dept: Option<String>,
    pub provision_kind: Option<crate::provision::DiskProvisionKind>,
    pub integrity_status: BackupIntegrityStatus,
    pub size_ok: bool,
    pub verification_error: Option<String>,
    pub content_sha256: Option<String>,
    pub coverage: Option<backup_coverage::BackupCoverage>,
    pub restore_preview: Option<backup_restore_preview::BackupRestorePreview>,
}

/// Load the canonical selector used by every backup frontend.
pub fn load_backup_selector(root: &Path) -> crate::selectors::BackupSelector {
    crate::selectors::BackupSelector::load(root)
}

pub fn resolve_backup_dir(flag: Option<&str>) -> std::path::PathBuf {
    crate::infrastructure::backup_store::config::resolve_backup_dir(flag)
}

pub fn has_configured_backup_dir() -> bool {
    crate::infrastructure::backup_store::config::conf_backup_dir().is_some()
}

pub fn backup_dir_argv_suffix(env_val: Option<String>) -> Vec<String> {
    crate::infrastructure::backup_store::config::backup_dir_argv_suffix(env_val)
}

impl BackupWorkspaceItem {
    pub fn health(&self) -> BackupHealth {
        self.integrity_status
            .health(self.size_ok, self.verification_error.is_some())
    }
    pub fn is_restorable(&self) -> bool {
        self.health().is_healthy()
    }
}

/// Build globally numbered workspace rows, reporting scan failures without partial results.
pub fn scan_backup_workspace_checked(root: &Path) -> Result<Vec<BackupWorkspaceItem>, String> {
    let selector = load_backup_selector(root);
    if let Some(error) = selector.catalog().scan_error() {
        return Err(error.to_string());
    }
    Ok(workspace_from_selector(&selector))
}

pub(crate) fn workspace_from_snapshot(
    snapshot: &catalog_snapshot::CatalogSnapshot,
) -> Result<Vec<BackupWorkspaceItem>, String> {
    let catalog = snapshot.catalog();
    if let Some(error) = catalog.scan_error() {
        return Err(error.to_owned());
    }
    Ok(workspace_from_selector(
        &crate::selectors::BackupSelector::from_catalog(catalog.clone()),
    ))
}

fn workspace_from_selector(
    selector: &crate::selectors::BackupSelector,
) -> Vec<BackupWorkspaceItem> {
    selector
        .numbered_with_indices()
        .into_iter()
        .map(|(index, entry)| {
            let ownership = crate::metainfo::backup_ownership(entry);
            BackupWorkspaceItem {
                display_cached: entry.display_cached,
                index,
                path: entry.path.clone(),
                file_name: entry
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<无效文件名>")
                    .to_string(),
                display_time: crate::infrastructure::backup_store::catalog::backup_display_time(
                    entry,
                ),
                size_bytes: entry
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.secs)
                    .and_then(|sectors| sectors.checked_mul(crate::common::SECTOR as u64)),
                vid: entry.meta.as_ref().map(|meta| meta.vid.clone()),
                pid: entry.meta.as_ref().map(|meta| meta.pid.clone()),
                device_id: entry.meta.as_ref().map(|meta| meta.device_id.clone()),
                onlyid: entry.meta.as_ref().and_then(|meta| meta.onlyid.clone()),
                identity: entry.meta.as_ref().and_then(|meta| meta.identity.clone()),
                user: ownership.as_ref().and_then(|value| value.user.clone()),
                dept: ownership.as_ref().and_then(|value| value.dept.clone()),
                provision_kind: entry.provision_kind,
                integrity_status: entry.integrity_status,
                size_ok: entry.size_ok,
                verification_error: entry.verification_error.clone(),
                content_sha256: entry.content_sha256.clone(),
                coverage: entry.coverage.clone(),
                restore_preview: entry.restore_preview.clone().or_else(|| {
                    entry
                        .manifest
                        .as_ref()
                        .map(backup_restore_preview::BackupRestorePreview::from_manifest)
                }),
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
    let entry = crate::infrastructure::backup_store::catalog::scan_backup_file(&canonical_path)
        .ok_or_else(|| format!("目标不是可读取的 .edpb 备份: {}", canonical_path.display()))?;
    if crate::backup_catalog::is_healthy(&entry) {
        Ok(())
    } else {
        Err(format!(
            "EDPB 校验失败: {}: {}",
            canonical_path.display(),
            entry
                .verification_error
                .as_deref()
                .unwrap_or("容器结构、Manifest 或 Artifact 完整性异常")
        ))
    }
}
