use super::*;

/// 备份命名所需盘事实(由 sysinfo/LBA4 预先收集, 测试可注入)。
pub struct DiskFacts {
    pub disk: u32,
    pub total_sectors: Option<u64>,
    pub vid: String,
    pub pid: String,
    pub label_id: Option<String>,
}

/// LBA4 开头的 `$$$<labelOnlyId>$$$` → 十进制字符串; 非法返回 None。
///
/// labelOnlyId 在部分盘上以有符号 32 位十进制文本保存；负的 10 位数连同
/// 分隔符需要 17B，因此不能只截取 16B。
pub fn lba4_label_id_from(head: &[u8]) -> Option<String> {
    if head.len() < 3 || &head[..3] != b"$$$" {
        return None;
    }
    let mut i = 3usize;
    let dstart = i; // 返回值含可选负号
    if head.get(i) == Some(&b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < head.len() && head[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None; // '-' 后至少一位数字
    }
    if i + 3 > head.len() || &head[i..i + 3] != b"$$$" {
        return None;
    }
    Some(String::from_utf8_lossy(&head[dstart..i]).into_owned())
}

/// LBA4 前 16B 身份标签。短扇区安全返回 None，调用方不得假设读取层一定给满 512B。
pub fn lba4_tag16_from(raw: &[u8]) -> Option<[u8; 16]> {
    raw.get(..16)?.try_into().ok()
}

/// 已在内存中的 LBA0-12 镜像是否为免密状态。
/// 供扫描、restore、备份创建共用，避免上层重复构造扇区闭包或二次读文件。
pub fn image_is_nopwd(data: &[u8], device_id: &str) -> bool {
    if data.len() < crate::common::METADATA_IMAGE_LEN {
        return false;
    }
    let read = |lba: u32| -> EdpCliResult<Vec<u8>> {
        Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    looks_nopwd(&read, device_id).unwrap_or(false)
}

pub fn ts_suffix_pos(name: &str) -> Option<usize> {
    // 新备份只认 .edpb；旧 .bin 不进入正式运行时解析路径。
    if !name.ends_with(".edpb") {
        return None;
    }
    let stem = &name[..name.len() - 5];
    let b = stem.as_bytes();
    if b.len() < 16 {
        return None;
    }
    let p = &b[b.len() - 16..];
    if p[0] == b'_'
        && p[1..9].iter().all(|c| c.is_ascii_digit())
        && p[9] == b'_'
        && p[10..16].iter().all(|c| c.is_ascii_digit())
    {
        Some(b.len() - 16)
    } else {
        None
    }
}

/// 备份文件名尾部 `_YYYYMMDD_HHMMSS.edpb` 转为可直接比较的 YYYYMMDDHHMMSS 数值。
/// 这是备份真实创建时间；文件系统 mtime 可能因复制/touch 改变，只作为旧文件兜底。
pub fn backup_name_time_key(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let pos = ts_suffix_pos(name)?;
    let end = name.len().checked_sub(5)?;
    let stamp = name.get(pos + 1..end)?;
    let mut digits = String::with_capacity(14);
    for ch in stamp.bytes() {
        if ch == b'_' {
            continue;
        }
        if !ch.is_ascii_digit() {
            return None;
        }
        digits.push(ch as char);
    }
    if digits.len() != 14 {
        return None;
    }
    digits.parse().ok()
}

pub fn backup_name_time_human(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let pos = ts_suffix_pos(name)?;
    let end = name.len().checked_sub(5)?;
    let stamp = name.get(pos + 1..end)?;
    if stamp.len() != 15 {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{}",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13]
    ))
}

/// `sort_by` 可直接使用的“最新备份优先”比较器。
/// 两边都有文件名时间时完全忽略 mtime；无法解析旧命名时才退回 mtime。
pub fn cmp_backup_newest_first(a: &BackupEntry, b: &BackupEntry) -> Ordering {
    match (backup_name_time_key(&a.path), backup_name_time_key(&b.path)) {
        (Some(at), Some(bt)) => bt.cmp(&at).then_with(|| a.path.cmp(&b.path)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)),
    }
}

