//! Bounded WAL parser and explicitly authorised recovery of native-block originals.
//! The journal records *old* blocks, not the desired target filesystem. Merely
//! finding a WAL never grants permission to open or write a physical disk.
use super::native_transaction::{rollback, NativeBlockDevice, NativeTransactionFailure};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 8] = b"EDPN4WAL";
const MAX_SNAPSHOT: usize = 128 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = (MAX_SNAPSHOT + (MAX_SNAPSHOT / 512) * 8 + 4096) as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeJournalState {
    SnapshotReady,
    WriteStarted,
    Committed,
    RollbackVerified,
    RollbackFailed,
    RecoveryVerified,
    RecoveryFailed,
}

#[derive(Debug)]
pub struct NativeJournalSnapshot {
    pub sector_bytes: u32,
    pub total_sectors: u64,
    pub device_identity: String,
    pub blocks: Vec<(u64, Vec<u8>)>,
    pub state: NativeJournalState,
}

fn invalid(reason: impl Into<String>) -> NativeTransactionFailure {
    NativeTransactionFailure {
        reason: reason.into(),
        rollback_verified: false,
    }
}

fn parse_journal(data: &[u8]) -> Result<NativeJournalSnapshot, NativeTransactionFailure> {
    fn read<'a>(
        data: &'a [u8],
        pos: &mut usize,
        count: usize,
    ) -> Result<&'a [u8], NativeTransactionFailure> {
        let end = pos
            .checked_add(count)
            .ok_or_else(|| invalid("WAL索引溢出"))?;
        let slice = data.get(*pos..end).ok_or_else(|| invalid("WAL截断"))?;
        *pos = end;
        Ok(slice)
    }
    fn number<const N: usize>(
        data: &[u8],
        pos: &mut usize,
    ) -> Result<[u8; N], NativeTransactionFailure> {
        Ok(read(data, pos, N)?.try_into().expect("fixed length"))
    }
    let mut pos = 0usize;
    if read(data, &mut pos, 8)? != MAGIC {
        return Err(invalid("WAL魔数错误"));
    }
    let version = u32::from_le_bytes(number::<4>(data, &mut pos)?);
    let sector_bytes = u32::from_le_bytes(number::<4>(data, &mut pos)?);
    let total_sectors = u64::from_le_bytes(number::<8>(data, &mut pos)?);
    let count = u64::from_le_bytes(number::<8>(data, &mut pos)?);
    let identity_len = u32::from_le_bytes(number::<4>(data, &mut pos)?) as usize;
    if version != 1
        || !crate::domain::hardware::valid_native_sector_bytes(sector_bytes)
        || total_sectors == 0
        || count == 0
        || count > (MAX_SNAPSHOT / sector_bytes as usize) as u64
        || identity_len == 0
        || identity_len > 1024
    {
        return Err(invalid("WAL版本、几何、身份或写集超过预算"));
    }
    let identity = read(data, &mut pos, identity_len)?;
    let device_identity = std::str::from_utf8(identity)
        .map_err(|_| invalid("WAL设备身份不是UTF-8"))?
        .to_owned();
    if device_identity.trim().is_empty() {
        return Err(invalid("WAL设备身份为空"));
    }
    let mut blocks = Vec::with_capacity(count as usize);
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let lba = u64::from_le_bytes(number::<8>(data, &mut pos)?);
        if lba >= total_sectors || !seen.insert(lba) {
            return Err(invalid("WAL原始块重复或越界"));
        }
        blocks.push((lba, read(data, &mut pos, sector_bytes as usize)?.to_vec()));
    }
    if blocks.last().is_none_or(|(lba, _)| *lba != 0) {
        return Err(invalid("WAL缺少末尾MBR块"));
    }
    let sha_start = pos;
    let recorded = read(data, &mut pos, 32)?;
    if recorded != Sha256::digest(&data[..sha_start]).as_slice() {
        return Err(invalid("WAL SHA-256验证失败"));
    }
    let tail = &data[pos..];
    let state = match tail {
        b"" => NativeJournalState::SnapshotReady,
        b"WRITE_STARTED\n" => NativeJournalState::WriteStarted,
        b"WRITE_STARTED\nCOMMITTED_SYNC_AND_READBACK_OK\n" => NativeJournalState::Committed,
        b"WRITE_STARTED\nROLLBACK_VERIFIED\n" => NativeJournalState::RollbackVerified,
        b"WRITE_STARTED\nROLLBACK_FAILED_MEDIA_UNKNOWN\n" => NativeJournalState::RollbackFailed,
        b"WRITE_STARTED\nRECOVERY_VERIFIED\n" |
        b"WRITE_STARTED\nROLLBACK_FAILED_MEDIA_UNKNOWN\nRECOVERY_VERIFIED\n" |
        b"WRITE_STARTED\nRECOVERY_FAILED_MEDIA_UNKNOWN\nRECOVERY_VERIFIED\n" |
        b"WRITE_STARTED\nROLLBACK_FAILED_MEDIA_UNKNOWN\nRECOVERY_FAILED_MEDIA_UNKNOWN\nRECOVERY_VERIFIED\n"
            => NativeJournalState::RecoveryVerified,
        b"WRITE_STARTED\nRECOVERY_FAILED_MEDIA_UNKNOWN\n" |
        b"WRITE_STARTED\nROLLBACK_FAILED_MEDIA_UNKNOWN\nRECOVERY_FAILED_MEDIA_UNKNOWN\n"
            => NativeJournalState::RecoveryFailed,
        _ => return Err(invalid("WAL状态追加记录损坏或未受支持")),
    };
    Ok(NativeJournalSnapshot {
        sector_bytes,
        total_sectors,
        device_identity,
        blocks,
        state,
    })
}

