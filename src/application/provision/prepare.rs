use super::*;
use crate::provision::DiskProvisionKind;

fn confirmed_filesystem(
    boot: &[u8],
    start_lba: u64,
    sectors: u64,
) -> Option<OfficialFilesystemFormat> {
    if boot.len() != SECTOR {
        return None;
    }
    if boot.get(3..11) == Some(b"EXFAT   ")
        && u64::from_le_bytes(boot.get(64..72)?.try_into().ok()?) == start_lba
        && u64::from_le_bytes(boot.get(72..80)?.try_into().ok()?) == sectors
    {
        return Some(OfficialFilesystemFormat::ExFat);
    }
    if boot.get(54..62) == Some(b"FAT16   ")
        && u32::from_le_bytes(boot.get(28..32)?.try_into().ok()?) as u64 == start_lba
    {
        let short = u16::from_le_bytes(boot.get(19..21)?.try_into().ok()?) as u64;
        let total = if short != 0 {
            short
        } else {
            u32::from_le_bytes(boot.get(32..36)?.try_into().ok()?) as u64
        };
        if total == sectors {
            return Some(OfficialFilesystemFormat::Fat16);
        }
    }
    None
}

fn resolved_source_password<'a>(
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

fn inspect_source_profile(
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
                let Ok(value) = crate::backup_deep::keys::decrypt_mode2(&raw, &key) else {
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

fn override_capacity(
    mib: Option<u64>,
    sectors: Option<u64>,
) -> EdpCliResult<Option<CapacityInput>> {
    if mib.is_some() && sectors.is_some() {
        return Err(err(EXIT_TARGET, "错误: 同一分区不能同时指定 MiB 与 sector"));
    }
    match (mib, sectors) {
        (Some(value), None) => {
            CapacityInput::from_quick(value, QuickCapacityUnit::MiB, CapacitySource::UserEdited)
                .map(Some)
                .map_err(|message| err(EXIT_TARGET, message))
        }
        (None, Some(value)) => CapacityInput::from_exact(value, CapacitySource::UserEdited)
            .map(Some)
            .map_err(|message| err(EXIT_TARGET, message)),
        (None, None) => Ok(None),
        _ => unreachable!(),
    }
}

pub(super) fn target_encrypt_capacity_override(
    _mode: OfficialPartitionMode,
    mib: Option<u64>,
    sectors: Option<u64>,
) -> EdpCliResult<Option<CapacityInput>> {
    // Quick and Exact always describe the target partition itself. In mode2
    // the fixed 63-sector CompatibilityReserve is a separate canonical
    // partition and must never be subtracted from the Encrypt input.
    override_capacity(mib, sectors)
}

struct PlannedMigrationImageReader<'a> {
    image: &'a SparseFilesystemImage,
}

impl PartitionReader for PlannedMigrationImageReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        self.image
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| std::io::Error::other("migration image read outside partition"))
    }
}

fn verify_migration_image(
    image: &SparseFilesystemImage,
    target: &crate::provision::TargetPartitionGeometry,
    migration: &super::migration::PreparedMigrationTarget,
) -> EdpCliResult<()> {
    let geometry = PartitionGeometry {
        index: migration.target_index,
        partition_type: 0,
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: target.start_lba,
        sector_size: SECTOR as u64,
        partition_size: target
            .sector_count
            .checked_mul(SECTOR as u64)
            .ok_or_else(|| err(EXIT_TARGET, "错误: K6 目标分区字节数溢出"))?,
        sector_count: target.sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let mut reader = PlannedMigrationImageReader { image };
    let report = analyze_partition(&geometry, &mut reader);
    if report.status != AnalysisStatus::Parsed
        || report.file_count != Some(migration.file_count)
        || report.directory_count != Some(migration.directory_count)
    {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: {} K6 目标文件系统生成后语义校验失败: {}",
                migration.role.label(),
                report.reason
            ),
        ));
    }
    let parsed = report.entries.ok_or_else(|| {
        err(
            EXIT_TARGET,
            format!(
                "错误: {} K6 目标文件系统缺少文件清单",
                migration.role.label()
            ),
        )
    })?;
    for staged in migration.entries.iter().filter(|entry| !entry.is_directory) {
        let parsed_entry = parsed
            .iter()
            .find(|entry| !entry.is_directory && entry.path == staged.path)
            .ok_or_else(|| {
                err(
                    EXIT_TARGET,
                    format!("错误: K6 目标镜像缺少文件 {:?}", staged.path),
                )
            })?;
        let mut reader = PlannedMigrationImageReader { image };
        let mut actual = Vec::new();
        let summary = stream_file_payload(
            &mut reader,
            parsed_entry,
            staged.data.len() as u64,
            &mut actual,
        )
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: K6 目标文件 {:?} 回读失败: {message}", staged.path),
            )
        })?;
        if summary.logical_size != staged.data.len() as u64 || actual != staged.data {
            return Err(err(
                EXIT_TARGET,
                format!("错误: K6 目标文件 {:?} 内容校验失败", staged.path),
            ));
        }
    }
    Ok(())
}

