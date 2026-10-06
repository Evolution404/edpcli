use super::catalog::{lba4_label_id_from, scan_backup_dir_checked, BackupEntry, DiskFacts};
use crate::common::{EdpCliError, EdpCliResult, EXIT_BACKUP, EXIT_IO, SECTOR};
use crate::ports::Clock;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

fn io_err(e: io::Error) -> EdpCliError {
    EdpCliError::new(EXIT_IO, format!("错误: {}", e))
}

fn validate_backup_device_id(device_id: &str) -> EdpCliResult<()> {
    let safe = device_id.starts_with("disk&ven_")
        && device_id.len() <= 128
        && device_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'&' | b'.' | b'-'));
    if safe {
        Ok(())
    } else {
        Err(EdpCliError::new(
            EXIT_BACKUP,
            format!(
                "错误: device_id 含不安全的备份文件名字符或长度异常，拒绝创建备份: {:?}",
                device_id
            ),
        ))
    }
}

// ══════════════════════════════════════════════════════════════════
// 0. 扇区设备
// ══════════════════════════════════════════════════════════════════
fn prepare_backup_capture<'a>(
    facts: &DiskFacts,
    data: &'a [u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, crate::edpb::CoreCapture<'a>)> {
    prepare_backup_capture_with_state(facts, data, device_id, bak_dir, clock, None)
}

fn prepare_backup_capture_with_state<'a>(
    facts: &DiskFacts,
    data: &'a [u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
    device_state: Option<&str>,
) -> EdpCliResult<(PathBuf, crate::edpb::CoreCapture<'a>)> {
    validate_backup_device_id(device_id)?;
    if data.len() != crate::common::METADATA_IMAGE_LEN {
        return Err(EdpCliError::new(
            EXIT_BACKUP,
            format!(
                "错误: 备份镜像长度 {}B，必须恰好为 {}B（LBA0-12）",
                data.len(),
                crate::common::METADATA_IMAGE_LEN
            ),
        ));
    }
    crate::infrastructure::atomic_file::private_directory(bak_dir).map_err(io_err)?;
    let epoch = clock.now_epoch();
    let ts = clock.fmt_ts(epoch);
    let secs = facts
        .total_sectors
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());
    let onlyid = lba4_label_id_from(&data[4 * SECTOR..5 * SECTOR]);
    let onlyid_part = onlyid
        .as_ref()
        .map(|value| format!("_onlyid{}", value))
        .unwrap_or_default();
    let file_identity = if device_state == Some("plain") {
        "plain"
    } else {
        device_id
    };
    let base = format!(
        "disk{}_{}_vid{}_pid{}_{}{}_{}",
        facts.disk, secs, facts.vid, facts.pid, file_identity, onlyid_part, ts
    );
    let path = bak_dir.join(format!("{}.edpb", base));
    let capture = crate::edpb::CoreCapture {
        snapshot_id: format!("{}-{}", onlyid.as_deref().unwrap_or(device_id), ts),
        created_epoch: epoch,
        disk_number: Some(facts.disk),
        vid: facts.vid.clone(),
        pid: facts.pid.clone(),
        device_id: device_id.to_string(),
        onlyid,
        total_sectors: facts.total_sectors,
        logical_sector_size: SECTOR as u32,
        edpcli_version: env!("CARGO_PKG_VERSION").to_string(),
        device_state: device_state
            .map(str::to_string)
            .unwrap_or_else(|| "edp".into()),
        lba0_12: data,
    };
    Ok((path, capture))
}

pub fn create_plain_backup(
    facts: &DiskFacts,
    data: &[u8],
    legacy_candidate: &str,
    metadata: crate::backup_metadata::MetadataAcquisition,
    identity: &crate::media_identity::MediaIdentitySnapshot,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<PathBuf> {
    let (path, capture) = prepare_backup_capture_with_state(
        facts,
        data,
        legacy_candidate,
        bak_dir,
        clock,
        Some("plain"),
    )?;
    let capture = crate::edpb::MetadataCapture {
        core: capture,
        partitions: metadata.partitions,
        regions: metadata.regions,
        extents: metadata.extents,
        artifacts: metadata.artifacts,
        notes: metadata.notes,
    };
    crate::edpb::write_metadata_backup_with_identity(&path, &capture, identity)
        .map_err(|error| EdpCliError::new(EXIT_BACKUP, format!("错误: {error}")))?;
    Ok(path)
}

/// 创建默认的 Metadata 级 EDPB 备份。
/// metadata 必须在源盘仍以只读方式打开时采集完成；本函数只写备份文件。
pub fn create_metadata_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    metadata: crate::backup_metadata::MetadataAcquisition,
    identity: &crate::media_identity::MediaIdentitySnapshot,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<PathBuf> {
    let (path, core) = prepare_backup_capture(facts, data, device_id, bak_dir, clock)?;
    let capture = crate::edpb::MetadataCapture {
        core,
        partitions: metadata.partitions,
        regions: metadata.regions,
        extents: metadata.extents,
        artifacts: metadata.artifacts,
        notes: metadata.notes,
    };
    crate::edpb::write_metadata_backup_with_identity(&path, &capture, identity)
        .map_err(|error| EdpCliError::new(EXIT_BACKUP, format!("错误: {error}")))?;
    Ok(path)
}

pub fn mtime_epoch(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupMatches {
    pub confirmed: Vec<PathBuf>,
    pub latest_created_epoch: Option<i64>,
    pub possible: Vec<PathBuf>,
}

/// Read-side backup affinity derived from verified EDPB canonical identity.
///
/// A/B/C relationships are confirmed. D-level same-model evidence is shown only as possible and
/// must never be silently promoted to ownership or destructive authorization.
pub fn find_backups(
    bak_dir: &Path,
    current: &crate::media_identity::MediaIdentitySnapshot,
) -> Result<BackupMatches, String> {
    Ok(match_backup_entries(
        &scan_backup_dir_checked(bak_dir)?,
        current,
    ))
}

/// Read-side matching against one refresh snapshot; never authorizes a write.
pub(crate) fn match_backup_entries(
    entries: &[BackupEntry],
    current: &crate::media_identity::MediaIdentitySnapshot,
) -> BackupMatches {
    use crate::media_identity::{match_media_identity, BackupAffinity, BackupAffinityPolicy};

    let mut matches = BackupMatches::default();
    for entry in entries {
        let Some(identity) = entry.meta.as_ref().and_then(|meta| meta.identity.as_ref()) else {
            continue;
        };
        let identity_match = match_media_identity(current, identity, None);
        match BackupAffinityPolicy::classify(&identity_match) {
            BackupAffinity::Confirmed => {
                if matches.confirmed.is_empty() {
                    matches.latest_created_epoch = entry.created_epoch;
                }
                matches.confirmed.push(entry.path.clone());
            }
            BackupAffinity::Possible => matches.possible.push(entry.path.clone()),
            BackupAffinity::Unrelated => {}
        }
    }
    matches
}
