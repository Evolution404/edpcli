//! Complete 4Kn *read-only evidence* capture. Physical restore is intentionally
//! absent: the legacy 512B writer/rollback contract does not apply here.
use super::*;
use crate::application::evidence::{EvidenceSource, SectorReader};
use crate::edpb::{
    ArtifactCompleteness, ArtifactInput, CoreCapture, Extent, ManifestPartition, MetadataCapture,
    Region, RestorePolicy, SemanticStatus,
};

pub(super) fn create_native_evidence_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    backup_dir: PathBuf,
    prompt: &mut dyn Prompter,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
    pin: Option<&MediaIdentityResumePin>,
) -> EdpCliResult<crate::application::post_restore::MetadataBackupReport> {
    guard_usb_disk(runner, disk)?;
    let mut source = EvidenceSource::open_disk(runner, disk)
        .map_err(|error| err(EXIT_BACKUP, format!("错误: 4Kn 只读取证打开失败: {error}")))?;
    if source.logical_sector_bytes() != 4096 {
        return Err(err(EXIT_BACKUP, "错误: 原生取证仅认证4096B逻辑扇区"));
    }
    let snapshot = crate::application::media_identity_observer::media_identity_from_protocol_image(
        runner,
        disk,
        source.protocol(),
    )?;
    if let Some(pin) = pin {
        pin.validate()
            .map_err(|error| err(EXIT_TARGET, format!("错误: 身份pin损坏: {error}")))?;
        pin.verify(&snapshot, source.protocol()).map_err(|error| {
            err(
                EXIT_TARGET,
                format!("错误: 4Kn 备份目标身份pin不一致: {error:?}"),
            )
        })?;
    }
    let did = source
        .identity()
        .device_id
        .as_deref()
        .filter(|did| {
            did.starts_with("disk&ven_")
                && did.len() <= 128
                && did
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'&' | b'.' | b'-'))
        })
        .ok_or_else(|| {
            err(
                EXIT_BACKUP,
                "错误: 4Kn EDP device_id 未确认或包含不安全字符",
            )
        })?
        .to_owned();
    if expected_device_id.is_some_and(|expected| expected != did)
        || expected_onlyid
            .is_some_and(|expected| source.identity().onlyid.as_deref() != Some(expected))
    {
        return Err(err(
            EXIT_TARGET,
            "错误: 4Kn 备份目标与已固定的设备身份不一致",
        ));
    }
    let kind = source.identity().provision_kind;
    if !kind.is_some_and(|kind| kind != crate::provision::DiskProvisionKind::Plain) {
        return Err(err(EXIT_BACKUP, "错误: 当前4Kn介质缺少已确认的EDP制盘模式"));
    }
    let total_sectors = source.total_sectors();
    let protocol = source
        .native_protocol_image()
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 完整4Kn协议原生镜像不可用"))?
        .clone();
    let projected = protocol.protocol_projection();
    let partitions = crate::domain::geometry::parse_partition_geometry_with_sector_bytes(
        &projected,
        &did,
        total_sectors,
        4096,
    )
    .map_err(|e| err(EXIT_BACKUP, format!("错误: LBA12分区几何未验证: {e}")))?;
    let lce = crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
        &projected,
        &did,
        total_sectors,
        4096,
    )
    .map_err(|e| err(EXIT_BACKUP, format!("错误: LBA7原生LCE未验证: {e}")))?;
    if lce.sector_count != 1 {
        return Err(err(EXIT_BACKUP, "错误: 4Kn LCE必须是一个完整原生块"));
    }
    let lce_bytes = source
        .read_native_sector(lce.start_lba)
        .map_err(|e| err(EXIT_BACKUP, format!("错误: LCE原生取证失败: {e}")))?;
    if lce_bytes.len() != 4096 {
        return Err(err(EXIT_BACKUP, "错误: 4Kn LCE采集不完整，禁止创建备份"));
    }

    let mut regions = vec![Region {
        id: "region.lba7_compatibility_extent".into(),
        role: "lba7_legacy_partition_compatibility_extent".into(),
        start_lba: Some(lce.start_lba),
        sector_count: Some(1),
        semantic_status: SemanticStatus::Identified,
    }];
    let mut extents = vec![Extent {
        id: "extent.lba7_compatibility".into(),
        region_id: regions[0].id.clone(),
        start_lba: lce.start_lba,
        sector_count: 1,
        purpose: "lba7_compatibility_extent_ciphertext".into(),
    }];
    let mut artifacts = vec![ArtifactInput {
        id: "raw.lba7_compatibility".into(),
        kind: "raw_sectors".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec![extents[0].id.clone()],
        derivation: None,
        restore_policy: RestorePolicy::EvidenceOnly,
        completeness: ArtifactCompleteness::Complete,
        data: lce_bytes,
    }];
    let mut manifest_partitions = Vec::new();
    let official_mode = kind
        .and_then(|kind| kind.official_mode())
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 4Kn协议分区模式缺乏官方映射"))?;
    for p in partitions {
        let block = source.read_native_sector(p.start_sector).map_err(|e| {
            err(
                EXIT_BACKUP,
                format!("错误: 分区{}首块取证失败: {e}", p.index),
            )
        })?;
        if block.len() != 4096 {
            return Err(err(EXIT_BACKUP, "错误: 原生分区首块读取不完整"));
        }
        let partition_type = crate::protocol::edpf::EdpPartitionType::from_raw(p.partition_type)
            .ok_or_else(|| err(EXIT_BACKUP, "错误: 未知EDP分区类型"))?;
        let role =
            match crate::provision::official_partition_role(official_mode, p.index, partition_type)
            {
                crate::provision::PartitionRole::Boot => "boot",
                crate::provision::PartitionRole::Share => "share",
                crate::provision::PartitionRole::Encrypt => "encrypt",
                crate::provision::PartitionRole::BootShareCombined => "boot_share_combined",
                crate::provision::PartitionRole::CompatibilityReserve => "compatibility_reserve",
            };
        manifest_partitions.push(ManifestPartition {
            index: (p.index + 1) as u32,
            role: Some(role.into()),
            partition_type: Some(p.partition_type.to_string()),
            start_lba: p.start_sector,
            sector_count: p.sector_count,
            filesystem_hint: None,
            volume_label_hint: None,
        });
        let region_id = format!("region.partition_header.{}", p.index);
        let extent_id = format!("extent.partition_header.{}", p.index);
        regions.push(Region {
            id: region_id.clone(),
            role: "native_partition_header_evidence".into(),
            start_lba: Some(p.start_sector),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        });
        extents.push(Extent {
            id: extent_id.clone(),
            region_id,
            start_lba: p.start_sector,
            sector_count: 1,
            purpose: "native_partition_header_evidence".into(),
        });
        artifacts.push(ArtifactInput {
            id: format!("raw.partition_header.{}", p.index),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id],
            derivation: None,
            restore_policy: RestorePolicy::EvidenceOnly,
            completeness: ArtifactCompleteness::Complete,
            data: block,
        });
    }
    let clock = SystemClock;
    let epoch = clock.now_epoch();
    let stamp = clock.fmt_ts(epoch);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    let name = format!("disk{disk}_native4kn_{did}_{stamp}_{nonce:09}.edpb");
    let path = backup_dir.join(name);
    let core = CoreCapture {
        snapshot_id: format!("native4kn-{disk}-{epoch}-{nonce}"),
        created_epoch: epoch,
        disk_number: Some(disk),
        vid: source
            .identity()
            .vid
            .clone()
            .unwrap_or_else(|| "xxxx".into()),
        pid: source
            .identity()
            .pid
            .clone()
            .unwrap_or_else(|| "xxxx".into()),
        device_id: did,
        onlyid: source.identity().onlyid.clone(),
        total_sectors: Some(total_sectors),
        logical_sector_size: 4096,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "edp".into(),
        lba0_12: protocol.native_bytes(),
    };
    let capture = MetadataCapture {
        core,
        partitions: manifest_partitions,
        regions,
        extents,
        artifacts,
        notes: vec!["4Kn evidence-only; no restore writes authorized".into()],
    };
    crate::edpb::write_metadata_backup_with_identity(&path, &capture, &snapshot)
        .map_err(|error| err(EXIT_BACKUP, format!("错误: 原生备份取证失败: {error}")))?;
    let verified = crate::edpb::verify_file(&path)
        .map_err(|error| err(EXIT_BACKUP, format!("错误: 原生取证容器校验失败: {error}")))?;
    let report = crate::application::post_restore::MetadataBackupReport {
        path,
        partition_count: verified.manifest.partitions.len(),
        edp_protocol_saved: true,
    };
    prompt.write_event(WriteEvent::BackupCreated {
        path: report.path.clone(),
    });
    Ok(report)
}
