//! Shared backup/restore application service.
//!
//! CLI and TUI must enter raw-disk mutation through this module. The safety chain remains single-source:
//! system-disk/USB whole-disk guard → selector pinning by callers →
//! unmount/lock → reopen identity recheck → atomic write → sync/readback/rollback.

use std::path::PathBuf;

use sha2::{Digest, Sha256};

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
    BackupCreatedIsNopwd,
    RestoreMatchesHeader {
        disk: u32,
        onlyid: String,
        count: usize,
    },
    RestoreMatchRow {
        index: usize,
        time: String,
        is_nopwd: bool,
        file_name: String,
    },
    RestoreSelectionRetry {
        message: String,
    },
    BackupShaVerified {
        digest: String,
    },
    RestoreSnapshotNopwdWarning,
    RestoreDryRunNotice {
        path: PathBuf,
        disk: u32,
    },
    RestoreTargetHeader {
        path: PathBuf,
    },
    RestoreWriteCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub path: PathBuf,
    pub is_nopwd: bool,
}

pub struct Ctx<'a> {
    pub runner: &'a dyn CmdRunner,
    pub clock: &'a dyn Clock,
    pub prompt: &'a mut dyn Prompter,
    pub backup_dir: PathBuf,
}

fn err(code: i32, msg: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, msg)
}

const HARDWARE_SERIAL_NOTE_PREFIX: &str = "hardware_serial_sha256=";

fn hardware_serial_digest(serial: &str) -> Option<String> {
    let serial = serial.trim();
    if serial.is_empty() {
        return None;
    }
    Some(format!("{:x}", Sha256::digest(serial.as_bytes())))
}

fn hardware_serial_note(serial: &str) -> Option<String> {
    hardware_serial_digest(serial).map(|digest| format!("{HARDWARE_SERIAL_NOTE_PREFIX}{digest}"))
}

fn manifest_hardware_serial_digest(manifest: &crate::edpb::Manifest) -> EdpCliResult<&str> {
    let mut matches = manifest
        .provenance
        .notes
        .iter()
        .filter_map(|note| note.strip_prefix(HARDWARE_SERIAL_NOTE_PREFIX));
    let Some(digest) = matches.next() else {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前盘 LBA4 身份已清空，而该备份没有硬件序列号绑定；为避免误写到另一块同型号盘，拒绝还原",
        ));
    };
    if matches.next().is_some()
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(err(
            EXIT_BACKUP,
            "错误: EDPB 硬件序列号绑定损坏或存在冲突，拒绝还原",
        ));
    }
    Ok(digest)
}

fn verify_hardware_bound_restore(
    runner: &dyn CmdRunner,
    disk: u32,
    manifest: &crate::edpb::Manifest,
) -> EdpCliResult<()> {
    let expected_serial = manifest_hardware_serial_digest(manifest)?;
    let current_serial = runner
        .hardware_serial(disk)
        .and_then(|serial| hardware_serial_digest(&serial))
        .ok_or_else(|| {
            err(
                EXIT_BACKUP,
                "错误: 当前盘 LBA4 身份已清空且无法读取 USB 硬件序列号，拒绝还原",
            )
        })?;
    if current_serial != expected_serial {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前 USB 硬件序列号与备份绑定不一致，拒绝还原",
        ));
    }

    let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
    if !vid.eq_ignore_ascii_case(&manifest.device.vid)
        || !pid.eq_ignore_ascii_case(&manifest.device.pid)
    {
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: 当前 USB VID/PID {}:{} 与备份 {}:{} 不一致，拒绝还原",
                vid, pid, manifest.device.vid, manifest.device.pid
            ),
        ));
    }

    let candidates = generate_candidates(runner, disk);
    if !candidates
        .iter()
        .any(|candidate| candidate == &manifest.device.device_id)
    {
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: 当前 USB/SCSI 硬件身份不能生成备份 device_id={}，拒绝还原",
                manifest.device.device_id
            ),
        ));
    }
    Ok(())
}

