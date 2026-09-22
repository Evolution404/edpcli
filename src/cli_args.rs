//! 命令行参数模型与零依赖解析器。
//!
//! 只负责 argv → 结构化命令，不做 I/O、不提权、不访问磁盘；所有歧义参数在这里
//! 统一拒绝，避免执行层出现“后一个覆盖前一个”或布尔 flag 带值的危险语义。

use crate::completion::Shell;
use crate::elevate::ELEVATED_FLAG;

// ══════════════════════════════════════════════════════════════════
// 2. 参数解析(手写, 零依赖)
// ══════════════════════════════════════════════════════════════════
#[derive(Default)]
pub struct DiskOpts {
    pub disk: Option<u32>,
    pub size: Option<f64>,
    pub backup_dir: Option<String>,
}

#[derive(Default)]
pub struct InspectOpts {
    pub disk: Option<u32>,
    pub backup: Option<String>,
    pub lbas: Vec<u32>,
    pub raw: bool,
    pub hex: bool,
    pub export: Option<String>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

#[derive(Default)]
pub struct InfoOpts {
    pub disk: Option<u32>,
    pub backup: Option<String>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

pub enum Parsed {
    List {
        backup_dir: Option<String>,
    },
    Tui,
    Backup {
        action: BackupAction,
        keep: usize,
        yes: bool,
        backup_dir: Option<String>,
    },
    Inspect(InspectOpts),
    Info(InfoOpts),
    Apply {
        opts: DiskOpts,
        dry_run: bool,
        force: bool,
        yes: bool,
    },
    Convert {
        dir: Option<String>,
        id: Option<String>,
        size: Option<f64>,
        out: Option<String>,
    },
    Completion {
        shell: Shell,
    },
    InternalComplete {
        kind: String,
        backup_dir: Option<String>,
    },
    Version {
        detailed: bool,
    },
    Help {
        topic: Option<String>,
    },
}

pub enum BackupAction {
    Create {
        disk: Option<u32>,
        deep: bool,
    },
    List,
    Verify {
        target: Option<String>,
    },
    Restore {
        target: Option<String>,
        disk: Option<u32>,
    },
    Prune,
    Delete {
        targets: Vec<String>,
    },
}

pub fn usage_text() -> String {
    format!(
        "edpcli — EDP/cems U 盘管理 CLI v{}\n\n\
用法: edpcli [命令] [选项]\n\n\
常用:\n\
  list      查看当前插入的 U 盘\n\
  tui       交互式 TUI（Vim 键位）\n\
  info      查看 U 盘或备份详细信息\n\
  apply     预览或执行 U 盘改造\n\
  backup    创建、查看、校验、恢复和清理备份\n\
  inspect   高级：检查底层 LBA/hex 数据\n\n\
其他:\n\
  convert   高级：离线转换快照\n\
  completion Shell 补全\n\
  version   版本与构建信息\n\
  help      帮助\n\n\
交互式终端中无参数 edpcli 默认进入 TUI；管道/重定向等非 TTY 环境仍等价于 edpcli list。\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn print_usage() {
    print!("{}", usage_text());
}

fn print_topic_help(topic: &str) {
    use crate::ui::bold;
    match topic {
        "info" => {
            println!(
                "{}",
                bold("用法: edpcli info [备份.edpb] [--disk N] [--id DEVICE_ID] [--backup-dir D]")
            );
            println!("未指定来源且只有一个可用目标盘时自动选择；多盘时交互选择。");
        }
        "apply" => {
            println!("{}", bold("用法: edpcli apply [--dry-run] [--disk N] [--size GB] [--force] [--yes] [--backup-dir D]"));
            println!("--dry-run 只执行识别与布局计算，不提交写入。");
        }
        "inspect" => {
            println!("{}", bold("用法: edpcli inspect [备份.edpb] [--disk N] [--lba 6,7,12] [--hex|--raw] [--export DIR]"));
            println!("LBA 必须通过 --lba 显式指定；不指定时显示 LBA0-12 概览。");
        }
        "backup" => {
            println!("{}", bold("用法: edpcli backup [动作] [选项]"));
            println!("  backup create [--disk N] [--deep]     只读备份；--deep 增加文件系统分析");
            println!("  backup [list]                          查看备份");
            println!("  backup restore [编号|文件] [--disk N] 恢复备份");
            println!("  backup verify [编号|文件]              校验备份");
            println!("  backup delete [编号|文件]...           删除备份");
            println!("  backup prune [--keep N]                按策略清理");
        }
        "completion" => {
            println!("{}", bold("用法: edpcli completion <zsh|bash|fish>"));
            println!("zsh : eval \"$(edpcli completion zsh)\"");
            println!("bash: eval \"$(edpcli completion bash)\"");
            println!("fish: edpcli completion fish | source");
        }
        "list" => println!("{}", bold("用法: edpcli list [--backup-dir D]")),
        "tui" => println!("{}", bold("用法: edpcli tui")),
        "convert" => println!(
            "{}",
            bold("用法: edpcli convert --dir <快照目录> --id <device_id> [--size GB] [--out DIR]")
        ),
        _ => print_usage(),
    }
}

pub(crate) fn print_help(topic: Option<&str>) {
    match topic {
        Some(topic) => print_topic_help(topic),
        None => print_usage(),
    }
}

/// 取旗标值: 支持 `--flag 值` 与 `--flag=值`。
fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    if let Some(v) = args[*i].strip_prefix(&format!("{}=", flag)) {
        return Ok(v.to_string());
    }
    *i += 1;
    if *i >= args.len() {
        return Err(format!("错误: {} 缺少参数值", flag));
    }
    Ok(args[*i].clone())
}

fn parse_disk_spec(s: &str) -> Result<u32, String> {
    crate::platform::parse_disk_selector(s).map_err(|error| {
        format!(
            "错误: --disk {}: {}；本平台接受 {}",
            s,
            error,
            crate::platform::disk_selector_syntax()
        )
    })
}

fn parse_size(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(format!("错误: --size 须为正数(GB), 得到 {}", s)),
    }
}

fn parse_keep(s: &str) -> Result<usize, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: --keep 须为大于等于 0 的整数, 得到 {}", s));
    }
    s.parse::<usize>()
        .map_err(|_| format!("错误: --keep 超出范围: {}", s))
}