fn read_bounded(file: &mut File) -> Result<NativeJournalSnapshot, NativeTransactionFailure> {
    let bytes = file
        .metadata()
        .map_err(|e| invalid(format!("WAL元数据读取失败: {e}")))?;
    if !bytes.is_file() || bytes.len() > MAX_FILE_BYTES {
        return Err(invalid("WAL不是受限大小的普通文件"));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| invalid(format!("WAL定位失败: {e}")))?;
    let mut contents = Vec::with_capacity(bytes.len() as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|e| invalid(format!("WAL读取失败: {e}")))?;
    if contents.len() as u64 != bytes.len() {
        return Err(invalid("WAL读取期间发生截断或增长"));
    }
    parse_journal(&contents)
}

fn open_journal(path: &Path, append: bool) -> Result<File, NativeTransactionFailure> {
    if crate::platform::is_raw_device_path(&path.to_string_lossy()) {
        return Err(invalid("禁止通过raw设备路径使用WAL"));
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| invalid(format!("WAL路径无效: {e}")))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(invalid("WAL必须是独立的受限普通文件，不能为符号链接"));
    }
    let mut opts = OpenOptions::new();
    opts.read(true).append(append);
    opts.open(path)
        .map_err(|e| invalid(format!("WAL打开失败: {e}")))
}

/// Read-only inspection; neither the path nor WAL contents authorise a device write.
pub fn inspect_native_journal(
    path: &Path,
) -> Result<NativeJournalSnapshot, NativeTransactionFailure> {
    read_bounded(&mut open_journal(path, false)?)
}