pub(crate) fn read_image(dev: &mut dyn SectorDev) -> EdpCliResult<Vec<u8>> {
    let mut img = Vec::with_capacity(METADATA_IMAGE_LEN);
    // 镜像布局必须严格保持 LBA0→12 的连续顺序；后续所有固定偏移都依赖此契约。
    for lba in 0..METADATA_SECTOR_COUNT as u32 {
        let sector = dev
            .read_sector(lba)
            .map_err(|e| err(EXIT_IO, format!("错误: {}", e)))?;
        if sector.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: LBA{} 读取 {}B，预期完整扇区 {}B",
                    lba,
                    sector.len(),
                    SECTOR
                ),
            ));
        }
        img.extend_from_slice(&sector);
    }
    Ok(img)
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
        if actual.as_deref() != Some(expected) {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备在选择/确认期间发生变化(expected device_id={expected}, actual device_id={})，拒绝继续",
                    actual.as_deref().unwrap_or("未知")
                ),
            ));
        }
    }
    Ok(())
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
    let img = read_image(dev)?;
    let total_sectors = sysinfo::disk_total_sectors(ctx.runner, disk)
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 无法获取磁盘总扇区数，无法创建备份"))?;
    let id = identify(ctx.runner, disk, &img[7 * SECTOR..8 * SECTOR]);
    let serial_note = ctx
        .runner
        .hardware_serial(disk)
        .and_then(|serial| hardware_serial_note(&serial));

    let (path, is_nopwd) = if let Some(device_id) = id.device_id {
        let (vid, pid) = sysinfo::usb_vid_pid(ctx.runner, disk);
        let facts = DiskFacts {
            disk,
            total_sectors: Some(total_sectors),
            vid,
            pid,
            label_id: diskio::lba4_label_id_from(&img[4 * SECTOR..5 * SECTOR]),
        };
        let acquire = if deep {
            crate::backup_deep::acquire_deep
        } else {
            crate::backup_metadata::acquire_metadata
        };
        let mut metadata = acquire(dev, &img, &device_id, total_sectors).map_err(|message| {
            err(
                EXIT_BACKUP,
                format!("错误: Metadata 级备份采集失败: {message}"),
            )
        })?;
        if let Some(note) = serial_note {
            metadata.notes.push(note);
        }
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
        let lba4_tag = diskio::lba4_tag16_from(&img[4 * SECTOR..5 * SECTOR])
            .ok_or_else(|| err(EXIT_BACKUP, "错误: Plain 备份读取 LBA4 身份标签失败"))?;
        if lba4_tag.iter().any(|&byte| byte != 0) {
            return Err(err(
                EXIT_BACKUP,
                "错误: LBA7 无法识别但 LBA4 仍保留 EDP 协议身份；疑似协议损坏，拒绝误判为 Plain 备份",
            ));
        }
        let device_id = generate_candidates(ctx.runner, disk)
            .into_iter()
            .next()
            .ok_or_else(|| {
                err(
                    EXIT_BACKUP,
                    "错误: 无法从 Plain 盘 USB/SCSI 硬件信息生成 device_id 候选",
                )
            })?;
        if crate::provision::DiskProvisionKind::from_metadata(&img, &device_id)
            != crate::provision::DiskProvisionKind::Plain
        {
            return Err(err(
                EXIT_BACKUP,
                "错误: LBA7 无法识别但当前介质仍呈现 EDP 模式，拒绝按 Plain 备份",
            ));
        }
        let note = serial_note.ok_or_else(|| {
            err(
                EXIT_BACKUP,
                "错误: Plain 盘缺少 LBA4 协议身份且无法读取 USB 硬件序列号，拒绝创建无法安全恢复的备份",
            )
        })?;
        let (vid, pid) = sysinfo::usb_vid_pid(ctx.runner, disk);
        if vid == "xxxx" || pid == "xxxx" {
            return Err(err(
                EXIT_BACKUP,
                "错误: 无法读取 Plain 盘 USB VID/PID，拒绝创建无法安全归属的备份",
            ));
        }
        let facts = DiskFacts {
            disk,
            total_sectors: Some(total_sectors),
            vid,
            pid,
            label_id: None,
        };
        diskio::create_plain_backup(
            &facts,
            &img,
            &device_id,
            &[note],
            &ctx.backup_dir,
            ctx.clock,
        )?
    };
    let report = BackupReport { path, is_nopwd };
    ctx.prompt.write_event(WriteEvent::BackupCreated {
        path: report.path.clone(),
    });
    if report.is_nopwd {
        ctx.prompt.write_event(WriteEvent::BackupCreatedIsNopwd);
    }
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

