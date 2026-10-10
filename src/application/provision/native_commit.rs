//! Shared native-block device commit path.
//!
//! Source transport (physical USB or opt-in macOS Disk Image) is a discovery
//! policy. Every supported logical block geometry uses the *same* immutable
//! plan, read-only snapshot, TargetSession lease, verified reopen and WAL
//! transaction. No 512B projection or HIL-specific raw writer exists here.

use std::time::Duration;

use crate::application::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::diskio::{NativeBlockDevice, NativeRawBlockDevice};
use crate::filesystem::NativeVirtualDiskPlan;
use crate::ports::CmdRunner;
use sha2::{Digest, Sha256};

/// Destructively apply a complete native-block write plan to the specified
/// whole device. The caller must have obtained explicit user approval.
/// The target must be an authorized real USB device or an explicitly opted-in
/// macOS Disk Image. No device-type or sector-size special branch is used to
/// author, apply or verify the write plan.
pub fn commit_native_plan_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: &NativeVirtualDiskPlan,
    wal_path: &std::path::Path,
) -> Result<(), String> {
    commit_native_plan_on_disk_with_source(runner, disk, plan, wal_path, None)
}

/// Apply an immutable planned write set and verify the exact source snapshot
/// captured *before* interactive confirmation.
pub fn commit_native_plan_on_disk_with_source(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: &NativeVirtualDiskPlan,
    wal_path: &std::path::Path,
    expected_prefix: Option<&[Vec<u8>]>,
) -> Result<(), String> {
    commit_native_plan_on_disk_with_source_observed(
        runner,
        disk,
        plan,
        wal_path,
        expected_prefix,
        &mut |_| {},
    )
}

pub fn commit_native_plan_on_disk_with_source_observed(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: &NativeVirtualDiskPlan,
    wal_path: &std::path::Path,
    expected_prefix: Option<&[Vec<u8>]>,
    observer: &mut dyn FnMut(crate::diskio::TransactionActivity),
) -> Result<(), String> {
    let session = TargetSession::<ReadOnly>::open_usb(runner, disk)
        .map_err(|e| format!("原生制盘目标校验失败: {}", e.msg))?;
    let geometry = session
        .native_geometry()
        .map_err(|e| format!("原生制盘设备几何无效: {}", e.msg))?;
    if geometry.native_sector_count != plan.total_sectors
        || geometry.logical_sector_bytes != plan.sector_bytes
    {
        return Err("原生制盘设备与写集几何不一致".into());
    }
    let path = crate::platform::raw_disk_path(disk);
    let mut dev = NativeRawBlockDevice::open_readonly(&path, geometry)
        .map_err(|e| format!("原生只读打开目标失败: {e}"))?;
    let mut source_prefix = Vec::with_capacity(13);
    for lba in 0..13 {
        source_prefix.push(
            dev.read_block_fresh(lba)
                .map_err(|e| format!("提交前读取LBA{lba}失败: {e}"))?,
        );
    }
    if expected_prefix.is_some_and(|expected| expected != source_prefix.as_slice()) {
        return Err("目标源盘协议在规划与确认之间改变，拒绝写入".into());
    }
    let source_hash = Sha256::digest(source_prefix.concat());
    let identity = format!(
        "native:disk{disk}:{}:{}:{}",
        geometry.capacity_bytes,
        geometry.logical_sector_bytes,
        hex_digest(&source_hash)
    );
    let session = session
        .prepare_native_write()
        .map_err(|e| format!("原生制盘获取设备写租约失败: {e}"))?;
    let mut locked = session
        .reopen_native_and_verify(&mut dev, Duration::from_secs(10), |fresh| {
            for (lba, original) in source_prefix.iter().enumerate() {
                let actual = fresh
                    .read_block_fresh(lba as u64)
                    .map_err(|e| format!("写前重开读取LBA{lba}失败: {e}"))?;
                if actual != *original {
                    return Err(format!("设备内容在确认/卸载期间改变: LBA{lba}"));
                }
            }
            Ok::<(), String>(())
        })
        .map_err(|e| match e {
            ReopenAndVerifyError::Reopen(e) => format!("原生设备重开失败: {e}"),
            ReopenAndVerifyError::Verify(e) => e,
            ReopenAndVerifyError::Geometry(e) => format!("原生设备几何或身份变化: {}", e.msg),
        })?;
    locked
        .execute_native_transaction_with_journal_observed(plan, wal_path, &identity, observer)
        .map_err(|e| {
            format!(
                "原生块制盘事务失败: {}; 回滚验证={}",
                e.reason, e.rollback_verified
            )
        })
}

fn hex_digest(data: &[u8]) -> String {
    data.iter().map(|v| format!("{v:02x}")).collect()
}
