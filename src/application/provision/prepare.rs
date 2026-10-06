use super::*;

#[path = "prepare/source_profile.rs"]
mod source_profile;
pub(super) use source_profile::read_plain_source_extents;
use source_profile::*;
#[path = "prepare/key_probe.rs"]
mod key_probe;
pub use key_probe::{
    probe_provision_key_domains_on_disk, verify_provision_source_password_on_disk,
};

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

/// The physical path for both plain and registered USB media. Source mode is
/// consulted only while deriving defaults and Preserve candidates.
pub fn prepare_target_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &OfficialProvisionRequest,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedNewProvision> {
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let geometry = target_session.writable_geometry()?;
    let total_sectors = geometry
        .writable_protocol_sectors()
        .map_err(|message| err(EXIT_TARGET, message))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let compatibility = locate_lba7_compatibility_extent_from_verified_usb_capacity(
        total_sectors,
        geometry.logical_sector_bytes.unwrap_or(0),
    )
    .ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 当前目标不符合已验证的 512B/255x63 USB LCE 几何",
        )
    })?;
    let source_metadata = read_image(dev)?;
    let source_identity =
        classify_live_source_identity(runner, disk, &source_metadata, total_sectors, dev)?;
    let source_kind = source_identity.protocol.provision_kind.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 来源盘型未确认；拒绝把未知/损坏介质按 Plain 或 EDP 继续制盘",
        )
    })?;
    let source_device_id = source_protocol_device_id(&target, &source_identity);
    let before_pin = MediaIdentityPin::new(source_identity, &source_metadata);
    let plain_source_extents = if source_kind == crate::provision::DiskProvisionKind::Plain {
        read_plain_source_extents(dev, total_sectors)?
    } else {
        Vec::new()
    };
    let source = if source_kind == crate::provision::DiskProvisionKind::Plain {
        None
    } else {
        inspect_source_profile(
            dev,
            &source_metadata,
            &source_device_id,
            total_sectors,
            &request.key_domains,
        )?
    };
    let source_identity = if source.is_some() {
        let base = crate::protocol::semantic::SemanticContext {
            device_id: Some(source_device_id.clone()),
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
    let mut target_plan = TargetProvisionPlan::build_with_plain_extents(
        source.as_ref(),
        &plain_source_extents,
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
        if part.password_disposition == Some(crate::provision::PasswordDisposition::Blocked) {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: {}密码域当前为“需重建”，但尚未获得用户的格式化授权",
                    part.geometry.role.label()
                ),
            ));
        }
        if part.disposition == RegionDisposition::Rebuild
            && part.geometry.role != PartitionRole::CompatibilityReserve
            && !request.format.choice(part.geometry.role).0
        {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: {}为 Rebuild，必须显式启用完整文件系统初始化；拒绝 K_new + old ciphertext",
                    part.geometry.role.label()
                ),
            ));
        }
    }
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
    .and_then(|metadata| metadata.with_lba8_identity(request.lba8_identity.clone()))
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘身份字段无效: {message}")))?;
    let profile = ProvisionProfile::canonical_v1().with_pass_info_policy(pass_info_policy);
    let spec = ProvisionSpec::new(target, metadata, profile)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘元数据无法编码: {message}")))?;
    let filesystems = effective_target_filesystems(&request.format, &target_plan);
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
            RegionDisposition::PreserveOpaque => {
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
            RegionDisposition::PreserveVerified => {
                file_keys.push([0; 16]);
                if let Some(record) = part.preserved_record {
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
                } else if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: {}密码域保留计划缺少来源 key record",
                            part.geometry.role.label()
                        ),
                    ));
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
    let format_targets = format_result.map_err(|error| {
        err(
            EXIT_TARGET,
            format!("错误: 无法构造目标格式化计划: {error}"),
        )
    })?;
    let expected_serial_digest = if format_targets.iter().any(|choice| choice.selected) {
        let evidence = super::super::media_identity::serial_digest_evidence(
            runner.hardware_serial(disk).as_deref(),
        );
        if evidence.quality != super::super::media_identity::SerialQuality::Usable {
            return Err(err(
                EXIT_TARGET,
                "错误: 无法读取可用 USB 硬件序列号，拒绝安排格式化",
            ));
        }
        evidence.sha256
    } else {
        None
    };
    let entropy = ProvisionEntropy::new(random_array::<252>()?);
    let write_image =
        build_official_provision_protocol_image(&spec, &entropy, &plan).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法构造目标协议镜像: {message}"),
            )
        })?;

    for key in &mut file_keys {
        key.fill(0);
    }
    validate_key_disposition_plan(&target_plan, &plan, &format_targets)?;
    validate_target_write_set(&target_plan, &write_image.patch, &format_targets)?;
    Ok(PreparedNewProvision {
        disk,
        device_id,
        source_kind,
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
            let plan = request.resolve_typed(total_sectors).map_err(|error| {
                err(
                    EXIT_TARGET,
                    format!("错误: 无法构造 Plain 分区计划: {error}"),
                )
            })?;
            prepare_plain_provision(runner, disk, plan, dev)
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
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let geometry = target_session.writable_geometry()?;
    let total_sectors = geometry
        .writable_protocol_sectors()
        .map_err(|message| err(EXIT_TARGET, message))?;
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
    let source_identity =
        classify_live_source_identity(runner, disk, &source_metadata, total_sectors, dev)?;
    let source_kind = source_identity.protocol.provision_kind.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 来源盘型未确认；拒绝把未知/损坏介质恢复为 Plain",
        )
    })?;
    let source_device_id = source_protocol_device_id(&target, &source_identity);
    let before_pin = MediaIdentityPin::new(source_identity, &source_metadata);
    let source_lce = if source_kind == crate::provision::DiskProvisionKind::Plain {
        None
    } else {
        let geometry =
            parse_lba7_compatibility_geometry(&source_metadata, &source_device_id, total_sectors)
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

    let mut volume_serials = Vec::with_capacity(plan.partitions.len());
    for _ in &plan.partitions {
        volume_serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let write_plan = build_plain_provision_write_plan(&plan, source_lce, &volume_serials).map_err(
        |message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法构造 Plain 写盘计划: {message}"),
            )
        },
    )?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::edpf::EdpPartitionType;

    fn target_part(
        role: PartitionRole,
        partition_type: EdpPartitionType,
        filesystem: FilesystemKind,
        disposition: RegionDisposition,
    ) -> crate::provision::TargetPartitionPlan {
        crate::provision::TargetPartitionPlan {
            geometry: crate::provision::TargetPartitionGeometry {
                role,
                partition_type,
                start_lba: 63,
                sector_count: 20_417,
                physically_encrypted: false,
                filesystem: Some(filesystem),
            },
            action: if disposition == RegionDisposition::Rebuild {
                PartitionAction::Rebuild
            } else {
                PartitionAction::PreserveExact
            },
            disposition,
            password_disposition: None,
            source_password_knowledge: None,
            target_password_policy: None,
            reason: String::new(),
            preserved_record: None,
        }
    }

    #[test]
    fn observed_protocol_device_id_wins_for_existing_edp_source_parsing() {
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: crate::platform::NativeTransport::Bot,
            windows_pnp_instance_id: None,
            inquiry: Some(crate::platform::InquiryInfo {
                vendor: "aigo".into(),
                product: "U335".into(),
                revision: "1100".into(),
            }),
        };
        let target = TargetIdentity::from_probe(&probe, 15_728_640).unwrap();
        assert_eq!(target.device_id(), "disk&ven_aigo&prod_u335");

        let mut source_identity = crate::media_identity::MediaIdentitySnapshot::default();
        source_identity.protocol.device_id = Some("disk&ven_aigo&prod_u335&rev_1100".into());
        assert_eq!(
            source_protocol_device_id(&target, &source_identity),
            "disk&ven_aigo&prod_u335&rev_1100"
        );
    }

    #[test]
    fn rebuild_uses_explicit_requested_filesystems_instead_of_stale_source_geometry() {
        let format = FormatOptions {
            boot: true,
            share: true,
            encrypt: true,
            boot_label: "BOOT".into(),
            share_label: "SHARE".into(),
            encrypt_label: "ENCRYPT".into(),
            boot_fs: FilesystemKind::Fat16,
            share_fs: FilesystemKind::Fat32,
            encrypt_fs: FilesystemKind::ExFat,
        };
        let target_plan = TargetProvisionPlan {
            mode: OfficialPartitionMode::DefaultThreePartition,
            partitions: vec![
                target_part(
                    PartitionRole::Boot,
                    EdpPartitionType::Boot,
                    FilesystemKind::ExFat,
                    RegionDisposition::Rebuild,
                ),
                target_part(
                    PartitionRole::Share,
                    EdpPartitionType::Share,
                    FilesystemKind::ExFat,
                    RegionDisposition::Rebuild,
                ),
                target_part(
                    PartitionRole::Encrypt,
                    EdpPartitionType::Encrypt,
                    FilesystemKind::Fat16,
                    RegionDisposition::Rebuild,
                ),
            ],
            unallocated_sectors: 0,
        };

        let filesystems = effective_target_filesystems(&format, &target_plan);
        assert_eq!(filesystems.boot, FilesystemKind::Fat16);
        assert_eq!(filesystems.share, FilesystemKind::Fat32);
        assert_eq!(filesystems.encrypt, FilesystemKind::ExFat);
    }

    #[test]
    fn preserved_partition_keeps_verified_source_filesystem() {
        let format = FormatOptions {
            share_fs: FilesystemKind::Fat32,
            ..FormatOptions::default()
        };
        let target_plan = TargetProvisionPlan {
            mode: OfficialPartitionMode::DefaultThreePartition,
            partitions: vec![target_part(
                PartitionRole::Share,
                EdpPartitionType::Share,
                FilesystemKind::ExFat,
                RegionDisposition::PreserveVerified,
            )],
            unallocated_sectors: 0,
        };

        let filesystems = effective_target_filesystems(&format, &target_plan);
        assert_eq!(filesystems.share, FilesystemKind::ExFat);
    }
}
