//! Native-block transaction core (512B/4Kn) with a model-only device port.
//! No adapter to raw disks is provided here: USB authorization, device identity
//! pinning, physical rollback and manufacturer protocol certification remain
//! independent requirements before any physical writer could implement it.

use std::collections::BTreeSet;
use std::io;

use crate::filesystem::NativeVirtualDiskPlan;

/// An injectable native-block port. There is intentionally no implementation
/// for an OS disk device in production; tests provide disposable memory media.
pub trait NativeBlockDevice {
    fn total_sectors(&self) -> u64;
    fn sector_bytes(&self) -> u32;
    fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>>;
    /// Fresh independent physical read after sync when the device supports it.
    fn read_block_fresh(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.read_block(lba)
    }
    fn write_block(&mut self, lba: u64, full_block: &[u8]) -> io::Result<()>;
    fn sync_blocks(&mut self) -> io::Result<()>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeTransactionFailure {
    pub reason: String,
    /// Only describes in-process rollback+readback; NOT power-loss durability.
    pub rollback_verified: bool,
}

impl std::fmt::Display for NativeTransactionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (in-process rollback verified: {})",
            self.reason, self.rollback_verified
        )
    }
}
impl std::error::Error for NativeTransactionFailure {}

fn fail(reason: impl Into<String>) -> NativeTransactionFailure {
    NativeTransactionFailure {
        reason: reason.into(),
        rollback_verified: false,
    }
}

const MAX_MIRROR_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn rollback(dev: &mut dyn NativeBlockDevice, originals: &[(u64, Vec<u8>)]) -> bool {
    rollback_observed(dev, originals, &mut |_, _, _| {})
}

fn rollback_observed(
    dev: &mut dyn NativeBlockDevice,
    originals: &[(u64, Vec<u8>)],
    notify: &mut dyn FnMut(crate::diskio::TransactionActivityPhase, u64, u64),
) -> bool {
    use crate::diskio::TransactionActivityPhase as Activity;
    let count = originals.len() as u64;
    notify(Activity::RollbackWrite, 0, count);
    let mut good = true;
    // The original MBR must be restored only after all other metadata, just
    // as new MBR LBA0 is committed last on the success path.
    for (index, (lba, block)) in originals
        .iter()
        .rev()
        .filter(|(lba, _)| *lba != 0)
        .chain(originals.iter().filter(|(lba, _)| *lba == 0))
        .enumerate()
    {
        if dev.write_block(*lba, block).is_err() {
            good = false;
        }
        notify(Activity::RollbackWrite, index as u64 + 1, count);
    }
    notify(Activity::RollbackSync, 0, 0);
    if dev.sync_blocks().is_err() {
        good = false;
    }
    notify(Activity::RollbackReadback, 0, count);
    for (index, (lba, original)) in originals.iter().enumerate() {
        match dev.read_block_fresh(*lba) {
            Ok(actual) if actual == *original => {}
            _ => good = false,
        }
        notify(Activity::RollbackReadback, index as u64 + 1, count);
    }
    good
}

/// Check entire native-block geometry and duplicate ownership before any I/O.
/// Snapshot all modified blocks, write LBA0 last, synchronize/read back every
/// write; on failure restore all snapshots in reverse and verify their bytes.
/// This guarantees neither atomic hardware writes nor crash recovery.
pub fn execute_native_transaction(
    dev: &mut dyn NativeBlockDevice,
    plan: &NativeVirtualDiskPlan,
) -> Result<(), NativeTransactionFailure> {
    if !crate::domain::hardware::valid_native_sector_bytes(plan.sector_bytes)
        || dev.sector_bytes() != plan.sector_bytes
        || dev.total_sectors() != plan.total_sectors
        || plan.writes.is_empty()
        || plan
            .writes
            .last()
            .is_none_or(|write| write.relative_lba != 0)
    {
        return Err(fail("原生块事务几何或MBR提交顺序不合法"));
    }
    let mut seen = BTreeSet::new();
    for write in &plan.writes {
        if write.relative_lba >= plan.total_sectors
            || write.data.len() != plan.sector_bytes as usize
            || !seen.insert(write.relative_lba)
        {
            return Err(fail("原生块事务存在截断、重复或越界LBA"));
        }
    }
    let mirror_bytes = plan
        .writes
        .len()
        .checked_mul(plan.sector_bytes as usize)
        .ok_or_else(|| fail("原生块回滚快照大小溢出"))?;
    if mirror_bytes > MAX_MIRROR_BYTES {
        return Err(fail("原生块回滚快照超过128MiB预算"));
    }
    let mut originals = Vec::with_capacity(plan.writes.len());
    // Snapshot and initial sync are read-only/pure validation steps.
    for write in &plan.writes {
        let old = dev
            .read_block(write.relative_lba)
            .map_err(|e| fail(format!("预检读取LBA{}失败: {e}", write.relative_lba)))?;
        if old.len() != plan.sector_bytes as usize {
            return Err(fail(format!("预检LBA{}返回截断块", write.relative_lba)));
        }
        originals.push((write.relative_lba, old));
    }
    execute_native_transaction_with_snapshot(dev, plan, &originals)
}