/// The physical path for both plain and registered USB media. Source mode is
/// consulted only while deriving defaults and Preserve candidates.
pub fn prepare_target_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &OfficialProvisionRequest,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedNewProvision> {
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .ok_or_else(|| {
            err(
                EXIT_TARGET,
                "错误: 当前目标不符合已验证的 512B/255x63 USB LCE 几何",
            )
        })?;
    let source_metadata = read_image(dev)?;
    let source_kind =
        crate::provision::DiskProvisionKind::from_metadata(&source_metadata, &device_id);
    let before_pin = MediaIdentityPin::new(
        media_identity_from_protocol_image(runner, disk, &source_metadata)?,
        &source_metadata,
    );
    let source = inspect_source_profile(
        dev,
        &source_metadata,
        &device_id,
        total_sectors,
        &request.key_domains,
    )?;
    let source_identity = if source.is_some() {
        let base = crate::protocol::semantic::SemanticContext {
            device_id: Some(device_id.clone()),
            vid: None,
            pid: None,
            size_bytes: Some(total_sectors * SECTOR as u64),
            onlyid: None,
        };
        Some(
            crate::metainfo::summarize(&base, |lba| {
                source_metadata
                    .get(lba as usize * SECTOR..(lba as usize + 1) * SECTOR)
                    .map(|raw| raw.to_vec())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "source metadata sector missing",
                        )
                    })
            })
            .map_err(|error| {
                err(
                    EXIT_TARGET,
                    format!("错误: 无法继承来源盘身份字段: {error}"),
                )
            })?,
        )
    } else {
        None
    };
    let selected_mode = official_mode(request.target)?;
    let inherited_pass_info_policy = source
        .as_ref()
        .and_then(|source| source.pass_info_policy)
        .unwrap_or_default();
    let pass_info_policy = PassInfoPolicy {
        force_change_password: request
            .force_change_password
            .unwrap_or(inherited_pass_info_policy.force_change_password),
        cancel_password_complexity_check: request
            .cancel_password_complexity_check
            .unwrap_or(inherited_pass_info_policy.cancel_password_complexity_check),
        max_share_password_errors: request
            .max_share_password_errors
            .unwrap_or(inherited_pass_info_policy.max_share_password_errors),
        max_encrypt_password_errors: request
            .max_encrypt_password_errors
            .unwrap_or(inherited_pass_info_policy.max_encrypt_password_errors),
    };
    let force_change_password = pass_info_policy.force_change_password;
    let prefill = prefill_for_target_mode(
        source.as_ref().map(|source| &source.profile),
        selected_mode,
        compatibility.start_lba,
        SECTOR as u64,
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法生成目标模式默认布局: {message}"),
        )
    })?;
    let encrypt_override = target_encrypt_capacity_override(
        selected_mode,
        request.encrypt_mib,
        request.encrypt_sectors,
    )?;
    let prefill = apply_target_geometry_overrides(
        prefill,
        source.as_ref().map(|source| &source.profile),
        TargetGeometryOverrides {
            boot: override_capacity(request.boot_mib, request.boot_sectors)?,
            share: override_capacity(request.share_mib, request.share_sectors)?,
            encrypt: encrypt_override,
            boot_start_lba: request.boot_start_lba,
            share_start_lba: request.share_start_lba,
            encrypt_start_lba: request.encrypt_start_lba,
        },
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 目标分区重叠或越界: {message}")))?;
    let mut targets = prefill
        .target_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标分区重叠或越界: {message}")))?;
    for target in &mut targets {
        let selected_format = request.format.choice(target.role).0;
        if !selected_format {
            if let Some(old) = source
                .as_ref()
                .and_then(|source| source.profile.partition(target.role))
            {
                if old.filesystem.is_some() {
                    target.filesystem = old.filesystem;
                }
            }
        }
    }
    let mut target_plan = TargetProvisionPlan::build(
        source.as_ref(),
        selected_mode,
        &targets,
        compatibility.start_lba,
        &request.key_domains,
    )
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法生成统一目标制盘计划: {message}"),
        )
    })?;
    let mut prepared_plain_import = None;
    if source.is_none() && source_kind == crate::provision::DiskProvisionKind::Plain {
        let import = super::migration::prepare_plain_to_official(dev, &target_plan)?;
        let role = target_plan.partitions[import.target_index].geometry.role;
        if !request.format.choice(role).0 {
            let target = &mut target_plan.partitions[import.target_index];
            target.disposition = RegionDisposition::Migrate;
            target.action = RegionDisposition::Migrate.legacy_action();
            target.migration_sources = import.sources;
            target.reason = "K6 Plain→EDP 文件级无损导入".into();
            prepared_plain_import = Some(import.prepared);
        }
    }
    let explicit_rebuild_roles = target_plan
        .partitions
        .iter()
        .filter(|part| request.format.choice(part.geometry.role).0)
        .map(|part| part.geometry.role)
        .collect::<Vec<_>>();
    for role in explicit_rebuild_roles {
        target_plan.force_rebuild_for_format(role);
    }
    for part in &target_plan.partitions {
        match part.disposition {
            RegionDisposition::Migrate => {
                if part.migration_sources.is_empty() {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}为 Migrate 但缺少 typed migration source",
                            part.geometry.role.label()
                        ),
                    ));
                }
                if request.format.choice(part.geometry.role).0 {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}不能同时请求 Migrate 与显式格式化",
                            part.geometry.role.label()
                        ),
                    ));
                }
            }
            RegionDisposition::Rebuild
                if part.geometry.role != PartitionRole::CompatibilityReserve
                    && !request.format.choice(part.geometry.role).0 =>
            {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: {}为 Rebuild，必须显式启用完整文件系统初始化；拒绝 K_new + old ciphertext",
                        part.geometry.role.label()
                    ),
                ));
            }
            _ => {}
        }
    }
    let prepared_migrations = if let Some(prepared) = prepared_plain_import {
        vec![prepared]
    } else {
        super::migration::prepare_migrations(
            dev,
            source.as_ref(),
            &target_plan,
            &key_domains,
        )?
    };
    let source_onlyid = source.as_ref().and_then(|_| {
        source_metadata
            .get(4 * SECTOR..5 * SECTOR)
            .and_then(diskio::lba4_label_id_from)
    });
    let onlyid = if request.label_id.trim().is_empty() {
        match source_onlyid {
            Some(value) => value,
            None => OnlyId::random_candidate()
                .map_err(|message| err(EXIT_TARGET, message))?
                .text()
                .to_string(),
        }
    } else {
        request.label_id.clone()
    };
    let inherited = |value: &str, source: Option<&str>| -> String {
        if value.trim().is_empty() {
            source.unwrap_or_default().to_string()
        } else {
            value.to_string()
        }
    };
    let user = inherited(
        &request.user,
        source_identity
            .as_ref()
            .and_then(|value| value.ownership.user.as_deref()),
    );
    let dept = inherited(
        &request.dept,
        source_identity
            .as_ref()
            .and_then(|value| value.ownership.dept.as_deref()),
    );
    let label = inherited(
        &request.label,
        source_identity
            .as_ref()
            .and_then(|value| value.safe6_label.as_deref()),
    );
    let label = if label.is_empty() {
        crate::provision::DEFAULT_SAFE6_LABEL.to_string()
    } else {
        label
    };
    let metadata = ProvisionMetadata::new(
        OnlyId::parse(&onlyid)
            .map_err(|message| err(EXIT_TARGET, format!("错误: 标签标识无效: {message}")))?,
        user,
        dept,
        label,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘身份字段无效: {message}")))?;
    let profile = ProvisionProfile::canonical_v1().with_pass_info_policy(pass_info_policy);
    let spec = ProvisionSpec::new(target, metadata, profile)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘元数据无法编码: {message}")))?;
    let mut filesystems = request.format.filesystems();
    for part in &target_plan.partitions {
        if let Some(format) = part.geometry.filesystem {
            match part.geometry.role {
                PartitionRole::Boot => filesystems.boot = format,
                PartitionRole::Share | PartitionRole::BootShareCombined => {
                    filesystems.share = format
                }
                PartitionRole::Encrypt => filesystems.encrypt = format,
                PartitionRole::CompatibilityReserve => {}
            }
        }
    }
    let mut plan = OfficialProvisionPlan::new(
        selected_mode,
        sizes(request, selected_mode)?,
        compatibility,
        wrap_legacy_lba7_file_key(DEFAULT_KEY_DOMAIN_PASSWORD, random_array::<8>()?),
        wrap_file_key(
            DEFAULT_KEY_DOMAIN_PASSWORD,
            random_array::<16>()?,
            FileKeyWrapMode::Sm4,
        ),
    )
    .map_err(|message| err(EXIT_TARGET, message))?
    .with_filesystems(filesystems)
    .with_target_geometry(&targets, SECTOR as u64)
    .map_err(|message| err(EXIT_TARGET, message))?;
    let mut file_keys = Vec::with_capacity(target_plan.partitions.len());
    for (index, part) in target_plan.partitions.iter().enumerate() {
        match part.disposition {
            RegionDisposition::PreserveOpaque | RegionDisposition::PreserveVerified => {
                let record = part.preserved_record.ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}保留计划缺少来源 key record",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                file_keys.push([0; 16]);
                if record.lba12.need_encrypt != 0 {
                    plan = plan
                        .with_partition_key_material(
                            index,
                            record.lba7_key_material(),
                            record
                                .lba12_key_material()
                                .map_err(|message| err(EXIT_TARGET, message))?,
                        )
                        .map_err(|message| err(EXIT_TARGET, message))?;
                }
            }
            RegionDisposition::RewrapVerified => {
                let record = part.preserved_record.ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}Rewrap 计划缺少来源 key record",
                            part.geometry.role.label()
                        ),
                    )
                })?;
                let source = source
                    .as_ref()
                    .ok_or_else(|| err(EXIT_TARGET, "错误: Rewrap 计划缺少来源注册信息"))?;
                let (_, Some(source_password)) =
                    resolved_source_password(source, &request.key_domains, part.geometry.role)
                else {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}Rewrap 需要已验证来源密码",
                            part.geometry.role.label()
                        ),
                    ));
                };
                let target_password = request
                    .key_domains
                    .target_password(part.geometry.role)
                    .ok_or_else(|| {
                        err(
                            EXIT_TARGET,
                            format!("错误: {}Rewrap 需要目标密码", part.geometry.role.label()),
                        )
                    })?;
                let mut legacy_key =
                    unwrap_legacy_lba7_file_key(source_password, record.lba7_key_material())
                        .map_err(|message| err(EXIT_TARGET, message))?;
                let file_key = record
                    .verified_sm4_file_key(source_password)
                    .map_err(|message| err(EXIT_TARGET, message))?;
                let lba7_material = wrap_legacy_lba7_file_key(target_password, legacy_key);
                legacy_key.fill(0);
                let lba12_material = wrap_file_key(target_password, file_key, FileKeyWrapMode::Sm4);
                file_keys.push(file_key);
                plan = plan
                    .with_partition_key_material(index, lba7_material, lba12_material)
                    .map_err(|message| err(EXIT_TARGET, message))?;
            }
            RegionDisposition::Rebuild => {
                if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                    let password = request
                        .key_domains
                        .target_password(part.geometry.role)
                        .ok_or_else(|| {
                            err(
                                EXIT_TARGET,
                                format!("错误: {}目标密码不能为空", part.geometry.role.label()),
                            )
                        })?;
                    let key = random_array::<16>()?;
                    file_keys.push(key);
                    plan = plan
                        .with_partition_key_material(
                            index,
                            wrap_legacy_lba7_file_key(password, random_array::<8>()?),
                            wrap_file_key(password, key, FileKeyWrapMode::Sm4),
                        )
                        .map_err(|message| err(EXIT_TARGET, message))?;
                } else {
                    file_keys.push([0; 16]);
                }
            }
            RegionDisposition::Migrate => {
                if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                    let password = request
                        .key_domains
                        .target_password(part.geometry.role)
                        .ok_or_else(|| {
                            err(
                                EXIT_TARGET,
                                format!(
                                    "错误: {} K6 迁移目标密码不能为空",
                                    part.geometry.role.label()
                                ),
                            )
                        })?;
                    let key = random_array::<16>()?;
                    file_keys.push(key);
                    plan = plan
                        .with_partition_key_material(
                            index,
                            wrap_legacy_lba7_file_key(password, random_array::<8>()?),
                            wrap_file_key(password, key, FileKeyWrapMode::Sm4),
                        )
                        .map_err(|message| err(EXIT_TARGET, message))?;
                } else {
                    file_keys.push([0; 16]);
                }
            }
            RegionDisposition::Drop => {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 目标分区 {}不能使用 Drop disposition",
                        part.geometry.role.label()
                    ),
                ));
            }
        }
    }
    let format_options = request.format.clone();
    let mut serials = Vec::with_capacity(target_plan.partitions.len());
    for _ in &target_plan.partitions {
        serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let format_result = plan_format_targets_with_keys(&plan, &format_options, &serials, &file_keys);
    let format_targets = format_result.map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法构造目标格式化计划: {message}"),
        )
    })?;
    let expected_serial_digest =
        if format_targets.iter().any(|choice| choice.selected) || !prepared_migrations.is_empty() {
            let evidence = super::super::media_identity::serial_digest_evidence(
                runner.hardware_serial(disk).as_deref(),
            );
            if evidence.quality != super::super::media_identity::SerialQuality::Usable {
                return Err(err(
                    EXIT_TARGET,
                    "错误: 无法读取可用 USB 硬件序列号，拒绝安排格式化或 K6 数据迁移",
                ));
            }
            evidence.sha256
        } else {
            None
        };
    let entropy = ProvisionEntropy::new(random_array::<252>()?);
    let mut write_image =
        build_official_provision_protocol_image(&spec, &entropy, &plan).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法构造目标协议镜像: {message}"),
            )
        })?;

    for migration in &prepared_migrations {
        let part = target_plan
            .partitions
            .get(migration.target_index)
            .ok_or_else(|| err(EXIT_TARGET, "错误: K6 目标分区索引越界"))?;
        let choice = format_targets
            .get(migration.target_index)
            .ok_or_else(|| err(EXIT_TARGET, "错误: K6 目标格式信息缺失"))?;
        if choice.target.role != migration.role || choice.selected {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: {} K6 迁移目标与格式化计划不一致",
                    migration.role.label()
                ),
            ));
        }
        let filesystem = part.geometry.filesystem.ok_or_else(|| {
            err(
                EXIT_TARGET,
                format!("错误: {} K6 迁移目标没有文件系统", migration.role.label()),
            )
        })?;
        let plain = build_migrated_filesystem(
            filesystem,
            part.geometry.start_lba,
            part.geometry.sector_count,
            serials[migration.target_index],
            &choice.volume_label,
            &migration.entries,
        )
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!(
                    "错误: {} K6 目标文件系统构造失败: {message}",
                    migration.role.label()
                ),
            )
        })?;
        verify_migration_image(&plain, &part.geometry, migration)?;
        let physical = if choice.target.physically_encrypted {
            encrypt_sparse_mode2(&plain, &file_keys[migration.target_index])
        } else {
            plain
        };
        for (&relative_lba, sector) in physical.sectors() {
            let absolute = part
                .geometry
                .start_lba
                .checked_add(relative_lba)
                .and_then(|lba| u32::try_from(lba).ok())
                .ok_or_else(|| err(EXIT_TARGET, "错误: K6 目标写入 LBA 溢出"))?;
            if write_image
                .patch
                .insert(absolute, sector.to_vec())
                .is_some()
            {
                return Err(err(
                    EXIT_TARGET,
                    format!("错误: K6 目标 LBA{absolute} 与既有写集合重叠"),
                ));
            }
        }
    }
    for key in &mut file_keys {
        key.fill(0);
    }
    validate_key_disposition_plan(&target_plan, &plan, &format_targets)?;
    validate_target_write_set(&target_plan, &write_image.patch, &format_targets)?;
    Ok(PreparedNewProvision {
        disk,
        device_id,
        mode: selected_mode,
        force_change_password,
        pass_info_policy,
        lce_start_lba: compatibility.start_lba,
        write_image,
        format_targets,
        target_plan: Some(target_plan),
        source_metadata: Some(source_metadata),
        before_pin,
        plan,
        expected_onlyid: onlyid,
        expected_serial_digest,
        expected_probe: probe,
        expected_lba3: None,
    })
}