fn parse_lbas(s: &str) -> Result<Vec<u32>, String> {
    if s.is_empty() {
        return Err("错误: --lba 缺少 LBA 值".into());
    }
    let mut out = Vec::new();
    for token in s.split(',') {
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "错误: --lba 仅接受 0-12 的逗号分隔列表, 得到 {}",
                s
            ));
        }
        let lba = token
            .parse::<u32>()
            .map_err(|_| format!("错误: inspect LBA 仅支持 0-12, 得到 {}", token))?;
        if lba > crate::common::METADATA_LAST_LBA {
            return Err(format!("错误: inspect LBA 仅支持 0-12, 得到 {}", token));
        }
        if !out.contains(&lba) {
            out.push(lba);
        }
    }
    Ok(out)
}

fn flag_name(a: &str) -> &str {
    a.split('=').next().unwrap_or(a)
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("错误: {} 重复指定", flag));
    }
    *slot = Some(value);
    Ok(())
}

fn set_switch(slot: &mut bool, raw: &str, flag: &str) -> Result<(), String> {
    if raw != flag {
        return Err(format!("错误: {} 是布尔旗标，不接受参数值: {}", flag, raw));
    }
    if *slot {
        return Err(format!("错误: {} 重复指定", flag));
    }
    *slot = true;
    Ok(())
}

