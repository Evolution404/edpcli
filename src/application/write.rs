//! Shared backup/restore application service.
//!
//! CLI and TUI must enter raw-disk mutation through this module. The safety chain remains single-source:
//! system-disk/USB whole-disk guard → selector pinning by callers →
//! unmount/lock → reopen identity recheck → atomic write → sync/readback/rollback.

use std::path::{Path, PathBuf};

use super::media_identity::{
    match_media_identity, BackupAffinity, BackupAffinityPolicy, MediaIdentityResumePin,
    MediaIdentitySnapshot, RestoreAuthorizationDecision, RestoreAuthorizationPolicy,
    RestoreGeometryRequirements,
};
use super::media_identity_observer::media_identity_from_protocol_image;
use super::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::common::*;
use crate::diskio::{self, raw_path, Clock, DiskFacts, SectorDev, SystemClock};
use crate::identify::{generate_candidates, identify};
use crate::selectors::{BackupSelector, DeviceSelector};
use crate::sysinfo::{self, CmdRunner};

use super::device::open_readonly_usb_disk;
pub use super::device::{guard_system_disk, guard_usb_disk};
pub use super::Prompter;

const OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// UI-neutral typed progress events emitted by backup/restore application flows.
/// Frontends own text, ANSI and TUI presentation; interactive confirmation remains on `Prompter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteEvent {
    BackupCreated {
        path: PathBuf,
    },
    RestoreMatchesHeader {
        disk: u32,
        onlyid: String,
        count: usize,
    },
    RestoreMatchRow {
        index: usize,
        time: String,
        file_name: String,
    },
    RestoreSelectionRetry {
        message: String,
    },
    BackupShaVerified {
        digest: String,
    },
    RestoreDryRunNotice {
        path: PathBuf,
        disk: u32,
    },
    RestoreTargetHeader {
        path: PathBuf,
    },
    RestoreWriteCompleted,
    PostRestoreAssessment {
        assessment: super::post_restore::PostRestoreAssessment,
    },
}

pub type BackupReport = super::post_restore::MetadataBackupReport;

pub struct Ctx<'a> {
    pub runner: &'a dyn CmdRunner,
    pub clock: &'a dyn Clock,
    pub prompt: &'a mut dyn Prompter,
    pub backup_dir: PathBuf,
}

fn err(code: i32, msg: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, msg)
}

fn backup_identity(
    manifest: &crate::edpb::Manifest,
) -> EdpCliResult<super::media_identity::MediaIdentitySnapshot> {
    crate::edpb::canonical_media_identity(manifest)
        .map_err(|message| err(EXIT_BACKUP, format!("错误: EDPB 身份证据无效: {message}")))
}

fn authorize_restore(
    backup: &MediaIdentitySnapshot,
    target: &MediaIdentitySnapshot,
    backup_tag16: Option<[u8; 16]>,
    target_tag16: &[u8],
    target_lba4_nonzero: bool,
    geometry: RestoreGeometryRequirements,
) -> EdpCliResult<()> {
    let backup_is_plain = matches!(
        backup.protocol.provision_kind,
        Some(crate::provision::DiskProvisionKind::Plain)
    );
    if target_lba4_nonzero
        && !backup_is_plain
        && (target_tag16.iter().all(|byte| *byte == 0)
            || target.protocol.device_id.is_none()
            || target.protocol.provision_kind.is_none())
    {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前盘 LBA4 非零但 EDP 协议身份损坏，拒绝 fallback Plain 或写入",
        ));
    }
    let identity_match = match_media_identity(backup, target, None);
    match RestoreAuthorizationPolicy::evaluate(backup, target, &identity_match, geometry, None) {
        RestoreAuthorizationDecision::Authorized => {}
        RestoreAuthorizationDecision::Reject(reason) => {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: restore authorization 拒绝写入: relationship={:?}, conflict={reason:?}",
                    identity_match.relationship
                ),
            ));
        }
    }
    if target_tag16.iter().any(|byte| *byte != 0) && !backup_is_plain {
        if target.protocol.device_id.is_none() || target.protocol.provision_kind.is_none() {
            return Err(err(
                EXIT_BACKUP,
                "错误: 当前盘 LBA4 非零但 EDP 协议身份损坏，拒绝 fallback Plain 或写入",
            ));
        }
        // Plain media has no EDP LBA4 identity tag/onlyid. Restoring a Plain backup
        // to the same physical device after it has been provisioned as EDP is a valid
        // rollback path. The strong physical-media + exact-geometry policy above is
        // the write grant; do not compare a Plain backup's LBA4 bytes as an EDP tag.
        if !backup_is_plain
            && backup_tag16
                .map(|backup_tag16| backup_tag16.as_slice() != target_tag16)
                .unwrap_or(true)
        {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: EDP 备份身份标签与当前盘不一致(current onlyid={}, backup onlyid={})，拒绝还原",
                    target.protocol.onlyid.as_deref().unwrap_or("未知"),
                    backup.protocol.onlyid.as_deref().unwrap_or("未知")
                ),
            ));
        }
    }
    Ok(())
}

