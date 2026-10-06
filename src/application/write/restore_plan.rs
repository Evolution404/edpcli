//! Verified metadata extents to a checked write transaction.
use super::*;

pub(super) fn build_metadata_restore_plan(
    reader: &crate::edpb::VerifiedBackupReader,
    backup_protocol: Option<&[u8]>,
    target_protocol: &[u8],
    target_device_id: Option<&str>,
    target_total_sectors: u64,
) -> EdpCliResult<diskio::WriteTransactionPlan> {
    let verified = reader.verified();
    let mut transaction = diskio::WriteTransactionPlan::new(target_total_sectors);
    for artifact in verified
        .manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.restore_policy == crate::edpb::RestorePolicy::Restorable)
    {
        if artifact.kind != "raw_sectors" {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: 可恢复 Artifact {} 不是 raw_sectors，拒绝写盘",
                    artifact.id
                ),
            ));
        }
        if artifact.source_extent_ids.len() != 1 {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: 可恢复 Artifact {} 必须且只能引用一个 Extent",
                    artifact.id
                ),
            ));
        }
        let extent = verified
            .manifest
            .extents
            .iter()
            .find(|extent| extent.id == artifact.source_extent_ids[0])
            .ok_or_else(|| {
                err(
                    EXIT_BACKUP,
                    format!("错误: Artifact {} 引用的 Extent 不存在", artifact.id),
                )
            })?;
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or_else(|| err(EXIT_BACKUP, "错误: 恢复 Extent LBA 范围溢出"))?;
        if extent.sector_count == 0 || end > target_total_sectors {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: Artifact {} 的恢复范围 {}+{} 超过目标盘几何",
                    artifact.id, extent.start_lba, extent.sector_count
                ),
            ));
        }

        if artifact.id == "raw.lba7_compatibility" {
            let protocol = backup_protocol
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 可恢复 LCE 缺少配套 LBA0-12 协议元数据"))?;
            let backup_total = verified
                .manifest
                .geometry
                .total_sectors
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 可恢复 LCE 的 EDPB 缺少总扇区数"))?;
            let geometry = crate::backup_metadata::parse_lba7_compatibility_geometry(
                protocol,
                &verified.manifest.device.device_id,
                backup_total,
            )
            .map_err(|message| {
                err(
                    EXIT_BACKUP,
                    format!("错误: 无法从备份 LBA7 复核 LCE 指针: {message}"),
                )
            })?;
            if extent.start_lba != geometry.start_lba
                || extent.sector_count != geometry.sector_count
            {
                return Err(err(
                    EXIT_BACKUP,
                    format!(
                        "错误: EDPB LCE Extent 与备份 LBA7 指针不一致(manifest={}+{}, lba7={}+{})",
                        extent.start_lba,
                        extent.sector_count,
                        geometry.start_lba,
                        geometry.sector_count
                    ),
                ));
            }
        }
        if artifact.id == "raw.tail.metadata_mirror_512k" {
            let expected_start = target_total_sectors
                .checked_sub(crate::backup_metadata::TAIL_METADATA_MIRROR_OFFSET_SECTORS)
                .ok_or_else(|| {
                    err(
                        EXIT_BACKUP,
                        "错误: 目标盘过小，无法恢复盘尾 metadata mirror",
                    )
                })?;
            if extent.start_lba != expected_start
                || extent.sector_count != crate::backup_metadata::TAIL_METADATA_MIRROR_SECTORS
            {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: 盘尾 metadata mirror Extent 与目标几何不一致",
                ));
            }
        }
        if artifact.id == "raw.tail.restore_node_end4" {
            let expected_start = target_total_sectors
                .checked_sub(crate::backup_metadata::TAIL_END4_MIRROR_OFFSET_SECTORS)
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 目标盘过小，无法恢复盘尾 restore node"))?;
            if extent.start_lba != expected_start || extent.sector_count != 1 {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: 盘尾 restore node Extent 与目标几何不一致",
                ));
            }
        }

        let bytes = reader.read_artifact(&artifact.id).map_err(|message| {
            err(
                EXIT_BACKUP,
                format!("错误: 读取 {} 失败: {message}", artifact.id),
            )
        })?;
        let expected_len = usize::try_from(extent.sector_count)
            .ok()
            .and_then(|count| count.checked_mul(SECTOR))
            .ok_or_else(|| err(EXIT_BACKUP, "错误: 恢复 Artifact 长度溢出"))?;
        if bytes.len() != expected_len {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: Artifact {} 数据长度 {}B ≠ {}B",
                    artifact.id,
                    bytes.len(),
                    expected_len
                ),
            ));
        }
        for offset in 0..extent.sector_count {
            let lba64 = extent
                .start_lba
                .checked_add(offset)
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 恢复 LBA 溢出"))?;
            let lba = u32::try_from(lba64)
                .map_err(|_| err(EXIT_BACKUP, "错误: 恢复 LBA 超出当前写入器范围"))?;
            let start = usize::try_from(offset)
                .ok()
                .and_then(|index| index.checked_mul(SECTOR))
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 恢复字节偏移溢出"))?;
            let stage = if lba == 0 {
                diskio::SectorWriteStage::Commit
            } else if lba <= METADATA_LAST_LBA {
                diskio::SectorWriteStage::Metadata
            } else {
                diskio::SectorWriteStage::Data
            };
            transaction
                .insert(
                    lba,
                    bytes[start..start + SECTOR].to_vec(),
                    stage,
                    format!("metadata restore {}", artifact.id),
                )
                .map_err(|message| {
                    err(
                        EXIT_BACKUP,
                        format!("错误: Metadata Restore 计划无效: {message}"),
                    )
                })?;
        }
    }
    let plain = verified
        .manifest
        .snapshot
        .device_state
        .eq_ignore_ascii_case("plain");
    let target_has_valid_edp = target_protocol.len() == METADATA_IMAGE_LEN
        && target_device_id
            .and_then(|device_id| {
                crate::provision::DiskProvisionKind::from_metadata(target_protocol, device_id)
            })
            .is_some();
    if plain && target_has_valid_edp {
        for lba in 1..=METADATA_LAST_LBA {
            if lba == 3 || transaction.writes().contains_key(&lba) {
                continue;
            }
            transaction
                .insert(
                    lba,
                    vec![0u8; SECTOR],
                    diskio::SectorWriteStage::Metadata,
                    "plain restore stale EDP protocol cleanup",
                )
                .map_err(|message| {
                    err(
                        EXIT_BACKUP,
                        format!("错误: Plain restore EDP 清理计划无效: {message}"),
                    )
                })?;
        }
    }
    if transaction.writes().is_empty() {
        return Err(err(EXIT_BACKUP, "错误: EDPB 没有任何可恢复元数据 Artifact"));
    }
    Ok(transaction)
}