/// Execute using the *same immutable original blocks* already sealed in a
/// durable pre-write WAL, instead of sampling a second rollback baseline.
/// This closes the gap in which external changes could otherwise make the
/// journal disagree with the rollback performed by the transaction.
pub(super) fn execute_native_transaction_with_snapshot(
    dev: &mut dyn NativeBlockDevice,
    plan: &NativeVirtualDiskPlan,
    originals: &[(u64, Vec<u8>)],
) -> Result<(), NativeTransactionFailure> {
    execute_native_transaction_with_snapshot_observed(dev, plan, originals, &mut |_| {})
}

pub(super) fn execute_native_transaction_with_snapshot_observed(
    dev: &mut dyn NativeBlockDevice,
    plan: &NativeVirtualDiskPlan,
    originals: &[(u64, Vec<u8>)],
    observer: &mut dyn FnMut(crate::diskio::TransactionActivity),
) -> Result<(), NativeTransactionFailure> {
    if !crate::domain::hardware::valid_native_sector_bytes(plan.sector_bytes)
        || plan.sector_bytes != dev.sector_bytes()
        || plan.total_sectors != dev.total_sectors()
        || plan
            .writes
            .last()
            .is_none_or(|write| write.relative_lba != 0)
        || plan.writes.is_empty()
        || originals.len() != plan.writes.len()
        || plan
            .writes
            .iter()
            .zip(originals)
            .any(|(write, (lba, old))| {
                write.relative_lba != *lba
                    || write.relative_lba >= plan.total_sectors
                    || write.data.len() != plan.sector_bytes as usize
                    || old.len() != plan.sector_bytes as usize
            })
        || plan
            .writes
            .len()
            .checked_mul(plan.sector_bytes as usize)
            .is_none_or(|size| size > MAX_MIRROR_BYTES)
    {
        return Err(fail("WAL原始块与目标写集或原生几何不一致"));
    }
    let mut seen = BTreeSet::new();
    if plan
        .writes
        .iter()
        .any(|write| !seen.insert(write.relative_lba))
    {
        return Err(fail("WAL执行计划存在重复LBA"));
    }
    use crate::diskio::{TransactionActivity, TransactionActivityPhase as Activity};
    let count = plan.writes.len() as u64;
    let mut notify = |phase, current, total| {
        let event = TransactionActivity {
            phase,
            current,
            total,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(event)));
    };
    notify(Activity::SyncPreflight, 0, 0);
    dev.sync_blocks()
        .map_err(|e| fail(format!("预检同步失败: {e}")))?;
    notify(Activity::Write, 0, count);
    let result: Result<(), String> = (|| {
        for (index, write) in plan.writes.iter().enumerate() {
            if index + 1 == plan.writes.len() {
                dev.sync_blocks()
                    .map_err(|e| format!("MBR提交前同步失败: {e}"))?;
            }
            dev.write_block(write.relative_lba, &write.data)
                .map_err(|e| format!("LBA{}写入失败: {e}", write.relative_lba))?;
            let actual = dev
                .read_block(write.relative_lba)
                .map_err(|e| format!("LBA{}写后回读失败: {e}", write.relative_lba))?;
            if actual != write.data {
                return Err(format!("LBA{}写后回读不一致", write.relative_lba));
            }
            notify(Activity::Write, index as u64 + 1, count);
        }
        notify(Activity::Sync, 0, 0);
        dev.sync_blocks()
            .map_err(|e| format!("最终同步失败: {e}"))?;
        // A second full verification after sync catches deferred corruption.
        notify(Activity::Readback, 0, count);
        for (index, write) in plan.writes.iter().enumerate() {
            let actual = dev
                .read_block_fresh(write.relative_lba)
                .map_err(|e| format!("LBA{}同步后独立读取失败: {e}", write.relative_lba))?;
            if actual != write.data {
                return Err(format!("LBA{}同步后回读不一致", write.relative_lba));
            }
            notify(Activity::Readback, index as u64 + 1, count);
        }
        Ok(())
    })();
    if let Err(reason) = result {
        let verified = rollback_observed(dev, originals, &mut notify);
        return Err(NativeTransactionFailure {
            reason,
            rollback_verified: verified,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::NativeFilesystemWrite;
    use std::collections::BTreeMap;

    struct MemoryMedia {
        bytes: u32,
        total: u64,
        blocks: BTreeMap<u64, Vec<u8>>,
        fail_write_once: Option<u64>,
        wrote_lbas: Vec<u64>,
    }
    impl NativeBlockDevice for MemoryMedia {
        fn total_sectors(&self) -> u64 {
            self.total
        }
        fn sector_bytes(&self) -> u32 {
            self.bytes
        }
        fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            Ok(self
                .blocks
                .get(&lba)
                .cloned()
                .unwrap_or(vec![0xa5; self.bytes as usize]))
        }
        fn write_block(&mut self, lba: u64, block: &[u8]) -> io::Result<()> {
            if self.fail_write_once == Some(lba) {
                self.fail_write_once = None;
                // Simulate a torn native-block write before an I/O error.
                self.blocks.insert(lba, vec![0xee; self.bytes as usize]);
                return Err(io::Error::other("injected torn write"));
            }
            self.wrote_lbas.push(lba);
            self.blocks.insert(lba, block.to_vec());
            Ok(())
        }
        fn sync_blocks(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn fixture(bytes: u32) -> (MemoryMedia, NativeVirtualDiskPlan) {
        let total = 128;
        let dev = MemoryMedia {
            bytes,
            total,
            blocks: BTreeMap::new(),
            fail_write_once: None,
            wrote_lbas: vec![],
        };
        let plan = NativeVirtualDiskPlan {
            sector_bytes: bytes,
            total_sectors: total,
            writes: vec![
                NativeFilesystemWrite {
                    relative_lba: 16,
                    data: vec![0x11; bytes as usize],
                },
                NativeFilesystemWrite {
                    relative_lba: 17,
                    data: vec![0x22; bytes as usize],
                },
                NativeFilesystemWrite {
                    relative_lba: 0,
                    data: vec![0x33; bytes as usize],
                },
            ],
        };
        (dev, plan)
    }
    #[test]
    fn native_transaction_commits_mbr_last_on_512_and_4096_byte_devices() {
        for bytes in [512, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let (mut dev, plan) = fixture(bytes);
            execute_native_transaction(&mut dev, &plan).unwrap();
            assert_eq!(dev.wrote_lbas, [16, 17, 0]);
            for w in &plan.writes {
                assert_eq!(dev.blocks[&w.relative_lba], w.data);
            }
        }
    }
    #[test]
    fn native_transaction_rolls_back_when_final_mbr_commit_fails() {
        for bytes in [512, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let (mut dev, plan) = fixture(bytes);
            dev.fail_write_once = Some(0);
            let failure = execute_native_transaction(&mut dev, &plan).unwrap_err();
            assert!(failure.rollback_verified, "{failure}");
            for lba in [0, 16, 17] {
                assert_eq!(dev.blocks[&lba], vec![0xa5; bytes as usize]);
            }
        }
    }

    #[test]
    fn native_transaction_rolls_back_torn_write_and_rejects_cross_geometry() {
        for bytes in [512, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let (mut dev, plan) = fixture(bytes);
            dev.fail_write_once = Some(17);
            let err = execute_native_transaction(&mut dev, &plan).unwrap_err();
            assert!(err.rollback_verified, "{err}");
            assert!(dev.blocks.values().all(|v| v.iter().all(|b| *b == 0xa5)));
            let (mut wrong, _) = fixture(if bytes == 512 { 4096 } else { 512 });
            assert!(execute_native_transaction(&mut wrong, &plan).is_err());
            assert!(wrong.blocks.is_empty());
        }
    }
}
