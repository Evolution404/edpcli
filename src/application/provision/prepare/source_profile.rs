//! Read-only source profile and filesystem defaults.
use super::*;

pub(in crate::application::provision) fn resolve_target_filesystems(
    targets: &mut [crate::provision::TargetPartitionGeometry],
    source: Option<&crate::provision::ExistingProvisionProfile>,
    format: &FormatOptions,
) {
    for target in targets {
        if format.choice(target.role).0 {
            target.filesystem = format.filesystems().for_role(target.role);
        } else if let Some(old) = source.and_then(|source| source.partition(target.role)) {
            // Unknown source types must stay unknown throughout the preservation plan.
            target.filesystem = old.filesystem;
        }
    }
}

pub(in crate::application::provision) fn source_protocol_device_id(
    target: &TargetIdentity,
    source_identity: &crate::media_identity::MediaIdentitySnapshot,
) -> String {
    source_identity
        .protocol
        .device_id
        .clone()
        .unwrap_or_else(|| target.device_id().to_string())
}

pub(in crate::application::provision) fn effective_target_filesystems(
    format: &FormatOptions,
    target_plan: &TargetProvisionPlan,
) -> crate::provision::OfficialPartitionFilesystems {
    let mut filesystems = format.filesystems();
    for part in &target_plan.partitions {
        if part.disposition == RegionDisposition::Rebuild {
            continue;
        }
        let Some(filesystem) = part.geometry.filesystem else {
            continue;
        };
        match part.geometry.role {
            PartitionRole::Boot => filesystems.boot = filesystem,
            PartitionRole::Share | PartitionRole::BootShareCombined => {
                filesystems.share = filesystem
            }
            PartitionRole::Encrypt => filesystems.encrypt = filesystem,
            PartitionRole::CompatibilityReserve => {}
        }
    }
    filesystems
}

pub(in crate::application::provision) fn confirmed_filesystem(
    boot: &[u8],
    start_lba: u64,
    sectors: u64,
) -> Option<FilesystemKind> {
    if boot.len() != SECTOR {
        return None;
    }
    let registry = crate::filesystem::default_registry();
    let mut detection_reader = crate::filesystem::BootSectorReader::new(boot, sectors);
    let detected = registry.detect(&mut detection_reader).ok()??;
    let geometry = crate::filesystem::FilesystemGeometry::new(start_lba, sectors, SECTOR as u32);
    let mut geometry_reader = crate::filesystem::BootSectorReader::new(boot, sectors);
    detected
        .driver
        .matches_geometry(&mut geometry_reader, geometry)
        .ok()?
        .then_some(detected.kind())
}

pub(in crate::application::provision) fn classify_live_source_identity(
    runner: &dyn CmdRunner,
    disk: u32,
    source_metadata: &[u8],
    total_sectors: u64,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<crate::media_identity::MediaIdentitySnapshot> {
    let identity = media_identity_from_protocol_image(runner, disk, source_metadata)?;
    Ok(
        crate::media_identity_observer::apply_runtime_plain_override(
            identity,
            source_metadata,
            total_sectors,
            |lba| {
                let lba = u32::try_from(lba)
                    .map_err(|_| format!("Plain runtime evidence LBA{lba} exceeds u32"))?;
                dev.read_sector(lba)
                    .map_err(|error| format!("read Plain runtime evidence LBA{lba}: {error}"))
            },
        ),
    )
}

pub(in crate::application::provision) fn read_plain_source_extents(
    dev: &mut dyn SectorDev,
    total_sectors: u64,
) -> EdpCliResult<Vec<crate::provision::PlainSourceExtent>> {
    let table = crate::partition_table::read_partition_table(total_sectors, |lba| {
        let lba =
            u32::try_from(lba).map_err(|_| format!("Plain partition LBA{lba} exceeds u32"))?;
        dev.read_sector(lba)
            .map_err(|error| format!("read Plain partition LBA{lba}: {error}"))
    })
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法读取普通盘物理分区表: {message}"),
        )
    })?;

    let mut extents = Vec::with_capacity(table.partitions.len());
    for partition in table.partitions {
        let start = u32::try_from(partition.start_lba).map_err(|_| {
            err(
                EXIT_TARGET,
                format!(
                    "错误: 普通盘分区起点 LBA{} 超出当前读取范围",
                    partition.start_lba
                ),
            )
        })?;
        let boot = dev.read_sector(start).map_err(|error| {
            err(
                EXIT_TARGET,
                format!(
                    "错误: 无法读取普通盘分区 P{} 启动扇区: {error}",
                    partition.index
                ),
            )
        })?;
        let filesystem = crate::filesystem::detect_boot_sector_with_geometry(
            partition.start_lba,
            partition.sector_count,
            &boot,
        )
        .ok()
        .flatten();
        extents.push(crate::provision::PlainSourceExtent {
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem,
        });
    }
    Ok(extents)
}

pub(in crate::application::provision) fn resolved_source_password<'a>(
    source: &ParsedExistingProvision,
    key_domains: &'a KeyDomainSecrets,
    role: PartitionRole,
) -> (SourcePasswordKnowledge, Option<&'a [u8]>) {
    let Some(domain) = KeyDomainRole::from_partition_role(role) else {
        return (SourcePasswordKnowledge::Unknown, None);
    };
    let user_password = key_domains.source_password(role);
    let knowledge = source.source_password_knowledge(domain, user_password);
    let password = match knowledge {
        SourcePasswordKnowledge::DefaultVerified => Some(DEFAULT_KEY_DOMAIN_PASSWORD),
        SourcePasswordKnowledge::UserVerified => user_password,
        SourcePasswordKnowledge::Unknown => None,
    };
    (knowledge, password)
}

pub(in crate::application::provision) fn inspect_source_profile(
    dev: &mut dyn SectorDev,
    source_metadata: &[u8],
    device_id: &str,
    total_sectors: u64,
    key_domains: &KeyDomainSecrets,
) -> EdpCliResult<Option<ParsedExistingProvision>> {
    let image = ProvisionImage::from_bytes(source_metadata.to_vec())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let mut source =
        parse_existing_provision(&image, device_id, total_sectors).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法可靠解析: {message}"),
            )
        })?;
    if let Some(source) = source.as_mut() {
        let parts = source.profile.partitions.clone();
        for (index, part) in parts.iter().enumerate() {
            if part.role == PartitionRole::CompatibilityReserve {
                continue;
            }
            let Ok(lba) = u32::try_from(part.start_lba) else {
                continue;
            };
            let Ok(raw) = dev.read_sector(lba) else {
                continue;
            };
            if raw.len() != SECTOR {
                continue;
            }
            let plaintext = if part.physically_encrypted {
                let (_, Some(password)) = resolved_source_password(source, key_domains, part.role)
                else {
                    continue;
                };
                let Ok(key) = source.records[index].verified_sm4_file_key(password) else {
                    continue;
                };
                let Ok(value) = crate::partition_transform::decrypt_mode2(&raw, &key) else {
                    continue;
                };
                value
            } else {
                raw
            };
            if let Some(filesystem) =
                confirmed_filesystem(&plaintext, part.start_lba, part.sector_count)
            {
                source
                    .confirm_filesystem(part.role, filesystem)
                    .map_err(|message| err(EXIT_TARGET, message))?;
            }
        }
    }
    Ok(source)
}
