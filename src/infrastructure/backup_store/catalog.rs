use super::create::mtime_epoch;
use crate::common::SECTOR;
use crate::infrastructure::clock::SystemClock;
use crate::ports::Clock;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub identity: Option<crate::media_identity::MediaIdentitySnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupIntegrityStatus {
    Verified,
    Invalid,
}

/// Shared verdict for catalog authorization and frontend presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupHealth {
    Verified,
    VerificationFailed,
    CoreDataInvalid,
    Invalid,
}
impl BackupHealth {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Verified => "EDPB ✓",
            Self::VerificationFailed => "校验失败",
            Self::CoreDataInvalid => "大小异常",
            Self::Invalid => "EDPB ✗",
        }
    }
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Verified)
    }
}
impl BackupIntegrityStatus {
    pub const fn health(self, size_ok: bool, verification_failed: bool) -> BackupHealth {
        if verification_failed {
            BackupHealth::VerificationFailed
        } else if !size_ok {
            BackupHealth::CoreDataInvalid
        } else if matches!(self, Self::Verified) {
            BackupHealth::Verified
        } else {
            BackupHealth::Invalid
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    /// Display-only cache provenance; never a write or delete grant.
    pub display_cached: bool,
    pub meta: Option<BackupMeta>,
    pub path: PathBuf,
    pub mtime: i64,
    pub provision_kind: Option<crate::provision::DiskProvisionKind>,
    pub integrity_status: BackupIntegrityStatus,
    pub size_ok: bool,
    /// 扫描时缓存的 LBA8 原始 512B；用于列表/元信息展示，避免随后再次打开同一备份。
    pub lba8: Option<[u8; SECTOR]>,
    /// 校验失败的原始原因，供前端呈现与授权判定。
    pub verification_error: Option<String>,
    /// 扫描时实际 `.edpb` 摘要；删除前复核同名内容。
    pub content_sha256: Option<String>,
    /// Typed region/extent/artifact coverage projected during the background scan.
    pub coverage: Option<crate::backup_coverage::BackupCoverage>,
    /// Verified manifest retained for higher-level read-side restore projection.
    pub manifest: Option<crate::edpb::Manifest>,
    /// Compact read-side presentation; directory scans do not retain full manifests.
    pub restore_preview: Option<crate::backup_restore_preview::BackupRestorePreview>,
}

impl BackupEntry {
    pub fn health(&self) -> BackupHealth {
        self.integrity_status
            .health(self.size_ok, self.verification_error.is_some())
    }
}

/// 扫描备份目录并给出跨盘管理所需的完整元数据。
///
/// 扫描必须是只读操作：身份以容器内容为权威，文件名仅用于时间展示和排序。
/// 历史文件无需改名即可正确归组；正式运行时仅扫描 `.edpb`。
const MAX_CATALOG_ENTRIES: usize = 4096;
const MAX_CATALOG_WEIGHT: usize = 64 * 1024 * 1024;

pub fn scan_backup_dir(dir: &Path) -> Vec<BackupEntry> {
    match scan_backup_dir_checked(dir) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("{error}");
            Vec::new()
        }
    }
}

/// Reject an incomplete catalog rather than renumbering a silently truncated one.
pub fn scan_backup_dir_checked(dir: &Path) -> Result<Vec<BackupEntry>, String> {
    scan_backup_dir_with_budget(dir, MAX_CATALOG_ENTRIES, MAX_CATALOG_WEIGHT)
}

fn scan_backup_dir_with_budget(
    dir: &Path,
    max_entries: usize,
    max_weight: usize,
) -> Result<Vec<BackupEntry>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let read_dir = fs::read_dir(dir)
        .map_err(|error| format!("备份目录不可读取 {}: {error}", dir.display()))?;
    let mut entries = Vec::new();
    let mut weight = 0usize;
    for item in read_dir {
        let item = item.map_err(|error| format!("备份目录读取失败: {error}"))?;
        if let Some((entry, retained_weight)) = scan_backup_file_impl(&item.path(), false, None) {
            if entries.len() >= max_entries {
                return Err(format!(
                    "备份目录扫描超过条目预算 {max_entries}，未返回不完整编号；请分目录管理备份"
                ));
            }
            weight = weight.checked_add(retained_weight).filter(|sum| *sum <= max_weight)
                .ok_or_else(|| format!("备份目录扫描超过累计展示数据预算 {max_weight}B，未返回不完整编号；请分目录管理备份"))?;
            entries.push(entry);
        }
    }
    entries.sort_by(cmp_backup_newest_first);
    Ok(entries)
}

