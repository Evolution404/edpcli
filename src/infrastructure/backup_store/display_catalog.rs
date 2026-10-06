//! Display-only cache. Exact restore/delete paths continue to verify fresh file bytes.
use super::catalog::{cmp_backup_newest_first, scan_backup_file_impl, BackupEntry};
use crate::ports::ReadControl;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};
const MAX_ENTRIES: usize = 4096;
const MAX_WEIGHT: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
    changed: (u64, u64),
    identity: (u64, u64),
}
fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || path.extension().and_then(|value| value.to_str()) != Some("edpb") {
        return None;
    }
    #[cfg(unix)]
    let (identity, changed) = {
        use std::os::unix::fs::MetadataExt;
        (
            (metadata.dev(), metadata.ino()),
            (metadata.ctime() as u64, metadata.ctime_nsec() as u64),
        )
    };
    #[cfg(windows)]
    let (identity, changed) = {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = fs::File::open(path).ok()?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a live owned file handle and a properly sized writable record.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return None;
        }
        (
            (
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            ),
            (metadata.creation_time(), metadata.last_write_time()),
        )
    };
    Some(Fingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        changed,
        identity,
    })
}
struct Cached {
    fingerprint: Fingerprint,
    entry: BackupEntry,
    weight: usize,
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<PathBuf, Cached>,
    weight: usize,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
#[derive(Debug)]
pub struct DisplayCatalogScan {
    pub entries: Vec<BackupEntry>,
    pub cache_hits: usize,
    pub bytes_read: u64,
    pub elapsed: Duration,
}

pub fn scan_backup_dir_display(
    dir: &Path,
    control: &ReadControl,
) -> Result<DisplayCatalogScan, String> {
    let start = Instant::now();
    let before = control.bytes_read();
    control.check().map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    let mut weight = 0usize;
    let mut hits = 0;
    if dir.exists() {
        for item in fs::read_dir(dir).map_err(|error| format!("备份目录不可读取: {error}"))?
        {
            control.check().map_err(|error| error.to_string())?;
            let path = item.map_err(|error| error.to_string())?.path();
            let Some(initial) = fingerprint(&path) else {
                continue;
            };
            if entries.len() >= MAX_ENTRIES {
                return Err("备份目录超过条目预算，未返回不完整编号".into());
            }
            let key = fs::canonicalize(&path).map_err(|error| error.to_string())?;
            let cache = CACHE.get_or_init(Default::default);
            let cached = {
                let cache = cache.lock().unwrap_or_else(|poison| poison.into_inner());
                cache
                    .entries
                    .get(&key)
                    .filter(|cached| cached.fingerprint == initial)
                    .map(|cached| (cached.entry.clone(), cached.weight))
            };
            let (mut entry, retained) = if let Some(cached) = cached {
                hits += 1;
                let (mut entry, weight) = cached;
                entry.display_cached = true;
                (entry, weight)
            } else {
                let scanned = scan_backup_file_impl(&path, false, Some(control));
                control.check().map_err(|error| error.to_string())?;
                let Some((entry, retained)) = scanned else {
                    continue;
                };
                if fingerprint(&path).as_ref() != Some(&initial) {
                    return Err("备份文件在目录展示扫描期间发生变化，未返回不完整编号".into());
                }
                let mut cache = cache.lock().unwrap_or_else(|poison| poison.into_inner());
                if let Some(old) = cache.entries.remove(&key) {
                    cache.weight = cache.weight.saturating_sub(old.weight);
                }
                if cache.entries.len() >= MAX_ENTRIES
                    || cache.weight.saturating_add(retained) > MAX_WEIGHT
                {
                    cache.entries.clear();
                    cache.weight = 0;
                }
                if retained <= MAX_WEIGHT {
                    cache.weight += retained;
                    cache.entries.insert(
                        key,
                        Cached {
                            fingerprint: initial,
                            entry: entry.clone(),
                            weight: retained,
                        },
                    );
                }
                (entry, retained)
            };
            entry.path = path;
            weight = weight
                .checked_add(retained)
                .filter(|value| *value <= MAX_WEIGHT)
                .ok_or("备份目录超过展示数据预算，未返回不完整编号")?;
            entries.push(entry);
        }
    }
    control.check().map_err(|error| error.to_string())?;
    entries.sort_by(cmp_backup_newest_first);
    Ok(DisplayCatalogScan {
        entries,
        cache_hits: hits,
        bytes_read: control.bytes_read().saturating_sub(before),
        elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edpb::{write_core_backup, CoreCapture};
    fn fixture(dir: &Path, name: &str) -> PathBuf {
        let raw = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/protocol/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin"));
        let capture = CoreCapture {
            snapshot_id: "display-cache-fixture".into(),
            created_epoch: 1_789_000_000,
            disk_number: Some(6),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some("1402259934".into()),
            total_sectors: Some(122_880_000),
            logical_sector_size: 512,
            edpcli_version: "test".into(),
            device_state: "encrypted".into(),
            lba0_12: raw,
        };
        let path = dir.join(name);
        write_core_backup(&path, &capture).unwrap();
        path
    }
    fn root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "edpcli-display-{}-{}",
            std::process::id(),
            crate::ports::Clock::now_epoch(&crate::infrastructure::clock::SystemClock)
        ));
        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let dir = dir.with_extension(crate::common::hex_lower(&random));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn control() -> ReadControl {
        ReadControl::new(1024 * 1024, Duration::from_secs(5))
    }
    #[test]
    fn display_refresh_reuses_only_unchanged_files_and_delete_reverifies_replacements() {
        let dir = root();
        let path = fixture(&dir, "source.edpb");
        let first = scan_backup_dir_display(&dir, &control()).unwrap();
        assert!(first.bytes_read > 0);
        assert_eq!(first.cache_hits, 0);
        let second = scan_backup_dir_display(&dir, &control()).unwrap();
        assert_eq!(second.bytes_read, 0);
        assert_eq!(second.cache_hits, 1);
        assert!(second.entries[0].display_cached);
        let stale = second.entries[0].clone();
        let original_len = fs::metadata(&path).unwrap().len();
        let replacement = dir.join("replacement.tmp");
        fs::write(&replacement, vec![0x42; original_len as usize]).unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(replacement, &path).unwrap();
        assert!(crate::backup_catalog::delete_entry_verified(&stale).is_err());
        assert!(path.exists());
        let third = scan_backup_dir_display(&dir, &control()).unwrap();
        assert_eq!(third.cache_hits, 0);
        assert!(third.bytes_read > 0);
        assert!(!third.entries[0].health().is_healthy());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cancellation_and_io_budget_never_publish_partial_numbering() {
        let dir = root();
        fixture(&dir, "source.edpb");
        let cancelled = control();
        cancelled.cancel();
        assert!(scan_backup_dir_display(&dir, &cancelled).is_err());
        assert_eq!(cancelled.bytes_read(), 0);
        let limited = ReadControl::new(1, Duration::from_secs(5));
        assert!(scan_backup_dir_display(&dir, &limited).is_err());
        assert!(limited.bytes_read() <= 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
