//! 命令行入口: 子命令解析、自动提权、交互提示、各处理器。
//!
//! 用法:
//!   edpcli                                     交互式 TTY 默认进入 TUI；非 TTY 等价于 list
//!   edpcli list                                列出外接盘(只读)
//!   edpcli info [备份.bin] [--disk N]          查看设备/备份详情
//!   edpcli apply --dry-run [--disk N]          预览改造
//!   edpcli apply [--disk N] [--force] [--yes]  实际写入
//!   edpcli backup restore [备份] [--disk N]    还原
//!   edpcli convert --dir <快照目录> --id <device_id> [--size GB] [--out <目录>]
//!
//! 实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
//! Kingston DT3.0 64G (每盘改前自动备份, 可随时 edpcli backup restore 还原)。

use std::io::{self, IsTerminal, Write};
use std::path::Path;

pub use crate::backup_cli::{backup_delete, backup_list, backup_prune, backup_verify};
use crate::cli_args::print_help;
pub use crate::cli_args::{
    parse_args, print_usage, BackupAction, DiskOpts, InfoOpts, InspectOpts, Parsed,
};
use crate::common::*;
use crate::completion;
#[cfg(test)]
use crate::diskio::SectorDev;
use crate::diskio::{self, raw_path, FileDev, SystemClock};
use crate::elevate::{self, ELEVATED_FLAG};
use crate::inspect_cli::inspect_flow;
use crate::metainfo_cli::info_flow;
use crate::sectors::convert;
use crate::selectors::DeviceSelector;
use crate::sysinfo::{ReadProbeCache, SysRunner};

// ══════════════════════════════════════════════════════════════════
// 1. 交互提示抽象(测试注入)
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
pub(crate) use crate::application::write::read_image;
pub use crate::application::write::{
    apply_flow, backup_create_flow, restore_flow, ApplyMode, Ctx, Prompter,
};
pub(crate) use crate::application::write::{auto_pick_disk, guard_usb_disk};
pub use crate::ui::{backup_menu_str, disk_menu_str};

pub struct StdPrompter;

impl Prompter for StdPrompter {
    fn prompt_line(&mut self, msg: &str) -> String {
        print!("{}", msg);
        let _ = io::stdout().flush();
        let mut s = String::new();
        let _ = io::stdin().read_line(&mut s);
        s
    }
    fn confirm_yes(&mut self, msg: &str) -> bool {
        self.prompt_line(msg).trim() == "YES"
    }
}

/// --yes: 一切确认自动通过。
pub struct AlwaysYes<P: Prompter>(pub P);

impl<P: Prompter> Prompter for AlwaysYes<P> {
    fn prompt_line(&mut self, msg: &str) -> String {
        self.0.prompt_line(msg)
    }
    fn confirm_yes(&mut self, _msg: &str) -> bool {
        true
    }
}

// ══════════════════════════════════════════════════════════════════
// 3. 外接盘一览
// ══════════════════════════════════════════════════════════════════
pub use crate::disk_scan::{print_disk_table, scan_disks, Row};

/// 离线转换(不碰真盘)。
pub fn convert_flow(
    dir: String,
    id: Option<String>,
    size: Option<f64>,
    out: Option<String>,
) -> i32 {
    let Some(id) = id else {
        eprintln!("错误: 离线模式需 --id <device_id>");
        return EXIT_USAGE;
    };
    let d = Path::new(&dir);
    let read = |lba: u32| -> EdpCliResult<Vec<u8>> { Ok(diskio::read_lba_file(d, lba)) };
    let result = match convert(&read, &id, size, true) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    };
    if let Some(out) = out {
        if let Err(e) = std::fs::create_dir_all(&out) {
            eprintln!("错误: 无法创建输出目录 {}: {}", out, e);
            return EXIT_IO;
        }
        for (lba, data) in [
            (0u32, &result.lba0),
            (6, &result.lba6),
            (7, &result.lba7),
            (12, &result.lba12),
        ] {
            let p = Path::new(&out).join(format!("LBA{:02}.bin", lba));
            if let Err(e) = std::fs::write(&p, data) {
                eprintln!("错误: 无法写入 {}: {}", p.display(), e);
                return EXIT_IO;
            }
        }
        if let Some(l9) = &result.lba9 {
            let p = Path::new(&out).join("LBA09.bin");
            if let Err(e) = std::fs::write(&p, l9) {
                eprintln!("错误: 无法写入 {}: {}", p.display(), e);
                return EXIT_IO;
            }
        }
        println!("\n产物已写入 {}/", out);
    }
    EXIT_OK
}

