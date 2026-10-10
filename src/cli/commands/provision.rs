use super::super::*;
use std::path::Path;

fn provision_request(opts: &ProvisionNewOpts) -> crate::application::provision::ProvisionRequest {
    match opts.target {
        crate::provision::ProvisionTarget::Plain => {
            crate::application::provision::ProvisionRequest::Plain(
                crate::application::provision::PlainProvisionRequest {
                    partitions: opts.plain_partitions.clone(),
                },
            )
        }
        crate::provision::ProvisionTarget::Official(_) => {
            crate::application::provision::ProvisionRequest::Official(Box::new(
                crate::application::provision::OfficialProvisionRequest {
                    target: opts.target,
                    algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
                    boot_start_lba: opts.boot_start_lba,
                    share_start_lba: opts.share_start_lba,
                    encrypt_start_lba: opts.encrypt_start_lba,
                    boot_mib: opts.boot_mib,
                    boot_sectors: opts.boot_sectors,
                    share_mib: opts.share_mib,
                    share_sectors: opts.share_sectors,
                    encrypt_mib: opts.encrypt_mib,
                    encrypt_sectors: opts.encrypt_sectors,
                    label_id: opts.label_id.clone(),
                    user: opts.user.clone(),
                    dept: opts.dept.clone(),
                    label: opts.label.clone(),
                    lba8_identity: crate::provision::Lba8Identity::default(),
                    key_domains: crate::provision::KeyDomainSecrets::new(
                        crate::provision::KeyDomainSecretPair::new(
                            (!opts.share_source_password.is_empty())
                                .then_some(opts.share_source_password.as_bytes()),
                            (!opts.share_target_password.is_empty())
                                .then_some(opts.share_target_password.as_bytes()),
                        ),
                        crate::provision::KeyDomainSecretPair::new(
                            (!opts.encrypt_source_password.is_empty())
                                .then_some(opts.encrypt_source_password.as_bytes()),
                            (!opts.encrypt_target_password.is_empty())
                                .then_some(opts.encrypt_target_password.as_bytes()),
                        ),
                    ),
                    volume_label: opts.boot_label.clone(),
                    format: crate::application::provision::FormatOptions {
                        boot: opts.format_boot,
                        share: opts.format_share,
                        encrypt: opts.format_encrypt,
                        boot_label: opts.boot_label.clone(),
                        share_label: opts.share_label.clone(),
                        encrypt_label: opts.encrypt_label.clone(),
                        boot_fs: opts.boot_fs,
                        share_fs: opts.share_fs,
                        encrypt_fs: opts.encrypt_fs,
                    },
                    preserve_unformatted: opts.preserve_unformatted,
                    force_change_password: opts.force_change_password,
                    cancel_password_complexity_check: opts.cancel_password_complexity_check,
                    max_share_password_errors: opts.max_share_password_errors,
                    max_encrypt_password_errors: opts.max_encrypt_password_errors,
                },
            ))
        }
    }
}

fn provision_resolve_disk(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    prompt: &mut dyn Prompter,
) -> EdpCliResult<u32> {
    DeviceSelector::new(disk_opt).resolve(runner, prompt)
}

