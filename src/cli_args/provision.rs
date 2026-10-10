use super::*;

fn parse_provision_target(s: &str) -> Result<crate::provision::ProvisionTarget, String> {
    match s.to_ascii_lowercase().as_str() {
        "plain" => Ok(crate::provision::ProvisionTarget::Plain),
        "mode0" => Ok(crate::provision::ProvisionTarget::OFFICIAL[0]),
        "mode1" => Ok(crate::provision::ProvisionTarget::OFFICIAL[1]),
        "mode2" => Ok(crate::provision::ProvisionTarget::OFFICIAL[2]),
        "mode3" => Ok(crate::provision::ProvisionTarget::OFFICIAL[3]),
        _ => Err(format!(
            "错误: --target 只接受 mode0/mode1/mode2/mode3/plain，得到 {s}"
        )),
    }
}

fn parse_provision_filesystem(value: &str) -> Result<crate::filesystem::FilesystemKind, String> {
    let filesystem = crate::filesystem::FilesystemKind::from_config_token(value)
        .ok_or_else(|| format!("错误: 当前仅支持 fat16/fat32/exfat 文件系统，得到 {value}"))?;
    crate::filesystem::validate_writable_filesystem(filesystem)
        .map_err(|_| format!("错误: 当前仅支持 fat16/fat32/exfat 文件系统，得到 {value}"))?;
    Ok(filesystem)
}