// ══════════════════════════════════════════════════════════════════
// 7. 入口
// ══════════════════════════════════════════════════════════════════
fn should_default_to_tui(argv: &[String]) -> bool {
    should_default_to_tui_for_test(
        argv.is_empty(),
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )
}

fn should_default_to_tui_for_test(
    argv_is_empty: bool,
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> bool {
    argv_is_empty && stdin_is_terminal && stdout_is_terminal
}

pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // 人直接在交互式终端输入 bare `edpcli` 时进入 TUI；管道、重定向和脚本
    // 继续走 CLI v2 的 bare=list 语义，避免破坏已有自动化。
    if should_default_to_tui(&argv) {
        return crate::tui::run();
    }

    let parsed = match parse_args(&argv) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{}", crate::ui::red(&msg));
            // 参数写错时只展示当前子命令的短帮助，避免每次都刷整页全局教程。
            if argv.first().map(String::as_str) != Some("__complete") {
                eprintln!();
                let topic = argv.first().map(String::as_str).filter(|cmd| {
                    matches!(
                        *cmd,
                        "list"
                            | "tui"
                            | "info"
                            | "apply"
                            | "backup"
                            | "inspect"
                            | "convert"
                            | "completion"
                    )
                });
                print_help(topic);
            }
            return EXIT_USAGE;
        }
    };
    let runner = SysRunner;
    match parsed {
        Parsed::Help { topic } => {
            print_help(topic.as_deref());
            EXIT_OK
        }
        Parsed::Completion { shell } => {
            print!("{}", completion::script(shell));
            EXIT_OK
        }
        Parsed::InternalComplete { kind, backup_dir } => {
            let probe = ReadProbeCache::new(&runner);
            for value in completion::dynamic_values(&kind, backup_dir.as_deref(), &probe) {
                println!("{}", value);
            }
            EXIT_OK
        }
        Parsed::Version { detailed } => {
            if detailed {
                println!("{}", crate::build_info::detailed());
            } else {
                println!("{}", crate::build_info::short());
            }
            EXIT_OK
        }
        Parsed::List { backup_dir } => list_flow(&runner, backup_dir),
        Parsed::Tui => crate::tui::run(),
        Parsed::Backup {
            action,
            keep,
            yes,
            backup_dir,
        } => {
            let bak = diskio::resolve_backup_dir(backup_dir.as_deref());
            match action {
                BackupAction::Create { disk } => backup_create_real_flow(&runner, disk, backup_dir),
                BackupAction::List => backup_list(&bak),
                BackupAction::Verify { target } => backup_verify(&bak, target.as_deref()),
                BackupAction::Restore { target, disk } => real_flow(
                    &runner,
                    disk,
                    None,
                    backup_dir,
                    FlowKind::Restore { bin: target, yes },
                ),
                BackupAction::Prune => backup_prune(&bak, keep, yes),
                BackupAction::Delete { targets } => {
                    let mut prompt = StdPrompter;
                    backup_delete(&bak, &targets, yes, &mut prompt)
                }
            }
        }
        Parsed::Inspect(opts) => {
            let probe = ReadProbeCache::new(&runner);
            inspect_flow(&probe, opts)
        }
        Parsed::Info(opts) => {
            let probe = ReadProbeCache::new(&runner);
            info_flow(&probe, opts)
        }
        Parsed::Convert { dir, id, size, out } => match dir {
            Some(d) => convert_flow(d, id, size, out),
            None => {
                eprintln!("错误: convert 需 --dir <快照目录>");
                EXIT_USAGE
            }
        },
        Parsed::Apply {
            opts,
            dry_run,
            force,
            yes,
        } => {
            let flow = if dry_run {
                FlowKind::Dry
            } else {
                FlowKind::Apply { force, yes }
            };
            real_flow(&runner, opts.disk, opts.size, opts.backup_dir, flow)
        }
    }
}

enum FlowKind {
    Dry,
    Apply { force: bool, yes: bool },
    Restore { bin: Option<String>, yes: bool },
}

pub(crate) fn argv_with_backup_dir_for_elevation(backup_dir_flag: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if backup_dir_flag.is_none() {
        argv.extend(diskio::backup_dir_argv_suffix(
            std::env::var("EDPCLI_BACKUP_DIR").ok(),
        ));
    }
    argv
}