fn backup_protocol_image(
    path: &Path,
    verified: &crate::edpb::VerifiedContainer,
) -> EdpCliResult<Option<Vec<u8>>> {
    let Some(artifact) = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == crate::edpb::RAW_PROTOCOL_ARTIFACT_ID)
    else {
        let plain_v3 = verified.manifest.schema == "edpb.manifest.v3"
            && verified
                .manifest
                .snapshot
                .device_state
                .eq_ignore_ascii_case("plain");
        return if plain_v3 {
            Ok(None)
        } else {
            Err(err(EXIT_BACKUP, "错误: EDPB 缺少 LBA0-12 原始 Artifact"))
        };
    };
    if artifact.restore_policy != crate::edpb::RestorePolicy::Restorable {
        return Err(err(
            EXIT_BACKUP,
            "错误: EDPB 的 LBA0-12 Artifact 未标记为可恢复，拒绝写盘",
        ));
    }
    let data = crate::edpb::read_raw_protocol(path).map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: 读取 EDPB LBA0-12 失败: {message}"),
        )
    })?;
    if data.len() != METADATA_IMAGE_LEN {
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: EDPB LBA0-12 大小 {} ≠ {}",
                data.len(),
                METADATA_IMAGE_LEN
            ),
        ));
    }
    Ok(Some(data))
}

fn build_metadata_restore_plan(
    path: &Path,
    verified: &crate::edpb::VerifiedContainer,
    backup_protocol: Option<&[u8]>,
    target_protocol: &[u8],
    target_device_id: Option<&str>,
    target_total_sectors: u64,
) -> EdpCliResult<diskio::WriteTransactionPlan> {
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

        let bytes = crate::edpb::read_artifact(path, &artifact.id).map_err(|message| {
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
    let plain_v3 = verified.manifest.schema == "edpb.manifest.v3"
        && verified
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
    if plain_v3 && target_has_valid_edp {
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

pub(crate) fn read_image(dev: &mut dyn SectorDev) -> EdpCliResult<Vec<u8>> {
    super::media_identity_observer::read_protocol_image_readonly(dev)
}

/// `reopen_rdwr` 会重新打开平台裸盘设备。确认期间既可能换盘，也可能有别的程序
/// 改动同一块盘的元数据。自动备份保存的是确认前 LBA0-12，因此第一笔写入前必须
/// 再读一次并逐扇区比对，保证“当前状态 == 刚刚备份的状态”。
pub(crate) fn verify_reopened_snapshot(
    dev: &mut dyn SectorDev,
    expected: &[u8],
) -> EdpCliResult<()> {
    if expected.len() != METADATA_IMAGE_LEN {
        return Err(err(EXIT_IO, "错误: 内部预写快照长度异常"));
    }
    // 先核身份，再核其余元数据；换盘时不要被 LBA0 的差异抢先掩盖诊断。
    for lba in
        std::iter::once(4u32).chain((0..METADATA_SECTOR_COUNT as u32).filter(|&lba| lba != 4))
    {
        let actual = dev
            .read_sector(lba)
            .map_err(|e| err(EXIT_IO, format!("错误: 重开后读取 LBA{} 失败: {}", lba, e)))?;
        if actual.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: 重开后 LBA{} 读取 {}B，预期完整扇区 {}B",
                    lba,
                    actual.len(),
                    SECTOR
                ),
            ));
        }
        let start = lba as usize * SECTOR;
        let before = &expected[start..start + SECTOR];
        if actual != before {
            if lba == 4 {
                let expected_id =
                    diskio::lba4_label_id_from(before).unwrap_or_else(|| "未知".into());
                let actual_id =
                    diskio::lba4_label_id_from(&actual).unwrap_or_else(|| "未知".into());
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备身份在确认/卸载期间发生变化(expected onlyid={}, actual onlyid={})，疑似换盘或重枚举，拒绝写入",
                        expected_id, actual_id
                    ),
                ));
            }
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备元数据在备份/确认期间发生变化(LBA{})，拒绝基于过期快照写入",
                    lba
                ),
            ));
        }
    }
    Ok(())
}

