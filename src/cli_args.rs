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
    pub onlyid: Option<String>,
    pub index: Option<usize>,
    pub lbas: Vec<u32>,
    pub raw: bool,
    pub hex: bool,
    pub export: Option<String>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

#[derive(Default)]
pub struct MetaInfoOpts {
    pub disk: Option<u32>,
    pub backup: Option<String>,
    pub onlyid: Option<String>,
    pub index: Option<usize>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

pub enum Parsed {
    List {
        backup_dir: Option<String>,
    },
    Backup {
        action: BackupAction,
        keep: usize,
        yes: bool,
        onlyid: Option<String>,
        backup_dir: Option<String>,
    },
    Inspect(InspectOpts),
    MetaInfo(MetaInfoOpts),
    Run(DiskOpts),
    Apply {
        opts: DiskOpts,
        force: bool,
        yes: bool,
    },
    Restore {
        bin: Option<String>,
        disk: Option<u32>,
        yes: bool,
        backup_dir: Option<String>,
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
        onlyid: Option<String>,
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
    List,
    Verify {
        target: Option<String>,
        index: Option<usize>,
    },
    Prune,
    Rm {
        targets: Vec<String>,
    },
}

pub fn print_usage() {
    use crate::ui::{bold, bold_cyan, pad_to, yellow};
    let cmd = |name: &str, desc: &str| format!("  {}  {}", bold_cyan(&pad_to(name, 8)), desc);
    let flag = |name: &str, desc: &str| format!("  {}  {}", yellow(&pad_to(name, 33)), desc);
    println!(
        "{}",
        bold(&format!(
            "edpcli — EDP/cems U 盘管理 CLI v{}",
            env!("CARGO_PKG_VERSION")
        ))
    );
    println!();
    println!("{}", bold("用法: edpcli <子命令> [选项]"));
    println!();
    println!("{}", bold("子命令:"));
    for (n, d) in [
        (
            "list",
            "列出外接盘: 编号/容量/接口/cems识别/免密检测/EDPF分区/备份（管理员权限下信息更全）",
        ),
        ("run", "真盘预览 dry-run（需管理员权限，可自动提权）"),
        ("apply", "真盘实际写入(自动备份 → 原子写入 → 读回校验)"),
        ("restore", "从备份还原 LBA0-13(缺省交互选择本盘备份)"),
        (
            "backup",
            "跨盘备份管理(list / verify / prune / rm，全程不提权)",
        ),
        (
            "inspect",
            "只读查看物理 U 盘或备份文件的扇区结构/解密字段/高亮 hex",
        ),
        (
            "metainfo",
            "汇总查看 U 盘/备份的身份、Dept/User、SAFE6 与分区元信息（别名 meta）",
        ),
        (
            "convert",
            "离线转换(不碰真盘): --dir <快照> --id <device_id>",
        ),
        ("completion", "生成 zsh / bash / fish Tab 补全脚本"),
        (
            "version",
            "显示版本、平台、架构、编译时间、Git 与 Rust 构建信息",
        ),
        ("help", "显示本帮助"),
    ] {
        println!("{}", cmd(n, d));
    }
    println!();
    println!("{}", bold("元信息查看:"));
    for (n, d) in [
        ("metainfo / meta", "查看当前 U 盘元信息"),
        ("meta <onlyid>", "直接查看该盘最新 [1] 备份"),
        ("meta <onlyid> <N>", "查看该盘第 N 份备份"),
        ("meta <备份.bin>", "直接查看指定备份文件"),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("扇区检查:"));
    for (n, d) in [
        (
            "inspect [LBA...] [--disk N]",
            "查看物理盘；未给 --disk 时自动选择 USB 盘",
        ),
        (
            "inspect [LBA...] [备份.bin]",
            "查看备份/镜像；也可显式使用 --backup <文件>",
        ),
        (
            "inspect --onlyid ID",
            "先列出该盘可选备份；再用 --index N 选择",
        ),
        (
            "inspect [LBA...] --onlyid ID --index N",
            "按 backup list 的盘内编号查看某份备份",
        ),
        (
            "inspect ... --hex",
            "在结构化字段后显示解密后的 512B 字段高亮 hex",
        ),
        ("inspect ... --raw", "显示原始扇区 hex，不套用解密字段颜色"),
        (
            "inspect ... --export DIR",
            "导出所查看 LBA 的 raw/decoded .bin 与 .hex",
        ),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("备份管理:"));
    for (n, d) in [
        (
            "backup list [--onlyid ID]",
            "跨盘总览，或只查看指定盘；显示编号 + 真实文件名",
        ),
        (
            "backup verify [备份.bin] [--onlyid ID] [--index N]",
            "校验全部、指定盘、指定编号或单份备份",
        ),
        (
            "backup prune [--onlyid ID] [--keep N] [--yes]",
            "按策略清理全部盘或指定盘的旧免密快照",
        ),
        (
            "backup rm --onlyid ID [编号|范围]...",
            "按盘编号删除；无编号时进入交互选择",
        ),
        (
            "backup rm <路径|文件名>... [--yes]",
            "按文件精确删除；默认预览并要求输入 YES",
        ),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("选项:"));
    for (n, d) in [
        (
            crate::platform::disk_selector_syntax(),
            "真盘选择器(缺省自动检测外部 USB 整盘)",
        ),
        ("--size <GB>", "Share 大小(默认占满到 Encrypt 前)"),
        ("--force", "已改造(免密)盘仍强制重写(默认拒绝)"),
        ("--yes", "免交互(自动确认一切 YES 提示)"),
        ("--onlyid <ID>", "backup / inspect 按物理盘 onlyid 筛选"),
        ("--index <N>", "inspect / backup verify 选择该盘第 N 份备份"),
        (
            "--id <device_id>",
            "inspect / metainfo 备份或镜像无法自动识别时手动提供 device_id",
        ),
        (
            "--backup-dir <目录>",
            "备份目录(默认 $EDPCLI_BACKUP_DIR、~/.edpcli.conf 或 ./backup)",
        ),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("Tab 补全:"));
    println!("  zsh   eval \"$(edpcli completion zsh)\"");
    println!("  bash  eval \"$(edpcli completion bash)\"");
    println!("  fish  edpcli completion fish | source");
}

fn print_topic_help(topic: &str) {
    use crate::ui::{bold, bold_cyan, dim};
    match topic {
        "inspect" => {
            println!("{}", bold("用法: edpcli inspect [LBA...] [来源] [选项]"));
            println!();
            println!("{}", bold("来源（选一种；不指定时查看当前物理 USB 盘）:"));
            println!(
                "  {}",
                bold_cyan("--disk N                         当前物理盘")
            );
            println!(
                "  {}",
                bold_cyan("<备份.bin> / --backup <文件>     备份文件或镜像")
            );
            println!(
                "  {}",
                bold_cyan("--onlyid ID                      先列出该盘可选备份")
            );
            println!(
                "  {}",
                bold_cyan("--onlyid ID --index N            查看该盘第 N 份备份")
            );
            println!();
            println!("{}", bold("常用:"));
            println!("  edpcli inspect --onlyid 1987718388");
            println!("  edpcli inspect --onlyid 1987718388 --index 1");
            println!("  edpcli inspect 6 7 12 --onlyid 1987718388 --index 1 --hex");
            println!("  edpcli inspect backup.bin 7 12 --hex");
            println!("  edpcli inspect 6 7 12 --disk 4 --hex");
            println!();
            println!(
                "{}",
                dim("不指定 LBA 时显示 LBA0-13 概览；--hex 展开解密视图，--raw 查看盘上原始字节。")
            );
        }
        "metainfo" | "meta" => {
            println!(
                "{}",
                bold("用法: edpcli metainfo [onlyid [N] | 备份.bin] [选项]")
            );
            println!("{}", bold("别名: edpcli meta"));
            println!();
            println!("{}", bold("常用:"));
            println!("  edpcli meta");
            println!("  edpcli meta 1987718388");
            println!("  edpcli meta 1987718388 2");
            println!("  edpcli meta backup.bin");
            println!("  edpcli metainfo --disk 4");
            println!();
            println!("{}", dim("onlyid 不写编号时默认查看最新 [1]；输出汇总 onlyid/device_id/Dept/User/SAFE6/分区等元信息。"));
        }
        "backup" => {
            println!("{}", bold("用法: edpcli backup [动作] [选项]"));
            println!();
            println!(
                "{}",
                bold("默认动作: list（因此 edpcli backup 可直接列出全部备份）")
            );
            println!(
                "  {}",
                bold_cyan("backup [list] [--onlyid ID]          查看备份")
            );
            println!(
                "  {}",
                bold_cyan("backup verify [文件] [--onlyid ID] [--index N] 校验备份")
            );
            println!(
                "  {}",
                bold_cyan("backup prune [--onlyid ID]          预览策略清理")
            );
            println!(
                "  {}",
                bold_cyan("backup rm --onlyid ID [编号|范围]   选择并删除备份")
            );
            println!();
            println!("{}", bold("常用:"));
            println!("  edpcli backup");
            println!("  edpcli backup --onlyid 1987718388");
            println!("  edpcli backup rm --onlyid 1987718388");
        }
        "completion" => {
            println!("{}", bold("用法: edpcli completion <zsh|bash|fish>"));
            println!();
            println!(
                "动态补全包括: 子命令、旗标、onlyid、备份编号、备份文件名、物理盘号和 LBA0-13。"
            );
            println!();
            println!("zsh : eval \"$(edpcli completion zsh)\"");
            println!("bash: eval \"$(edpcli completion bash)\"");
            println!("fish: edpcli completion fish | source");
        }
        "restore" => {
            println!(
                "{}",
                bold("用法: edpcli restore [备份.bin] [--disk N] [--yes] [--backup-dir D]")
            );
            println!("不指定备份文件时，会自动列出当前物理盘匹配的备份并让你选择。");
        }
        "run" => println!(
            "{}",
            bold("用法: edpcli run [--disk N] [--size GB] [--backup-dir D]")
        ),
        "apply" => println!(
            "{}",
            bold("用法: edpcli apply [--disk N] [--size GB] [--force] [--yes] [--backup-dir D]")
        ),
        "list" => println!("{}", bold("用法: edpcli list [--backup-dir D]")),
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

fn parse_onlyid(s: &str) -> Result<String, String> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: --onlyid 须为整数形式, 得到 {}", s));
    }
    Ok(s.to_string())
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
        return Ok(Parsed::Help { topic: None }); // 裸 edpcli: 打印用法, 不做任何动作
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
            let mut onlyid = None;
            let mut backup_dir = None;
            let mut i = 1usize;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--onlyid" => {
                        let v = take_value(&rest, &mut i, "--onlyid")?;
                        set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
                    }
                    "--backup-dir" => {
                        let v = take_value(&rest, &mut i, "--backup-dir")?;
                        set_once(&mut backup_dir, v, "--backup-dir")?;
                    }
                    other => return Err(format!("错误: __complete 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::InternalComplete {
                kind,
                onlyid,
                backup_dir,
            })
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
            let mut i = 0;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--backup" | "--image" => {
                            let flag = flag_name(a).to_string();
                            let v = take_value(&rest, &mut i, &flag)?;
                            set_once(&mut opts.backup, v, "--backup/--image")?;
                        }
                        "--onlyid" => {
                            let v = take_value(&rest, &mut i, "--onlyid")?;
                            set_once(&mut opts.onlyid, parse_onlyid(&v)?, "--onlyid")?;
                        }
                        "--index" => {
                            let v = take_value(&rest, &mut i, "--index")?;
                            let n = v
                                .parse::<usize>()
                                .map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
                            if n == 0 {
                                return Err("错误: --index 从 1 开始".into());
                            }
                            set_once(&mut opts.index, n, "--index")?;
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
                    let lba = a
                        .parse::<u32>()
                        .map_err(|_| format!("错误: inspect LBA 仅支持 0-13, 得到 {}", a))?;
                    if lba > 13 {
                        return Err(format!("错误: inspect LBA 仅支持 0-13, 得到 {}", a));
                    }
                    opts.lbas.push(lba);
                } else if opts.backup.is_none() {
                    // 最常见的离线查看不应强迫用户记 --backup：
                    // `edpcli inspect backup.bin 7 12` 与显式 --backup 等价。
                    opts.backup = Some(a.to_string());
                } else {
                    return Err(format!("错误: inspect 多余的位置参数: {}", a));
                }
                i += 1;
            }
            let source_count = usize::from(opts.disk.is_some())
                + usize::from(opts.backup.is_some())
                + usize::from(opts.onlyid.is_some());
            if source_count > 1 {
                return Err(
                    "错误: inspect 的 --disk / --backup / --onlyid 三种来源只能选一种".into(),
                );
            }
            if opts.raw && opts.hex {
                return Err("错误: inspect 的 --raw 与 --hex 语义相反，不能同时使用".into());
            }
            if opts.index.is_some() && opts.onlyid.is_none() {
                return Err("错误: --index 只能与 inspect --onlyid 一起使用".into());
            }
            Ok(Parsed::Inspect(opts))
        }
        "metainfo" | "meta" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("metainfo".into()),
                });
            }
            let mut opts = MetaInfoOpts::default();
            let mut positionals = Vec::new();
            let mut i = 0usize;
            while i < rest.len() {
                let a = rest[i].as_str();
                let signed_numeric = a
                    .strip_prefix('-')
                    .map(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                    .unwrap_or(false);
                if a.starts_with('-') && a != "-" && !signed_numeric {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            set_once(&mut opts.disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--backup" | "--image" => {
                            let flag = flag_name(a).to_string();
                            let v = take_value(&rest, &mut i, &flag)?;
                            set_once(&mut opts.backup, v, "--backup/--image")?;
                        }
                        "--onlyid" => {
                            let v = take_value(&rest, &mut i, "--onlyid")?;
                            set_once(&mut opts.onlyid, parse_onlyid(&v)?, "--onlyid")?;
                        }
                        "--index" => {
                            let v = take_value(&rest, &mut i, "--index")?;
                            let n = v
                                .parse::<usize>()
                                .map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
                            if n == 0 {
                                return Err("错误: --index 从 1 开始".into());
                            }
                            set_once(&mut opts.index, n, "--index")?;
                        }
                        "--id" => {
                            let v = take_value(&rest, &mut i, "--id")?;
                            set_once(&mut opts.device_id, v, "--id")?;
                        }
                        "--backup-dir" => {
                            let v = take_value(&rest, &mut i, "--backup-dir")?;
                            set_once(&mut opts.backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: metainfo 不认识选项 {}", other)),
                    }
                } else {
                    positionals.push(a.to_string());
                }
                i += 1;
            }
            if !positionals.is_empty() {
                if opts.disk.is_some() || opts.backup.is_some() || opts.onlyid.is_some() {
                    return Err(
                        "错误: metainfo 的位置参数不能与 --disk/--backup/--onlyid 混用".into(),
                    );
                }
                match positionals.as_slice() {
                    [one]
                        if one
                            .strip_prefix('-')
                            .unwrap_or(one)
                            .bytes()
                            .all(|b| b.is_ascii_digit()) =>
                    {
                        opts.onlyid = Some(parse_onlyid(one)?);
                    }
                    [one] => opts.backup = Some(one.clone()),
                    [id, index]
                        if id
                            .strip_prefix('-')
                            .unwrap_or(id)
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                            && index.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        opts.onlyid = Some(parse_onlyid(id)?);
                        let n = index
                            .parse::<usize>()
                            .map_err(|_| format!("错误: 备份编号须为正整数, 得到 {}", index))?;
                        if n == 0 {
                            return Err("错误: 备份编号从 1 开始".into());
                        }
                        opts.index = Some(n);
                    }
                    _ => {
                        return Err(
                            "错误: metainfo 位置参数仅支持 <onlyid> [编号] 或 <备份.bin>".into(),
                        )
                    }
                }
            }
            let source_count = usize::from(opts.disk.is_some())
                + usize::from(opts.backup.is_some())
                + usize::from(opts.onlyid.is_some());
            if source_count > 1 {
                return Err(
                    "错误: metainfo 的 --disk / --backup / --onlyid 三种来源只能选一种".into(),
                );
            }
            if opts.index.is_some() && opts.onlyid.is_none() {
                return Err("错误: --index 只能与 metainfo --onlyid 一起使用".into());
            }
            Ok(Parsed::MetaInfo(opts))
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
            // 若第一个 token 是旗标，也按省略 `list` 处理，例如 `edpcli backup --onlyid ID`。
            let (action_name, tail): (&str, &[String]) = match rest.first() {
                None => ("list", &rest[..]),
                Some(s) if s.starts_with('-') => ("list", &rest[..]),
                Some(s) => (s.as_str(), &rest[1..]),
            };
            let mut backup_dir = None;
            let mut onlyid = None;
            let mut keep = None;
            let mut yes = false;
            let action = match action_name {
                "list" => {
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--backup-dir" => {
                                let v = take_value(tail, &mut i, "--backup-dir")?;
                                set_once(&mut backup_dir, v, "--backup-dir")?;
                            }
                            "--onlyid" => {
                                let v = take_value(tail, &mut i, "--onlyid")?;
                                set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
                            }
                            other => return Err(format!("错误: backup list 不认识选项 {}", other)),
                        }
                        i += 1;
                    }
                    BackupAction::List
                }
                "verify" => {
                    let mut target = None;
                    let mut index = None;
                    let mut i = 0;
                    while i < tail.len() {
                        let a = tail[i].as_str();
                        if a.starts_with('-') && a != "-" {
                            match flag_name(a) {
                                "--backup-dir" => {
                                    let v = take_value(tail, &mut i, "--backup-dir")?;
                                    set_once(&mut backup_dir, v, "--backup-dir")?;
                                }
                                "--onlyid" => {
                                    let v = take_value(tail, &mut i, "--onlyid")?;
                                    set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
                                }
                                "--index" => {
                                    let v = take_value(tail, &mut i, "--index")?;
                                    let n = v.parse::<usize>().map_err(|_| {
                                        format!("错误: --index 须为正整数, 得到 {}", v)
                                    })?;
                                    if n == 0 {
                                        return Err("错误: --index 从 1 开始".into());
                                    }
                                    set_once(&mut index, n, "--index")?;
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
                    if target.is_some() && onlyid.is_some() {
                        return Err(
                            "错误: backup verify 的单文件参数与 --onlyid 不能同时使用".into()
                        );
                    }
                    if index.is_some() && onlyid.is_none() {
                        return Err("错误: backup verify --index 只能与 --onlyid 一起使用".into());
                    }
                    if target.is_some() && index.is_some() {
                        return Err(
                            "错误: backup verify 的单文件参数与 --index 不能同时使用".into()
                        );
                    }
                    BackupAction::Verify { target, index }
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
                            "--onlyid" => {
                                let v = take_value(tail, &mut i, "--onlyid")?;
                                set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
                            }
                            other => {
                                return Err(format!("错误: backup prune 不认识选项 {}", other))
                            }
                        }
                        i += 1;
                    }
                    BackupAction::Prune
                }
                "rm" => {
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
                                "--onlyid" => {
                                    let v = take_value(tail, &mut i, "--onlyid")?;
                                    set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
                                }
                                other => {
                                    return Err(format!("错误: backup rm 不认识选项 {}", other))
                                }
                            }
                        } else {
                            targets.push(a.to_string());
                        }
                        i += 1;
                    }
                    if targets.is_empty() && onlyid.is_none() {
                        return Err("错误: backup rm 至少需要一个路径或文件名".into());
                    }
                    if targets.is_empty() && onlyid.is_some() && yes {
                        return Err(
                            "错误: backup rm --onlyid 配合 --yes 时必须显式给出编号或范围".into(),
                        );
                    }
                    BackupAction::Rm { targets }
                }
                other => {
                    return Err(format!(
                        "错误: 未知 backup 动作: {} (可用 list / verify / prune / rm)",
                        other
                    ))
                }
            };
            Ok(Parsed::Backup {
                action,
                keep: keep.unwrap_or(2),
                yes,
                onlyid,
                backup_dir,
            })
        }
        "run" | "apply" => {
            let is_apply = first.as_str() == "apply";
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some(first.to_string()),
                });
            }
            let mut opts = DiskOpts::default();
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
                    "--force" if is_apply => set_switch(&mut force, &rest[i], "--force")?,
                    "--yes" if is_apply => set_switch(&mut yes, &rest[i], "--yes")?,
                    "--force" | "--yes" => {
                        return Err(format!("错误: {} 只用于 apply", flag_name(&rest[i])))
                    }
                    other => return Err(format!("错误: {} 不认识选项 {}", first, other)),
                }
                i += 1;
            }
            if is_apply {
                Ok(Parsed::Apply { opts, force, yes })
            } else {
                Ok(Parsed::Run(opts))
            }
        }
        "restore" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("restore".into()),
                });
            }
            let mut bin: Option<String> = None;
            let mut disk = None;
            let mut yes = false;
            let mut backup_dir = None;
            let mut i = 0;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            set_once(&mut disk, parse_disk_spec(&v)?, "--disk")?;
                        }
                        "--yes" => set_switch(&mut yes, a, "--yes")?,
                        "--backup-dir" => {
                            let v = take_value(&rest, &mut i, "--backup-dir")?;
                            set_once(&mut backup_dir, v, "--backup-dir")?;
                        }
                        other => return Err(format!("错误: restore 不认识选项 {}", other)),
                    }
                } else if bin.is_none() {
                    bin = Some(a.to_string());
                } else {
                    return Err(format!("错误: restore 只接受一个备份文件参数({})", a));
                }
                i += 1;
            }
            Ok(Parsed::Restore {
                bin,
                disk,
                yes,
                backup_dir,
            })
        }
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