fn list_needs_elevation(rows: &[Row], elevated: bool, has_sentinel: bool) -> bool {
    crate::application::device_scan_needs_elevation(rows, elevated, has_sentinel)
}

fn list_flow(runner: &SysRunner, backup_dir_flag: Option<String>) -> i32 {
    let bak = diskio::resolve_backup_dir(backup_dir_flag.as_deref());
    let rows = crate::application::scan_device_dashboard(runner, &bak);

    // 先无特权只读探测；只有真实遇到 PermissionDenied 才自动请求平台管理员授权。
    // 无外接盘时不会无意义弹授权提示；需要权限时由平台层负责交互并重执行自身。
    let has_sentinel = std::env::args().any(|arg| arg == ELEVATED_FLAG);
    if list_needs_elevation(&rows, elevate::is_root(), has_sentinel) {
        println!("检测到外接盘，但读取身份、姓名和部门等裸盘信息需要管理员权限。");
        let argv = argv_with_backup_dir_for_elevation(backup_dir_flag.as_deref());
        elevate::ensure_elevated(&argv);
        unreachable!();
    }

    print!("{}", print_disk_table(&rows));
    EXIT_OK
}

fn backup_create_real_flow(
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
    let mut dev = match FileDev::open_rdonly(&raw_path(disk)) {
        Ok(dev) => dev,
        Err(error) => {
            eprintln!(
                "错误: 无法只读打开 {}: {}（需要管理员权限？）",
                raw_path(disk),
                error
            );
            return EXIT_IO;
        }
    };
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt: &mut prompt,
        backup_dir: diskio::resolve_backup_dir(backup_dir_flag.as_deref()),
    };
    finish(backup_create_flow(disk, &mut ctx, &mut dev).map(|_| EXIT_OK))
}

/// apply/backup restore 的公共外壳:
///   1) 显式目标的系统盘拒绝无需管理员权限，提权前先判；
///   2) 未提权时把目标统一固定为平台原生选择器；未给 --disk 时先以用户身份选盘；
///   3) 提权路径：（必要时交互选盘）→ 打开平台裸盘设备 → 执行流程。
fn real_flow(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    size: Option<f64>,
    backup_dir_flag: Option<String>,
    kind: FlowKind,
) -> i32 {
    let yes = matches!(
        kind,
        FlowKind::Apply { yes: true, .. } | FlowKind::Restore { yes: true, .. }
    );
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
    let bak = diskio::resolve_backup_dir(backup_dir_flag.as_deref());
    // 手动管理员会话且旗标/配置都未命中时，明确告知备份去向。
    // 自动提权的子进程带哨兵，不重复提示。
    let has_sentinel = std::env::args().any(|a| a == ELEVATED_FLAG);
    if !has_sentinel
        && crate::platform::has_elevation_origin()
        && backup_dir_flag.is_none()
        && std::env::var("EDPCLI_BACKUP_DIR")
            .unwrap_or_default()
            .is_empty()
        && diskio::conf_backup_dir().is_none()
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
    let mut ctx = Ctx {
        runner,
        clock: &SystemClock,
        prompt: prompter,
        backup_dir: bak,
    };
    let n = match disk_opt {
        Some(n) => n,
        None => match auto_pick_disk(runner, &mut *ctx.prompt) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("{}", crate::ui::red(&e.msg));
                return e.code;
            }
        },
    };
    if let Err(e) = guard_usb_disk(runner, n) {
        eprintln!("{}", crate::ui::red(&e.msg));
        return e.code;
    }
    let mut dev = match FileDev::open_rdonly(&raw_path(n)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("错误: 无法打开 {}: {}（需要管理员权限？）", raw_path(n), e);
            return EXIT_IO;
        }
    };
    let r = match kind {
        FlowKind::Dry => apply_flow(ApplyMode::DryRun, n, size, &mut ctx, &mut dev),
        FlowKind::Apply { force, .. } => {
            apply_flow(ApplyMode::Write { force }, n, size, &mut ctx, &mut dev)
        }
        FlowKind::Restore { bin, .. } => restore_flow(bin, n, &mut ctx, &mut dev),
    };
    finish(r)
}