pub(in crate::cli) fn target_plan_summary_lines(
    plan: &crate::provision::TargetProvisionPlan,
) -> Vec<String> {
    use crate::provision::{RegionDisposition, SourcePasswordKnowledge};

    let mut lines = Vec::with_capacity(plan.partitions.len() * 2 + 1);
    for partition in &plan.partitions {
        let geometry = &partition.geometry;
        let end_lba = geometry
            .start_lba
            .checked_add(geometry.sector_count)
            .and_then(|end| end.checked_sub(1))
            .unwrap_or(u64::MAX);
        let fate = match partition.password_disposition {
            Some(crate::provision::PasswordDisposition::Passthrough(
                crate::provision::PassthroughBasis::Verified,
            )) => "Passthrough · verified · FileKey/wrapper/data extent unchanged",
            Some(crate::provision::PasswordDisposition::Passthrough(
                crate::provision::PassthroughBasis::OpaqueCompatible,
            )) => "Passthrough · opaque-compatible · original key material/data extent unchanged",
            Some(crate::provision::PasswordDisposition::Rewrap) => {
                "Rewrap · K_old unchanged · wrapper only"
            }
            Some(crate::provision::PasswordDisposition::Rebuild) => {
                "Rebuild · K_new + full filesystem initialization"
            }
            Some(crate::provision::PasswordDisposition::Blocked) => {
                "Blocked · rebuild required but not authorized"
            }
            None => match partition.disposition {
                RegionDisposition::PreserveOpaque => "Preserve · original extent unchanged",
                RegionDisposition::PreserveVerified => "Preserve · verified extent unchanged",
                RegionDisposition::RewrapVerified => "Preserve · extent unchanged",
                RegionDisposition::Rebuild => {
                    "Rebuild · K_new + 完整 filesystem initialization · 原数据不可原样保留"
                }
                RegionDisposition::Drop => "Drop · 来源区域不进入目标",
            },
        };
        let password = match partition.source_password_knowledge {
            Some(SourcePasswordKnowledge::DefaultVerified) => "source=DefaultVerified",
            Some(SourcePasswordKnowledge::UserVerified) => "source=UserVerified",
            Some(SourcePasswordKnowledge::Unknown) => "source=Unknown",
            None => "source=no-key-domain",
        };
        let password_action = match partition.password_disposition {
            Some(crate::provision::PasswordDisposition::Passthrough(_)) => "password=passthrough",
            Some(crate::provision::PasswordDisposition::Rewrap) => "password=rewrap",
            Some(crate::provision::PasswordDisposition::Rebuild) => "password=rebuild",
            Some(crate::provision::PasswordDisposition::Blocked) => "password=blocked",
            None => "password=no-key-domain",
        };
        lines.push(format!(
            "  {} type{} start={} end={} sectors={} · {}",
            geometry.role.label(),
            geometry.partition_type.raw(),
            geometry.start_lba,
            end_lba,
            geometry.sector_count,
            fate
        ));
        lines.push(format!(
            "    {} · {} · {}",
            password, password_action, partition.reason
        ));
    }
    lines.push(format!(
        "  unallocated={} sectors",
        plan.unallocated_sectors
    ));
    lines
}

fn print_new_provision_summary(prepared: &crate::application::provision::PreparedNewProvision) {
    let mode = crate::provision::ProvisionTarget::Official(prepared.mode)
        .mode_number()
        .expect("official mode always has a mode number");
    println!(
        "制盘计划: disk{} mode{}  device_id={}",
        prepared.disk, mode, prepared.device_id
    );
    println!(
        "目标扇区={}  LCE=LBA{}  计划写入={}扇区",
        prepared.write_image.total_sectors,
        prepared.lce_start_lba,
        prepared.write_image.touched_sector_count()
    );
    if let Some(target_plan) = &prepared.target_plan {
        println!("最终精确分区（TargetProvisionPlan）:");
        for line in target_plan_summary_lines(target_plan) {
            println!("{line}");
        }
    } else {
        println!("最终精确分区:");
        for choice in &prepared.format_targets {
            let geometry = &choice.target.geometry;
            let end_lba = geometry
                .start_sector
                .saturating_add(geometry.sector_count())
                .saturating_sub(1);
            println!(
                "  {} type{} start={} end={} sectors={}",
                choice.target.role.label(),
                geometry.partition_type.raw(),
                geometry.start_sector,
                end_lba,
                geometry.sector_count()
            );
        }
    }
    println!("制盘后格式化:");
    for choice in &prepared.format_targets {
        let target = &choice.target;
        println!(
            "  {}  {} type{} {} {}{} 卷标={}",
            if !target.format_capable {
                "—"
            } else if choice.selected {
                "☑"
            } else {
                "☐"
            },
            target.role.label(),
            target.geometry.partition_type.raw(),
            if !target.format_capable {
                "不可格式化"
            } else if target.physically_encrypted {
                "加密"
            } else {
                "明文"
            },
            choice
                .filesystem
                .map(|format| format.windows_format_name())
                .unwrap_or("—"),
            target
                .visible_mbr_type
                .map(|mbr| format!(" / MBR 0x{mbr:02X}"))
                .unwrap_or_default(),
            if target.format_capable {
                choice.volume_label.as_str()
            } else {
                "—"
            }
        );
    }
}

