//! Read-only metadata-backup creation paths exposed through application::write.

use super::*;

/// The v3 capture/restore contract still assumes 512B sectors, even though
/// native 4Kn protocol reads are already supported. Reject rather than create
/// a misleading truncated backup that cannot be round-trip restored.
fn verify_native_backup_capture_compatibility(
    geometry: Option<crate::domain::hardware::ObservedDeviceGeometry>,
) -> EdpCliResult<()> {
    // Missing geometry retains legacy behavior; only *known* non-512B
    // geometries are newly refused. This ensures unchanged 512B/unknown
    // discovery flows while preventing known 4Kn protocol truncation.
    if let Some(logical) = geometry.and_then(|observed| observed.logical_sector_bytes) {
        if logical != SECTOR as u32 {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: 当前 EDPB 元数据备份仅通过 512B 逻辑扇区验证，目标扇区大小 {logical}B；原生备份与同几何恢复尚未完成，不创建不完整备份"
                ),
            ));
        }
    }
    Ok(())
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
) -> EdpCliResult<crate::application::post_restore::MetadataBackupReport> {
    guard_usb_disk(ctx.runner, disk)?;
    let observed = observe_media_identity_readonly(ctx.runner, disk, dev)?;
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
        };
        let metadata =
            crate::backup_metadata::acquire_metadata(dev, &img, &device_id, total_sectors)
                .map_err(|message| {
                    err(
                        EXIT_BACKUP,
                        format!("错误: Metadata 级备份采集失败: {message}"),
                    )
                })?;
        crate::infrastructure::backup_store::create::create_metadata_backup(
            &facts,
            &img,
            &device_id,
            metadata,
            &identity,
            &ctx.backup_dir,
            ctx.clock,
        )?
    } else {
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
        };
        let metadata = crate::backup_metadata::acquire_plain_metadata(dev, total_sectors).map_err(
            |message| {
                err(
                    EXIT_BACKUP,
                    format!("错误: Plain 元数据备份采集失败: {message}"),
                )
            },
        )?;
        crate::infrastructure::backup_store::create::create_plain_backup(
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
    let report = crate::application::post_restore::MetadataBackupReport {
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
) -> EdpCliResult<crate::application::post_restore::MetadataBackupReport> {
    verify_native_backup_capture_compatibility(crate::platform::system::device_geometry(
        runner, disk,
    ))?;
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_expected_identity(runner, disk, expected_onlyid, expected_device_id, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    backup_create_flow(disk, &mut ctx, &mut dev)
}

pub fn backup_create_on_disk_with_pin(
    runner: &dyn CmdRunner,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected: &MediaIdentityResumePin,
) -> EdpCliResult<crate::application::post_restore::MetadataBackupReport> {
    verify_native_backup_capture_compatibility(crate::platform::system::device_geometry(
        runner, disk,
    ))?;
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    verify_resume_identity_pin(runner, disk, expected, &mut dev)?;
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt,
        backup_dir,
    };
    backup_create_flow(disk, &mut ctx, &mut dev)
}

#[cfg(test)]
mod native_backup_capture_gate_tests {
    use super::*;

    #[test]
    fn legacy_edpb_capture_keeps_512b_path_and_rejects_unverified_4kn() {
        let observed = |logical_sector_bytes| crate::domain::hardware::ObservedDeviceGeometry {
            capacity_bytes: 255_944_818_688,
            logical_sector_bytes,
            physical_sector_bytes: Some(4096),
        };
        assert!(verify_native_backup_capture_compatibility(Some(observed(Some(512)))).is_ok());
        assert!(verify_native_backup_capture_compatibility(None).is_ok());
        assert!(verify_native_backup_capture_compatibility(Some(observed(None))).is_ok());
        let four_kn = verify_native_backup_capture_compatibility(Some(observed(Some(4096))))
            .expect_err("incompatible backup must be refused");
        assert_eq!(four_kn.code, EXIT_BACKUP);
        assert!(four_kn.msg.contains("不创建不完整备份"));
    }
}
