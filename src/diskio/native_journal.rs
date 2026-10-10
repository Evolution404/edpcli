//! Durable pre-write journal for a *known* native write set. This records only
//! modified blocks, not a complete disk image. A journal is diagnostic and
//! recoverable with separately authenticated device identity; it does NOT
//! promise power-loss atomicity or authorize physical access.
use super::native_transaction::{
    execute_native_transaction, NativeBlockDevice, NativeTransactionFailure,
};
use crate::filesystem::NativeVirtualDiskPlan;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"EDPN4WAL";
const MAX_SNAPSHOT: usize = 128 * 1024 * 1024;

fn failure(message: impl Into<String>) -> NativeTransactionFailure {
    NativeTransactionFailure {
        reason: message.into(),
        rollback_verified: false,
    }
}

fn append_state(file: &mut fs::File, message: &str) -> io::Result<()> {
    file.write_all(message.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

/// Only call while a TargetSession is holding the verified write lease.
/// The WAL must be on separate persistent storage, never the target USB.
/// The journal persists after success to document the write set and outcome.
/// On interrupted execution its presence indicates an unresolved transaction.
pub fn execute_native_transaction_with_journal(
    dev: &mut dyn NativeBlockDevice,
    plan: &NativeVirtualDiskPlan,
    path: &Path,
    device_identity: &str,
) -> Result<(), NativeTransactionFailure> {
    if device_identity.trim().is_empty() || device_identity.len() > 1024 {
        return Err(failure("写前快照必须绑定完整设备身份"));
    }
    if !matches!(plan.sector_bytes, 512 | 4096)
        || dev.sector_bytes() != plan.sector_bytes
        || dev.total_sectors() != plan.total_sectors
        || plan.writes.is_empty()
        || plan.writes.last().is_none_or(|b| b.relative_lba != 0)
    {
        return Err(failure("WAL目标原生几何或LBA0提交顺序不匹配"));
    }
    let mut seen = BTreeSet::new();
    let size = plan
        .writes
        .len()
        .checked_mul(plan.sector_bytes as usize)
        .filter(|v| *v <= MAX_SNAPSHOT)
        .ok_or_else(|| failure("WAL写前快照超过128MiB预算"))?;
    for block in &plan.writes {
        if block.relative_lba >= plan.total_sectors
            || block.data.len() != plan.sector_bytes as usize
            || !seen.insert(block.relative_lba)
        {
            return Err(failure("WAL写集存在重复、越界、截断块"));
        }
    }
    // Disallow dangerous journal destinations and existing paths. create_new
    // also refuses to follow a pre-existing symbolic link.
    if crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err(failure("拒绝把WAL写入raw设备"));
    }
    let mut snapshots = Vec::with_capacity(plan.writes.len());
    for block in &plan.writes {
        let old = dev
            .read_block_fresh(block.relative_lba)
            .map_err(|e| failure(format!("WAL快照LBA{}失败: {e}", block.relative_lba)))?;
        if old.len() != plan.sector_bytes as usize {
            return Err(failure("WAL写前块不完整"));
        }
        snapshots.push(old);
    }
    let mut journal = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| failure(format!("创建专属WAL失败: {e}")))?;
    let mut payload =
        Vec::with_capacity(size.saturating_add(64 + plan.writes.len() * 8 + device_identity.len()));
    payload.extend_from_slice(MAGIC);
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&plan.sector_bytes.to_le_bytes());
    payload.extend_from_slice(&plan.total_sectors.to_le_bytes());
    payload.extend_from_slice(&(plan.writes.len() as u64).to_le_bytes());
    payload.extend_from_slice(&(device_identity.len() as u32).to_le_bytes());
    payload.extend_from_slice(device_identity.as_bytes());
    for (planned, old) in plan.writes.iter().zip(&snapshots) {
        payload.extend_from_slice(&planned.relative_lba.to_le_bytes());
        payload.extend_from_slice(old);
    }
    let fingerprint = Sha256::digest(&payload);
    payload.extend_from_slice(&fingerprint);
    journal
        .write_all(&payload)
        .and_then(|()| journal.sync_all())
        .map_err(|e| failure(format!("WAL持久化失败（绝不写盘）: {e}")))?;
    if let Some(parent) = path.parent() {
        crate::platform::sync_directory(parent)
            .map_err(|e| failure(format!("WAL目录持久化失败（绝不写盘）: {e}")))?;
    }
    // Verify data using a newly opened read-only handle before *any* write.
    let mut fresh =
        fs::File::open(path).map_err(|e| failure(format!("WAL独立重新打开失败: {e}")))?;
    let mut verify = vec![0; payload.len()];
    fresh
        .read_exact(&mut verify)
        .map_err(|e| failure(format!("WAL重新读取失败: {e}")))?;
    if verify != payload {
        return Err(failure("WAL内容或SHA-256重新校验失败"));
    }
    // Reject in-flight swaps between taking the WAL and touching target.
    for (planned, old) in plan.writes.iter().zip(&snapshots) {
        let fresh = dev
            .read_block_fresh(planned.relative_lba)
            .map_err(|e| failure(format!("WAL预写再次读LBA{}失败: {e}", planned.relative_lba)))?;
        if fresh != *old {
            return Err(failure(format!(
                "WAL预写复核LBA{}已变化，拒绝写盘",
                planned.relative_lba
            )));
        }
    }
    // A separate journal state line marks that the last preflight succeeded.
    append_state(&mut journal, "WRITE_STARTED")
        .map_err(|e| failure(format!("WAL状态同步失败（未写盘）: {e}")))?;
    match execute_native_transaction(dev, plan) {
        Ok(()) => append_state(&mut journal, "COMMITTED_SYNC_AND_READBACK_OK")
            .map_err(|e| failure(format!("介质已提交但WAL无法确认完成，状态不确定: {e}"))),
        Err(mut error) => {
            let state = if error.rollback_verified {
                "ROLLBACK_VERIFIED"
            } else {
                "ROLLBACK_FAILED_MEDIA_UNKNOWN"
            };
            if let Err(e) = append_state(&mut journal, state) {
                error.reason = format!("{}；追加WAL故障状态失败: {e}", error.reason);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::NativeFilesystemWrite;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_JOURNAL_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fake {
        sectors: BTreeMap<u64, Vec<u8>>,
        fail_once: Option<u64>,
        writes: Vec<u64>,
    }
    impl NativeBlockDevice for Fake {
        fn total_sectors(&self) -> u64 {
            16
        }
        fn sector_bytes(&self) -> u32 {
            4096
        }
        fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            if lba >= 16 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "overflow"));
            }
            Ok(self.sectors.get(&lba).cloned().unwrap_or(vec![0x55; 4096]))
        }
        fn write_block(&mut self, lba: u64, block: &[u8]) -> io::Result<()> {
            self.writes.push(lba);
            if self.fail_once == Some(lba) {
                self.fail_once = None;
                self.sectors.insert(lba, vec![0x22; 4096]);
                return Err(io::Error::other("injected torn native write"));
            }
            self.sectors.insert(lba, block.to_vec());
            Ok(())
        }
        fn sync_blocks(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn fixture() -> (Fake, NativeVirtualDiskPlan, std::path::PathBuf) {
        // Reserve a dedicated fixture directory atomically. A timestamp on
        // its own does not guarantee that parallel tests cannot reuse a WAL
        // file, and the production writer correctly refuses existing files.
        let directory = loop {
            let candidate = std::env::temp_dir().join(format!(
                "edp-native-journal-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_JOURNAL_FIXTURE.fetch_add(1, Ordering::Relaxed),
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("无法隔离WAL测试目录: {error}"),
            }
        };
        let path = directory.join("snapshot.wal");
        (
            Fake {
                sectors: BTreeMap::new(),
                fail_once: None,
                writes: vec![],
            },
            NativeVirtualDiskPlan {
                sector_bytes: 4096,
                total_sectors: 16,
                writes: vec![
                    NativeFilesystemWrite {
                        relative_lba: 1,
                        data: vec![9; 4096],
                    },
                    NativeFilesystemWrite {
                        relative_lba: 0,
                        data: vec![7; 4096],
                    },
                ],
            },
            path,
        )
    }
    #[test]
    fn journal_is_durable_before_first_write_and_survives_success() {
        let (mut dev, plan, path) = fixture();
        execute_native_transaction_with_journal(&mut dev, &plan, &path, "usb:test:4096").unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.starts_with(MAGIC));
        assert!(bytes.ends_with(b"COMMITTED_SYNC_AND_READBACK_OK\n"));
        assert_eq!(dev.writes, [1, 0]);
        assert_eq!(dev.sectors[&0], vec![7; 4096]);
        assert!(
            execute_native_transaction_with_journal(&mut dev, &plan, &path, "usb:test:4096")
                .is_err()
        );
        fs::remove_file(&path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn injected_torn_write_retains_verified_wal_and_restores_blocks() {
        let (mut dev, plan, path) = fixture();
        dev.fail_once = Some(0);
        let error =
            execute_native_transaction_with_journal(&mut dev, &plan, &path, "usb:test:4096")
                .unwrap_err();
        assert!(error.rollback_verified, "{error}");
        assert!(fs::read(&path).unwrap().ends_with(b"ROLLBACK_VERIFIED\n"));
        assert_eq!(dev.sectors[&0], vec![0x55; 4096]);
        assert_eq!(dev.sectors[&1], vec![0x55; 4096]);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn incomplete_snapshot_prevents_any_write() {
        let (mut dev, mut plan, path) = fixture();
        plan.writes[0].data.truncate(4095);
        assert!(
            execute_native_transaction_with_journal(&mut dev, &plan, &path, "usb:test:4096")
                .is_err()
        );
        assert!(dev.writes.is_empty());
        assert!(!path.exists());
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