/// restore 主流程: bin=None 时交互列出本盘备份并选择。
pub fn restore_flow(
    bin: Option<String>,
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<i32> {
    let target_session = TargetSession::<ReadOnly>::open_usb(ctx.runner, disk)?;
    let img = read_image(dev)?;
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = diskio::lba4_label_id_from(lba4);
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;
    let selector = BackupSelector::load(&ctx.backup_dir);
    let path: PathBuf = match (bin, label_id.as_deref()) {
        (Some(target), Some(onlyid)) => selector
            .resolve_restore_target(&target, onlyid)
            .map(|entry| entry.path.clone())
            .map_err(|message| err(EXIT_BACKUP, format!("错误: {message}")))?,
        (Some(target), None) => selector
            .resolve_one(&target)
            .map(|entry| entry.path.clone())
            .map_err(|message| err(EXIT_BACKUP, format!("错误: {message}")))?,
        (None, Some(onlyid)) => {
            let view = selector.for_onlyid(onlyid);
            let choices = view.numbered_with_indices();
            if choices.is_empty() {
                return Err(err(
                    EXIT_BACKUP,
                    format!(
                        "错误: 备份目录未找到本盘备份 (onlyid={}); 可先执行 edpcli backup create",
                        onlyid
                    ),
                ));
            }
            ctx.prompt.write_event(WriteEvent::RestoreMatchesHeader {
                disk,
                onlyid: onlyid.to_string(),
                count: choices.len(),
            });
            for (index, entry) in &choices {
                ctx.prompt.write_event(WriteEvent::RestoreMatchRow {
                    index: *index,
                    time: diskio::backup_display_time(&entry.path, entry.mtime),
                    is_nopwd: entry.is_nopwd,
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
                match view.resolve_one(input) {
                    Ok(entry) => break entry.path.clone(),
                    Err(message) => ctx
                        .prompt
                        .write_event(WriteEvent::RestoreSelectionRetry { message }),
                }
            }
        }
        (None, None) => {
            return Err(err(
                EXIT_BACKUP,
                "错误: 当前盘无法读取 onlyid，无法安全筛选可恢复备份，拒绝交互还原",
            ));
        }
    };

    // EDPB 自包含校验 + 可恢复 Artifact 预检。
    let verified = crate::edpb::verify_file(&path).map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: EDPB 校验失败 {}: {}", path.display(), message),
        )
    })?;
    let raw_artifact = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == crate::edpb::RAW_PROTOCOL_ARTIFACT_ID)
        .ok_or_else(|| err(EXIT_BACKUP, "错误: EDPB 缺少 LBA0-12 原始 Artifact"))?;
    if raw_artifact.restore_policy != crate::edpb::RestorePolicy::Restorable {
        return Err(err(
            EXIT_BACKUP,
            "错误: EDPB 的 LBA0-12 Artifact 未标记为可恢复，拒绝写盘",
        ));
    }
    let data = crate::edpb::read_raw_protocol(&path).map_err(|message| {
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
    ctx.prompt.write_event(WriteEvent::BackupShaVerified {
        digest: verified.file_sha256.clone(),
    });

    // 非零 LBA4 继续以原始 16B 协议身份终验；Plain/LBA4=0 仅允许走强硬件绑定终验。
    let backup_lba4 = &data[4 * SECTOR..5 * SECTOR];
    let backup_tag16 = diskio::lba4_tag16_from(backup_lba4)
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 备份 LBA4 缺少 16B 身份标签"))?;
    if tag16.iter().any(|&b| b != 0) {
        if backup_tag16 != tag16 {
            let current_id = label_id.as_deref().unwrap_or("未知");
            let backup_id =
                diskio::lba4_label_id_from(backup_lba4).unwrap_or_else(|| "未知".into());
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: 备份属于另一块盘(current onlyid={}, backup onlyid={})，拒绝还原",
                    current_id, backup_id
                ),
            ));
        }
    } else {
        verify_hardware_bound_restore(ctx.runner, disk, &verified.manifest)?;
    }
    let current_total_sectors = sysinfo::disk_total_sectors(ctx.runner, disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得当前目标盘总扇区数，拒绝恢复"))?;
    if let Some(backup_total_sectors) = verified.manifest.geometry.total_sectors {
        if backup_total_sectors != current_total_sectors {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 备份容量 {} sectors 与当前盘 {} sectors 不一致，拒绝恢复",
                    backup_total_sectors, current_total_sectors
                ),
            ));
        }
    }

    // Deep EDPB may carry the active six-sector LBA7 compatibility extent.
    // It is restorable only when its Manifest extent is bound back to the exact
    // LBA7 pointer encoded in the same backup protocol image.
    let restorable_lce = if let Some(artifact) = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "raw.lba7_compatibility")
    {
        if artifact.restore_policy == crate::edpb::RestorePolicy::Restorable {
            if artifact.source_extent_ids.len() != 1 {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: EDPB LCE Artifact 必须且只能引用一个 Extent",
                ));
            }
            let extent = verified
                .manifest
                .extents
                .iter()
                .find(|extent| extent.id == artifact.source_extent_ids[0])
                .ok_or_else(|| err(EXIT_BACKUP, "错误: EDPB LCE Artifact 引用的 Extent 不存在"))?;
            let backup_total = verified
                .manifest
                .geometry
                .total_sectors
                .ok_or_else(|| err(EXIT_BACKUP, "错误: 可恢复 LCE 的 EDPB 缺少总扇区数"))?;
            let geometry = crate::backup_metadata::parse_lba7_compatibility_geometry(
                &data,
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
            let bytes =
                crate::edpb::read_artifact(&path, "raw.lba7_compatibility").map_err(|message| {
                    err(EXIT_BACKUP, format!("错误: 读取 EDPB LCE 失败: {message}"))
                })?;
            Some((geometry, bytes))
        } else {
            None
        }
    } else {
        None
    };
    let tagged_nopwd = verified.manifest.snapshot.device_state == "passwordless";
    let nopwd_snap =
        tagged_nopwd || diskio::image_is_nopwd(&data, &verified.manifest.device.device_id);
    if nopwd_snap {
        ctx.prompt
            .write_event(WriteEvent::RestoreSnapshotNopwdWarning);
        ctx.prompt.write_event(WriteEvent::RestoreDryRunNotice {
            path: path.clone(),
            disk,
        });
        return Ok(EXIT_OK);
    }
    ctx.prompt
        .write_event(WriteEvent::RestoreTargetHeader { path: path.clone() });
    let restore_scope = if restorable_lce.is_some() {
        "LBA0-12 + LCE"
    } else {
        "LBA0-12"
    };
    if !ctx
        .prompt
        .confirm_write_yes(&format!("  → disk{} {}? 输入 YES: ", disk, restore_scope))
    {
        return Err(err(EXIT_CANCELLED, "已取消"));
    }
    let target_session = target_session
        .prepare_write()
        .map_err(|e| err(EXIT_IO, format!("错误: 无法卸载 disk{}: {}", disk, e)))?;
    if tag16.iter().all(|&byte| byte == 0) {
        verify_hardware_bound_restore(ctx.runner, disk, &verified.manifest)?;
    }
    let _target_session = target_session
        .reopen_and_verify(dev, OPEN_WAIT, |dev| verify_reopened_snapshot(dev, &img))
        .map_err(|error| match error {
            ReopenAndVerifyError::Reopen(e) => err(
                EXIT_IO,
                format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e),
            ),
            ReopenAndVerifyError::Verify(error) => error,
        })?;
    let mut transaction = diskio::WriteTransactionPlan::new(current_total_sectors);
    if let Some((geometry, bytes)) = &restorable_lce {
        let expected_len = usize::try_from(geometry.sector_count)
            .ok()
            .and_then(|count| count.checked_mul(SECTOR))
            .ok_or_else(|| err(EXIT_BACKUP, "错误: LCE 恢复长度溢出"))?;
        if bytes.len() != expected_len {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: EDPB LCE 数据长度 {}B ≠ {}B",
                    bytes.len(),
                    expected_len
                ),
            ));
        }
        for offset in 0..geometry.sector_count {
            let lba64 = geometry
                .start_lba
                .checked_add(offset)
                .ok_or_else(|| err(EXIT_BACKUP, "错误: LCE 恢复 LBA 溢出"))?;
            let lba = u32::try_from(lba64)
                .map_err(|_| err(EXIT_BACKUP, "错误: LCE 恢复 LBA 超出当前写入器范围"))?;
            let start = usize::try_from(offset)
                .ok()
                .and_then(|index| index.checked_mul(SECTOR))
                .ok_or_else(|| err(EXIT_BACKUP, "错误: LCE 恢复字节偏移溢出"))?;
            transaction
                .insert(
                    lba,
                    bytes[start..start + SECTOR].to_vec(),
                    diskio::SectorWriteStage::Data,
                    "backup restore LCE",
                )
                .map_err(|message| {
                    err(EXIT_BACKUP, format!("错误: LCE 恢复计划无效: {message}"))
                })?;
        }
    }
    for lba in 1..METADATA_SECTOR_COUNT as u32 {
        transaction
            .insert(
                lba,
                data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec(),
                diskio::SectorWriteStage::Metadata,
                "backup restore protocol",
            )
            .map_err(|message| {
                err(
                    EXIT_BACKUP,
                    format!("错误: 协议恢复计划 LBA{lba} 无效: {message}"),
                )
            })?;
    }
    transaction
        .insert(
            0,
            data[..SECTOR].to_vec(),
            diskio::SectorWriteStage::Commit,
            "backup restore MBR",
        )
        .map_err(|message| err(EXIT_BACKUP, format!("错误: MBR 恢复计划无效: {message}")))?;
    diskio::execute_write_transaction(dev, &transaction)?;
    ctx.prompt.write_event(WriteEvent::RestoreWriteCompleted);
    Ok(EXIT_OK)
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
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_expected_identity(runner, disk, expected_onlyid, expected_device_id, &mut dev)?;
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
