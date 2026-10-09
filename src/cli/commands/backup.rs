use super::super::*;

pub(in crate::cli) fn backup_create_real_flow(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    backup_dir_flag: Option<String>,
) -> i32 {
    if let Some(disk) = disk_opt {
        if let Err(error) = guard_usb_disk(runner, disk) {
            eprintln!("{}", crate::ui::red(&error.msg));
            return error.code;
        }
    }

    if !elevate::is_root() {
        let mut prompt = StdPrompter;
        let disk = match DeviceSelector::new(disk_opt).resolve(runner, &mut prompt) {
            Ok(disk) => disk,
            Err(error) => {
                eprintln!("{}", crate::ui::red(&error.msg));
                return error.code;
            }
        };
        let mut argv = argv_with_backup_dir_for_elevation(backup_dir_flag.as_deref());
        DeviceSelector::new(disk_opt).pin_argv(&mut argv, disk);
        return elevate::ensure_elevated(&argv);
    }

    let mut prompt = StdPrompter;
    let disk = match DeviceSelector::new(disk_opt).resolve(runner, &mut prompt) {
        Ok(disk) => disk,
        Err(error) => {
            eprintln!("{}", crate::ui::red(&error.msg));
            return error.code;
        }
    };
    finish(
        crate::application::write::backup_create_on_disk(
            runner,
            disk,
            crate::application::resolve_backup_dir(backup_dir_flag.as_deref()),
            &mut prompt,
            None,
            None,
        )
        .map(|report| {
            let native_evidence = crate::edpb::verify_file(&report.path)
                .is_ok_and(|verified| verified.manifest.schema == "edpb.manifest.v4");
            if native_evidence {
                println!(
                    "4Kn 只读取证完成：{} 个分区首块、完整原生 EDP 协议及 LCE；暂不支持恢复写盘或保证文件系统完整恢复",
                    report.partition_count
                );
            } else {
                println!(
                    "元数据备份完成：{} 个分区；分区结构已保存；EDP 协议{}；文件系统与用户数据不包含",
                    report.partition_count,
                    if report.edp_protocol_saved { "已保存" } else { "不适用" }
                );
            }
            EXIT_OK
        }),
    )
}