/// Revalidate the identity shown by the TUI immediately before starting the
/// selected operation. The write flow still performs its existing fresh
/// snapshot and post-reopen checks; this closes the earlier selection/confirmation
/// window where another USB device could take the same platform disk number.
fn protocol_image_confirms_plain(protocol_image: &[u8], total_sectors: u64) -> bool {
    if protocol_image.len() != METADATA_IMAGE_LEN {
        return false;
    }
    let lba4 = &protocol_image[4 * SECTOR..5 * SECTOR];
    diskio::lba4_label_id_from(lba4).is_none()
        && crate::partition_table::confirmed_plain_protocol_prefix(protocol_image, total_sectors)
}

pub fn verify_expected_identity(
    runner: &dyn CmdRunner,
    disk: u32,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<()> {
    if let Some(expected) = expected_onlyid {
        let raw = dev
            .read_sector(4)
            .map_err(|error| err(EXIT_IO, format!("错误: 身份复核读取 LBA4 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!("错误: 身份复核 LBA4 读取 {}B，预期 {SECTOR}B", raw.len()),
            ));
        }
        let actual = diskio::lba4_label_id_from(&raw);
        if actual.as_deref() != Some(expected) {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备在选择/确认期间发生变化(expected onlyid={expected}, actual onlyid={})，拒绝继续",
                    actual.as_deref().unwrap_or("未知")
                ),
            ));
        }
    }
    if let Some(expected) = expected_device_id {
        let raw = dev
            .read_sector(7)
            .map_err(|error| err(EXIT_IO, format!("错误: 身份复核读取 LBA7 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!("错误: 身份复核 LBA7 读取 {}B，预期 {SECTOR}B", raw.len()),
            ));
        }
        let actual = identify(runner, disk, &raw).device_id;
        if let Some(actual) = actual {
            if actual != expected {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备在选择/确认期间发生变化(expected device_id={expected}, actual device_id={actual})，拒绝继续"
                    ),
                ));
            }
        } else {
            // Plain whole-disk FAT/exFAT/NTFS media may legitimately carry
            // filesystem boot code in LBA4/LBA7. Nonzero bytes alone are not
            // EDP evidence. Preserve fail-closed behavior by requiring the
            // same positive Plain proof used by the canonical media observer
            // whenever LBA4's identity-tag bytes are nonzero.
            let lba4 = dev.read_sector(4).map_err(|error| {
                err(
                    EXIT_IO,
                    format!("错误: Plain 身份复核读取 LBA4 失败: {error}"),
                )
            })?;
            if lba4.len() != SECTOR {
                return Err(err(
                    EXIT_IO,
                    format!(
                        "错误: Plain 身份复核 LBA4 读取 {}B，预期 {SECTOR}B",
                        lba4.len()
                    ),
                ));
            }
            let tag = diskio::lba4_tag16_from(&lba4)
                .ok_or_else(|| err(EXIT_IO, "错误: Plain 身份复核无法读取 LBA4 身份标签"))?;
            if diskio::lba4_label_id_from(&lba4).is_some() {
                return Err(err(
                    EXIT_TARGET,
                    "错误: LBA7 无法识别但 LBA4 仍含有效 EDP onlyid；疑似损坏 EDP，拒绝按 Plain 硬件身份继续",
                ));
            }
            if tag.iter().any(|&byte| byte != 0) {
                let image = read_image(dev)?;
                let total_sectors = sysinfo::disk_total_sectors(runner, disk).ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        "错误: Plain 身份复核无法取得物理总扇区数，拒绝按硬件身份继续",
                    )
                })?;
                if !protocol_image_confirms_plain(&image, total_sectors) {
                    return Err(err(
                        EXIT_TARGET,
                        "错误: LBA7 无法识别且 LBA4 非零，同时缺少可验证 Plain 正向证据；疑似损坏 EDP，拒绝继续",
                    ));
                }
            }
            if !generate_candidates(runner, disk)
                .iter()
                .any(|candidate| candidate == expected)
            {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备在选择/确认期间发生变化(expected Plain device_id={expected}, 当前硬件候选不匹配)，拒绝继续"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn verify_resume_identity_pin(
    runner: &dyn CmdRunner,
    disk: u32,
    expected: &MediaIdentityResumePin,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<()> {
    let fresh = super::media_identity_observer::observe_media_identity_readonly(runner, disk, dev)?;
    expected
        .verify(&fresh.snapshot, &fresh.protocol_image)
        .map_err(|conflict| {
            err(
                EXIT_TARGET,
                format!("错误: TUI resume 目标介质身份 pin 不一致: {conflict:?}"),
            )
        })
}

pub(crate) fn auto_pick_disk(
    runner: &dyn CmdRunner,
    prompt: &mut dyn Prompter,
) -> EdpCliResult<u32> {
    DeviceSelector::new(None).resolve(runner, prompt)
}

/// 为当前已选定 U 盘创建 Metadata 级 EDPB 备份。
///
/// 这是纯只读介质路径：读取身份、LBA0-12、分区关键元数据和盘尾证据，
/// 然后交给 Metadata writer。此函数不得调用
/// prepare_write、reopen_rdwr 或任何扇区写入。
pub fn backup_create_flow(
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<BackupReport> {
    backup_create_level_flow(disk, ctx, dev, false)
}

/// Explicit Deep opt-in; shares the existing read-only device and identity path.
pub fn backup_create_level_flow(
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
    deep: bool,
) -> EdpCliResult<BackupReport> {
    guard_usb_disk(ctx.runner, disk)?;
    let observed =
        super::media_identity_observer::observe_media_identity_readonly(ctx.runner, disk, dev)?;
    let img = observed.protocol_image;
    let identity = observed.snapshot;
    let total_sectors = identity
        .hardware
        .total_sectors
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 无法获取磁盘总扇区数，无法创建备份"))?;

    let path = if let Some(device_id) = identity.protocol.device_id.clone() {
        if identity.protocol.provision_kind.is_none() {
            return Err(err(
                EXIT_BACKUP,
                "错误: 已观察到 EDP device_id，但协议结构不完整；拒绝把损坏状态创建为正常备份",
            ));
        }
        let vid = identity
            .hardware
            .vid
            .map(|value| format!("{value:04x}"))
            .unwrap_or_else(|| "xxxx".into());
        let pid = identity
            .hardware
            .pid
            .map(|value| format!("{value:04x}"))
            .unwrap_or_else(|| "xxxx".into());
        let facts = DiskFacts {
            disk,
            total_sectors: Some(total_sectors),
            vid,
            pid,
            label_id: identity.protocol.onlyid.clone(),
        };
        let acquire = if deep {
            crate::backup_deep::acquire_deep
        } else {
            crate::backup_metadata::acquire_metadata
        };
        let metadata = acquire(dev, &img, &device_id, total_sectors).map_err(|message| {
            err(
                EXIT_BACKUP,
                format!("错误: Metadata 级备份采集失败: {message}"),
            )
        })?;
        let save = if deep {
            diskio::create_deep_backup
        } else {
            diskio::create_metadata_backup
        };
        save(
            &facts,
            &img,
            &device_id,
            metadata,
            &identity,
            &ctx.backup_dir,
            ctx.clock,
        )?
    } else {
        if deep {
            return Err(err(
                EXIT_BACKUP,
                "错误: Plain 盘没有 EDP 分区语义，--deep 备份不可用；请使用普通 backup create",
            ));
        }
        if identity.protocol.provision_kind != Some(crate::provision::DiskProvisionKind::Plain) {
            return Err(err(
                EXIT_BACKUP,
                "错误: 当前介质 LBA4 保留非零/损坏的 EDP 协议身份；拒绝误判为 Plain 备份",
            ));
        }
        let legacy_candidate = identity
            .derived
            .device_id_candidates
            .first()
            .cloned()
            .ok_or_else(|| {
                err(
                    EXIT_BACKUP,
                    "错误: 无法从 Plain 盘 USB/SCSI 硬件信息生成 derived EDP device_id candidate",
                )
            })?;
        let vid = identity
            .hardware
            .vid
            .map(|value| format!("{value:04x}"))
            .ok_or_else(|| err(EXIT_BACKUP, "错误: 无法读取 Plain 盘 USB VID，拒绝创建备份"))?;
        let pid = identity
            .hardware
            .pid
            .map(|value| format!("{value:04x}"))
            .ok_or_else(|| err(EXIT_BACKUP, "错误: 无法读取 Plain 盘 USB PID，拒绝创建备份"))?;
        let facts = DiskFacts {
            disk,
            total_sectors: Some(total_sectors),
            vid,
            pid,
            label_id: None,
        };
        let metadata = crate::backup_metadata::acquire_plain_metadata(dev, total_sectors).map_err(
            |message| {
                err(
                    EXIT_BACKUP,
                    format!("错误: Plain 元数据备份采集失败: {message}"),
                )
            },
        )?;
        diskio::create_plain_backup(
            &facts,
            &img,
            &legacy_candidate,
            metadata,
            &identity,
            &ctx.backup_dir,
            ctx.clock,
        )?
    };
    let verified = crate::edpb::verify_file(&path).map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: 新创建的 EDPB 未通过完整性检查: {message}"),
        )
    })?;
    let report = BackupReport {
        path,
        partition_count: verified.manifest.partitions.len(),
        edp_protocol_saved: verified
            .manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.id == crate::edpb::RAW_PROTOCOL_ARTIFACT_ID),
    };
    ctx.prompt.write_event(WriteEvent::BackupCreated {
        path: report.path.clone(),
    });
    Ok(report)
}