pub fn probe_provision_key_domains_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
) -> EdpCliResult<ProvisionKeyProbe> {
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    let source_metadata = read_image(&mut dev)?;
    let image = ProvisionImage::from_bytes(source_metadata.clone())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let parsed =
        parse_existing_provision(&image, &device_id, total_sectors).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法可靠解析: {message}"),
            )
        })?;
    let source_kind = DiskProvisionKind::from_metadata(&source_metadata, &device_id);
    let domain_probe = |domain: KeyDomainRole| {
        parsed
            .as_ref()
            .and_then(|source| {
                source
                    .record_for_domain(domain)
                    .map(|record| (source, record))
            })
            .map(|(source, record)| {
                (
                    Some(source.source_password_knowledge(domain, None)),
                    record.lba12.need_encrypt != 0
                        && FileKeyWrapMode::from_raw(record.lba12.encrypt_mode)
                            == Some(FileKeyWrapMode::Sm4),
                )
            })
            .unwrap_or((None, false))
    };
    let (share, share_opaque_profile) = domain_probe(KeyDomainRole::Share);
    let (encrypt, encrypt_opaque_profile) = domain_probe(KeyDomainRole::Encrypt);
    Ok(ProvisionKeyProbe {
        source_kind,
        share,
        share_opaque_profile,
        encrypt,
        encrypt_opaque_profile,
    })
}