pub fn backup_display_time(path: &Path, mtime: i64) -> String {
    backup_name_time_human(path).unwrap_or_else(|| SystemClock.fmt_human(mtime))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupMeta {
    pub disk: u32,
    pub secs: Option<u64>,
    pub vid: String,
    pub pid: String,
    pub device_id: String,
    pub onlyid: Option<String>,
    pub tagged_nopwd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupIntegrityStatus {
    Verified,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub meta: Option<BackupMeta>,
    pub path: PathBuf,
    pub mtime: i64,
    pub is_nopwd: bool,
    pub provision_kind: crate::provision::DiskProvisionKind,
    pub integrity_status: BackupIntegrityStatus,
    pub size_ok: bool,
    /// 扫描时缓存的 LBA8 原始 512B；用于列表/元信息展示，避免随后再次打开同一备份。
    pub lba8: Option<[u8; SECTOR]>,
    /// 扫描时实际 `.bin` 内容摘要；删除前用于确认同名文件未被替换/改写。
    pub content_sha256: Option<String>,
}

fn strip_numeric_suffix<'a>(s: &'a str, marker: &str) -> Option<(&'a str, String)> {
    let pos = s.rfind(marker)?;
    let value = &s[pos + marker.len()..];
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let start = usize::from(bytes.first() == Some(&b'-'));
    if start == bytes.len() || !bytes[start..].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((&s[..pos], value.to_string()))
}

/// 解析本工具备份文件名。device_id 自身含 `&` / `_`，因此不能按 `_` 粗暴 split；
/// 固定锚点只使用 disk/secs/vid/pid 与尾部时间戳/状态/onlyid。
pub fn parse_backup_name(name: &str) -> Option<BackupMeta> {
    let ts_pos = ts_suffix_pos(name)?;
    let stem_before_ts = &name[..ts_pos];
    let after_disk = stem_before_ts.strip_prefix("disk")?;
    let disk_end = after_disk.find('_')?;
    let disk_s = &after_disk[..disk_end];
    if disk_s.is_empty() || !disk_s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let disk = disk_s.parse::<u32>().ok()?;
    let rest = &after_disk[disk_end + 1..];

    let vid_pos = rest.find("_vid")?;
    let secs_s = &rest[..vid_pos];
    let secs = if secs_s == "unknown" {
        None
    } else if !secs_s.is_empty() && secs_s.bytes().all(|b| b.is_ascii_digit()) {
        Some(secs_s.parse::<u64>().ok()?)
    } else {
        return None;
    };

    let after_vid = &rest[vid_pos + 4..];
    let pid_pos = after_vid.find("_pid")?;
    let vid = &after_vid[..pid_pos];
    if vid.is_empty() || !vid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let after_pid = &after_vid[pid_pos + 4..];
    let device_pos = after_pid.find('_')?;
    let pid = &after_pid[..device_pos];
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    let mut tail = &after_pid[device_pos + 1..];
    let tagged_nopwd = tail.ends_with("_nopwd");
    if tagged_nopwd {
        tail = &tail[..tail.len() - "_nopwd".len()];
    }
    let (device_id, onlyid) = match strip_numeric_suffix(tail, "_onlyid") {
        Some((did, id)) => (did, Some(id)),
        None => match strip_numeric_suffix(tail, "_lid") {
            Some((did, id)) => (did, Some(id)),
            None => (tail, None),
        },
    };
    if !device_id.starts_with("disk&ven_") {
        return None;
    }

    Some(BackupMeta {
        disk,
        secs,
        vid: vid.to_string(),
        pid: pid.to_string(),
        device_id: device_id.to_string(),
        onlyid,
        tagged_nopwd,
    })
}

/// 扫描备份目录并给出跨盘管理所需的完整元数据。
///
/// 扫描必须是只读操作：文件名仅用于解析设备信息；只要备份内容可读且包含完整
/// LBA4，onlyid 始终以 LBA4 为权威（即使文件名声称了另一个 onlyid）。旧 `_lid`
/// 与无 onlyid 文件因此都无需改名即可正确归组；正式运行时仅扫描 `.edpb`。
pub fn scan_backup_dir(dir: &Path) -> Vec<BackupEntry> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        if let Some(entry) = scan_backup_file(&item.path()) {
            entries.push(entry);
        }
    }
    entries.sort_by(cmp_backup_newest_first);
    entries
}