pub fn backup_create_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
    deep: bool,
) -> EdpCliResult<BackupReport> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_expected_identity(runner, disk, expected_onlyid, expected_device_id, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    backup_create_level_flow(disk, &mut ctx, &mut dev, deep)
}

pub fn backup_create_on_disk_with_pin(
    runner: &dyn CmdRunner,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
    deep: bool,
) -> EdpCliResult<BackupReport> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_resume_identity_pin(runner, disk, expected, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    backup_create_level_flow(disk, &mut ctx, &mut dev, deep)
}

/// restore 主流程: bin=None 时交互列出本盘备份并选择。
pub fn restore_flow(
    bin: Option<String>,
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<i32> {
    restore_flow_typed(bin, disk, ctx, dev).map(|_| EXIT_OK)
}

/// Restore only the metadata transaction and return its verified result.
/// The subsequent read-only assessment is reported independently via WriteEvent.
pub fn restore_flow_typed(
    bin: Option<String>,
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<super::post_restore::MetadataRestoreOutcome> {
    let target_session = TargetSession::<ReadOnly>::open_usb(ctx.runner, disk)?;
    let img = read_image(dev)?;
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = diskio::lba4_label_id_from(lba4);
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;
    let target_identity = media_identity_from_protocol_image(ctx.runner, disk, &img)?;
    let target_lba4_nonzero = lba4.iter().any(|byte| *byte != 0);
    let selector = BackupSelector::load(&ctx.backup_dir);
    let path: PathBuf = match bin {
        Some(target) => selector
            .resolve_one(&target)
            .map(|entry| entry.path.clone())
            .map_err(|message| err(EXIT_BACKUP, format!("错误: {message}")))?,
        None => {
            let choices: Vec<_> = selector
                .numbered_with_indices()
                .into_iter()
                .filter(|(_, entry)| {
                    entry
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.identity.as_ref())
                        .is_some_and(|backup| {
                            BackupAffinityPolicy::classify(&match_media_identity(
                                backup,
                                &target_identity,
                                None,
                            )) == BackupAffinity::Confirmed
                        })
                })
                .collect();
            if choices.is_empty() {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: 备份目录未找到本盘备份；可先执行 edpcli backup create",
                ));
            }
            ctx.prompt.write_event(WriteEvent::RestoreMatchesHeader {
                disk,
                onlyid: label_id.clone().unwrap_or_else(|| "未知".into()),
                count: choices.len(),
            });
            for (index, entry) in &choices {
                ctx.prompt.write_event(WriteEvent::RestoreMatchRow {
                    index: *index,
                    time: diskio::backup_display_time(&entry.path, entry.mtime),
                    file_name: entry
                        .path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("<无效文件名>")
                        .to_string(),
                });
            }
            loop {
                let input = ctx.prompt.prompt_line("选择全局备份编号 (回车取消): ");
                let input = input.trim();
                if input.is_empty() {
                    return Err(err(EXIT_CANCELLED, "已取消"));
                }
                match selector.resolve_one(input) {
                    Ok(entry) if choices.iter().any(|(_, choice)| choice.path == entry.path) => {
                        break entry.path.clone();
                    }
                    Ok(_) => ctx.prompt.write_event(WriteEvent::RestoreSelectionRetry {
                        message: "备份编号不在当前介质的匹配候选中".into(),
                    }),
                    Err(message) => ctx
                        .prompt
                        .write_event(WriteEvent::RestoreSelectionRetry { message }),
                }
            }
        }
    };

    // EDPB 自包含校验 + 可恢复 Artifact 预检。
    let verified = crate::edpb::verify_file(&path).map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: EDPB 校验失败 {}: {}", path.display(), message),
        )
    })?;
    let backup_protocol = backup_protocol_image(&path, &verified)?;
    ctx.prompt.write_event(WriteEvent::BackupShaVerified {
        digest: verified.file_sha256.clone(),
    });

    // Protocol tag remains a separate consistency check for EDP backups only; it
    // cannot override the strong physical-media + exact-geometry write grant.
    let backup_tag16 = backup_protocol
        .as_deref()
        .and_then(|data| diskio::lba4_tag16_from(&data[4 * SECTOR..5 * SECTOR]));
    let backup_identity = backup_identity(&verified.manifest)?;
    let current_total_sectors = sysinfo::disk_total_sectors(ctx.runner, disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得当前目标盘总扇区数，拒绝恢复"))?;
    let geometry = RestoreGeometryRequirements {
        total_sectors: current_total_sectors,
        logical_sector_size: SECTOR as u32,
    };
    let transaction = build_metadata_restore_plan(
        &path,
        &verified,
        backup_protocol.as_deref(),
        &img,
        target_identity.protocol.device_id.as_deref(),
        current_total_sectors,
    )?;
    ctx.prompt
        .write_event(WriteEvent::RestoreTargetHeader { path: path.clone() });
    let restore_scope = format!("元数据 {} sectors", transaction.writes().len());
    if !ctx
        .prompt
        .confirm_write_yes(&format!("  → disk{} {}? 输入 YES: ", disk, restore_scope))
    {
        return Err(err(EXIT_CANCELLED, "已取消"));
    }
    authorize_restore(
        &backup_identity,
        &target_identity,
        backup_tag16,
        &tag16,
        target_lba4_nonzero,
        geometry,
    )?;
    let target_session = target_session
        .prepare_write()
        .map_err(|e| err(EXIT_IO, format!("错误: 无法卸载 disk{}: {}", disk, e)))?;
    let _target_session = target_session
        .reopen_and_verify(dev, OPEN_WAIT, |dev| {
            verify_reopened_snapshot(dev, &img)?;
            let fresh = super::media_identity_observer::observe_media_identity_readonly(
                ctx.runner, disk, dev,
            )?;
            authorize_restore(
                &backup_identity,
                &fresh.snapshot,
                backup_tag16,
                &tag16,
                target_lba4_nonzero,
                geometry,
            )
        })
        .map_err(|error| match error {
            ReopenAndVerifyError::Reopen(e) => err(
                EXIT_IO,
                format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e),
            ),
            ReopenAndVerifyError::Verify(error) => error,
        })?;
    diskio::execute_write_transaction(dev, &transaction)?;
    let report = super::post_restore::MetadataRestoreReport {
        metadata_restored: true,
        readback_verified: true,
        restored_artifact_ids: verified
            .manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.restore_policy == crate::edpb::RestorePolicy::Restorable)
            .map(|artifact| artifact.id.clone())
            .collect(),
    };
    ctx.prompt.write_event(WriteEvent::RestoreWriteCompleted);
    let format_target_pin =
        super::media_identity_observer::observe_media_identity_readonly(ctx.runner, disk, dev)
            .ok()
            .map(|observed| {
                let pin = crate::media_identity::MediaIdentityPin::new(
                    observed.snapshot,
                    &observed.protocol_image,
                );
                MediaIdentityResumePin::from_pin(&pin)
            });
    let assessment = super::post_restore::assess_partitions_readonly(
        dev,
        &verified.manifest.snapshot.device_state,
        &verified.manifest.device.device_id,
        current_total_sectors,
        &verified.manifest.partitions,
    )
    .unwrap_or_else(|error| {
        super::post_restore::PostRestoreAssessment::unsupported(
            &verified.manifest.partitions,
            format!("恢复后只读检查失败: {error}"),
        )
    });
    ctx.prompt.write_event(WriteEvent::PostRestoreAssessment {
        assessment: assessment.clone(),
    });
    Ok(super::post_restore::MetadataRestoreOutcome {
        report,
        assessment,
        partitions: verified.manifest.partitions.clone(),
        device_state: verified.manifest.snapshot.device_state.clone(),
        device_id: verified.manifest.device.device_id.clone(),
        total_sectors: current_total_sectors,
        format_target_pin,
    })
}