/// Called only by a TargetSession in NativeWriteLocked after re-probing hardware,
/// reauthenticating the caller's pinned identity and acquiring its write lease.
/// Never expose this as an unrestricted CLI/USB restore operation.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "4Kn native physical recovery is not yet released")
)]
pub(crate) fn recover_native_journal_locked(
    dev: &mut dyn NativeBlockDevice,
    path: &Path,
    pinned_identity: &str,
) -> Result<(), NativeTransactionFailure> {
    let mut file = open_journal(path, true)?;
    let snapshot = read_bounded(&mut file)?;
    if pinned_identity.trim().is_empty()
        || snapshot.device_identity != pinned_identity
        || snapshot.sector_bytes != dev.sector_bytes()
        || snapshot.total_sectors != dev.total_sectors()
    {
        return Err(invalid("WAL身份或原生几何与当前已授权设备不同"));
    }
    if !matches!(
        snapshot.state,
        NativeJournalState::WriteStarted
            | NativeJournalState::RollbackFailed
            | NativeJournalState::RecoveryFailed
    ) {
        return Err(invalid("WAL未处于需要恢复的中断状态"));
    }
    let good = rollback(dev, &snapshot.blocks);
    let state = if good {
        "RECOVERY_VERIFIED\n"
    } else {
        "RECOVERY_FAILED_MEDIA_UNKNOWN\n"
    };
    use std::io::Write;
    file.write_all(state.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| invalid(format!("恢复状态无法持久化，介质状态未知: {e}")))?;
    if !good {
        return Err(invalid("WAL原始块恢复或独立回读失败，介质状态未知"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diskio::native_journal::execute_native_transaction_with_journal;
    use crate::filesystem::{NativeFilesystemWrite, NativeVirtualDiskPlan};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Memory {
        blocks: BTreeMap<u64, Vec<u8>>,
        writes: Vec<u64>,
        block_bytes: u32,
    }
    impl NativeBlockDevice for Memory {
        fn total_sectors(&self) -> u64 {
            16
        }
        fn sector_bytes(&self) -> u32 {
            self.block_bytes
        }
        fn read_block(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
            Ok(self
                .blocks
                .get(&lba)
                .cloned()
                .unwrap_or(vec![0x55; self.block_bytes as usize]))
        }
        fn write_block(&mut self, lba: u64, data: &[u8]) -> std::io::Result<()> {
            self.writes.push(lba);
            self.blocks.insert(lba, data.to_vec());
            Ok(())
        }
        fn sync_blocks(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn fixture_with_bytes(
        native_bytes: u32,
    ) -> (Memory, NativeVirtualDiskPlan, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "edp-wal-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("journal.wal");
        (
            Memory {
                blocks: BTreeMap::new(),
                writes: vec![],
                block_bytes: native_bytes,
            },
            NativeVirtualDiskPlan {
                sector_bytes: native_bytes,
                total_sectors: 16,
                writes: vec![
                    NativeFilesystemWrite {
                        relative_lba: 1,
                        data: vec![0x22; native_bytes as usize],
                    },
                    NativeFilesystemWrite {
                        relative_lba: 0,
                        data: vec![0x33; native_bytes as usize],
                    },
                ],
            },
            path,
        )
    }
    fn fixture() -> (Memory, NativeVirtualDiskPlan, std::path::PathBuf) {
        fixture_with_bytes(4096)
    }
    fn cleanup(path: &Path) {
        fs::remove_file(path).unwrap();
        fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    fn interrupted(path: &Path) {
        let bytes = fs::read(path).unwrap();
        let marker = b"COMMITTED_SYNC_AND_READBACK_OK\n";
        assert!(bytes.ends_with(marker));
        fs::write(path, &bytes[..bytes.len() - marker.len()]).unwrap();
    }

    #[test]
    fn native_wal_recovery_accepts_any_verified_512_multiple_and_restores_exact_blocks() {
        for sector_bytes in [512u32, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let (mut dev, plan, path) = fixture_with_bytes(sector_bytes);
            execute_native_transaction_with_journal(
                &mut dev,
                &plan,
                &path,
                "generic:immutable-identity",
            )
            .unwrap();
            let stored = inspect_native_journal(&path).unwrap();
            assert_eq!(stored.sector_bytes, sector_bytes);
            assert_eq!(stored.state, NativeJournalState::Committed);
            assert_eq!(stored.blocks.len(), 2);
            interrupted(&path);
            dev.writes.clear();
            recover_native_journal_locked(&mut dev, &path, "generic:immutable-identity").unwrap();
            assert_eq!(dev.writes, [1, 0]);
            assert_eq!(dev.blocks[&0], vec![0x55; sector_bytes as usize]);
            assert_eq!(dev.blocks[&1], vec![0x55; sector_bytes as usize]);
            assert_eq!(
                inspect_native_journal(&path).unwrap().state,
                NativeJournalState::RecoveryVerified
            );
            cleanup(&path);
        }
    }

    #[test]
    fn interrupted_native_4kn_wal_recovery_restores_mbr_last_and_does_not_repeat() {
        let (mut dev, plan, path) = fixture();
        execute_native_transaction_with_journal(&mut dev, &plan, &path, "u391:identity").unwrap();
        interrupted(&path);
        assert_eq!(
            inspect_native_journal(&path).unwrap().state,
            NativeJournalState::WriteStarted
        );
        dev.writes.clear();
        recover_native_journal_locked(&mut dev, &path, "u391:identity").unwrap();
        assert_eq!(dev.writes, vec![1, 0]);
        assert_eq!(dev.blocks[&0], vec![0x55; 4096]);
        assert_eq!(dev.blocks[&1], vec![0x55; 4096]);
        assert_eq!(
            inspect_native_journal(&path).unwrap().state,
            NativeJournalState::RecoveryVerified
        );
        assert!(recover_native_journal_locked(&mut dev, &path, "u391:identity").is_err());
        cleanup(&path);
    }

    #[test]
    fn corrupted_wal_identity_and_geometry_are_rejected_before_any_write() {
        let (mut dev, plan, path) = fixture();
        execute_native_transaction_with_journal(&mut dev, &plan, &path, "u391:identity").unwrap();
        interrupted(&path);
        dev.writes.clear();
        assert!(recover_native_journal_locked(&mut dev, &path, "other-usb").is_err());
        dev.block_bytes = 512;
        assert!(recover_native_journal_locked(&mut dev, &path, "u391:identity").is_err());
        dev.block_bytes = 4096;
        let mut damaged = fs::read(&path).unwrap();
        damaged[45] ^= 0x80;
        fs::write(&path, damaged).unwrap();
        assert!(inspect_native_journal(&path).is_err());
        assert!(recover_native_journal_locked(&mut dev, &path, "u391:identity").is_err());
        assert!(dev.writes.is_empty());
        cleanup(&path);
    }

    #[test]
    fn successful_commit_must_not_be_recovered() {
        let (mut dev, plan, path) = fixture();
        execute_native_transaction_with_journal(&mut dev, &plan, &path, "u391:identity").unwrap();
        assert_eq!(
            inspect_native_journal(&path).unwrap().state,
            NativeJournalState::Committed
        );
        dev.writes.clear();
        assert!(recover_native_journal_locked(&mut dev, &path, "u391:identity").is_err());
        assert!(dev.writes.is_empty());
        cleanup(&path);
    }
}