fn print_plain_provision_summary(prepared: &crate::application::provision::PreparedPlainProvision) {
    println!(
        "制盘计划: disk{} Plain  device_id={}",
        prepared.disk, prepared.device_id
    );
    println!(
        "目标扇区={}  分区={}  计划写入={}扇区",
        prepared.plan.total_sectors,
        prepared.plan.partitions.len(),
        prepared.write_plan.writes.len()
    );
    for (index, partition) in prepared.plan.partitions.iter().enumerate() {
        let end = partition
            .start_lba
            .saturating_add(partition.sector_count)
            .saturating_sub(1);
        println!(
            "  P{} start={} end={} sectors={} fs={} label={}",
            index + 1,
            partition.start_lba,
            end,
            partition.sector_count,
            partition.filesystem.windows_format_name(),
            partition.volume_label
        );
    }
    for gap in &prepared.plan.gaps {
        println!("  gap start={} sectors={}", gap.start_lba, gap.sector_count);
    }
    println!(
        "来源状态={}  来源 LCE cleanup={}",
        prepared.source_kind.short_name(),
        prepared
            .source_lce_start_lba
            .map(|lba| format!("LBA{lba}"))
            .unwrap_or_else(|| "无".into())
    );
}

fn print_provision_summary(prepared: &crate::application::provision::PreparedProvision) {
    match prepared {
        crate::application::provision::PreparedProvision::Official(prepared) => {
            print_new_provision_summary(prepared)
        }
        crate::application::provision::PreparedProvision::Plain(prepared) => {
            print_plain_provision_summary(prepared)
        }
        crate::application::provision::PreparedProvision::Native(native) => {
            println!(
                "原生统一制盘计划：disk{}，来源{:?}，目标{}，逻辑扇区{}B，写入{}块",
                native.disk,
                native.source,
                native.target.full_name(),
                native.plan.sector_bytes,
                native.plan.writes.len()
            );
            let names = |items: &[String]| {
                if items.is_empty() {
                    "无".to_owned()
                } else {
                    items.join("、")
                }
            };
            println!("来源数据丢弃：{}", names(&native.impact.source_discarded));
            println!("来源数据保留：{}", names(&native.impact.source_retained));
            println!("目标格式化：{}", names(&native.impact.target_formatted));
            println!("密钥操作：{}", names(&native.impact.key_operations));
        }
    }
}

/// Build a deterministic disposable OEM-layout candidate using synthetic
/// identity, FileKeys and passwords. Exposed only by --synthetic-demo and only
/// as a freshly created ordinary file, never through USB discovery or sudo.
fn export_synthetic_4kn_edp_demo(
    out: &str,
    total_sectors: u64,
    mode: crate::provision::OfficialPartitionMode,
    algorithm: crate::provision::FileKeyWrapMode,
) -> Result<(), String> {
    use crate::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use crate::provision::{
        wrap_file_key, wrap_legacy_lba7_file_key, OfficialPartitionSizes, OfficialProvisionPlan,
        OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
        TargetIdentity,
    };
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total_sectors)?;
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934")?,
        "VIRTUAL",
        "SYNTHETIC",
        "EDP OFFLINE DEMO",
    )?;
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1())?;
    let cylinders = total_sectors / (255 * 63);
    let compat = crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry(
        cylinders, 255, 63, 4096,
    )
    .ok_or("虚拟盘太小，无法定位EDP 4Kn原生LCE")?;
    let key = [0x42u8; 16]; // fixed, PUBLIC TEST-ONLY key, never production
    let options = crate::application::provision::FormatOptions {
        boot: matches!(
            mode,
            crate::provision::OfficialPartitionMode::DefaultThreePartition
                | crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition
        ),
        share: mode != crate::provision::OfficialPartitionMode::WholeDiskEncrypted,
        encrypt: mode != crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition,
        ..Default::default()
    };
    let plan = OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD, [0; 8]),
        wrap_file_key(
            crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD,
            key,
            algorithm,
        ),
    )?
    .with_filesystems(options.filesystems());
    let count = plan.format_targets_native(4096)?.len();
    crate::application::provision::native_image::export_native_edp_4kn_image(
        Path::new(out),
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &plan,
        &options,
        &vec![0x1234_5678; count],
        &vec![key; count],
    )
}