pub fn verify_provision_source_password_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    domain: KeyDomainRole,
    password: &[u8],
) -> EdpCliResult<SourcePasswordKnowledge> {
    if password.is_empty() {
        return Err(err(EXIT_TARGET, "错误: 来源密码不能为空"));
    }
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    let source_metadata = read_image(&mut dev)?;
    let image = ProvisionImage::from_bytes(source_metadata)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let source = parse_existing_provision(&image, target.device_id(), total_sectors)
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法解析: {message}"),
            )
        })?
        .ok_or_else(|| err(EXIT_TARGET, "错误: 当前来源盘没有可验证的 EDP key domain"))?;
    let record = source
        .record_for_domain(domain)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 当前来源模式不包含该密码域"))?;
    record
        .verified_sm4_file_key(password)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源密码验证失败: {message}")))?;
    Ok(if password == DEFAULT_KEY_DOMAIN_PASSWORD {
        SourcePasswordKnowledge::DefaultVerified
    } else {
        SourcePasswordKnowledge::UserVerified
    })
}

pub fn prepare_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &ProvisionRequest,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedProvision> {
    match request {
        ProvisionRequest::Official(request) => {
            let mut prepared = prepare_target_provision(runner, disk, request, dev)?;
            capture_manufacturer_lba3(dev, &mut prepared)?;
            Ok(PreparedProvision::Official(Box::new(prepared)))
        }
        ProvisionRequest::Plain(request) => {
            let total_sectors = sysinfo::disk_total_sectors(runner, disk)
                .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
            let plan = request.resolve(total_sectors).map_err(|message| {
                err(
                    EXIT_TARGET,
                    format!("错误: 无法构造 Plain 分区计划: {message}"),
                )
            })?;
            prepare_plain_provision_with_key_domains(runner, disk, plan, &request.key_domains, dev)
                .map(|prepared| PreparedProvision::Plain(Box::new(prepared)))
        }
    }
}

