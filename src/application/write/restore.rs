//! Metadata restore transaction and its disk-entry adapters.

use super::*;

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
) -> EdpCliResult<crate::application::post_restore::MetadataRestoreOutcome> {
    let target_session = TargetSession::<ReadOnly>::open_usb(ctx.runner, disk)?;
    let observed = observe_media_identity_readonly(ctx.runner, disk, dev)?;
    let img = observed.protocol_image;
    let target_identity = observed.snapshot;
    let raw_target_device_id = identify(ctx.runner, disk, &img[7 * SECTOR..8 * SECTOR]).device_id;
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = target_identity.protocol.onlyid.clone();
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;
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

    // One verified snapshot binds the manifest and all bytes consumed by restore.
    let reader = crate::edpb::VerifiedBackupReader::open(&path).map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: EDPB 校验失败 {}: {}", path.display(), message),
        )
    })?;
    let verified = reader.verified();
    let backup_protocol = backup_protocol_image(&reader)?;
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
        &reader,
        backup_protocol.as_deref(),
        &img,
        raw_target_device_id.as_deref(),
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
            let fresh =
                crate::application::media_identity_observer::observe_media_identity_readonly(
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
    let report = crate::application::post_restore::MetadataRestoreReport {
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
        crate::application::media_identity_observer::observe_media_identity_readonly(
            ctx.runner, disk, dev,
        )
        .ok()
        .map(|observed| {
            let pin = crate::media_identity::MediaIdentityPin::new(
                observed.snapshot,
                &observed.protocol_image,
            );
            MediaIdentityResumePin::from_pin(&pin)
        });
    let assessment = crate::application::post_restore::assess_partitions_readonly(
        dev,
        &verified.manifest.snapshot.device_state,
        &verified.manifest.device.device_id,
        current_total_sectors,
        &verified.manifest.partitions,
    )
    .unwrap_or_else(|error| {
        crate::application::post_restore::PostRestoreAssessment::unsupported(
            &verified.manifest.partitions,
            format!("恢复后只读检查失败: {error}"),
        )
    });
    ctx.prompt.write_event(WriteEvent::PostRestoreAssessment {
        assessment: assessment.clone(),
    });
    let layout = crate::application::post_restore::project_restored_layout_readonly(
        dev,
        &verified.manifest.snapshot.device_state,
        &verified.manifest.device.device_id,
        current_total_sectors,
        &verified.manifest.partitions,
    );
    Ok(crate::application::post_restore::MetadataRestoreOutcome {
        report,
        assessment,
        partitions: verified.manifest.partitions.clone(),
        device_state: verified.manifest.snapshot.device_state.clone(),
        device_id: verified.manifest.device.device_id.clone(),
        total_sectors: current_total_sectors,
        layout,
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
) -> EdpCliResult<crate::application::post_restore::MetadataRestoreOutcome> {
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

pub fn restore_on_disk_typed_with_pin(
    runner: &dyn CmdRunner,
    bin: Option<String>,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
) -> EdpCliResult<crate::application::post_restore::MetadataRestoreOutcome> {
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_resume_identity_pin(runner, disk, expected, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    restore_flow_typed(bin, disk, &mut ctx, &mut dev)
}

pub fn restore_on_disk_with_pin(
    runner: &dyn CmdRunner,
    bin: Option<String>,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
) -> EdpCliResult<i32> {
    restore_on_disk_typed_with_pin(runner, bin, disk, backup_dir, prompt, expected).map(|_| EXIT_OK)
}
