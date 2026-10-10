//! Read-only same-geometry restore *preview* for validated native EDPB v4.
//! Evidence-only remains evidence-only. This preview neither upgrades the
//! manifest's restore rights nor yields a media write grant or writable plan.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::domain::hardware::NativeReadGeometry;
use crate::edpb::{ArtifactCompleteness, RestorePolicy, VerifiedBackupReader};

/// The exact, fully native block ranges that a future approved restore must
/// account for. LBA0 is the final prospective commit block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRestoreReadOnlyPreview {
    pub device_id: String,
    pub logical_sector_bytes: u32,
    pub total_sectors: u64,
    pub backup_sha256: String,
    pub proposed_write_sha256: String,
    pub proposed_lbas_in_write_order: Vec<u64>,
    pub native_lce_start: u64,
    pub native_lce_blocks: u64,
}

/// Verify a v4 source and project complete native block evidence without ever
/// opening a disk, converting v4 evidence to Restorable, or taking a write lease.
/// `expected_device_id` and `target_geometry` are assertions supplied by
/// a caller; actual hardware identity must be checked separately.
pub fn plan_native_restore_readonly(
    backup_path: &Path,
    expected_device_id: &str,
    target_geometry: NativeReadGeometry,
) -> Result<NativeRestoreReadOnlyPreview, String> {
    let reader = VerifiedBackupReader::open(backup_path)?;
    let verified = reader.verified();
    let manifest = &verified.manifest;
    let sector_bytes = manifest.geometry.logical_sector_size;
    if manifest.schema != "edpb.manifest.v4"
        || !matches!(sector_bytes, 1024 | 2048 | 4096)
        || manifest.snapshot.device_state.eq_ignore_ascii_case("plain")
        || manifest.geometry.total_sectors != Some(target_geometry.native_sector_count)
        || manifest.geometry.capacity_bytes != Some(target_geometry.capacity_bytes)
        || target_geometry.logical_sector_bytes != sector_bytes
    {
        return Err("EDPB v4来源与目标不是相同的标准原生几何，禁止恢复规划".into());
    }
    if expected_device_id.is_empty() || manifest.device.device_id != expected_device_id {
        return Err("EDPB原生恢复预览的来源设备身份与目标不一致".into());
    }

    let mut blocks = BTreeMap::<u64, Vec<u8>>::new();
    let mut lce_extent = None;
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|item| item.kind == "raw_sectors")
    {
        if artifact.restore_policy != RestorePolicy::EvidenceOnly
            || artifact.completeness != ArtifactCompleteness::Complete
        {
            return Err(format!("来源证据{}不完整或意外获得恢复权限", artifact.id));
        }
        let [extent_id] = artifact.source_extent_ids.as_slice() else {
            return Err(format!("来源证据{}未绑定唯一原生范围", artifact.id));
        };
        let extent = manifest
            .extents
            .iter()
            .find(|extent| &extent.id == extent_id)
            .ok_or_else(|| format!("来源证据{}范围丢失", artifact.id))?;
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .filter(|end| *end <= target_geometry.native_sector_count)
            .ok_or("来源证据范围超出目标盘")?;
        if end == extent.start_lba {
            return Err("来源证据范围为空".into());
        }
        let bytes = reader.read_artifact(&artifact.id)?;
        let required = usize::try_from(extent.sector_count)
            .ok()
            .and_then(|count| count.checked_mul(sector_bytes as usize))
            .ok_or("来源证据字节长度溢出")?;
        if bytes.len() != required {
            return Err(format!("来源证据{}原生块长度不完整", artifact.id));
        }
        if artifact.id == "raw.lba7_compatibility" {
            lce_extent = Some((extent.start_lba, extent.sector_count));
        }
        for (offset, block) in bytes.chunks_exact(sector_bytes as usize).enumerate() {
            let lba = extent.start_lba + offset as u64;
            if blocks.insert(lba, block.to_vec()).is_some() {
                return Err(format!("原生LBA{lba}重复，拒绝恢复预览"));
            }
        }
    }
    if !(0..13u64).all(|lba| blocks.contains_key(&lba)) {
        return Err("EDPB恢复预览缺失完整原生LBA0–12".into());
    }
    let (native_lce_start, native_lce_blocks) = lce_extent.ok_or("EDPB恢复预览缺失完整原生LCE")?;
    // Preserve every byte of the native LCE block, including unowned tails.
    let mut ordered_lbas = blocks
        .keys()
        .copied()
        .filter(|lba| *lba != 0)
        .collect::<Vec<_>>();
    ordered_lbas.push(0);
    let mut digest = Sha256::new();
    for lba in &ordered_lbas {
        digest.update(lba.to_le_bytes());
        digest.update(blocks.get(lba).expect("all proposed LBAs are present"));
    }
    Ok(NativeRestoreReadOnlyPreview {
        device_id: expected_device_id.into(),
        logical_sector_bytes: sector_bytes,
        total_sectors: target_geometry.native_sector_count,
        backup_sha256: verified.file_sha256.clone(),
        proposed_write_sha256: digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        proposed_lbas_in_write_order: ordered_lbas,
        native_lce_start,
        native_lce_blocks,
    })
}