/// backup restore 的公共外壳:
///   1) 显式目标的系统盘拒绝无需管理员权限，提权前先判；
///   2) 未提权时把目标统一固定为平台原生选择器；未给 --disk 时先以用户身份选盘；
///   3) 提权路径：（必要时交互选盘）→ 打开平台裸盘设备 → 执行流程。
pub(in crate::cli) fn real_flow(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    backup_dir_flag: Option<String>,
    bin: Option<String>,
    yes: bool,
) -> i32 {
    if let Some(n) = disk_opt {
        if let Err(e) = guard_usb_disk(runner, n) {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    }
    if !elevate::is_root() {
        // 不依赖提权后的环境继承：备份目录转为显式旗标随 argv 过界。
        let mut argv = argv_with_backup_dir_for_elevation(backup_dir_flag.as_deref());
        let pinned_disk = match disk_opt {
            Some(n) => n,
            None => {
                let mut sp = StdPrompter;
                match auto_pick_disk(runner, &mut sp) {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("{}", crate::ui::red(&e.msg));
                        return e.code;
                    }
                }
            }
        };
        DeviceSelector::new(disk_opt).pin_argv(&mut argv, pinned_disk);
        return elevate::ensure_elevated(&argv);
    }
    let bak = crate::application::resolve_backup_dir(backup_dir_flag.as_deref());
    // 手动管理员会话且旗标/配置都未命中时，明确告知备份去向。
    // 自动提权的子进程带哨兵，不重复提示。
    let has_sentinel = std::env::args().any(|a| a == ELEVATED_FLAG);
    if !has_sentinel
        && crate::platform::has_elevation_origin()
        && backup_dir_flag.is_none()
        && std::env::var("EDPCLI_BACKUP_DIR")
            .unwrap_or_default()
            .is_empty()
        && !crate::application::has_configured_backup_dir()
    {
        let cwd_bak = std::env::current_dir().unwrap_or_default().join("backup");
        eprintln!(
            "{}",
            crate::ui::yellow(&format!(
                "注意: 当前管理员会话未继承 $EDPCLI_BACKUP_DIR，备份将落在 {}。建议直接 edpcli <子命令>（自动提权），或在 ~/.edpcli.conf 写 backup_dir 固定目录",
                cwd_bak.display()
            ))
        );
    }
    let mut std_prompter = StdPrompter;
    let mut always = AlwaysYes(StdPrompter); // 无状态, 独立实例
    let prompter: &mut dyn Prompter = if yes { &mut always } else { &mut std_prompter };
    let n = match disk_opt {
        Some(n) => n,
        None => match auto_pick_disk(runner, &mut *prompter) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("{}", crate::ui::red(&e.msg));
                return e.code;
            }
        },
    };
    let outcome = match crate::application::write::restore_on_disk_typed(
        runner, bin, n, bak, prompter, None, None,
    ) {
        Ok(outcome) => outcome,
        Err(error) => return finish(Err(error)),
    };
    loop {
        let choice = prompter.prompt_line("选择要处理的分区编号（回车结束）: ");
        let choice = choice.trim();
        if choice.is_empty() {
            break;
        }
        let Ok(index) = choice.parse::<u32>() else {
            eprintln!("分区编号无效: {choice}");
            continue;
        };
        let Some(partition) = outcome
            .assessment
            .partitions
            .iter()
            .find(|partition| partition.index == index)
        else {
            eprintln!("恢复后评估中没有分区 {index}");
            continue;
        };
        if !partition.requires_original_key
            && partition.state
                != crate::application::post_restore::PostRestorePartitionState::NeedsFormat
        {
            eprintln!("分区 {index} 当前不能安全格式化");
            continue;
        }
        let reinitialize = if partition.requires_original_key {
            match prompter
                .prompt_line(
                    "加密分区处理方式：original 沿用原密钥 / reinitialize 清空并重建 / 回车跳过: ",
                )
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "original" => false,
                "reinitialize" => true,
                "" => continue,
                other => {
                    eprintln!("未知处理方式: {other}");
                    continue;
                }
            }
        } else {
            false
        };
        if !reinitialize
            && partition.requires_original_key
            && !matches!(
                partition.state,
                crate::application::post_restore::PostRestorePartitionState::NeedsFormat
                    | crate::application::post_restore::PostRestorePartitionState::PasswordRequired
            )
        {
            eprintln!("分区 {index} 当前不能安全沿用原密钥格式化");
            continue;
        }
        let filesystem = prompter.prompt_line("选择空文件系统 fat16 / exfat（回车跳过）: ");
        let filesystem = match filesystem.trim().to_ascii_lowercase().as_str() {
            "fat16" => crate::filesystem::FilesystemKind::Fat16,
            "exfat" => crate::filesystem::FilesystemKind::ExFat,
            "" => continue,
            other => {
                eprintln!("当前 portable writer 不支持 {other}");
                continue;
            }
        };
        let request = crate::application::post_restore::PartitionFormatRequest {
            partition_index: index,
            filesystem,
        };
        let label = outcome
            .partitions
            .iter()
            .find(|candidate| candidate.index == index)
            .and_then(|candidate| candidate.volume_label_hint.as_deref())
            .unwrap_or("恢复卷");
        if reinitialize {
            let new_password = prompter.prompt_secret("新密码（输入时不回显，回车取消）: ");
            let confirmation = prompter.prompt_secret("确认新密码（输入时不回显）: ");
            let reinitialize_request =
                match crate::application::post_restore::EncryptedPartitionReinitializeRequest::new(
                    index,
                    new_password.as_bytes(),
                    confirmation.as_bytes(),
                ) {
                    Ok(request) => request,
                    Err(message) => {
                        eprintln!("分区 {index} 未进入重建: {message}");
                        continue;
                    }
                };
            let result = crate::application::post_restore::reinitialize_encrypted_partition_after_restore_on_disk(
                runner,
                n,
                prompter,
                &outcome,
                &reinitialize_request,
                filesystem,
                label,
            );
            match result.result {
                Ok(()) => println!("分区 {index} 已使用新密码和新 FileKey 重建；读回验证通过"),
                Err(error) => {
                    eprintln!("分区 {index} 清空重建失败: {error}；已停止后续写入");
                    return error.exit_code();
                }
            }
            // The restored-protocol resume pin predates the new key records.
            break;
        } else if partition.requires_original_key {
            let password = if partition.state
                == crate::application::post_restore::PostRestorePartitionState::PasswordRequired
            {
                Some(prompter.prompt_secret("输入原密码（输入时不回显，回车取消）: "))
            } else {
                None
            };
            let result =
                crate::application::post_restore::format_encrypted_partition_after_restore_on_disk(
                    runner,
                    n,
                    prompter,
                    &outcome,
                    &request,
                    password
                        .as_ref()
                        .map(crate::provision::SecretBytes::as_bytes),
                    label,
                );
            match result.result {
                Ok(()) => println!("分区 {index} 使用原密钥域格式化成功，读回验证通过"),
                Err(crate::application::post_restore::EncryptedPostRestoreError::FileKey(
                    error,
                )) => {
                    eprintln!("分区 {index} 原密钥验证失败: {error}");
                    return crate::common::EXIT_TARGET;
                }
                Err(crate::application::post_restore::EncryptedPostRestoreError::Operation(
                    error,
                )) => {
                    eprintln!("分区 {index} 格式化失败: {error}；已停止后续写入");
                    return error.exit_code();
                }
            }
        } else {
            let result = crate::application::post_restore::format_partition_after_restore_on_disk(
                runner, n, prompter, &outcome, &request, label,
            );
            match result.result {
                Ok(()) => println!("分区 {index} 格式化成功，读回与重新评估均通过"),
                Err(error) => {
                    eprintln!("分区 {index} 格式化失败: {error}；已停止后续写入");
                    return error.exit_code();
                }
            }
        }
    }
    EXIT_OK
}