/// Build a full sparse Mode1 image from a verified 4Kn EDPB without opening
/// a USB device or producing a command capable of physical writes.
fn export_native_mode1_from_backup(backup: &str, out: &str) -> Result<(), String> {
    let plan = crate::application::provision::native_preflight::plan_native_mode1_from_backup(
        Path::new(backup),
    )?;
    crate::application::provision::native_image::export_native_plain_image(Path::new(out), &plan)
}

pub(in crate::cli) fn provision_flow(runner: &SysRunner, action: ProvisionAction) -> i32 {
    match action {
        ProvisionAction::SourceBackedPlan { opts, backup } => {
            let disk = opts
                .disk
                .expect("source-backed plan parser requires a disk");
            if let Err(error) = guard_usb_disk(runner, disk) {
                return finish(Err(error));
            }
            if !elevate::is_root() {
                let argv = SecretArgv(std::env::args().skip(1).collect());
                return elevate::ensure_elevated(&argv);
            }
            let verified =
                match crate::application::evidence::verify_native_backup_against_disk_readonly(
                    runner,
                    disk,
                    Path::new(&backup),
                ) {
                    Ok(verified) => verified,
                    Err(error) => return finish(Err(EdpCliError::new(EXIT_TARGET, error))),
                };
            println!(
                "只读来源认证通过：disk{}，{}B逻辑扇区，已逐块核对{}个原生来源证据块；以下目标计划使用统一Native规划器，不单独生成Mode1写集。",
                disk, verified.logical_sector_bytes, verified.verified_native_blocks
            );
            provision_flow(runner, ProvisionAction::Plan(opts))
        }
        ProvisionAction::VerifySource { disk, backup } => {
            if let Err(error) = guard_usb_disk(runner, disk) {
                return finish(Err(error));
            }
            if !elevate::is_root() {
                let argv = SecretArgv(std::env::args().skip(1).collect());
                return elevate::ensure_elevated(&argv);
            }
            match crate::application::evidence::verify_native_backup_against_disk_readonly(
                runner,
                disk,
                Path::new(&backup),
            ) {
                Ok(matched) => {
                    println!(
                        "原生只读来源比对通过: disk{}，{}个完整{}B原生块，设备总扇区={}；EDPB协议、LCE、分区首块与实盘一致。未卸载、未写盘、未解除实盘制盘门禁。",
                        matched.disk, matched.verified_native_blocks,
                        matched.logical_sector_bytes, matched.total_sectors,
                    );
                    EXIT_OK
                }
                Err(message) => finish(Err(crate::common::EdpCliError::new(EXIT_TARGET, message))),
            }
        }
        ProvisionAction::NativeEdpDemoImage {
            out,
            total_sectors,
            mode,
            algorithm,
        } => match export_synthetic_4kn_edp_demo(&out, total_sectors, mode, algorithm) {
            Ok(()) => {
                println!("4Kn EDP虚拟测试盘创建成功: {out}（{}原生4096B扇区、mode{}、EncryptMode={}；仅测试密钥/身份，不可保存真实数据；未访问USB）", total_sectors, mode as u8, algorithm.raw());
                EXIT_OK
            }
            Err(message) => finish(Err(crate::common::EdpCliError::new(EXIT_TARGET, message))),
        },

        ProvisionAction::NativeMode1BackupImage { backup, out } => {
            match export_native_mode1_from_backup(&backup, &out) {
                Ok(()) => {
                    println!("4Kn Mode0→Mode1离线增量写集稀疏镜像已生成: {out}；包含二合一明文exFAT文件系统元数据与原type4密钥/LCE保留；镜像不包含原保密区用户数据，不可整体dd写盘；未连接、卸载或写入任何U盘。");
                    EXIT_OK
                }
                Err(message) => finish(Err(crate::common::EdpCliError::new(EXIT_TARGET, message))),
            }
        }
        ProvisionAction::NativeImage {
            out,
            total_sectors,
            sector_bytes,
            partitions,
        } => {
            // No device lookup, no elevation, and no raw disk write path.
            // This branch can only create a brand-new ordinary file image.
            let plan = match crate::application::provision::native_image::plan_native_plain_image(
                total_sectors,
                sector_bytes,
                &partitions,
            ) {
                Ok(value) => value,
                Err(message) => {
                    return finish(Err(crate::common::EdpCliError::new(EXIT_TARGET, message)))
                }
            };
            match crate::application::provision::native_image::export_native_plain_image(
                Path::new(&out),
                &plan,
            ) {
                Ok(()) => {
                    println!("离线原生Plain制盘镜像: {}（{}个{}B扇区，{}个分区；仅新建普通文件，未访问USB）", out, total_sectors, sector_bytes, partitions.len().max(1));
                    EXIT_OK
                }
                Err(message) => finish(Err(crate::common::EdpCliError::new(EXIT_IO, message))),
            }
        }
        ProvisionAction::Plan(mut opts) => {
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv = SecretArgv(std::env::args().skip(1).collect());
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                return elevate::ensure_elevated(&argv);
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            if let Err(error) = prompt_provision_passwords(&mut opts, &mut prompt) {
                return finish(Err(error));
            }
            let request = provision_request(&opts);
            match crate::application::provision::native_flow::prepare_native_provision_on_disk(
                runner, disk, &request,
            ) {
                Ok(native) => {
                    print_provision_summary(
                        &crate::application::provision::PreparedProvision::Native(Box::new(native)),
                    );
                    EXIT_OK
                }
                Err(message) => finish(Err(EdpCliError::new(EXIT_TARGET, message))),
            }
        }
        ProvisionAction::Image { mut opts, out } => {
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv = SecretArgv(std::env::args().skip(1).collect());
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                return elevate::ensure_elevated(&argv);
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            if let Err(error) = prompt_provision_passwords(&mut opts, &mut prompt) {
                return finish(Err(error));
            }
            let request = provision_request(&opts);
            let native =
                match crate::application::provision::native_flow::prepare_native_provision_on_disk(
                    runner, disk, &request,
                ) {
                    Ok(value) => value,
                    Err(message) => return finish(Err(EdpCliError::new(EXIT_TARGET, message))),
                };
            let sector_bytes = native.plan.sector_bytes;
            let prepared =
                crate::application::provision::PreparedProvision::Native(Box::new(native));
            print_provision_summary(&prepared);
            match crate::application::provision::export_provision_image(Path::new(&out), &prepared)
            {
                Ok(()) => {
                    println!("原生稀疏制盘镜像写入完成：{}；{}B逻辑扇区，按来源影响保留或重建目标区域；未写入当前设备",
                        out, sector_bytes);
                    EXIT_OK
                }
                Err(error) => finish(Err(error)),
            }
        }
        ProvisionAction::Write {
            mut opts,
            yes,
            backup_dir,
            include_virtual,
        } => {
            crate::platform::set_include_virtual(include_virtual);
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv =
                    SecretArgv(argv_with_backup_dir_for_elevation(backup_dir.as_deref()));
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                return elevate::ensure_elevated(&argv);
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            if let Err(error) = prompt_provision_passwords(&mut opts, &mut prompt) {
                return finish(Err(error));
            }
            let request = provision_request(&opts);
            let prepared =
                match crate::application::provision::native_flow::prepare_native_provision_on_disk(
                    runner, disk, &request,
                ) {
                    Ok(value) => value,
                    Err(message) => return finish(Err(EdpCliError::new(EXIT_TARGET, message))),
                };
            println!(
                "原生统一制盘规划：disk{}，来源{:?} → 目标{}，逻辑扇区{}B，原生写集{}块。",
                disk,
                prepared.source,
                prepared.target.full_name(),
                prepared.plan.sector_bytes,
                prepared.plan.writes.len()
            );
            let names = |parts: &[String]| {
                if parts.is_empty() {
                    "无".to_owned()
                } else {
                    parts.join("、")
                }
            };
            println!("来源数据丢弃：{}", names(&prepared.impact.source_discarded));
            println!("来源数据保留：{}", names(&prepared.impact.source_retained));
            println!("目标格式化：{}", names(&prepared.impact.target_formatted));
            println!("密钥操作：{}", names(&prepared.impact.key_operations));
            let confirmed = if yes {
                true
            } else {
                prompt.confirm_yes(&crate::ui::bold(&format!(
                    "将按上方明确列示的来源数据丢弃/保留和目标格式化方案执行 disk{}（{}）；LBA0–12协议将更新，通过原生WAL事务提交。确认输入 YES: ",
                    disk, opts.target.full_name()
                )))
            };
            if !confirmed {
                return finish(Err(EdpCliError::new(EXIT_CANCELLED, "已取消(未写盘)")));
            }
            let root = crate::application::resolve_backup_dir(backup_dir.as_deref());
            let native =
                crate::application::provision::PreparedProvision::Native(Box::new(prepared));
            match crate::application::provision::commit_provision_with_backup_on_disk_with_progress(
                runner,
                &native,
                root,
                &mut prompt,
                &mut |event| {
                    if event.work.is_none() {
                        println!(
                            "[原生制盘 {:.2}%] {}：{}",
                            f64::from(event.overall.basis_points()) / 100.0,
                            event.phase.label(),
                            event.step.label()
                        );
                    }
                },
            ) {
                Ok(outcome) => {
                    println!(
                        "原生制盘完成：disk{}，原生写集与WAL已同步及回读；事务WAL：{}",
                        disk,
                        outcome.backup.path.display()
                    );
                    outcome.exit_code()
                }
                Err(error) => finish(Err(error)),
            }
        }
    }
}