/// Load and validate one exact backup without hashing every sibling in its directory.
pub fn scan_backup_file(path: &Path) -> Option<BackupEntry> {
    scan_backup_file_impl(path, true, None).map(|(entry, _)| entry)
}

pub(super) fn scan_backup_file_impl(
    path: &Path,
    retain_manifest: bool,
    control: Option<&crate::ports::ReadControl>,
) -> Option<(BackupEntry, usize)> {
    let file_type = fs::symlink_metadata(path).ok()?.file_type();
    if !file_type.is_file() || path.extension().and_then(|e| e.to_str()) != Some("edpb") {
        return None;
    }
    let verification = match control {
        Some(control) => crate::edpb::VerifiedBackupReader::open_controlled(path, control),
        None => crate::edpb::VerifiedBackupReader::open(path),
    };
    let verification_error = verification.as_ref().err().cloned();
    let reader = verification.ok();
    let verified = reader.as_ref().map(|reader| reader.verified());
    let content_sha256 = verified
        .map(|container| container.file_sha256.clone())
        .or_else(|| {
            let mut file = fs::File::open(path).ok()?;
            crate::sha256::sha256_reader_hex(
                &mut crate::bounded_read::ControlledRead {
                    reader: &mut file,
                    control,
                },
                crate::edpb::MAX_CONTAINER_BYTES,
            )
            .ok()
        });
    let coverage = verified.map(|container| {
        crate::backup_coverage::BackupCoverage::from_manifest(&container.manifest)
    });
    let raw = reader
        .as_ref()
        .and_then(|reader| reader.read_raw_protocol().ok());
    let meta = verified.as_ref().and_then(|container| {
        let manifest = &container.manifest;
        let mut identity = crate::edpb::canonical_media_identity(manifest).ok()?;
        if identity.protocol.provision_kind.is_none() {
            if let Some(raw) = raw.as_ref() {
                if let Some(device_id) = identity.protocol.device_id.as_deref() {
                    identity.protocol.provision_kind =
                        crate::provision::DiskProvisionKind::from_metadata(raw, device_id);
                } else if identity.protocol.onlyid.is_none() {
                    let total_sectors = manifest.geometry.total_sectors.unwrap_or(0);
                    if crate::partition_table::confirmed_plain_protocol_prefix(raw, total_sectors) {
                        identity.protocol.provision_kind =
                            Some(crate::provision::DiskProvisionKind::Plain);
                    }
                }
            }
        }
        Some(BackupMeta {
            disk: manifest.observation.disk_number.unwrap_or(0),
            secs: manifest.geometry.total_sectors,
            vid: manifest.device.vid.clone(),
            pid: manifest.device.pid.clone(),
            device_id: manifest.device.device_id.clone(),
            onlyid: manifest.device.onlyid.clone(),
            identity: Some(identity),
        })
    });
    let lba8 = raw.as_ref().and_then(|data| {
        data.get(8 * SECTOR..9 * SECTOR)
            .and_then(|bytes| bytes.try_into().ok())
    });
    // Historical/Core/EDP metadata containers require the fixed LBA0-12
    // protocol artifact. Plain v3 metadata deliberately does not store that
    // protocol core; its raw partition-table artifacts are validated by
    // verify_file()/validate_manifest_graph instead. Do not classify that
    // intentional omission as a size error.
    let size_ok = verified.as_ref().is_some_and(|container| {
        let manifest = &container.manifest;
        let plain_metadata_v3 = manifest.schema == "edpb.manifest.v3"
            && manifest.snapshot.device_state.eq_ignore_ascii_case("plain")
            && manifest.snapshot.capture_level == crate::edpb::CaptureLevel::Metadata;
        plain_metadata_v3
            || raw
                .as_ref()
                .is_some_and(|data| data.len() == crate::common::METADATA_IMAGE_LEN)
    });
    let integrity_status = if verified.is_some() && size_ok {
        BackupIntegrityStatus::Verified
    } else {
        BackupIntegrityStatus::Invalid
    };
    let provision_kind = meta
        .as_ref()
        .and_then(|meta| meta.identity.as_ref())
        .and_then(|identity| identity.protocol.provision_kind);
    // Conservative accounting units, not a claim about allocator RSS. Entry count
    // separately bounds fixed overhead; one verified container remains transient.
    let retained_weight = verified
        .map(|container| {
            serde_json::to_vec(&container.manifest)
                .map(|bytes| bytes.len().saturating_mul(8))
                .unwrap_or(usize::MAX)
        })
        .unwrap_or(4096)
        .saturating_add(path.as_os_str().len().saturating_mul(2))
        .saturating_add(4096);
    let restore_preview = verified.map(|container| {
        crate::backup_restore_preview::BackupRestorePreview::from_manifest(&container.manifest)
    });
    Some((
        BackupEntry {
            display_cached: false,
            meta,
            path: path.to_path_buf(),
            mtime: mtime_epoch(path),
            provision_kind,
            integrity_status,
            size_ok,
            lba8,
            verification_error: verification_error
                .or_else(|| (!size_ok).then(|| "EDPB 协议核心长度不匹配".into())),
            content_sha256,
            coverage,
            manifest: retain_manifest
                .then(|| verified.map(|container| container.manifest.clone()))
                .flatten(),
            restore_preview,
        },
        retained_weight,
    ))
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

/// Automatic prune grouping requires strong canonical identity evidence.
///
/// Prefer usable USB serial evidence, but never trust a serial in isolation: some controllers
/// clone the same USB serial across physically different capacities. Strong grouping therefore also
/// requires matching VID:PID + exact total_sectors + logical_sector_size. v3 carries the reviewed
/// raw serial. Without a usable raw serial, require an observed EDP
/// device_id + onlyid pair plus the same hardware geometry. Model/capacity-only evidence and
/// filename-derived metadata never form an automatic deletion group.
pub fn backup_group_key(entry: &BackupEntry) -> Option<String> {
    entry
        .meta
        .as_ref()?
        .identity
        .as_ref()?
        .strong_backup_group_key()
}

/// Non-destructive list grouping is intentionally broader than prune grouping.
///
/// A healthy verified EDPB with weak identity (for example Plain without a usable USB serial) is
/// still a tool-owned backup and must be displayed normally. Such an entry receives a unique
/// singleton key here; this does not grant prune or destructive-write authority.
pub fn backup_list_group_key(entry: &BackupEntry) -> Option<String> {
    if let Some(key) = backup_group_key(entry) {
        return Some(format!("identity:{key}"));
    }
    (entry.integrity_status == BackupIntegrityStatus::Verified
        && entry.size_ok
        && entry.meta.is_some())
    .then(|| format!("entry:{}", entry.path.to_string_lossy()))
}

/// backup prune 的纯策略层：
/// - 每个 canonical identity 组按备份文件名时间新→旧保留 keep 份；
/// - 仅旧命名无法解析时间时才回退文件系统 mtime；
/// - 无论 keep 是否为 0，每组至少保留最新 1 份，防止清到零份。
pub fn prune_candidates(entries: &[BackupEntry], keep: usize) -> Vec<PathBuf> {
    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    for entry in entries {
        if let Some(key) = backup_group_key(entry) {
            groups.entry(key).or_default().push(entry);
        }
    }

    let mut out = Vec::new();
    for group in groups.values_mut() {
        group.sort_by(|a, b| cmp_backup_newest_first(a, b));
        let preserve = keep.max(1);
        let mut deletable: Vec<&BackupEntry> = group.iter().copied().skip(preserve).collect();
        deletable.reverse();
        out.extend(deletable.into_iter().map(|entry| entry.path.clone()));
    }
    out
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn catalog_budget_rejects_partial_results_and_keeps_bad_entries_below_budget() {
        let dir = std::env::temp_dir().join(format!(
            "edpcli_catalog_budget_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        for n in 0..3 {
            fs::write(dir.join(format!("{n}.edpb")), b"invalid").unwrap();
        }
        let entries = scan_backup_dir_with_budget(&dir, 3, usize::MAX).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .all(|entry| entry.verification_error.is_some()));
        assert!(scan_backup_dir_with_budget(&dir, 2, usize::MAX)
            .unwrap_err()
            .contains("条目预算"));
        assert!(scan_backup_dir_with_budget(&dir, 3, 1)
            .unwrap_err()
            .contains("累计展示数据预算"));
        fs::remove_dir_all(dir).unwrap();
    }
}