pub fn restore_on_disk_typed(
    runner: &dyn CmdRunner,
    bin: Option<String>,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
) -> EdpCliResult<super::post_restore::MetadataRestoreOutcome> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_expected_identity(runner, disk, expected_onlyid, expected_device_id, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    restore_flow_typed(bin, disk, &mut ctx, &mut dev)
}

pub fn restore_on_disk(
    runner: &dyn CmdRunner,
    bin: Option<String>,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
) -> EdpCliResult<i32> {
    restore_on_disk_typed(
        runner,
        bin,
        disk,
        backup_dir,
        prompt,
        expected_onlyid,
        expected_device_id,
    )
    .map(|_| EXIT_OK)
}

pub fn restore_on_disk_with_pin(
    runner: &dyn CmdRunner,
    bin: Option<String>,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
) -> EdpCliResult<i32> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_resume_identity_pin(runner, disk, expected, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    restore_flow(bin, disk, &mut ctx, &mut dev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct ShortSectorDev;

    impl SectorDev for ShortSectorDev {
        fn read_sector(&mut self, _lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![0u8; SECTOR - 1])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_rejects_short_sector_without_panicking() {
        let error = read_image(&mut ShortSectorDev).unwrap_err();
        assert_eq!(error.code, EXIT_IO);
        assert!(error.msg.contains("512B"), "{}", error.msg);
    }

    fn ntfs_plain_protocol_image(total_sectors: u64) -> Vec<u8> {
        let mut image = vec![0u8; METADATA_IMAGE_LEN];
        let boot = &mut image[..SECTOR];
        boot[..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
        boot[13] = 8;
        boot[21] = 0xf8;
        boot[40..48].copy_from_slice(&(total_sectors - 1).to_le_bytes());
        boot[48..56].copy_from_slice(&4u64.to_le_bytes());
        boot[56..64].copy_from_slice(&8u64.to_le_bytes());
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);
        image
    }

    #[test]
    fn plain_confirmation_allows_nonzero_lba4_filesystem_code() {
        let total = 30_277_632u64;
        let mut image = ntfs_plain_protocol_image(total);
        image[4 * SECTOR..4 * SECTOR + 16].copy_from_slice(&[
            0x66, 0x61, 0x90, 0x1f, 0x07, 0xc3, 0x06, 0x1e, 0x66, 0x60, 0x66, 0xb8, 1, 0, 0, 0,
        ]);
        assert!(protocol_image_confirms_plain(&image, total));
    }

    #[test]
    fn plain_confirmation_rejects_edp_onlyid_even_with_valid_filesystem_boot() {
        let total = 30_277_632u64;
        let mut image = ntfs_plain_protocol_image(total);
        image[4 * SECTOR..4 * SECTOR + 9].copy_from_slice(b"$$$123$$$");
        assert!(!protocol_image_confirms_plain(&image, total));
    }

    fn restore_test_hardware(
        serial: &str,
    ) -> super::super::media_identity::HardwareIdentityEvidence {
        let raw_serial = serial.to_string();
        let serial = super::super::media_identity::serial_digest_evidence(Some(serial));
        super::super::media_identity::HardwareIdentityEvidence {
            vid: Some(0x2bdf),
            pid: Some(0x0300),
            serial: Some(raw_serial),
            serial_sha256: serial.sha256,
            serial_quality: serial.quality,
            vendor: Some("HIKSEMI".into()),
            product: Some("HIKSEMI".into()),
            revision: Some("1.00".into()),
            transport: Some(crate::platform::NativeTransport::Uas),
            total_sectors: Some(245_760_000),
            logical_sector_size: Some(SECTOR as u32),
        }
    }

    fn restore_test_plain(serial: &str) -> MediaIdentitySnapshot {
        MediaIdentitySnapshot::plain(
            restore_test_hardware(serial),
            super::super::media_identity::DerivedProtocolEvidence::default(),
            super::super::media_identity::IdentityObservation::default(),
        )
    }

    fn restore_test_edp(serial: &str, onlyid: &str) -> MediaIdentitySnapshot {
        MediaIdentitySnapshot {
            hardware: restore_test_hardware(serial),
            protocol: super::super::media_identity::ProtocolIdentityEvidence {
                device_id: Some("disk&ven_hiksemi&prod_".into()),
                onlyid: Some(onlyid.into()),
                provision_kind: Some(crate::provision::DiskProvisionKind::Mode0),
                lba4_identity_digest: Some(format!("digest-{onlyid}")),
            },
            derived: super::super::media_identity::DerivedProtocolEvidence::default(),
            observation: super::super::media_identity::IdentityObservation::default(),
        }
    }

    #[test]
    fn plain_backup_can_restore_same_physical_device_after_edp_provisioning() {
        let backup = restore_test_plain("HIKSEMI-SERIAL-001");
        let target = restore_test_edp("HIKSEMI-SERIAL-001", "914806819");
        let geometry = RestoreGeometryRequirements {
            total_sectors: 245_760_000,
            logical_sector_size: SECTOR as u32,
        };
        let backup_tag = [0u8; 16];
        let target_tag = *b"$$$914806819$$$";
        authorize_restore(
            &backup,
            &target,
            Some(backup_tag),
            &target_tag,
            true,
            geometry,
        )
        .expect("same physical media Plain rollback must be authorized");
    }

    #[test]
    fn edp_backup_tag_mismatch_remains_rejected() {
        let backup = restore_test_edp("HIKSEMI-SERIAL-001", "111111111");
        let target = restore_test_edp("HIKSEMI-SERIAL-001", "914806819");
        let geometry = RestoreGeometryRequirements {
            total_sectors: 245_760_000,
            logical_sector_size: SECTOR as u32,
        };
        let error = authorize_restore(
            &backup,
            &target,
            Some(*b"$$$111111111$$$\0"),
            b"$$$914806819$$$\0",
            true,
            geometry,
        )
        .unwrap_err();
        assert!(error.msg.contains("身份标签与当前盘不一致"));
    }

    struct PatternSectorDev;

    impl SectorDev for PatternSectorDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![lba as u8; SECTOR])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_preserves_lba_zero_to_twelve_order() {
        let image = read_image(&mut PatternSectorDev).unwrap();
        assert_eq!(image.len(), METADATA_IMAGE_LEN);
        for lba in 0..METADATA_SECTOR_COUNT {
            assert!(
                image[lba * SECTOR..(lba + 1) * SECTOR]
                    .iter()
                    .all(|&byte| byte == lba as u8),
                "LBA{} 在拼接镜像中的位置错误",
                lba
            );
        }
    }
}