fn prompt_provision_passwords(
    opts: &mut ProvisionNewOpts,
    prompt: &mut dyn Prompter,
) -> EdpCliResult<()> {
    if !opts.prompt_passwords {
        return Ok(());
    }
    fn read(
        prompt: &mut dyn Prompter,
        label: &str,
        required: bool,
    ) -> EdpCliResult<crate::domain::secret::SecretText> {
        let bytes = prompt.prompt_secret(label);
        if required && bytes.is_empty() {
            return Err(EdpCliError::new(EXIT_CANCELLED, "密码输入已取消或为空"));
        }
        std::str::from_utf8(bytes.as_bytes())
            .map_err(|_| EdpCliError::new(EXIT_USAGE, "密码必须是 UTF-8 文本"))?;
        // SAFETY: bytes validated above; transfer allocation directly to the secret text owner.
        Ok(unsafe { String::from_utf8_unchecked(bytes.into_vec()) }.into())
    }
    opts.share_source_password = read(prompt, "共享区原密码（无则留空）: ", false)?;
    opts.share_target_password = read(prompt, "共享区目标密码: ", true)?;
    opts.encrypt_source_password = read(prompt, "加密区原密码（无则留空）: ", false)?;
    opts.encrypt_target_password = read(prompt, "加密区目标密码: ", true)?;
    Ok(())
}
