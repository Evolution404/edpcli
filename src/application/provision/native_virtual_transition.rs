//! Disposable regular-file-only destructive virtual reprovision simulator.
//! It is NOT a data-preserving migration or physical-disk authorization.
use super::native_image::{export_native_plain_image, verify_native_virtual_image};
use crate::filesystem::NativeVirtualDiskPlan;
use crate::protocol::image::NativeProtocolImage;
use crate::provision::DiskProvisionKind;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVirtualTransitionEvidence {
    pub source_mode: DiskProvisionKind,
    pub target_mode: DiskProvisionKind,
    pub logical_sector_bytes: u32,
    pub total_native_sectors: u64,
    pub source_protocol_sha256: String,
    pub target_protocol_sha256: String,
    pub target_written_blocks: usize,
    /// ALWAYS destructive: no user partitions/files or unknown source sectors are copied.
    pub destructive_rebuild: bool,
}

fn protocol_from_disk(
    path: &Path,
    geometry: &NativeVirtualDiskPlan,
) -> Result<NativeProtocolImage, String> {
    let mut file = File::open(path).map_err(|e| format!("无法重新打开来源/目标虚拟盘: {e}"))?;
    let size = usize::try_from(geometry.sector_bytes).map_err(|_| "原生逻辑扇区超出内存块大小")?;
    let mut blocks = vec![0u8; size.checked_mul(13).ok_or("协议块长度溢出")?];
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("协议起点定位失败: {e}"))?;
    file.read_exact(&mut blocks)
        .map_err(|e| format!("无法完整读取13个原生协议块: {e}"))?;
    NativeProtocolImage::from_native_bytes(geometry.sector_bytes, blocks)
        .map_err(|e| format!("无法构造原生协议投影: {e}"))
}

fn validate_protocol_mode(
    raw: &NativeProtocolImage,
    total_sectors: u64,
    expected: DiskProvisionKind,
    device_id: &str,
) -> Result<String, String> {
    let projection = raw.protocol_projection();
    if expected == DiskProvisionKind::Plain {
        if !crate::partition_table::confirmed_plain_protocol_prefix(&projection, total_sectors) {
            return Err("声称普通盘但MBR/分区表未获确认".into());
        }
        // A valid EDP profile wins over the plain-table fallback.
        if crate::provision::parse_existing_provision_native(raw, device_id, total_sectors)?
            .is_some()
        {
            return Err("普通盘声明与已识别EDP协议冲突".into());
        }
    } else {
        let parsed =
            crate::provision::parse_existing_provision_native(raw, device_id, total_sectors)?
                .ok_or("未识别完整EDP协议，不能将其认作已认证来源模式")?;
        if DiskProvisionKind::from_mode(parsed.profile.source_mode) != expected {
            return Err(format!(
                "模式不一致：声明 {:?}，原生协议实测 {:?}",
                expected, parsed.profile.source_mode
            ));
        }
    }
    Ok(Sha256::digest(projection)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect())
}

fn protocol_from_plan(plan: &NativeVirtualDiskPlan) -> Result<NativeProtocolImage, String> {
    if plan.total_sectors < 13 {
        return Err("目标虚拟盘不足13个原生扇区".into());
    }
    let block_size = plan.sector_bytes as usize;
    let mut bytes = vec![0u8; block_size.checked_mul(13).ok_or("目标协议字节长度溢出")?];
    for write in &plan.writes {
        if write.relative_lba < 13 {
            if write.data.len() != block_size {
                return Err("目标协议写入未使用完整原生块".into());
            }
            let start = write.relative_lba as usize * block_size;
            bytes[start..start + block_size].copy_from_slice(&write.data);
        }
    }
    NativeProtocolImage::from_native_bytes(plan.sector_bytes, bytes)
        .map_err(|err| format!("目标协议投影失败: {err}"))
}

fn verify_disk_mode(
    path: &Path,
    plan: &NativeVirtualDiskPlan,
    expected: DiskProvisionKind,
    device_id: &str,
) -> Result<String, String> {
    verify_native_virtual_image(path, plan)?;
    if plan.total_sectors < 13 {
        return Err("原生镜像不足13块，无法进行来源/目标模式认证".into());
    }
    let raw = protocol_from_disk(path, plan)?;
    validate_protocol_mode(&raw, plan.total_sectors, expected, device_id)
}
/// Build a brand-new **disposable** sparse file, after verifying an existing
/// regular-file source against its exact original expected write set, mode
/// and geometry. No data is copied from source: this explicitly models a
/// fresh, destructive target re-creation, NOT an in-place or lossless migration.
///
/// Both sides must share native geometry and disk capacity. The new target
/// must not pre-exist; raw paths/symlinks cannot be used. There is no USB handle.
pub fn simulate_destructive_native_virtual_transition(
    source_path: &Path,
    source_plan: &NativeVirtualDiskPlan,
    source_mode: DiskProvisionKind,
    target_path: &Path,
    target_plan: &NativeVirtualDiskPlan,
    target_mode: DiskProvisionKind,
    device_id: &str,
) -> Result<NativeVirtualTransitionEvidence, String> {
    if device_id.trim().is_empty() {
        return Err("离线来源/目标设备身份不能为空".into());
    }
    if source_plan.sector_bytes != target_plan.sector_bytes
        || source_plan.total_sectors != target_plan.total_sectors
    {
        return Err("离线转换禁止改变设备原生扇区大小或整盘容量".into());
    }
    if target_path.starts_with("/dev") || source_path.starts_with("/dev") {
        return Err("原生虚拟转换只接受普通文件，不接受设备路径".into());
    }
    if fs::symlink_metadata(target_path).is_ok() {
        return Err("目标虚拟镜像已存在，禁止覆盖来源或目标文件".into());
    }
    // The source is an independently reopened owned-file snapshot, not a
    // live USB. Verify every owned block before trusting its source mode.
    let source_digest = verify_disk_mode(source_path, source_plan, source_mode, device_id)?;
    // Reject wrong claimed target mode BEFORE creating any output file.
    // The target protocol is rebuilt from its complete native write set.
    let expected_target_digest = validate_protocol_mode(
        &protocol_from_plan(target_plan)?,
        target_plan.total_sectors,
        target_mode,
        device_id,
    )?;
    // New-file-only exporter syncs and verifies every authored full native LBA.
    // This operation never copies source data or preserves a source key.
    export_native_plain_image(target_path, target_plan)?;
    let target_digest = verify_disk_mode(target_path, target_plan, target_mode, device_id)?;
    if target_digest != expected_target_digest {
        return Err("目标镜像重新打开后的协议摘要与规划摘要不一致".into());
    }
    if verify_disk_mode(source_path, source_plan, source_mode, device_id)? != source_digest {
        return Err("来源虚拟盘在目标生成期间发生变化，认证无效".into());
    }
    Ok(NativeVirtualTransitionEvidence {
        source_mode,
        target_mode,
        logical_sector_bytes: source_plan.sector_bytes,
        total_native_sectors: source_plan.total_sectors,
        source_protocol_sha256: source_digest,
        target_protocol_sha256: target_digest,
        target_written_blocks: target_plan.writes.len(),
        destructive_rebuild: true,
    })
}
