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
                            Some(opts.share_target_password.as_bytes()),
                        ),
                        crate::provision::KeyDomainSecretPair::new(
                            (!opts.encrypt_source_password.is_empty())
                                .then_some(opts.encrypt_source_password.as_bytes()),
                            Some(opts.encrypt_target_password.as_bytes()),
                        ),
                    ),
                    volume_label: opts.volume_label.clone(),
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
    }
}

pub(in crate::cli) fn provision_flow(runner: &SysRunner, action: ProvisionAction) -> i32 {
    match action {
        ProvisionAction::Plan(opts) => {
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
                let mut argv: Vec<String> = std::env::args().skip(1).collect();
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            match crate::application::provision::prepare_provision_on_disk(runner, disk, &request) {
                Ok(prepared) => {
                    print_provision_summary(&prepared);
                    EXIT_OK
                }
                Err(error) => finish(Err(error)),
            }
        }
        ProvisionAction::Image { opts, out } => {
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
                let mut argv: Vec<String> = std::env::args().skip(1).collect();
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            let prepared = match crate::application::provision::prepare_provision_on_disk(
                runner, disk, &request,
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            print_provision_summary(&prepared);
            match crate::application::provision::export_provision_image(Path::new(&out), &prepared)
            {
                Ok(()) => {
                    println!("稀疏制盘镜像已写入 {}（已保留目标盘原始 LBA3）", out);
                    EXIT_OK
                }
                Err(error) => finish(Err(error)),
            }
        }
        ProvisionAction::Write {
            opts,
            yes,
            backup_dir,
        } => {
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
                let mut argv = argv_with_backup_dir_for_elevation(backup_dir.as_deref());
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            let prepared = match crate::application::provision::prepare_provision_on_disk(
                runner, disk, &request,
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            print_provision_summary(&prepared);
            let confirmed = if yes {
                true
            } else {
                prompt.confirm_yes(&crate::ui::bold(&format!(
                    "将按上述制盘计划写入 disk{}（{}）；将保留制造商 LBA3，并按共享 application 安全事务执行。输入 YES: ",
                    disk, opts.target.full_name()
                )))
            };
            if !confirmed {
                return finish(Err(EdpCliError::new(EXIT_CANCELLED, "已取消(未写盘)")));
            }
            let write = match crate::application::provision::commit_provision_with_backup_on_disk_with_progress(
                runner,
                &prepared,
                crate::application::resolve_backup_dir(backup_dir.as_deref()),
                &mut prompt,
                &mut |event| {
                    if event.work.is_none() {
                        println!(
                            "[制盘进度 {:.2}%] {}：{}",
                            f64::from(event.overall.basis_points()) / 100.0,
                            event.phase.label(),
                            event.step.label(),
                        );
                    }
                },
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let status = write.execution_status();
            for line in write.summary_lines() {
                println!("{line}");
            }
            status.exit_code()
        }
    }
}
