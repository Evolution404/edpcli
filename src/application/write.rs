//! Shared backup/restore application service.
//!
//! CLI and TUI must enter raw-disk mutation through this module. The safety chain remains single-source:
//! system-disk/USB whole-disk guard → selector pinning by callers →
//! unmount/lock → reopen identity recheck → atomic write → sync/readback/rollback.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::common::*;
use crate::diskio::{self, raw_path, Clock, DiskFacts, SectorDev};
use crate::identify::identify;
use crate::selectors::{BackupSelector, DeviceSelector};
use crate::sysinfo::{self, CmdRunner};

pub use super::device::{guard_system_disk, guard_usb_disk};
pub use super::Prompter;

const OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// 写盘流程的类型化进度阶段。每个变体对应旧版一处文本输出；CLI 文本契约由
/// `render_event_text` 逐字节复刻(含 ui:: 样式与换行位置)，交互确认(prompt_line/
/// confirm_yes)不属于进度，仍留在 Prompter。
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

/// 逐字节复刻旧 output!/outputln! 的文本(含样式与换行位置)。CLI 与测试黄金基线共用。
pub fn render_event_text(event: &WriteEvent) -> String {
    match event {
        WriteEvent::BackupCreated { path } => {
            format!("{}  {}\n", crate::ui::green("备份"), path.display())
        }
        WriteEvent::BackupCreatedIsNopwd => format!(
            "{}\n",
            crate::ui::yellow("注意: 本份备份为【免密状态】快照 — 还原它不会回到加密原盘。")
        ),
        WriteEvent::RestoreMatchesHeader {
            disk,
            onlyid,
            count,
        } => format!("disk{} · onlyid={} 匹配备份 {} 个:\n", disk, onlyid, count),
        WriteEvent::RestoreMatchRow {
            index,
            time,
            is_nopwd,
            file_name,
        } => format!(
            "  [{}] {}   {}   {}\n",
            index,
            time,
            if *is_nopwd {
                "免密状态"
            } else {
                "加密原盘"
            },
            file_name
        ),
        WriteEvent::RestoreSelectionRetry { message } => {
            format!("{}\n", crate::ui::yellow(message))
        }
        WriteEvent::BackupShaVerified { digest } => {
            format!("{}  {}\n", crate::ui::green("SHA-256 校验通过"), digest)
        }
        WriteEvent::RestoreSnapshotNopwdWarning => format!(
            "{}\n",
            crate::ui::yellow(
                "注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。"
            )
        ),
        WriteEvent::RestoreDryRunNotice { path, disk } => format!(
            "{}\n",
            crate::ui::dim(&format!(
                "[dry-run] 将还原 {} → disk{} LBA0-12 ({}B) — 未写入(免密快照不作还原)。",
                path.display(),
                disk,
                METADATA_IMAGE_LEN
            ))
        ),
        WriteEvent::RestoreTargetHeader { path } => format!(
            "{}  {}\n",
            crate::ui::bold("还原"),
            crate::ui::truncate_mid(&path.display().to_string(), 64)
        ),
        WriteEvent::RestoreWriteCompleted => format!(
            "{}\n",
            crate::ui::green("已还原, 读回校验通过。请拔出重插。")
        ),
    }
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
) -> EdpCliResult<(PathBuf, bool)> {
    backup_create_level_flow(disk, ctx, dev, false)
}

/// Explicit Deep opt-in; shares the existing read-only device and identity path.
pub fn backup_create_level_flow(
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
    deep: bool,
) -> EdpCliResult<(PathBuf, bool)> {
    guard_usb_disk(ctx.runner, disk)?;
    let img = read_image(dev)?;
    let id = identify(ctx.runner, disk, &img[7 * SECTOR..8 * SECTOR]);
    let device_id = id.device_id.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 无法识别 device_id(LBA7 两候选均未解出 EDPF)，拒绝创建无法归属的备份",
        )
    })?;
    let (vid, pid) = sysinfo::usb_vid_pid(ctx.runner, disk);
    let facts = DiskFacts {
        disk,
        total_sectors: sysinfo::disk_total_sectors(ctx.runner, disk),
        vid,
        pid,
        label_id: diskio::lba4_label_id_from(&img[4 * SECTOR..5 * SECTOR]),
    };
    let total_sectors = facts.total_sectors.ok_or_else(|| {
        err(
            EXIT_BACKUP,
            "错误: 无法获取磁盘总扇区数，无法创建 Metadata 级备份",
        )
    })?;
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
    let created = save(
        &facts,
        &img,
        &device_id,
        metadata,
        &ctx.backup_dir,
        ctx.clock,
    )?;
    ctx.prompt.write_event(WriteEvent::BackupCreated {
        path: created.0.clone(),
    });
    if created.1 {
        ctx.prompt.write_event(WriteEvent::BackupCreatedIsNopwd);
    }
    Ok(created)
}

/// restore 主流程: bin=None 时交互列出本盘备份并选择。
pub fn restore_flow(
    bin: Option<String>,
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<i32> {
    guard_usb_disk(ctx.runner, disk)?;
    let img = read_image(dev)?;
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = diskio::lba4_label_id_from(lba4);
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;
    if tag16.iter().all(|&b| b == 0) {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前盘 LBA4 身份标签为空，无法确认备份归属，拒绝还原",
        ));
    }

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

    // 同一物理盘终验仍以原始 LBA4 前 16B 为准；Manifest 不替代原始证据。
    let backup_lba4 = &data[4 * SECTOR..5 * SECTOR];
    let backup_tag16 = diskio::lba4_tag16_from(backup_lba4)
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 备份 LBA4 缺少 16B 身份标签"))?;
    if tag16.iter().any(|&b| b != 0) && backup_tag16 != tag16 {
        let current_id = label_id.as_deref().unwrap_or("未知");
        let backup_id = diskio::lba4_label_id_from(backup_lba4).unwrap_or_else(|| "未知".into());
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: 备份属于另一块盘(current onlyid={}, backup onlyid={})，拒绝还原",
                current_id, backup_id
            ),
        ));
    }
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
    if !ctx.prompt.confirm_yes(&crate::ui::bold(&format!(
        "  → disk{} LBA0-12? 输入 YES: ",
        disk
    ))) {
        return Err(err(EXIT_CANCELLED, "已取消"));
    }
    let _write_guard = sysinfo::prepare_write(ctx.runner, disk)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法卸载 disk{}: {}", disk, e)))?;
    dev.reopen_rdwr(OPEN_WAIT).map_err(|e| {
        err(
            EXIT_IO,
            format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e),
        )
    })?;
    verify_reopened_snapshot(dev, &img)?;
    let writes: BTreeMap<u32, Vec<u8>> = (0..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            (
                lba,
                data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec(),
            )
        })
        .collect();
    diskio::atomic_write_sectors(dev, &writes)?;
    ctx.prompt.write_event(WriteEvent::RestoreWriteCompleted);
    Ok(EXIT_OK)
}