/// Load and validate one exact backup without hashing every sibling in its directory.
pub fn scan_backup_file(path: &Path) -> Option<BackupEntry> {
    let file_type = fs::symlink_metadata(path).ok()?.file_type();
    if !file_type.is_file() || path.extension().and_then(|e| e.to_str()) != Some("edpb") {
        return None;
    }
    let file_data = fs::read(path).ok();
    let content_sha256 = file_data.as_ref().map(|data| sha256_hex(data));
    let verified = crate::edpb::verify_file(path).ok();
    let raw = verified
        .as_ref()
        .and_then(|_| crate::edpb::read_raw_protocol(path).ok());
    let meta = verified.as_ref().map(|container| {
        let manifest = &container.manifest;
        BackupMeta {
            disk: manifest.observation.disk_number.unwrap_or(0),
            secs: manifest.geometry.total_sectors,
            vid: manifest.device.vid.clone(),
            pid: manifest.device.pid.clone(),
            device_id: manifest.device.device_id.clone(),
            onlyid: manifest.device.onlyid.clone(),
            tagged_nopwd: manifest.snapshot.device_state == "passwordless",
        }
    });
    let lba8 = raw.as_ref().and_then(|data| {
        data.get(8 * SECTOR..9 * SECTOR)
            .and_then(|bytes| bytes.try_into().ok())
    });
    let size_ok = raw
        .as_ref()
        .map(|data| data.len() == crate::common::METADATA_IMAGE_LEN)
        .unwrap_or(false);
    let integrity_status = if verified.is_some() && size_ok {
        BackupIntegrityStatus::Verified
    } else {
        BackupIntegrityStatus::Invalid
    };
    let is_nopwd = match (&meta, &raw) {
        (Some(meta), Some(data)) => image_is_nopwd(data, &meta.device_id),
        _ => false,
    };
    let provision_kind = match (&meta, &raw) {
        (Some(meta), Some(data)) => {
            crate::provision::DiskProvisionKind::from_metadata(data, &meta.device_id)
        }
        _ => crate::provision::DiskProvisionKind::Plain,
    };
    Some(BackupEntry {
        meta,
        path: path.to_path_buf(),
        mtime: mtime_epoch(path),
        is_nopwd,
        provision_kind,
        integrity_status,
        size_ok,
        lba8,
        content_sha256,
    })
}

/// Shell completion 专用的轻量备份索引。
///
/// 这里只判断普通 `.edpb` 文件并按文件名时间排序；绝不读取备份内容、
/// 计算内容摘要或解析 LBA。完整健康状态仍由 `scan_backup_dir` 负责。
pub fn scan_backup_names(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = read_dir
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("edpb") {
                return None;
            }
            Some(path)
        })
        .collect();
    paths.sort_by(
        |a, b| match (backup_name_time_key(a), backup_name_time_key(b)) {
            (Some(at), Some(bt)) => bt.cmp(&at).then_with(|| a.cmp(b)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.cmp(b),
        },
    );
    paths
}

/// 同一物理盘的备份分组键。现代命名优先使用 onlyid；历史/缺失 onlyid 时退化为
/// (device_id, total sectors)。未识别文件不参与自动清理策略。
pub fn backup_group_key(entry: &BackupEntry) -> Option<String> {
    let meta = entry.meta.as_ref()?;
    Some(match &meta.onlyid {
        Some(id) => format!("onlyid:{id}"),
        None => format!(
            "legacy:{}:{}",
            meta.device_id,
            meta.secs
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
    })
}

/// `backup prune` 的纯策略层：
/// - 加密原盘备份从不成为候选；
/// - 每盘只对已按内容确认的免密快照按备份文件名时间新→旧保留 `keep` 份；
///   仅旧命名无法解析时间时才回退文件系统 mtime；
/// - 若该盘组没有任何加密原盘备份，则至少保留最新 1 份快照，防止清到零份。
pub fn prune_candidates(entries: &[BackupEntry], keep: usize) -> Vec<PathBuf> {
    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    for entry in entries {
        if let Some(key) = backup_group_key(entry) {
            groups.entry(key).or_default().push(entry);
        }
    }

    let mut out = Vec::new();
    for group in groups.values() {
        let has_original = group.iter().any(|e| !e.is_nopwd);
        let mut snaps: Vec<&BackupEntry> = group.iter().copied().filter(|e| e.is_nopwd).collect();
        snaps.sort_by(|a, b| cmp_backup_newest_first(a, b));
        let preserve = if has_original { keep } else { keep.max(1) };
        let mut deletable: Vec<&BackupEntry> = snaps.into_iter().skip(preserve).collect();
        // 候选清单按最旧→较新展示/删除，便于人工核对；保留判定仍严格按最新优先。
        deletable.reverse();
        out.extend(deletable.into_iter().map(|e| e.path.clone()));
    }
    out
}