pub fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let args: Vec<&String> = argv
        .iter()
        .filter(|a| a.as_str() != ELEVATED_FLAG)
        .collect();
    let Some(first) = args.first() else {
        return Ok(Parsed::List { backup_dir: None });
    };
    let rest: Vec<String> = args[1..].iter().map(|s| (*s).clone()).collect();
    match first.as_str() {
        "-h" | "--help" => Ok(Parsed::Help { topic: None }),
        "help" => {
            if rest.len() > 1 {
                return Err("错误: help 最多接受一个子命令名称".into());
            }
            Ok(Parsed::Help {
                topic: rest.first().cloned(),
            })
        }
        "-V" | "--version" => Ok(Parsed::Version { detailed: false }),
        "version" => Ok(Parsed::Version { detailed: true }),
        "completion" => {
            if rest.is_empty() || rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("completion".into()),
                });
            }
            if rest.len() != 1 {
                return Err("错误: completion 只接受一个 shell: zsh / bash / fish".into());
            }
            let shell = Shell::parse(&rest[0]).ok_or_else(|| {
                format!("错误: 不支持的 shell: {} (可用 zsh / bash / fish)", rest[0])
            })?;
            Ok(Parsed::Completion { shell })
        }
        "__complete" => {
            let Some(kind) = rest.first().cloned() else {
                return Err("错误: __complete 缺少候选类型".into());
            };
            let mut backup_dir = None;
            let mut i = 1usize;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--backup-dir" => {
                        let v = take_value(&rest, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: __complete 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::InternalComplete { kind, backup_dir })
        }
        "tui" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("tui".into()),
                });
            }
            crate::tui::parse_resume_args(argv)?;
            Ok(Parsed::Tui)
        }
        "list" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("list".into()),
                });
            }
            let mut backup_dir = None;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--backup-dir" => {
                        let v = take_value(&rest, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: list 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::List { backup_dir })
        }
        "inspect" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("inspect".into()),
                });
            }
            let mut opts = InspectOpts::default();
            let mut lba_seen = false;
            let mut i = 0;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--lba" => {
                            if lba_seen {
                                return Err("错误: --lba 重复指定".into());
                            }
                            let v = take_value(&rest, &mut i, "--lba")?;
                            opts.lbas = parse_lbas(&v)?;
                            lba_seen = true;
                        }
                        "--raw" => set_switch(&mut opts.raw, a, "--raw")?,
                        "--hex" => set_switch(&mut opts.hex, a, "--hex")?,
                        "--export" => {
                            let v = take_value(&rest, &mut i, "--export")?;
                            set_once(&mut opts.export, v, "--export")?;
                        }
                        "--id" => {
                            let v = take_value(&rest, &mut i, "--id")?;
                            set_once(&mut opts.device_id, v, "--id")?;
                        }
                        "--backup-dir" => {
                            let v = take_value(&rest, &mut i, "--backup-dir")?;
                            set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: inspect 不认识选项 {}", other)),
                    }
                } else if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(format!(
                        "错误: v2 不接受裸 LBA {}。请使用: edpcli inspect --lba {}",
                        a, a
                    ));
                } else if opts.backup.is_none() {
                    // 最常见的离线查看不应强迫用户记 --backup：
                    // `edpcli inspect backup.edpb 7 12` 与显式 --backup 等价。
                    opts.backup = Some(a.to_string());
                } else {
                    return Err(format!("错误: inspect 多余的位置参数: {}", a));
                }
                i += 1;
            }
            let source_count =
                usize::from(opts.disk.is_some()) + usize::from(opts.backup.is_some());
            if source_count > 1 {
                return Err("错误: inspect 的 --disk 与备份文件只能选一种".into());
            }
            if opts.raw && opts.hex {
                return Err("错误: inspect 的 --raw 与 --hex 语义相反，不能同时使用".into());
            }
            Ok(Parsed::Inspect(opts))
        }
        "info" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("info".into()),
                });
            }
            let mut opts = InfoOpts::default();
            let mut i = 0usize;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--id" => {
                            let v = take_value(&rest, &mut i, "--id")?;
                            set_once(&mut opts.device_id, v, "--id")?;
                        }
                        "--backup-dir" => {
                            let v = take_value(&rest, &mut i, "--backup-dir")?;
                            set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: info 不认识选项 {}", other)),
                    }
                } else if opts.backup.is_none() {
                    if opts.disk.is_some() {
                        return Err("错误: info 的备份文件不能与 --disk 同时使用".into());
                    }
                    opts.backup = Some(a.to_string());
                } else {
                    return Err(format!("错误: info 只接受一个备份文件参数({})", a));
                }
                i += 1;
            }
            if opts.disk.is_some() && opts.backup.is_some() {
                return Err("错误: info 的 --disk 与备份文件只能选一种".into());
            }
            Ok(Parsed::Info(opts))
        }
        "backup" => {
            if rest.iter().any(|a| a == "-h" || a == "--help")
                || rest.first().map(String::as_str) == Some("help")
            {
                return Ok(Parsed::Help {
                    topic: Some("backup".into()),
                });
            }
            // 人工使用时 `edpcli backup` 的自然含义就是“看看有哪些备份”。
            // 若第一个 token 是旗标，也按省略 `list` 处理。
            let (action_name, tail): (&str, &[String]) = match rest.first() {
                None => ("list", &rest[..]),
                Some(s) if s.starts_with('-') => ("list", &rest[..]),
                Some(s) => (s.as_str(), &rest[1..]),
            };
            let mut backup_dir = None;
            let mut keep = None;
            let mut yes = false;
            let action = match action_name {
                "create" => {
                    let mut disk = None;
                    let mut deep = false;
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--deep" => {
                                if tail[i] != "--deep" || deep { return Err("错误: --deep 不接受值或重复指定".into()); }
                                deep = true;
                            }
                            "--disk" => {
                                let v = take_value(tail, &mut i, "--disk")?;
                                set_once(&mut disk, parse_disk_spec(&v)?, "--disk")?;
                            }
                            "--backup-dir" => {
                                let v = take_value(tail, &mut i, "--backup-dir")?;
                                set_once(&mut backup_dir, v, "--backup-dir")?;
                            }
                            other => return Err(format!("错误: backup create 不认识选项 {}", other)),
                        }
                        i += 1;
                    }
                    BackupAction::Create { disk, deep }
                }
                "list" => {
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--backup-dir" => {
                                let v = take_value(tail, &mut i, "--backup-dir")?;
                                set_once(&mut backup_dir, v, "--backup-dir")?;
                            }
                            other => return Err(format!("错误: backup list 不认识选项 {}", other)),
                        }
                        i += 1;
                    }
                    BackupAction::List
                }
                "verify" => {
                    let mut target = None;
                    let mut i = 0;
                    while i < tail.len() {
                        let a = tail[i].as_str();
                        if a.starts_with('-') && a != "-" {
                            match flag_name(a) {
                                "--backup-dir" => {
                                    let v = take_value(tail, &mut i, "--backup-dir")?;
                                    set_once(&mut backup_dir, v, "--backup-dir")?;
                                }
                                other => {
                                    return Err(format!("错误: backup verify 不认识选项 {}", other))
                                }
                            }
                        } else if target.is_none() {
                            target = Some(a.to_string());
                        } else {
                            return Err(format!(
                                "错误: backup verify 只接受一个备份文件参数({})",
                                a
                            ));
                        }
                        i += 1;
                    }
                    BackupAction::Verify { target }
                }
                "restore" => {
                    let mut target = None;
                    let mut disk = None;
                    let mut i = 0;
                    while i < tail.len() {
                        let a = tail[i].as_str();
                        if a.starts_with('-') && a != "-" {
                            match flag_name(a) {
                                "--disk" => {
                                    let v = take_value(tail, &mut i, "--disk")?;
                                    set_once(&mut disk, parse_disk_spec(&v)?, "--disk")?;
                                }
                                "--yes" => set_switch(&mut yes, a, "--yes")?,
                                "--backup-dir" => {
                                    let v = take_value(tail, &mut i, "--backup-dir")?;
                                    set_once(&mut backup_dir, v, "--backup-dir")?;
                                }
                                other => {
                                    return Err(format!("错误: backup restore 不认识选项 {}", other))
                                }
                            }
                        } else if target.is_none() {
                            target = Some(a.to_string());
                        } else {
                            return Err(format!(
                                "错误: backup restore 只接受一个备份编号或文件参数({})",
                                a
                            ));
                        }
                        i += 1;
                    }
                    BackupAction::Restore { target, disk }
                }
                "prune" => {
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--keep" => {
                                let v = take_value(tail, &mut i, "--keep")?;
                                set_once(&mut keep, parse_keep(&v)?, "--keep")?;
                            }
                            "--yes" => set_switch(&mut yes, &tail[i], "--yes")?,
                            "--backup-dir" => {
                                let v = take_value(tail, &mut i, "--backup-dir")?;
                                set_once(&mut backup_dir, v, "--backup-dir")?;
                            }
                            other => {
                                return Err(format!("错误: backup prune 不认识选项 {}", other))
                            }
                        }
                        i += 1;
                    }
                    BackupAction::Prune
                }
                "delete" => {
                    let mut targets = Vec::new();
                    let mut i = 0;
                    while i < tail.len() {
                        let a = tail[i].as_str();
                        if a.starts_with('-') && a != "-" {
                            match flag_name(a) {
                                "--yes" => set_switch(&mut yes, a, "--yes")?,
                                "--backup-dir" => {
                                    let v = take_value(tail, &mut i, "--backup-dir")?;
                                    set_once(&mut backup_dir, v, "--backup-dir")?;
                                }
                                other => {
                                    return Err(format!("错误: backup delete 不认识选项 {}", other))
                                }
                            }
                        } else {
                            targets.push(a.to_string());
                        }
                        i += 1;
                    }
                    if targets.is_empty() && yes {
                        return Err("错误: backup delete --yes 必须显式给出编号或文件".into());
                    }
                    BackupAction::Delete { targets }
                }
                "rm" => {
                    return Err(
                        "错误: v2 已取消 backup rm。请使用: edpcli backup delete".into(),
                    )
                }
                other => {
                    return Err(format!(
                        "错误: 未知 backup 动作: {} (可用 create / list / restore / verify / delete / prune)",
                        other
                    ))
                }
            };
            Ok(Parsed::Backup {
                action,
                keep: keep.unwrap_or(2),
                yes,
                backup_dir,
            })
        }
        "apply" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("apply".into()),
                });
            }
            let mut opts = DiskOpts::default();
            let mut dry_run = false;
            let mut force = false;
            let mut yes = false;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--disk" => {
                        let v = take_value(&rest, &mut i, "--disk")?;
                        set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                    }
                    "--size" => {
                        let v = take_value(&rest, &mut i, "--size")?;
                        set_once(&mut opts.size, parse_size(&v)?, "--size")?;
                    }
                    "--backup-dir" => {
                        let v = take_value(&rest, &mut i, "--backup-dir")?;
                        set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                    }
                    "--dry-run" => set_switch(&mut dry_run, &rest[i], "--dry-run")?,
                    "--force" => set_switch(&mut force, &rest[i], "--force")?,
                    "--yes" => set_switch(&mut yes, &rest[i], "--yes")?,
                    other => return Err(format!("错误: apply 不认识选项 {}", other)),
                }
                i += 1;
            }
            if dry_run && (force || yes) {
                return Err("错误: apply --dry-run 不接受 --force 或 --yes".into());
            }
            Ok(Parsed::Apply {
                opts,
                dry_run,
                force,
                yes,
            })
        }
        "run" => Err("错误: v2 已取消 run。请使用: edpcli apply --dry-run".into()),
        "restore" => Err("错误: v2 已取消顶层 restore。请使用: edpcli backup restore".into()),
        "meta" | "metainfo" => Err("错误: v2 已取消 meta/metainfo。请使用: edpcli info".into()),
        "convert" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("convert".into()),
                });
            }
            let mut dir = None;
            let mut id = None;
            let mut size = None;
            let mut out = None;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--dir" => {
                        let v = take_value(&rest, &mut i, "--dir")?;
                        set_once(&mut dir, v, "--dir")?;
                    }
                    "--id" => {
                        let v = take_value(&rest, &mut i, "--id")?;
                        set_once(&mut id, v, "--id")?;
                    }
                    "--size" => {
                        let v = take_value(&rest, &mut i, "--size")?;
                        set_once(&mut size, parse_size(&v)?, "--size")?;
                    }
                    "--out" => {
                        let v = take_value(&rest, &mut i, "--out")?;
                        set_once(&mut out, v, "--out")?;
                    }
                    other => return Err(format!("错误: convert 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::Convert { dir, id, size, out })
        }
        other if other.starts_with('-') => {
            Err(format!("错误: 未知选项 {} (首个参数应为子命令)", other))
        }
        other => Err(format!(
            "错误: 未知子命令: {} (edpcli help 查看用法)",
            other
        )),
    }
}