fn parse_plain_partition(
    value: &str,
) -> Result<crate::application::provision::PlainPartitionRequest, String> {
    let mut fields = value.splitn(4, ':');
    let start = fields
        .next()
        .ok_or_else(|| "错误: --partition 缺少 start LBA".to_string())?;
    let size = fields
        .next()
        .ok_or_else(|| "错误: --partition 格式应为 START:SIZE:FS[:LABEL]".to_string())?;
    let filesystem = fields
        .next()
        .ok_or_else(|| "错误: --partition 格式应为 START:SIZE:FS[:LABEL]".to_string())?;
    let volume_label = fields.next().unwrap_or("普通卷").to_string();

    let start_lba = parse_positive_u64(start, "--partition START")?;
    let lower_size = size.to_ascii_lowercase();
    let size = if lower_size == "fill" {
        crate::application::provision::PlainPartitionSize::Fill
    } else if let Some(value) = lower_size.strip_suffix("mib") {
        crate::application::provision::PlainPartitionSize::MiB(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix("gib") {
        crate::application::provision::PlainPartitionSize::GiB(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix("sectors") {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix('s') {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            size,
            "--partition SIZE",
        )?)
    };

    Ok(crate::application::provision::PlainPartitionRequest {
        start_lba,
        size,
        filesystem: parse_provision_filesystem(filesystem)?,
        volume_label,
    })
}

fn parse_provision_opts(
    rest: &[String],
    action: &str,
) -> Result<(ProvisionNewOpts, Option<String>, bool, Option<String>), String> {
    let mut disk = None;
    let mut target = None;
    let mut plain_partitions = Vec::new();
    let mut boot_mib = None;
    let mut boot_start_lba = None;
    let mut share_start_lba = None;
    let mut encrypt_start_lba = None;
    let mut boot_sectors = None;
    let mut share_mib = None;
    let mut share_sectors = None;
    let mut encrypt_mib = None;
    let mut encrypt_sectors = None;
    let mut label_id = None;
    let mut user = None;
    let mut dept = None;
    let mut label = None;
    let mut share_source_password: Option<crate::domain::secret::SecretText> = None;
    let mut share_target_password: Option<crate::domain::secret::SecretText> = None;
    let mut encrypt_source_password: Option<crate::domain::secret::SecretText> = None;
    let mut encrypt_target_password: Option<crate::domain::secret::SecretText> = None;
    let mut prompt_passwords = false;
    let mut format_boot = false;
    let mut format_share = false;
    let mut format_encrypt = false;
    let mut boot_label = None;
    let mut share_label = None;
    let mut encrypt_label = None;
    let mut boot_fs = None;
    let mut share_fs = None;
    let mut encrypt_fs = None;
    let mut force_change_password = None;
    let mut cancel_password_complexity_check = None;
    let mut max_share_password_errors = None;
    let mut max_encrypt_password_errors = None;
    let mut out = None;
    let mut backup_dir = None;
    let mut yes = false;
    let mut i = 0usize;
    while i < rest.len() {
        if crate::command_spec::option("provision", Some(action), flag_name(&rest[i])).is_none() {
            return Err(format!("错误: 未知 provision 参数 {}", flag_name(&rest[i])));
        }
        match flag_name(&rest[i]) {
            "--disk" => {
                let value = take_value(rest, &mut i, "--disk")?;
                set_once(&mut disk, parse_disk_spec(&value)?, "--disk")?;
            }
            "--target" => {
                let value = take_value(rest, &mut i, "--target")?;
                set_once(&mut target, parse_provision_target(&value)?, "--target")?;
            }
            "--partition" => {
                let value = take_value(rest, &mut i, "--partition")?;
                if plain_partitions.len() >= crate::provision::MAX_PLAIN_PARTITIONS {
                    return Err(format!(
                        "错误: 普通盘最多支持 {} 个 MBR 主分区",
                        crate::provision::MAX_PLAIN_PARTITIONS
                    ));
                }
                plain_partitions.push(parse_plain_partition(&value)?);
            }
            "--boot-mib" => {
                let value = take_value(rest, &mut i, "--boot-mib")?;
                set_once(
                    &mut boot_mib,
                    parse_positive_u64(&value, "--boot-mib")?,
                    "--boot-mib",
                )?;
            }
            "--boot-start-sector" => {
                let value = take_value(rest, &mut i, "--boot-start-sector")?;
                set_once(
                    &mut boot_start_lba,
                    parse_positive_u64(&value, "--boot-start-sector")?,
                    "--boot-start-sector",
                )?;
            }
            "--share-start-sector" => {
                let value = take_value(rest, &mut i, "--share-start-sector")?;
                set_once(
                    &mut share_start_lba,
                    parse_positive_u64(&value, "--share-start-sector")?,
                    "--share-start-sector",
                )?;
            }
            "--encrypt-start-sector" => {
                let value = take_value(rest, &mut i, "--encrypt-start-sector")?;
                set_once(
                    &mut encrypt_start_lba,
                    parse_positive_u64(&value, "--encrypt-start-sector")?,
                    "--encrypt-start-sector",
                )?;
            }
            "--boot-sectors" => {
                let value = take_value(rest, &mut i, "--boot-sectors")?;
                set_once(
                    &mut boot_sectors,
                    parse_positive_u64(&value, "--boot-sectors")?,
                    "--boot-sectors",
                )?;
            }
            "--share-mib" => {
                let value = take_value(rest, &mut i, "--share-mib")?;
                set_once(
                    &mut share_mib,
                    parse_positive_u64(&value, "--share-mib")?,
                    "--share-mib",
                )?;
            }
            "--share-sectors" => {
                let value = take_value(rest, &mut i, "--share-sectors")?;
                set_once(
                    &mut share_sectors,
                    parse_positive_u64(&value, "--share-sectors")?,
                    "--share-sectors",
                )?;
            }
            "--encrypt-mib" => {
                let value = take_value(rest, &mut i, "--encrypt-mib")?;
                set_once(
                    &mut encrypt_mib,
                    parse_positive_u64(&value, "--encrypt-mib")?,
                    "--encrypt-mib",
                )?;
            }
            "--encrypt-sectors" => {
                let value = take_value(rest, &mut i, "--encrypt-sectors")?;
                set_once(
                    &mut encrypt_sectors,
                    parse_positive_u64(&value, "--encrypt-sectors")?,
                    "--encrypt-sectors",
                )?;
            }
            "--label-id" => {
                let value = take_value(rest, &mut i, "--label-id")?;
                set_once(&mut label_id, value, "--label-id")?;
            }
            "--user" => {
                let value = take_value(rest, &mut i, "--user")?;
                set_once(&mut user, value, "--user")?;
            }
            "--dept" => {
                let value = take_value(rest, &mut i, "--dept")?;
                set_once(&mut dept, value, "--dept")?;
            }
            "--label" => {
                let value = take_value(rest, &mut i, "--label")?;
                set_once(&mut label, value, "--label")?;
            }
            "--share-source-password" => {
                let value = take_value(rest, &mut i, "--share-source-password")?;
                set_once(
                    &mut share_source_password,
                    value.into(),
                    "--share-source-password",
                )?;
            }
            "--share-target-password" => {
                let value = take_value(rest, &mut i, "--share-target-password")?;
                set_once(
                    &mut share_target_password,
                    value.into(),
                    "--share-target-password",
                )?;
            }
            "--encrypt-source-password" => {
                let value = take_value(rest, &mut i, "--encrypt-source-password")?;
                set_once(
                    &mut encrypt_source_password,
                    value.into(),
                    "--encrypt-source-password",
                )?;
            }
            "--encrypt-target-password" => {
                let value = take_value(rest, &mut i, "--encrypt-target-password")?;
                set_once(
                    &mut encrypt_target_password,
                    value.into(),
                    "--encrypt-target-password",
                )?;
            }
            "--prompt-passwords" => {
                set_switch(&mut prompt_passwords, &rest[i], "--prompt-passwords")?
            }
            "--format-boot" => set_switch(&mut format_boot, &rest[i], "--format-boot")?,
            "--format-share" => set_switch(&mut format_share, &rest[i], "--format-share")?,
            "--format-encrypt" => set_switch(&mut format_encrypt, &rest[i], "--format-encrypt")?,
            "--boot-label" => {
                let value = take_value(rest, &mut i, "--boot-label")?;
                set_once(&mut boot_label, value, "--boot-label")?;
            }
            "--share-label" => {
                let value = take_value(rest, &mut i, "--share-label")?;
                set_once(&mut share_label, value, "--share-label")?;
            }
            "--encrypt-label" => {
                let value = take_value(rest, &mut i, "--encrypt-label")?;
                set_once(&mut encrypt_label, value, "--encrypt-label")?;
            }
            "--boot-fs" => {
                let value = take_value(rest, &mut i, "--boot-fs")?;
                set_once(
                    &mut boot_fs,
                    parse_provision_filesystem(&value)?,
                    "--boot-fs",
                )?;
            }
            "--share-fs" => {
                let value = take_value(rest, &mut i, "--share-fs")?;
                set_once(
                    &mut share_fs,
                    parse_provision_filesystem(&value)?,
                    "--share-fs",
                )?;
            }
            "--encrypt-fs" => {
                let value = take_value(rest, &mut i, "--encrypt-fs")?;
                set_once(
                    &mut encrypt_fs,
                    parse_provision_filesystem(&value)?,
                    "--encrypt-fs",
                )?;
            }
            "--force-change-password" => {
                if rest[i] != "--force-change-password" {
                    return Err(format!(
                        "错误: --force-change-password 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if force_change_password.replace(true).is_some() {
                    return Err("错误: 强制改密策略重复或冲突指定".into());
                }
            }
            "--no-force-change-password" => {
                if rest[i] != "--no-force-change-password" {
                    return Err(format!(
                        "错误: --no-force-change-password 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if force_change_password.replace(false).is_some() {
                    return Err("错误: 强制改密策略重复或冲突指定".into());
                }
            }
            "--cancel-password-complexity-check" => {
                if rest[i] != "--cancel-password-complexity-check" {
                    return Err(format!(
                        "错误: --cancel-password-complexity-check 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if cancel_password_complexity_check.replace(true).is_some() {
                    return Err("错误: 密码复杂性验证策略重复或冲突指定".into());
                }
            }
            "--enforce-password-complexity-check" => {
                if rest[i] != "--enforce-password-complexity-check" {
                    return Err(format!(
                        "错误: --enforce-password-complexity-check 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if cancel_password_complexity_check.replace(false).is_some() {
                    return Err("错误: 密码复杂性验证策略重复或冲突指定".into());
                }
            }
            "--share-max-password-errors" => {
                let value = take_value(rest, &mut i, "--share-max-password-errors")?;
                set_once(
                    &mut max_share_password_errors,
                    value.parse::<u8>().map_err(|_| {
                        format!("错误: --share-max-password-errors 须为 0..255，得到 {value}")
                    })?,
                    "--share-max-password-errors",
                )?;
            }
            "--encrypt-max-password-errors" => {
                let value = take_value(rest, &mut i, "--encrypt-max-password-errors")?;
                set_once(
                    &mut max_encrypt_password_errors,
                    value.parse::<u8>().map_err(|_| {
                        format!("错误: --encrypt-max-password-errors 须为 0..255，得到 {value}")
                    })?,
                    "--encrypt-max-password-errors",
                )?;
            }
            "--out" => {
                let value = take_value(rest, &mut i, "--out")?;
                set_once(&mut out, value, "--out")?;
            }
            "--backup-dir" => {
                let value = take_value(rest, &mut i, "--backup-dir")?;
                set_once(&mut backup_dir, value, "--backup-dir")?;
            }
            "--yes" => set_switch(&mut yes, &rest[i], "--yes")?,
            other => return Err(format!("错误: provision 不认识选项 {other}")),
        }
        i += 1;
    }

    if prompt_passwords
        && (share_source_password.is_some()
            || share_target_password.is_some()
            || encrypt_source_password.is_some()
            || encrypt_target_password.is_some())
    {
        return Err("错误: --prompt-passwords 不能与 argv 密码参数混用".into());
    }
    let target = target.ok_or("错误: provision 必须指定 --target mode0|mode1|mode2|mode3|plain")?;
    if target == crate::provision::ProvisionTarget::Plain {
        let has_official_only = boot_mib.is_some()
            || boot_start_lba.is_some()
            || share_start_lba.is_some()
            || encrypt_start_lba.is_some()
            || boot_sectors.is_some()
            || share_mib.is_some()
            || share_sectors.is_some()
            || encrypt_mib.is_some()
            || encrypt_sectors.is_some()
            || label_id.is_some()
            || user.is_some()
            || dept.is_some()
            || label.is_some()
            || share_source_password.is_some()
            || share_target_password.is_some()
            || encrypt_source_password.is_some()
            || encrypt_target_password.is_some()
            || prompt_passwords
            || format_boot
            || format_share
            || format_encrypt
            || boot_label.is_some()
            || share_label.is_some()
            || encrypt_label.is_some()
            || boot_fs.is_some()
            || share_fs.is_some()
            || encrypt_fs.is_some()
            || force_change_password.is_some()
            || cancel_password_complexity_check.is_some()
            || max_share_password_errors.is_some()
            || max_encrypt_password_errors.is_some();
        if has_official_only {
            return Err(
                "错误: --target plain 只接受 --disk/--partition/--out/--yes/--backup-dir；密码及官方目标参数不能混用"
                    .into(),
            );
        }
        return Ok((
            ProvisionNewOpts {
                disk,
                target,
                plain_partitions,
                boot_start_lba: None,
                share_start_lba: None,
                encrypt_start_lba: None,
                boot_mib: None,
                boot_sectors: None,
                share_mib: None,
                share_sectors: None,
                encrypt_mib: None,
                encrypt_sectors: None,
                label_id: String::new(),
                user: String::new(),
                dept: String::new(),
                label: String::new(),
                share_source_password: Default::default(),
                share_target_password: Default::default(),
                encrypt_source_password: Default::default(),
                encrypt_target_password: Default::default(),
                prompt_passwords: false,
                format_boot: false,
                format_share: false,
                format_encrypt: false,
                boot_label: String::new(),
                share_label: String::new(),
                encrypt_label: String::new(),
                boot_fs: crate::filesystem::FilesystemKind::Fat16,
                share_fs: crate::filesystem::FilesystemKind::ExFat,
                encrypt_fs: crate::filesystem::FilesystemKind::ExFat,
                force_change_password: None,
                cancel_password_complexity_check: None,
                max_share_password_errors: None,
                max_encrypt_password_errors: None,
            },
            out,
            yes,
            backup_dir,
        ));
    }
    if !plain_partitions.is_empty() {
        return Err("错误: --partition 仅用于 --target plain".into());
    }
    if boot_mib.is_some() && boot_sectors.is_some() {
        return Err("错误: --boot-mib 与 --boot-sectors 不能同时指定".into());
    }
    if share_mib.is_some() && share_sectors.is_some() {
        return Err("错误: --share-mib 与 --share-sectors 不能同时指定".into());
    }
    if encrypt_mib.is_some() && encrypt_sectors.is_some() {
        return Err("错误: --encrypt-mib 与 --encrypt-sectors 不能同时指定".into());
    }
    if matches!(target.mode_number(), Some(1 | 2)) && boot_sectors.is_some() {
        return Err("错误: --boot-sectors 仅用于 mode0".into());
    }

    Ok((
        ProvisionNewOpts {
            disk,
            target,
            plain_partitions,
            boot_start_lba,
            share_start_lba,
            encrypt_start_lba,
            boot_mib,
            boot_sectors,
            share_mib,
            share_sectors,
            encrypt_mib,
            encrypt_sectors,
            label_id: label_id.unwrap_or_default(),
            user: user.unwrap_or_default(),
            dept: dept.unwrap_or_default(),
            label: label.unwrap_or_default(),
            share_source_password: share_source_password.unwrap_or_default(),
            share_target_password: share_target_password
                .unwrap_or_else(|| crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT.into()),
            encrypt_source_password: encrypt_source_password.unwrap_or_default(),
            encrypt_target_password: encrypt_target_password
                .unwrap_or_else(|| crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT.into()),
            prompt_passwords,
            format_boot,
            format_share,
            format_encrypt,
            boot_label: boot_label.unwrap_or_else(|| "启动区".into()),
            share_label: share_label.unwrap_or_else(|| "交换区".into()),
            encrypt_label: encrypt_label.unwrap_or_else(|| "保密区".into()),
            boot_fs: boot_fs.unwrap_or(crate::filesystem::FilesystemKind::Fat16),
            share_fs: share_fs.unwrap_or(crate::filesystem::FilesystemKind::ExFat),
            encrypt_fs: encrypt_fs.unwrap_or(crate::filesystem::FilesystemKind::ExFat),
            force_change_password,
            cancel_password_complexity_check,
            max_share_password_errors,
            max_encrypt_password_errors,
        },
        out,
        yes,
        backup_dir,
    ))
}

/// A separate offline image grammar, intentionally disconnected from --disk,
/// elevation and the old fixed-512B live-device preparation path.
fn parse_native_image_opts(rest: &[String]) -> Result<Parsed, String> {
    let mut out = None;
    let mut total_sectors = None;
    let mut sector_bytes = None;
    let mut target = None;
    let mut partitions = Vec::new();
    let mut synthetic_demo = false;
    let mut algorithm = None;

    let mut i = 0;
    while i < rest.len() {
        match flag_name(&rest[i]) {
            "--out" => {
                let value = take_value(rest, &mut i, "--out")?;
                set_once(&mut out, value, "--out")?;
            }
            "--total-sectors" => {
                let value = take_value(rest, &mut i, "--total-sectors")?;
                set_once(
                    &mut total_sectors,
                    parse_positive_u64(&value, "--total-sectors")?,
                    "--total-sectors",
                )?;
            }
            "--sector-bytes" => {
                let value = take_value(rest, &mut i, "--sector-bytes")?;
                let bytes = value
                    .parse::<u32>()
                    .map_err(|_| "错误: --sector-bytes 必须是512或4096".to_string())?;
                if !matches!(bytes, 512 | 4096) {
                    return Err("错误: --sector-bytes 当前仅认证512和4096".into());
                }
                set_once(&mut sector_bytes, bytes, "--sector-bytes")?;
            }
            "--target" => {
                let value = take_value(rest, &mut i, "--target")?;
                parse_provision_target(&value)?;
                set_once(&mut target, value, "--target")?;
            }
            "--synthetic-demo" => {
                if synthetic_demo {
                    return Err("错误: --synthetic-demo 重复".into());
                }
                synthetic_demo = true;
            }
            "--algorithm" => {
                let value = take_value(rest, &mut i, "--algorithm")?;
                let mode = match value.to_ascii_lowercase().as_str() {
                    "sms4" | "sm4" => crate::provision::FileKeyWrapMode::Sm4,
                    "aes" => crate::provision::FileKeyWrapMode::A7f0,
                    "aes-cross" | "aes_cross" => crate::provision::FileKeyWrapMode::Aes128Ecb,
                    _ => return Err("错误: 虚拟测试算法只接受 sms4/aes/aes-cross".into()),
                };
                set_once(&mut algorithm, mode, "--algorithm")?;
            }
            "--partition" => {
                let value = take_value(rest, &mut i, "--partition")?;
                if partitions.len() >= crate::provision::MAX_PLAIN_PARTITIONS {
                    return Err("错误: 普通MBR最多4个分区".into());
                }
                partitions.push(parse_plain_partition(&value)?);
            }
            other => {
                return Err(format!(
                    "错误: 离线原生镜像不接受参数{other}，尤其不允许--disk"
                ))
            }
        }
        i += 1;
    }
    let target = target.ok_or("错误: 离线原生镜像必须指定 --target")?;
    let out = out.ok_or("错误: 缺少 --out FILE")?;
    let total_sectors = total_sectors.ok_or("错误: 缺少 --total-sectors N")?;
    let sector_bytes = sector_bytes.ok_or("错误: 缺少 --sector-bytes 512|4096")?;
    if target == "plain" {
        if synthetic_demo || algorithm.is_some() {
            return Err("错误: Plain 原生镜像不接受 --synthetic-demo 或 --algorithm".into());
        }
        return Ok(Parsed::Provision(ProvisionAction::NativeImage {
            out,
            total_sectors,
            sector_bytes,
            partitions,
        }));
    }
    if !synthetic_demo || !partitions.is_empty() || sector_bytes != 4096 {
        return Err("错误: EDP原生新盘目前仅在4096B离线模拟模式接受 --synthetic-demo，且禁止 --partition；不支持实体盘".into());
    }
    let mode = parse_provision_target(&target)?
        .official_mode()
        .ok_or("错误: 离线EDP模拟必须是mode0..mode3")?;
    Ok(Parsed::Provision(ProvisionAction::NativeEdpDemoImage {
        out,
        total_sectors,
        mode,
        algorithm: algorithm.unwrap_or(crate::provision::FileKeyWrapMode::Sm4),
    }))
}

pub(super) fn parse_provision(rest: &[String]) -> Result<Parsed, String> {
    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Parsed::Help {
            topic: Some("provision".into()),
        });
    }
    let Some(action) = rest.first().map(String::as_str) else {
        return Err("错误: provision 需要动作 plan / image / write".into());
    };
    let tail = &rest[1..];
    if action == "verify-source" {
        let mut disk = None;
        let mut backup = None;
        let mut i = 0;
        while i < tail.len() {
            match flag_name(&tail[i]) {
                "--disk" => {
                    let arg = take_value(tail, &mut i, "--disk")?;
                    set_once(&mut disk, parse_disk_spec(&arg)?, "--disk")?;
                }
                "--backup" => {
                    let arg = take_value(tail, &mut i, "--backup")?;
                    set_once(&mut backup, arg, "--backup")?;
                }
                other => return Err(format!("错误: verify-source 不认识选项 {other}")),
            }
            i += 1;
        }
        return Ok(Parsed::Provision(ProvisionAction::VerifySource {
            disk: disk.ok_or("错误: verify-source 必须提供 --disk N")?,
            backup: backup.ok_or("错误: verify-source 必须提供 --backup FILE")?,
        }));
    }
    // The existing plan command gets a dedicated native 4Kn source-backed
    // path. Explicit EDPB is required so the legacy 512B live path stays
    // untouched; this path has no write/elevation bypass or password input.
    if action == "plan" && tail.iter().any(|a| flag_name(a) == "--source-backup") {
        let mut disk = None;
        let mut backup = None;
        let mut target = None;
        let mut i = 0;
        while i < tail.len() {
            match flag_name(&tail[i]) {
                "--disk" => {
                    let value = take_value(tail, &mut i, "--disk")?;
                    set_once(&mut disk, parse_disk_spec(&value)?, "--disk")?;
                }
                "--source-backup" => {
                    let value = take_value(tail, &mut i, "--source-backup")?;
                    set_once(&mut backup, value, "--source-backup")?;
                }
                "--target" => {
                    let value = take_value(tail, &mut i, "--target")?;
                    set_once(&mut target, value, "--target")?;
                }
                other => {
                    return Err(format!(
                        "错误: 4Kn来源预检不接受{other}；不允许写入或格式化参数"
                    ))
                }
            }
            i += 1;
        }
        if target.as_deref() != Some("mode1") {
            return Err("错误: 4Kn来源预检目前只接受--target mode1".into());
        }
        return Ok(Parsed::Provision(ProvisionAction::NativeMode1Plan {
            disk: disk.ok_or("错误: 4Kn来源预检必须提供--disk N")?,
            backup: backup.ok_or("错误: 4Kn来源预检必须提供--source-backup FILE.edpb")?,
        }));
    }
    if action == "image" && tail.iter().any(|a| flag_name(a) == "--source-backup") {
        let mut backup = None;
        let mut out = None;
        let mut target = None;
        let mut i = 0;
        while i < tail.len() {
            match flag_name(&tail[i]) {
                "--source-backup" => {
                    let value = take_value(tail, &mut i, "--source-backup")?;
                    set_once(&mut backup, value, "--source-backup")?;
                }
                "--out" => {
                    let value = take_value(tail, &mut i, "--out")?;
                    set_once(&mut out, value, "--out")?;
                }
                "--target" => {
                    let value = take_value(tail, &mut i, "--target")?;
                    set_once(&mut target, value, "--target")?;
                }
                other => {
                    return Err(format!(
                        "错误: 4Kn离线来源转换不接受 {other}；禁止--disk、密码或实体写入参数"
                    ))
                }
            }
            i += 1;
        }
        if target.as_deref() != Some("mode1") {
            return Err("错误: 来源备份离线转换只接受 --target mode1".into());
        }
        return Ok(Parsed::Provision(ProvisionAction::NativeMode1BackupImage {
            backup: backup.ok_or("错误: 缺少 --source-backup EDPB")?,
            out: out.ok_or("错误: 缺少 --out FILE")?,
        }));
    }
    // Explicit native geometry opts choose the isolated offline-only image
    // grammar; live USB plans and writes cannot consume these arguments.
    if action == "image"
        && tail
            .iter()
            .any(|v| matches!(flag_name(v), "--sector-bytes" | "--total-sectors"))
    {
        return parse_native_image_opts(tail);
    }
    match action {
        "plan" | "image" | "write" => {
            let (opts, out, yes, backup_dir) = parse_provision_opts(tail, action)?;
            match action {
                "plan" => {
                    if out.is_some() || yes || backup_dir.is_some() {
                        return Err(
                            "错误: provision plan 不接受 --out、--yes 或 --backup-dir".into()
                        );
                    }
                    Ok(Parsed::Provision(ProvisionAction::Plan(Box::new(opts))))
                }
                "image" => {
                    if yes || backup_dir.is_some() {
                        return Err("错误: provision image 不接受 --yes 或 --backup-dir".into());
                    }
                    let out = out.ok_or("错误: provision image 必须指定 --out FILE")?;
                    Ok(Parsed::Provision(ProvisionAction::Image {
                        opts: Box::new(opts),
                        out,
                    }))
                }
                "write" => {
                    if out.is_some() {
                        return Err("错误: provision write 不接受 --out".into());
                    }
                    Ok(Parsed::Provision(ProvisionAction::Write {
                        opts: Box::new(opts),
                        yes,
                        backup_dir,
                    }))
                }
                _ => unreachable!(),
            }
        }
        other => Err(format!(
            "错误: 未知 provision 动作: {other} (可用 plan / image / write)"
        )),
    }
}
