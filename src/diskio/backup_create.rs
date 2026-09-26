use super::*;

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

fn sync_dir(dir: &Path) -> EdpCliResult<()> {
    crate::platform::sync_directory(dir).map_err(io_err)
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
) -> EdpCliResult<(PathBuf, bool, crate::edpb::CoreCapture<'a>)> {
    prepare_backup_capture_with_state(facts, data, device_id, bak_dir, clock, None)
}

fn prepare_backup_capture_with_state<'a>(
    facts: &DiskFacts,
    data: &'a [u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
    device_state: Option<&str>,
) -> EdpCliResult<(PathBuf, bool, crate::edpb::CoreCapture<'a>)> {
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
    fs::create_dir_all(bak_dir).map_err(io_err)?;
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
    let is_nopwd = device_state.is_none() && image_is_nopwd(data, device_id);
    let state_part = match device_state {
        Some("plain") => "_plain",
        _ if is_nopwd => "_nopwd",
        _ => "",
    };
    let base = format!(
        "disk{}_{}_vid{}_pid{}_{}{}{}_{}",
        facts.disk, secs, facts.vid, facts.pid, device_id, onlyid_part, state_part, ts
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
        device_state: device_state.map(str::to_string).unwrap_or_else(|| {
            if is_nopwd {
                "passwordless"
            } else {
                "encrypted"
            }
            .into()
        }),
        lba0_12: data,
    };
    Ok((path, is_nopwd, capture))
}

/// 创建自包含 EDPB Core 备份。
/// 仅供明确需要 Core 级快照的内部路径/测试使用；用户正常备份走 Metadata 级路径。
pub fn create_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, bool)> {
    let (path, is_nopwd, capture) = prepare_backup_capture(facts, data, device_id, bak_dir, clock)?;
    crate::edpb::write_core_backup(&path, &capture)
        .map_err(|error| EdpCliError::new(EXIT_BACKUP, format!("错误: {error}")))?;
    sync_dir(bak_dir)?;
    Ok((path, is_nopwd))
}

pub fn create_plain_backup(
    facts: &DiskFacts,
    data: &[u8],
    legacy_candidate: &str,
    identity: &crate::application::media_identity::MediaIdentitySnapshot,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, bool)> {
    let (path, is_nopwd, capture) = prepare_backup_capture_with_state(
        facts,
        data,
        legacy_candidate,
        bak_dir,
        clock,
        Some("plain"),
    )?;
    crate::edpb::write_core_backup_with_identity(&path, &capture, identity)
        .map_err(|error| EdpCliError::new(EXIT_BACKUP, format!("错误: {error}")))?;
    sync_dir(bak_dir)?;
    Ok((path, is_nopwd))
}

/// 创建默认的 Metadata 级 EDPB 备份。
/// metadata 必须在源盘仍以只读方式打开时采集完成；本函数只写备份文件。
pub fn create_metadata_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    metadata: crate::backup_metadata::MetadataAcquisition,
    identity: &crate::application::media_identity::MediaIdentitySnapshot,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, bool)> {
    let (path, is_nopwd, core) = prepare_backup_capture(facts, data, device_id, bak_dir, clock)?;
    let capture = crate::edpb::MetadataCapture {
        core,
        regions: metadata.regions,
        extents: metadata.extents,
        artifacts: metadata.artifacts,
        notes: metadata.notes,
    };
    crate::edpb::write_metadata_backup_with_identity(&path, &capture, identity)
        .map_err(|error| EdpCliError::new(EXIT_BACKUP, format!("错误: {error}")))?;
    sync_dir(bak_dir)?;
    Ok((path, is_nopwd))
}

/// Save an acquired Deep superset; this writes only the destination container.
pub fn create_deep_backup(
    facts: &DiskFacts,
    data: &[u8],
    device_id: &str,
    deep: crate::backup_metadata::MetadataAcquisition,
    identity: &crate::application::media_identity::MediaIdentitySnapshot,
    bak_dir: &Path,
    clock: &dyn Clock,
) -> EdpCliResult<(PathBuf, bool)> {
    let (path, is_nopwd, core) = prepare_backup_capture(facts, data, device_id, bak_dir, clock)?;
    let capture = crate::edpb::MetadataCapture {
        core,
        regions: deep.regions,
        extents: deep.extents,
        artifacts: deep.artifacts,
        notes: deep.notes,
    };
    crate::edpb::write_deep_backup_with_identity(&path, &capture, identity)
        .map_err(|e| EdpCliError::new(EXIT_BACKUP, format!("错误: {e}")))?;
    sync_dir(bak_dir)?;
    Ok((path, is_nopwd))
}

pub fn mtime_epoch(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 匹配备份目录中本盘备份, 新→旧排序。
/// 身份来自 EDPB manifest；非零 LBA4 16B 标签仍作为同盘终验。
pub fn find_backups(
    bak_dir: &Path,
    facts: &DiskFacts,
    device_id: Option<&str>,
    my_tag: Option<[u8; 16]>,
) -> Vec<PathBuf> {
    let tag = my_tag.filter(|value| value.iter().any(|&byte| byte != 0));
    scan_backup_dir(bak_dir)
        .into_iter()
        .filter(|entry| {
            let Some(meta) = entry.meta.as_ref() else {
                return false;
            };
            if meta.vid != facts.vid || meta.pid != facts.pid {
                return false;
            }
            if let Some(total) = facts.total_sectors {
                if meta.secs != Some(total) {
                    return false;
                }
            }
            if let Some(expected_device_id) = device_id {
                if meta.device_id != expected_device_id {
                    return false;
                }
            }
            if let Some(expected_tag) = tag {
                let Ok(raw) = crate::edpb::read_raw_protocol(&entry.path) else {
                    return false;
                };
                let Some(actual_tag) = raw.get(4 * SECTOR..5 * SECTOR).and_then(lba4_tag16_from)
                else {
                    return false;
                };
                if actual_tag != expected_tag {
                    return false;
                }
            }
            true
        })
        .map(|entry| entry.path)
        .collect()
}
