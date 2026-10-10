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

/// Evidence frozen while creating the user-approved source-aware plan.
/// The raw LBA0–12 prefix is passed separately; these pins cover the old
/// partition boot blocks, full original LCE and available hardware identity.
#[derive(Clone, Copy)]
pub struct NativeSourceGuard<'a> {
    pub device_id: &'a str,
    pub hardware_probe: &'a crate::platform::HardwareProbe,
    pub hardware_serial: Option<&'a str>,
    pub pinned_blocks: &'a [(u64, Vec<u8>)],
}

fn verify_frozen_hardware(
    expected_device_id: &str,
    expected_probe: &crate::platform::HardwareProbe,
    expected_serial: Option<&str>,
    actual_device_id: &str,
    actual_probe: &crate::platform::HardwareProbe,
    actual_serial: Option<&str>,
) -> Result<(), String> {
    if expected_device_id != actual_device_id {
        return Err("目标硬件DeviceID在计划与写入之间发生变化，拒绝写盘".into());
    }
    // Vendor/model alone is not unique. Reject a changed VID/PID, INQUIRY
    // or Windows PnP identity, even when LBA0-12 and capacity are identical.
    if (expected_probe.vid.is_some() && expected_probe.vid != actual_probe.vid)
        || (expected_probe.pid.is_some() && expected_probe.pid != actual_probe.pid)
        || (expected_probe.inquiry.is_some() && expected_probe.inquiry != actual_probe.inquiry)
        || (expected_probe.windows_pnp_instance_id.is_some()
            && expected_probe.windows_pnp_instance_id != actual_probe.windows_pnp_instance_id)
    {
        return Err("目标硬件VID/PID或Inquiry身份发生变化，拒绝写盘".into());
    }
    if expected_serial.is_some() && expected_serial != actual_serial {
        return Err("目标硬件序列号在计划与写入之间发生变化，拒绝写盘".into());
    }
    Ok(())
}

fn verify_frozen_blocks(
    pins: &[(u64, Vec<u8>)],
    sector_bytes: usize,
    mut read: impl FnMut(u64) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    for (lba, expected) in pins {
        if expected.len() != sector_bytes || read(*lba)? != *expected {
            return Err(format!(
                "来源LBA{lba}在确认后改变（分区首块或LCE），拒绝写盘"
            ));
        }
    }
    Ok(())
}

fn verify_frozen_source(
    runner: &dyn CmdRunner,
    disk: u32,
    total_sectors: u64,
    sector_bytes: usize,
    guard: &NativeSourceGuard<'_>,
    read: impl FnMut(u64) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let probe = crate::platform::system::native_provision_probe(runner, disk)?;
    let live_device_id = crate::provision::TargetIdentity::from_probe(&probe, total_sectors)?
        .device_id()
        .to_owned();
    let live_serial = runner.hardware_serial(disk);
    verify_frozen_hardware(
        guard.device_id,
        guard.hardware_probe,
        guard.hardware_serial,
        &live_device_id,
        &probe,
        live_serial.as_deref(),
    )?;
    verify_frozen_blocks(guard.pinned_blocks, sector_bytes, read)
}

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
        None,
        &mut |_| {},
    )
}

pub fn commit_native_plan_on_disk_with_source_observed(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: &NativeVirtualDiskPlan,
    wal_path: &std::path::Path,
    expected_prefix: Option<&[Vec<u8>]>,
    source_guard: Option<NativeSourceGuard<'_>>,
    observer: &mut dyn FnMut(crate::diskio::TransactionActivity),
) -> Result<(), String> {
    if source_guard.is_some() && expected_prefix.is_none() {
        return Err("冻结来源证据必须同时包含原生LBA0–12".into());
    }
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
    if let Some(guard) = &source_guard {
        verify_frozen_source(
            runner,
            disk,
            geometry.native_sector_count,
            geometry.logical_sector_bytes as usize,
            guard,
            |lba| dev.read_block_fresh(lba).map_err(|e| e.to_string()),
        )?;
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
            if let Some(guard) = &source_guard {
                verify_frozen_source(
                    runner,
                    disk,
                    geometry.native_sector_count,
                    geometry.logical_sector_bytes as usize,
                    guard,
                    |lba| fresh.read_block_fresh(lba).map_err(|e| e.to_string()),
                )?;
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

#[cfg(test)]
mod frozen_source_guard_tests {
    use super::*;

    fn fixture_probe() -> crate::platform::HardwareProbe {
        crate::platform::HardwareProbe {
            vid: Some(0x1234),
            pid: Some(0x5678),
            transport: crate::platform::NativeTransport::Uas,
            windows_pnp_instance_id: None,
            inquiry: Some(crate::platform::InquiryInfo {
                vendor: "a".into(),
                product: "a".into(),
                revision: "1.00".into(),
            }),
        }
    }

    #[test]
    fn rejects_same_geometry_and_protocol_on_other_device_or_serial() {
        let probe = fixture_probe();
        let check = |id, serial| {
            verify_frozen_hardware(
                "disk&ven_a&prod_a",
                &probe,
                Some("SERIAL-A"),
                id,
                &probe,
                serial,
            )
        };
        assert!(check("disk&ven_a&prod_a", Some("SERIAL-A")).is_ok());
        assert!(check("disk&ven_b&prod_a", Some("SERIAL-A")).is_err());
        assert!(check("disk&ven_a&prod_a", Some("SERIAL-B")).is_err());
        assert!(check("disk&ven_a&prod_a", None).is_err());
    }

    #[test]
    fn rejects_identical_device_id_and_protocol_on_changed_usb_vid_or_inquiry() {
        let probe = fixture_probe();
        let mut changed = probe.clone();
        changed.vid = Some(0xabcd);
        let check = |other: &crate::platform::HardwareProbe| {
            verify_frozen_hardware(
                "disk&ven_a&prod_a",
                &probe,
                None,
                "disk&ven_a&prod_a",
                other,
                None,
            )
        };
        assert!(check(&changed).is_err());
        changed = probe.clone();
        changed.inquiry.as_mut().unwrap().revision = "2.00".into();
        assert!(check(&changed).is_err());
    }

    #[test]
    fn rejects_changed_source_partition_boot_or_lce_native_tail_at_all_sector_widths() {
        for sector in [512, 1024, 2048, 4096] {
            let pins = vec![
                (63u64, vec![0x19u8; sector]),
                (50_000u64, vec![0x72u8; sector]),
            ];
            assert!(verify_frozen_blocks(&pins, sector, |lba| {
                Ok(pins.iter().find(|(n, _)| *n == lba).unwrap().1.clone())
            })
            .is_ok());
            let rejected = verify_frozen_blocks(&pins, sector, |lba| {
                let mut block = pins.iter().find(|(n, _)| *n == lba).unwrap().1.clone();
                if lba == 50_000 {
                    block[sector - 1] ^= 1;
                }
                Ok(block)
            });
            assert!(rejected.unwrap_err().contains("LBA50000"));
            assert!(verify_frozen_blocks(&pins, sector, |lba| {
                let mut block = pins.iter().find(|(n, _)| *n == lba).unwrap().1.clone();
                block.truncate(512.min(sector - 1));
                Ok(block)
            })
            .is_err());
        }
    }
}