fn finish(r: EdpCliResult<i32>) -> i32 {
    match r {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}", crate::ui::red(&e.msg));
            e.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sectors::EdpfPartition;

    struct ShortSectorDev;

    impl SectorDev for ShortSectorDev {
        fn read_sector(&mut self, _lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![0u8; SECTOR - 1])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_rejects_short_sector_without_panicking() {
        let err = read_image(&mut ShortSectorDev).unwrap_err();
        assert_eq!(err.code, EXIT_IO);
        assert!(err.msg.contains("512B"), "{}", err.msg);
    }

    struct PatternSectorDev;

    impl SectorDev for PatternSectorDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![lba as u8; SECTOR])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_preserves_lba_zero_to_twelve_order() {
        let image = read_image(&mut PatternSectorDev).unwrap();
        assert_eq!(image.len(), METADATA_IMAGE_LEN);
        for lba in 0..METADATA_SECTOR_COUNT {
            assert!(
                image[lba * SECTOR..(lba + 1) * SECTOR]
                    .iter()
                    .all(|&byte| byte == lba as u8),
                "LBA{} 在拼接镜像中的位置错误",
                lba
            );
        }
    }

    #[test]
    fn default_tui_requires_bare_interactive_terminal() {
        assert!(should_default_to_tui_for_test(true, true, true));
        assert!(!should_default_to_tui_for_test(false, true, true));
        assert!(!should_default_to_tui_for_test(true, false, true));
        assert!(!should_default_to_tui_for_test(true, true, false));
    }

    #[test]
    fn parse_bare_and_subcommands() {
        assert!(matches!(
            parse_args(&[]).unwrap(),
            Parsed::List { backup_dir: None }
        ));
        assert!(matches!(
            parse_args(&["help".into()]).unwrap(),
            Parsed::Help { topic: None }
        ));
        assert!(matches!(
            parse_args(&["version".into()]).unwrap(),
            Parsed::Version { detailed: true }
        ));
        match parse_args(&[
            "apply".into(),
            "--disk".into(),
            "6".into(),
            "--force".into(),
            "--yes".into(),
        ])
        .unwrap()
        {
            Parsed::Apply {
                opts,
                dry_run,
                force,
                yes,
            } => {
                assert_eq!(opts.disk, Some(6));
                assert!(!dry_run);
                assert!(force && yes);
            }
            _ => panic!("应解析为 Apply"),
        }
        match parse_args(&[
            "apply".into(),
            "--dry-run".into(),
            "--disk".into(),
            "4".into(),
        ])
        .unwrap()
        {
            Parsed::Apply { opts, dry_run, .. } => {
                assert_eq!(opts.disk, Some(4));
                assert!(dry_run);
            }
            _ => panic!("应解析为 apply --dry-run"),
        }
        match parse_args(&["info".into(), "backup.bin".into()]).unwrap() {
            Parsed::Info(opts) => {
                assert_eq!(opts.backup.as_deref(), Some("backup.bin"));
            }
            _ => panic!("应解析为 info"),
        }
        match parse_args(&[
            "convert".into(),
            "--dir".into(),
            "d".into(),
            "--id".into(),
            "i".into(),
        ])
        .unwrap()
        {
            Parsed::Convert { dir, id, .. } => {
                assert_eq!(dir.as_deref(), Some("d"));
                assert_eq!(id.as_deref(), Some("i"));
            }
            _ => panic!(),
        }
        match parse_args(&[
            "inspect".into(),
            "--lba".into(),
            "6,7,12".into(),
            "--hex".into(),
            "--backup-dir".into(),
            "/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.lbas, vec![6, 7, 12]);
                assert!(opts.hex);
                assert_eq!(opts.backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 inspect"),
        }
        match parse_args(&[
            "inspect".into(),
            "x.bin".into(),
            "--lba".into(),
            "9".into(),
            "--raw".into(),
            "--id".into(),
            "disk&ven_x&prod_y".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.backup.as_deref(), Some("x.bin"));
                assert_eq!(opts.lbas, vec![9]);
                assert!(opts.raw);
                assert_eq!(opts.device_id.as_deref(), Some("disk&ven_x&prod_y"));
            }
            _ => panic!("应解析为 inspect backup"),
        }
        match parse_args(&[
            "backup".into(),
            "prune".into(),
            "--keep".into(),
            "0".into(),
            "--yes".into(),
            "--backup-dir=/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Backup {
                action: BackupAction::Prune,
                keep,
                yes,
                backup_dir,
            } => {
                assert_eq!(keep, 0);
                assert!(yes);
                assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 backup prune"),
        }
        match parse_args(&["backup".into(), "verify".into(), "2".into()]).unwrap() {
            Parsed::Backup {
                action: BackupAction::Verify { target },
                keep,
                yes,
                ..
            } => {
                assert_eq!(target.as_deref(), Some("2"));
                assert_eq!(keep, 2);
                assert!(!yes);
            }
            _ => panic!("应解析为 backup verify"),
        }
        match parse_args(&[
            "backup".into(),
            "delete".into(),
            "2,3,4".into(),
            "--yes".into(),
            "--backup-dir".into(),
            "/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Backup {
                action: BackupAction::Delete { targets },
                yes,
                backup_dir,
                ..
            } => {
                assert_eq!(targets, vec!["2,3,4"]);
                assert!(yes);
                assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 backup delete"),
        }
        assert!(matches!(
            parse_args(&["backup".into(), "create".into(), "--disk=4".into()]).unwrap(),
            Parsed::Backup {
                action: BackupAction::Create { disk: Some(4) },
                ..
            }
        ));
    }

    #[test]
    fn elevation_reexec_pins_explicit_disk_to_platform_selector() {
        let selector = crate::platform::disk_selector_value(6);

        let mut split = vec![
            "apply".to_string(),
            "--disk".to_string(),
            "6".to_string(),
            "--yes".to_string(),
        ];
        DeviceSelector::new(Some(6)).pin_argv(&mut split, 6);
        assert_eq!(split[2], selector);
        assert_eq!(
            split.iter().filter(|arg| arg.as_str() == "--disk").count(),
            1
        );

        let mut inline = vec![
            "apply".to_string(),
            "--dry-run".to_string(),
            "--disk=6".to_string(),
        ];
        DeviceSelector::new(Some(6)).pin_argv(&mut inline, 6);
        assert_eq!(inline[2], format!("--disk={selector}"));

        let mut automatic = vec![
            "backup".to_string(),
            "restore".to_string(),
            "--yes".to_string(),
        ];
        DeviceSelector::new(None).pin_argv(&mut automatic, 6);
        assert_eq!(
            automatic,
            vec!["backup", "restore", "--yes", "--disk", &selector]
        );
    }

    #[test]
    fn list_requests_elevation_only_for_permission_denied_rows() {
        let base = Row {
            disk: 6,
            size: 62_914_560_000,
            vid: "0dd8".into(),
            pid: "2005".into(),
            proto: "USB".into(),
            device_id: None,
            onlyid: None,
            dept: None,
            user: None,
            n_baks: 0,
            denied: false,
            probe_error: None,
            is_nopwd: false,
            partitions: None,
        };
        assert!(!list_needs_elevation(
            std::slice::from_ref(&base),
            false,
            false
        ));

        let denied = Row {
            denied: true,
            ..base
        };
        assert!(list_needs_elevation(
            std::slice::from_ref(&denied),
            false,
            false
        ));
        assert!(!list_needs_elevation(
            std::slice::from_ref(&denied),
            true,
            false
        ));
        assert!(!list_needs_elevation(&[denied], false, true));
    }

    #[test]
    fn parse_usage_errors() {
        assert!(parse_args(&["bogus".into()]).is_err());
        assert!(parse_args(&["run".into()]).is_err());
        assert!(parse_args(&["restore".into()]).is_err());
        assert!(parse_args(&["meta".into()]).is_err());
        assert!(parse_args(&["apply".into(), "--disk".into()]).is_err());
        assert!(parse_args(&["apply".into(), "--disk".into(), "x".into()]).is_err());
        assert!(parse_args(&["apply".into(), "--size".into(), "-3".into()]).is_err());
        assert!(parse_args(&["apply".into(), "--dry-run".into(), "--force".into()]).is_err());
        assert!(matches!(
            parse_args(&["backup".into()]).unwrap(),
            Parsed::Backup {
                action: BackupAction::List,
                ..
            }
        ));
        assert!(parse_args(&["backup".into(), "rm".into()]).is_err());
        assert!(parse_args(&["backup".into(), "delete".into(), "--yes".into(),]).is_err());
        assert!(parse_args(&[
            "backup".into(),
            "prune".into(),
            "--keep".into(),
            "-1".into()
        ])
        .is_err());
        assert!(parse_args(&[
            "backup".into(),
            "verify".into(),
            "a.bin".into(),
            "b.bin".into()
        ])
        .is_err());
        assert!(parse_args(&[
            "backup".into(),
            "verify".into(),
            "a.bin".into(),
            "--onlyid".into(),
            "1".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "backup".into(),
            "list".into(),
            "--onlyid".into(),
            "abc".into(),
        ])
        .is_err());
        assert!(parse_args(&["backup".into(), "list".into(), "--yes".into()]).is_err());
        assert!(parse_args(&[
            "inspect".into(),
            "--disk".into(),
            "4".into(),
            "x.bin".into(),
        ])
        .is_err());
        assert!(parse_args(&["inspect".into(), "--onlyid".into(), "1402259934".into(),]).is_err());
        assert!(parse_args(&[
            "inspect".into(),
            "--lba".into(),
            "7".into(),
            "--raw".into(),
            "--hex".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "backup".into(),
            "verify".into(),
            "--index".into(),
            "1".into(),
        ])
        .is_err());
        // 哨兵旗标被剥离
        assert!(matches!(
            parse_args(&[
                "apply".into(),
                "--disk".into(),
                "6".into(),
                "--_elevated".into()
            ])
            .unwrap(),
            Parsed::Apply { .. }
        ));
    }

    #[test]
    fn boolean_flags_reject_inline_values() {
        for argv in [
            vec!["apply", "--yes=no"],
            vec!["apply", "--force=false"],
            vec!["apply", "--dry-run=false"],
            vec!["backup", "prune", "--yes=0"],
            vec!["backup", "delete", "1", "--yes=no"],
            vec!["backup", "restore", "backup.bin", "--yes=false"],
            vec!["inspect", "--raw=true"],
            vec!["inspect", "--hex=1"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("布尔旗标带值必须报错");
            assert!(err.contains("不接受参数值"), "{err}");
        }
    }

    #[test]
    fn boolean_flags_reject_duplicates() {
        for argv in [
            vec!["apply", "--yes", "--yes"],
            vec!["apply", "--force", "--force"],
            vec!["apply", "--dry-run", "--dry-run"],
            vec!["backup", "prune", "--yes", "--yes"],
            vec!["backup", "restore", "backup.bin", "--yes", "--yes"],
            vec!["inspect", "--raw", "--raw"],
            vec!["inspect", "--hex", "--hex"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("布尔旗标重复必须报错");
            assert!(err.contains("重复"), "{err}");
        }
    }

    #[test]
    fn inspect_lba_is_limited_to_zero_through_twelve() {
        for bad in ["13", "14", "99", "4294967295"] {
            let args = vec!["inspect".to_string(), "--lba".to_string(), bad.to_string()];
            let err = parse_args(&args)
                .err()
                .expect("inspect 不应接受 LBA0-12 之外的扇区");
            assert!(err.contains("LBA") && err.contains("0-12"), "{err}");
        }
        for good in ["0", "4", "12"] {
            let args = vec!["inspect".to_string(), "--lba".to_string(), good.to_string()];
            assert!(parse_args(&args).is_ok(), "LBA{good} 应被接受");
        }
    }

    #[test]
    fn single_value_flags_reject_duplicates() {
        for argv in [
            vec!["apply", "--disk", "4", "--disk", "6"],
            vec!["apply", "--size", "10", "--size", "20"],
            vec!["backup", "restore", "--disk=4", "--disk=6"],
            vec!["backup", "create", "--disk=4", "--disk=6"],
            vec!["backup", "prune", "--keep", "1", "--keep", "2"],
            vec!["info", "--disk", "4", "--disk", "6"],
            vec!["inspect", "--lba", "1", "--lba", "2"],
            vec!["convert", "--dir", "a", "--dir", "b", "--id", "x"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("单值旗标重复必须报错");
            assert!(err.contains("重复"), "{err}");
        }
    }

    #[test]
    fn disk_table_rendering() {
        let parts = vec![
            EdpfPartition {
                ptype: 1,
                active: 1,
                enc: 0,
                start_lba: 32,
                size_bytes: 16_384,
            },
            EdpfPartition {
                ptype: 2,
                active: 1,
                enc: 1,
                start_lba: 63,
                size_bytes: 59_750_819_680,
            },
            EdpfPartition {
                ptype: 4,
                active: 0,
                enc: 1,
                start_lba: 116_707_328,
                size_bytes: 3_143_761_920,
            },
        ];
        let rows = vec![
            Row {
                disk: 4,
                size: 64_000_000_000,
                vid: "0951".into(),
                pid: "1666".into(),
                proto: "USB".into(),
                device_id: None,
                onlyid: None,
                dept: None,
                user: None,
                n_baks: 0,
                denied: false,
                probe_error: None,
                is_nopwd: false,
                partitions: None,
            },
            Row {
                disk: 6,
                size: 62_914_560_000,
                vid: "0dd8".into(),
                pid: "2005".into(),
                proto: "USB".into(),
                device_id: Some("disk&ven_netac&prod_onlydisk".into()),
                onlyid: Some("1402259934".into()),
                dept: Some("国网江苏省电力有限公司泰州供电公司".into()),
                user: Some("宋旭琳".into()),
                n_baks: 3,
                denied: false,
                probe_error: None,
                is_nopwd: true,
                partitions: Some(parts),
            },
            Row {
                disk: 7,
                size: 500_107_862_016,
                vid: "xxxx".into(),
                pid: "xxxx".into(),
                proto: "Thunderbolt".into(),
                device_id: None,
                onlyid: None,
                dept: None,
                user: None,
                n_baks: 0,
                denied: false,
                probe_error: None,
                is_nopwd: false,
                partitions: None,
            },
        ];
        let out = print_disk_table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "外接盘 3 个:");
        let disk4_line = lines.iter().find(|l| l.contains("disk4")).unwrap();
        assert!(disk4_line.contains("非 cems 盘"));
        let disk6_line = lines.iter().find(|l| l.contains("disk6")).unwrap();
        assert!(
            disk6_line.contains("cems盘")
                && disk6_line.contains("[免密]")
                && disk6_line.contains("宋旭琳")
                && disk6_line.contains("泰州供电公司"),
            "{}",
            disk6_line
        );
        // EDPF 明细行: 类型 + 大小 + LBA 范围
        let edpf = lines.iter().find(|l| l.contains("EDPF")).unwrap();
        assert!(
            edpf.contains("Share 59.75GB (LBA 63~116,700,881)"),
            "{}",
            edpf
        );
        assert!(
            edpf.contains("Encrypt 3.14GB (LBA 116,707,328~122,847,487)"),
            "{}",
            edpf
        );
        assert!(edpf.contains("Boot 0.00GB (LBA 32~63)"), "{}", edpf);
        let meta = lines.iter().find(|l| l.contains("onlyid")).unwrap();
        assert!(meta.contains("onlyid=1402259934") && meta.contains("备份 3 份"));
        let disk7_line = lines.iter().find(|l| l.contains("disk7")).unwrap();
        assert!(disk7_line.contains("非 USB") && disk7_line.contains("不支持"));
        assert_eq!(print_disk_table(&[]).trim(), "未检测到外接盘。");
    }

    #[test]
    fn menus_are_numbered() {
        use crate::sysinfo::ExtDisk;
        let disks = vec![
            ExtDisk {
                n: 4,
                size: 64_000_000_000,
                vid: "0951".into(),
                pid: "1666".into(),
                proto: "USB".into(),
            },
            ExtDisk {
                n: 6,
                size: 62_914_560_000,
                vid: "0dd8".into(),
                pid: "2005".into(),
                proto: "USB".into(),
            },
        ];
        let m = disk_menu_str(&disks);
        assert!(
            m.contains("编号") && m.contains("设备") && m.contains("VID:PID"),
            "{}",
            m
        );
        assert!(
            m.lines()
                .any(|line| line.contains("1") && line.contains("disk4")),
            "{}",
            m
        );
        assert!(
            m.lines()
                .any(|line| line.contains("2") && line.contains("disk6")),
            "{}",
            m
        );
        assert!(m.contains("disk4") && m.contains("64.00GB") && m.contains("0951:1666"));

        let b = backup_menu_str(&[
            ("2026-09-16 23:36".into(), true),
            ("2026-08-27 22:25".into(), false),
        ]);
        assert!(
            b.contains("编号") && b.contains("时间") && b.contains("状态"),
            "{}",
            b
        );
        assert!(
            b.lines()
                .any(|line| line.contains("1") && line.contains("2026-09-16 23:36")),
            "{}",
            b
        );
        assert!(b.contains("免密状态"));
        assert!(
            b.lines()
                .any(|line| line.contains("2") && line.contains("2026-08-27 22:25")),
            "{}",
            b
        );
        assert!(b.contains("加密原盘"));
    }
}