pub fn prepare_plain_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: PlainProvisionPlan,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedPlainProvision> {
    prepare_plain_provision_with_key_domains(
        runner,
        disk,
        plan,
        &KeyDomainSecrets::default(),
        dev,
    )
}

fn prepare_plain_provision_with_key_domains(
    runner: &dyn CmdRunner,
    disk: u32,
    plan: PlainProvisionPlan,
    key_domains: &KeyDomainSecrets,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedPlainProvision> {
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    if total_sectors != plan.total_sectors {
        return Err(err(
            EXIT_TARGET,
            format!(
                "错误: Plain 计划容量 {} sectors 与当前目标 {} sectors 不一致",
                plan.total_sectors, total_sectors
            ),
        ));
    }
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();

    let source_metadata = read_image(dev)?;
    let before_pin = MediaIdentityPin::new(
        media_identity_from_protocol_image(runner, disk, &source_metadata)?,
        &source_metadata,
    );
    let lba7 = &source_metadata[7 * SECTOR..8 * SECTOR];
    let lba12 = &source_metadata[12 * SECTOR..13 * SECTOR];
    let source_kind = crate::provision::DiskProvisionKind::from_sectors(lba7, lba12, &device_id);
    let source_lce =
        if source_kind == crate::provision::DiskProvisionKind::Plain {
            None
        } else {
            let geometry =
                parse_lba7_compatibility_geometry(&source_metadata, &device_id, total_sectors)
                    .map_err(|message| {
                        err(
                EXIT_TARGET,
                format!("错误: 无法从来源 LBA7 实际 entry 解析 LCE，拒绝恢复普通盘: {message}"),
            )
                    })?;
            Some(PlainCleanupExtent::new(
                geometry.start_lba,
                geometry.sector_count,
            ))
        };

    let staged_to_plain = if source_kind == crate::provision::DiskProvisionKind::Plain {
        Vec::new()
    } else {
        let image = ProvisionImage::from_bytes(source_metadata.clone()).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: EDP→Plain 来源协议镜像无效: {message}"),
            )
        })?;
        let existing = parse_existing_provision(&image, &device_id, total_sectors)
            .map_err(|message| {
                err(
                    EXIT_TARGET,
                    format!("错误: EDP→Plain 来源注册结构无法解析: {message}"),
                )
            })?
            .ok_or_else(|| err(EXIT_TARGET, "错误: EDP→Plain 缺少来源注册结构"))?;
        let target_capacity = plan.partitions[0]
            .sector_count
            .checked_mul(SECTOR as u64)
            .ok_or_else(|| err(EXIT_TARGET, "错误: Plain P1 容量溢出"))?;
        super::migration::prepare_existing_to_plain(
            dev,
            &existing,
            target_capacity,
            &request.key_domains,
        )?
    };

    let mut volume_serials = Vec::with_capacity(plan.partitions.len());
    for _ in &plan.partitions {
        volume_serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let write_plan = if staged_to_plain.is_empty() {
        build_plain_provision_write_plan(&plan, source_lce, &volume_serials)
    } else {
        let mut migrations = vec![Vec::new(); plan.partitions.len()];
        migrations[0] = staged_to_plain;
        build_plain_migrated_provision_write_plan(&plan, source_lce, &volume_serials, &migrations)
    }
    .map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 无法构造 Plain 写盘计划: {message}"),
        )
    })?;

    Ok(PreparedPlainProvision {
        disk,
        device_id,
        plan,
        write_plan,
        source_kind,
        source_lce_start_lba: source_lce.map(|extent| extent.start_lba),
        source_metadata,
        before_pin,
        expected_probe: probe,
    })
}
