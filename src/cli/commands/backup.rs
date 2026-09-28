use super::super::*;

pub(in crate::cli) fn backup_create_real_flow(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    backup_dir_flag: Option<String>,
    deep: bool,
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
        elevate::ensure_elevated(&argv);
        unreachable!();
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
            deep,
        )
        .map(|_| EXIT_OK),
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
        elevate::ensure_elevated(&argv); // 内部以子进程退出码结束, 不返回
        unreachable!();
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
    let r = crate::application::write::restore_on_disk(runner, bin, n, bak, prompter, None, None);
    finish(r)
}
