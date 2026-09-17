//! 命令行入口: 子命令解析、自动提权、交互提示、各处理器。
//!
//! 用法:
//!   nopwd                                     打印用法(裸命令不做任何动作)
//!   nopwd list                                列出外接盘(不写入, 不提权)
//!   nopwd run    [--disk N] [--size GB]       预览 dry-run(自动提权)
//!   nopwd apply  [--disk N] [--size GB] [--force] [--yes]   实际写入
//!   nopwd restore [<备份.bin>] [--disk N] [--yes]           还原(缺省交互选择)
//!   nopwd convert --dir <快照目录> --id <device_id> [--size GB] [--out <目录>]
//!
//! 实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
//! Kingston DT3.0 64G (每盘改前自动备份, 可随时 nopwd restore 还原)。

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::backup_catalog::{self, BackupCatalog};
use crate::common::*;
use crate::completion::{self, Shell};
use crate::diskio::{self, backup_disk, backup_is_nopwd, find_backups, raw_path, DiskFacts,
                    BackupEntry, BackupMeta, Md5Status, FileDev, SectorDev, Clock, SystemClock};
use crate::elevate::{self, ELEVATED_FLAG};
use crate::identify::identify;
use crate::inspect::{self, InspectMeta};
use crate::sectors::{convert, looks_nopwd};
use crate::sysinfo::{self, CmdRunner, SysRunner};

const OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

// ══════════════════════════════════════════════════════════════════
// 1. 交互提示抽象(测试注入)
// ══════════════════════════════════════════════════════════════════
pub trait Prompter {
    /// 读一行(选择器输入)。
    fn prompt_line(&mut self, msg: &str) -> String;
    /// YES 确认(输入恰为 YES)。
    fn confirm_yes(&mut self, msg: &str) -> bool;
}

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

/// 处理器上下文: 全部外部依赖经此注入(测试以假件替换)。
pub struct Ctx<'a> {
    pub runner: &'a dyn CmdRunner,
    pub clock: &'a dyn Clock,
    pub prompt: &'a mut dyn Prompter,
    pub backup_dir: PathBuf,
}

fn err(code: i32, msg: impl Into<String>) -> NopwdError {
    NopwdError::new(code, msg)
}

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

pub enum Parsed {
    List { backup_dir: Option<String> },
    Backup {
        action: BackupAction,
        keep: usize,
        yes: bool,
        onlyid: Option<String>,
        backup_dir: Option<String>,
    },
    Inspect(InspectOpts),
    Run(DiskOpts),
    Apply { opts: DiskOpts, force: bool, yes: bool },
    Restore { bin: Option<String>, disk: Option<u32>, yes: bool, backup_dir: Option<String> },
    Convert { dir: Option<String>, id: Option<String>, size: Option<f64>, out: Option<String> },
    Completion { shell: Shell },
    InternalComplete {
        kind: String,
        onlyid: Option<String>,
        backup_dir: Option<String>,
    },
    Version,
    Help { topic: Option<String> },
}

pub enum BackupAction {
    List,
    Verify { target: Option<String>, index: Option<usize> },
    Prune,
    Rm { targets: Vec<String> },
}

pub fn print_usage() {
    use crate::ui::{bold, bold_cyan, pad_to, yellow};
    let cmd = |name: &str, desc: &str| format!("  {}  {}", bold_cyan(&pad_to(name, 8)), desc);
    let flag = |name: &str, desc: &str| format!("  {}  {}", yellow(&pad_to(name, 33)), desc);
    println!(
        "{}",
        bold(&format!(
            "cems 加密 U 盘 → 无密码盘(纯 Rust 标准库, 零依赖) v{}",
            env!("CARGO_PKG_VERSION")
        ))
    );
    println!();
    println!("{}", bold("用法: nopwd <子命令> [选项]"));
    println!();
    println!("{}", bold("子命令:"));
    for (n, d) in [
        ("list", "列出外接盘: 编号/容量/接口/cems识别/免密检测/EDPF分区/备份(sudo 下更全)"),
        ("run", "真盘预览 dry-run(需管理员, 自动 sudo)"),
        ("apply", "真盘实际写入(自动备份 → 原子写入 → 读回校验)"),
        ("restore", "从备份还原 LBA0-13(缺省交互选择本盘备份)"),
        ("backup", "跨盘备份管理(list / verify / prune / rm，全程不提权)"),
        ("inspect", "只读查看物理 U 盘或备份文件的扇区结构/解密字段/高亮 hex"),
        ("convert", "离线转换(不碰真盘): --dir <快照> --id <device_id>"),
        ("completion", "生成 zsh / bash / fish Tab 补全脚本"),
        ("version", "显示版本"),
        ("help", "显示本帮助"),
    ] {
        println!("{}", cmd(n, d));
    }
    println!();
    println!("{}", bold("扇区检查:"));
    for (n, d) in [
        ("inspect [LBA...] [--disk N]", "查看物理盘；未给 --disk 时自动选择 USB 盘"),
        ("inspect [LBA...] [备份.bin]", "查看备份/镜像；也可显式使用 --backup <文件>"),
        ("inspect --onlyid ID", "先列出该盘可选备份；再用 --index N 选择"),
        ("inspect [LBA...] --onlyid ID --index N", "按 backup list 的盘内编号查看某份备份"),
        ("inspect ... --hex", "在结构化字段后显示解密后的 512B 字段高亮 hex"),
        ("inspect ... --raw", "显示原始扇区 hex，不套用解密字段颜色"),
        ("inspect ... --export DIR", "导出所查看 LBA 的 raw/decoded .bin 与 .hex"),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("备份管理:"));
    for (n, d) in [
        ("backup list [--onlyid ID]", "跨盘总览，或只查看指定盘；显示编号 + 真实文件名"),
        ("backup verify [备份.bin] [--onlyid ID] [--index N]", "校验全部、指定盘、指定编号或单份备份"),
        ("backup prune [--onlyid ID] [--keep N] [--yes]", "按策略清理全部盘或指定盘的旧免密快照"),
        ("backup rm --onlyid ID [编号|范围]...", "按盘编号删除；无编号时进入交互选择"),
        ("backup rm <路径|文件名>... [--yes]", "按文件精确删除；默认预览并要求输入 YES"),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("选项:"));
    for (n, d) in [
        ("--disk <N|/dev/diskN|/dev/rdiskN>", "真盘号(缺省自动检测外部 USB 盘; 须 disk2+)"),
        ("--size <GB>", "Share 大小(默认占满到 Encrypt 前)"),
        ("--force", "已改造(免密)盘仍强制重写(默认拒绝)"),
        ("--yes", "免交互(自动确认一切 YES 提示)"),
        ("--onlyid <ID>", "backup / inspect 按物理盘 onlyid 筛选"),
        ("--index <N>", "inspect / backup verify 选择该盘第 N 份备份"),
        ("--id <device_id>", "inspect 备份/镜像无法自动识别时手动提供 device_id"),
        ("--backup-dir <目录>", "备份目录(默认 $NOPWD_BACKUP_DIR、~/.nopwd.conf 或 ./backup)"),
    ] {
        println!("{}", flag(n, d));
    }
    println!();
    println!("{}", bold("Tab 补全:"));
    println!("  zsh   eval \"$(nopwd completion zsh)\"");
    println!("  bash  eval \"$(nopwd completion bash)\"");
    println!("  fish  nopwd completion fish | source");
}

fn print_topic_help(topic: &str) {
    use crate::ui::{bold, bold_cyan, dim};
    match topic {
        "inspect" => {
            println!("{}", bold("用法: nopwd inspect [LBA...] [来源] [选项]"));
            println!();
            println!("{}", bold("来源（选一种；不指定时查看当前物理 USB 盘）:"));
            println!("  {}", bold_cyan("--disk N                         当前物理盘"));
            println!("  {}", bold_cyan("<备份.bin> / --backup <文件>     备份文件或镜像"));
            println!("  {}", bold_cyan("--onlyid ID                      先列出该盘可选备份"));
            println!("  {}", bold_cyan("--onlyid ID --index N            查看该盘第 N 份备份"));
            println!();
            println!("{}", bold("常用:"));
            println!("  nopwd inspect --onlyid 1987718388");
            println!("  nopwd inspect --onlyid 1987718388 --index 1");
            println!("  nopwd inspect 6 7 12 --onlyid 1987718388 --index 1 --hex");
            println!("  nopwd inspect backup.bin 7 12 --hex");
            println!("  nopwd inspect 6 7 12 --disk 4 --hex");
            println!();
            println!("{}", dim("不指定 LBA 时显示 LBA0-13 概览；--hex 展开解密视图，--raw 查看盘上原始字节。"));
        }
        "backup" => {
            println!("{}", bold("用法: nopwd backup [动作] [选项]"));
            println!();
            println!("{}", bold("默认动作: list（因此 nopwd backup 可直接列出全部备份）"));
            println!("  {}", bold_cyan("backup [list] [--onlyid ID]          查看备份"));
            println!("  {}", bold_cyan("backup verify [文件] [--onlyid ID] [--index N] 校验备份"));
            println!("  {}", bold_cyan("backup prune [--onlyid ID]          预览策略清理"));
            println!("  {}", bold_cyan("backup rm --onlyid ID [编号|范围]   选择并删除备份"));
            println!();
            println!("{}", bold("常用:"));
            println!("  nopwd backup");
            println!("  nopwd backup --onlyid 1987718388");
            println!("  nopwd backup rm --onlyid 1987718388");
        }
        "completion" => {
            println!("{}", bold("用法: nopwd completion <zsh|bash|fish>"));
            println!();
            println!("动态补全包括: 子命令、旗标、onlyid、备份编号、备份文件名、物理盘号和 LBA0-13。" );
            println!();
            println!("zsh : eval \"$(nopwd completion zsh)\"");
            println!("bash: eval \"$(nopwd completion bash)\"");
            println!("fish: nopwd completion fish | source");
        }
        "restore" => {
            println!("{}", bold("用法: nopwd restore [备份.bin] [--disk N] [--yes] [--backup-dir D]"));
            println!("不指定备份文件时，会自动列出当前物理盘匹配的备份并让你选择。" );
        }
        "run" => println!("{}", bold("用法: nopwd run [--disk N] [--size GB] [--backup-dir D]")),
        "apply" => println!("{}", bold("用法: nopwd apply [--disk N] [--size GB] [--force] [--yes] [--backup-dir D]")),
        "list" => println!("{}", bold("用法: nopwd list [--backup-dir D]")),
        "convert" => println!("{}", bold("用法: nopwd convert --dir <快照目录> --id <device_id> [--size GB] [--out DIR]")),
        _ => print_usage(),
    }
}

fn print_help(topic: Option<&str>) {
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
    let n = s
        .strip_prefix("/dev/rdisk")
        .or_else(|| s.strip_prefix("/dev/disk"))
        .unwrap_or(s);
    if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
        n.parse::<u32>().map_err(|_| format!("错误: --disk {} 超出范围", s))
    } else {
        Err(format!("错误: --disk 无法解析: {} (接受 4 / /dev/disk4 / /dev/rdisk4)", s))
    }
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

pub fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let args: Vec<&String> = argv.iter().filter(|a| a.as_str() != ELEVATED_FLAG).collect();
    let Some(first) = args.first() else {
        return Ok(Parsed::Help { topic: None }); // 裸 nopwd: 打印用法, 不做任何动作
    };
    let rest: Vec<String> = args[1..].iter().map(|s| (*s).clone()).collect();
    match first.as_str() {
        "-h" | "--help" => Ok(Parsed::Help { topic: None }),
        "help" => {
            if rest.len() > 1 {
                return Err("错误: help 最多接受一个子命令名称".into());
            }
            Ok(Parsed::Help { topic: rest.first().cloned() })
        }
        "-V" | "--version" | "version" => Ok(Parsed::Version),
        "completion" => {
            if rest.is_empty() || rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help { topic: Some("completion".into()) });
            }
            if rest.len() != 1 {
                return Err("错误: completion 只接受一个 shell: zsh / bash / fish".into());
            }
            let shell = Shell::parse(&rest[0])
                .ok_or_else(|| format!("错误: 不支持的 shell: {} (可用 zsh / bash / fish)", rest[0]))?;
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
                        onlyid = Some(parse_onlyid(&v)?);
                    }
                    "--backup-dir" => {
                        backup_dir = Some(take_value(&rest, &mut i, "--backup-dir")?);
                    }
                    other => return Err(format!("错误: __complete 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::InternalComplete { kind, onlyid, backup_dir })
        }
        "list" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help { topic: Some("list".into()) });
            }
            let mut backup_dir = None;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--backup-dir" => backup_dir = Some(take_value(&rest, &mut i, "--backup-dir")?),
                    other => return Err(format!("错误: list 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::List { backup_dir })
        }
        "inspect" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help { topic: Some("inspect".into()) });
            }
            let mut opts = InspectOpts::default();
            let mut i = 0;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a.starts_with('-') && a != "-" {
                    match flag_name(a) {
                        "--disk" => {
                            let v = take_value(&rest, &mut i, "--disk")?;
                            opts.disk = Some(parse_disk_spec(&v)?);
                        }
                        "--backup" | "--image" => {
                            let flag = flag_name(a).to_string();
                            opts.backup = Some(take_value(&rest, &mut i, &flag)?);
                        }
                        "--onlyid" => {
                            let v = take_value(&rest, &mut i, "--onlyid")?;
                            opts.onlyid = Some(parse_onlyid(&v)?);
                        }
                        "--index" => {
                            let v = take_value(&rest, &mut i, "--index")?;
                            let n = v.parse::<usize>().map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
                            if n == 0 {
                                return Err("错误: --index 从 1 开始".into());
                            }
                            opts.index = Some(n);
                        }
                        "--raw" => opts.raw = true,
                        "--hex" => opts.hex = true,
                        "--export" => opts.export = Some(take_value(&rest, &mut i, "--export")?),
                        "--id" => opts.device_id = Some(take_value(&rest, &mut i, "--id")?),
                        "--backup-dir" => {
                            opts.backup_dir = Some(take_value(&rest, &mut i, "--backup-dir")?)
                        }
                        other => return Err(format!("错误: inspect 不认识选项 {}", other)),
                    }
                } else if let Ok(lba) = a.parse::<u32>() {
                    opts.lbas.push(lba);
                } else if opts.backup.is_none() {
                    // 最常见的离线查看不应强迫用户记 --backup：
                    // `nopwd inspect backup.bin 7 12` 与显式 --backup 等价。
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
                return Err("错误: inspect 的 --disk / --backup / --onlyid 三种来源只能选一种".into());
            }
            if opts.raw && opts.hex {
                return Err("错误: inspect 的 --raw 与 --hex 语义相反，不能同时使用".into());
            }
            if opts.index.is_some() && opts.onlyid.is_none() {
                return Err("错误: --index 只能与 inspect --onlyid 一起使用".into());
            }
            Ok(Parsed::Inspect(opts))
        }
        "backup" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") || rest.first().map(String::as_str) == Some("help") {
                return Ok(Parsed::Help { topic: Some("backup".into()) });
            }
            // 人工使用时 `nopwd backup` 的自然含义就是“看看有哪些备份”。
            // 若第一个 token 是旗标，也按省略 `list` 处理，例如 `nopwd backup --onlyid ID`。
            let (action_name, tail): (&str, &[String]) = match rest.first() {
                None => ("list", &rest[..]),
                Some(s) if s.starts_with('-') => ("list", &rest[..]),
                Some(s) => (s.as_str(), &rest[1..]),
            };
            let mut backup_dir = None;
            let mut onlyid = None;
            let mut keep = 2usize;
            let mut yes = false;
            let action = match action_name {
                "list" => {
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--backup-dir" => backup_dir = Some(take_value(tail, &mut i, "--backup-dir")?),
                            "--onlyid" => {
                                let v = take_value(tail, &mut i, "--onlyid")?;
                                onlyid = Some(parse_onlyid(&v)?);
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
                                "--backup-dir" => backup_dir = Some(take_value(tail, &mut i, "--backup-dir")?),
                                "--onlyid" => {
                                    let v = take_value(tail, &mut i, "--onlyid")?;
                                    onlyid = Some(parse_onlyid(&v)?);
                                }
                                "--index" => {
                                    let v = take_value(tail, &mut i, "--index")?;
                                    let n = v
                                        .parse::<usize>()
                                        .map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
                                    if n == 0 {
                                        return Err("错误: --index 从 1 开始".into());
                                    }
                                    index = Some(n);
                                }
                                other => return Err(format!("错误: backup verify 不认识选项 {}", other)),
                            }
                        } else if target.is_none() {
                            target = Some(a.to_string());
                        } else {
                            return Err(format!("错误: backup verify 只接受一个备份文件参数({})", a));
                        }
                        i += 1;
                    }
                    if target.is_some() && onlyid.is_some() {
                        return Err("错误: backup verify 的单文件参数与 --onlyid 不能同时使用".into());
                    }
                    if index.is_some() && onlyid.is_none() {
                        return Err("错误: backup verify --index 只能与 --onlyid 一起使用".into());
                    }
                    if target.is_some() && index.is_some() {
                        return Err("错误: backup verify 的单文件参数与 --index 不能同时使用".into());
                    }
                    BackupAction::Verify { target, index }
                }
                "prune" => {
                    let mut i = 0;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--keep" => {
                                let v = take_value(tail, &mut i, "--keep")?;
                                keep = parse_keep(&v)?;
                            }
                            "--yes" => yes = true,
                            "--backup-dir" => backup_dir = Some(take_value(tail, &mut i, "--backup-dir")?),
                            "--onlyid" => {
                                let v = take_value(tail, &mut i, "--onlyid")?;
                                onlyid = Some(parse_onlyid(&v)?);
                            }
                            other => return Err(format!("错误: backup prune 不认识选项 {}", other)),
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
                                "--yes" => yes = true,
                                "--backup-dir" => backup_dir = Some(take_value(tail, &mut i, "--backup-dir")?),
                                "--onlyid" => {
                                    let v = take_value(tail, &mut i, "--onlyid")?;
                                    onlyid = Some(parse_onlyid(&v)?);
                                }
                                other => return Err(format!("错误: backup rm 不认识选项 {}", other)),
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
                        return Err("错误: backup rm --onlyid 配合 --yes 时必须显式给出编号或范围".into());
                    }
                    BackupAction::Rm { targets }
                }
                other => return Err(format!("错误: 未知 backup 动作: {} (可用 list / verify / prune / rm)", other)),
            };
            Ok(Parsed::Backup { action, keep, yes, onlyid, backup_dir })
        }
        "run" | "apply" => {
            let is_apply = first.as_str() == "apply";
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help { topic: Some(first.to_string()) });
            }
            let mut opts = DiskOpts::default();
            let mut force = false;
            let mut yes = false;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--disk" => {
                        let v = take_value(&rest, &mut i, "--disk")?;
                        opts.disk = Some(parse_disk_spec(&v)?);
                    }
                    "--size" => {
                        let v = take_value(&rest, &mut i, "--size")?;
                        opts.size = Some(parse_size(&v)?);
                    }
                    "--backup-dir" => {
                        opts.backup_dir = Some(take_value(&rest, &mut i, "--backup-dir")?);
                    }
                    "--force" if is_apply => force = true,
                    "--yes" if is_apply => yes = true,
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
                return Ok(Parsed::Help { topic: Some("restore".into()) });
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
                            disk = Some(parse_disk_spec(&v)?);
                        }
                        "--yes" => yes = true,
                        "--backup-dir" => backup_dir = Some(take_value(&rest, &mut i, "--backup-dir")?),
                        other => return Err(format!("错误: restore 不认识选项 {}", other)),
                    }
                } else if bin.is_none() {
                    bin = Some(a.to_string());
                } else {
                    return Err(format!("错误: restore 只接受一个备份文件参数({})", a));
                }
                i += 1;
            }
            Ok(Parsed::Restore { bin, disk, yes, backup_dir })
        }
        "convert" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help { topic: Some("convert".into()) });
            }
            let mut dir = None;
            let mut id = None;
            let mut size = None;
            let mut out = None;
            let mut i = 0;
            while i < rest.len() {
                match flag_name(&rest[i]) {
                    "--dir" => dir = Some(take_value(&rest, &mut i, "--dir")?),
                    "--id" => id = Some(take_value(&rest, &mut i, "--id")?),
                    "--size" => {
                        let v = take_value(&rest, &mut i, "--size")?;
                        size = Some(parse_size(&v)?);
                    }
                    "--out" => out = Some(take_value(&rest, &mut i, "--out")?),
                    other => return Err(format!("错误: convert 不认识选项 {}", other)),
                }
                i += 1;
            }
            Ok(Parsed::Convert { dir, id, size, out })
        }
        other if other.starts_with('-') => Err(format!("错误: 未知选项 {} (首个参数应为子命令)", other)),
        other => Err(format!("错误: 未知子命令: {} (nopwd help 查看用法)", other)),
    }
}

// ══════════════════════════════════════════════════════════════════
// 3. 外接盘一览
// ══════════════════════════════════════════════════════════════════
pub use crate::disk_scan::{print_disk_table, scan_disks, Row};

/// restore 选单条目(时间已格式化 + 是否免密快照)。
pub fn backup_menu_str(entries: &[(String, bool)]) -> String {
    use crate::ui::{green, pad_to};
    let mut out = String::new();
    for (i, (time, is_nopwd)) in entries.iter().enumerate() {
        let tag = if *is_nopwd {
            green("[免密状态]")
        } else {
            "[加密原盘]".to_string()
        };
        out.push_str(&format!("  {})  {}   {}\n", i + 1, time, tag));
    }
    let _ = pad_to; // (对齐保留给后续扩展)
    out
}

/// 多 USB 盘选单。
pub fn disk_menu_str(disks: &[sysinfo::ExtDisk]) -> String {
    use crate::ui::{bold, pad_left, pad_to};
    let w = disks.iter().map(|d| format!("disk{}", d.n).len()).max().unwrap_or(1);
    let mut out = String::new();
    for (i, d) in disks.iter().enumerate() {
        out.push_str(&format!(
            "  {})  {}  {}  {}:{}\n",
            i + 1,
            bold(&pad_to(&format!("disk{}", d.n), w + 2)),
            pad_left(&fmt_gb(d.size), 8),
            d.vid,
            d.pid
        ));
    }
    out
}

// ══════════════════════════════════════════════════════════════════
// 4. 真盘流程
// ══════════════════════════════════════════════════════════════════
fn read_image(dev: &mut dyn SectorDev) -> NopwdResult<Vec<u8>> {
    let mut img = Vec::with_capacity(14 * SECTOR);
    for lba in 0..14u32 {
        let sector = dev
            .read_sector(lba)
            .map_err(|e| err(EXIT_IO, format!("错误: {}", e)))?;
        if sector.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: LBA{} 读取 {}B，预期完整扇区 {}B",
                    lba,
                    sector.len(),
                    SECTOR
                ),
            ));
        }
        img.extend_from_slice(&sector);
    }
    Ok(img)
}

fn guard_system_disk(disk: u32) -> NopwdResult<()> {
    if disk < 2 {
        return Err(err(EXIT_TARGET, format!("错误: 拒绝系统盘 disk{}(须 disk2+)", disk)));
    }
    Ok(())
}

fn auto_pick_disk(runner: &dyn CmdRunner, prompt: &mut dyn Prompter) -> NopwdResult<u32> {
    let disks = sysinfo::list_usb_disks(runner);
    if disks.is_empty() {
        return Err(err(EXIT_TARGET, "错误: 未检测到外部 USB 盘。插入后重试, 或 --disk N 手动指定。"));
    }
    if disks.len() == 1 {
        return Ok(disks[0].n);
    }
    println!("检测到多个 USB 盘:");
    print!("{}", disk_menu_str(&disks));
    loop {
        let c = prompt.prompt_line(&crate::ui::bold(&format!("选择 [1-{}] (回车取消): ", disks.len())));
        let c = c.trim();
        if c.is_empty() {
            return Err(err(EXIT_CANCELLED, "已取消"));
        }
        if let Ok(n) = c.parse::<usize>() {
            if (1..=disks.len()).contains(&n) {
                return Ok(disks[n - 1].n);
            }
        }
        println!("{}", crate::ui::yellow("无效输入"));
    }
}

/// run/apply 共用主流程(apply=false 即 dry-run)。disk 为已选定并通过系统盘防护的盘号。
pub fn apply_flow(
    apply: bool,
    force: bool,
    disk: u32,
    size_gb: Option<f64>,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> NopwdResult<i32> {
    guard_system_disk(disk)?;
    let runner = ctx.runner;

    let secs = sysinfo::disk_total_sectors(runner, disk);
    let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
    let sz = match secs {
        Some(s) => fmt_gb(s * SECTOR as u64),
        None => "unknown 扇".to_string(),
    };
    println!("{}  disk{} · {} · USB {}:{}", crate::ui::bold("盘"), disk, sz, vid, pid);

    let img = read_image(dev)?;
    let id = identify(runner, disk, &img[7 * SECTOR..8 * SECTOR]);
    let did = match id.device_id {
        Some(d) => d,
        None => {
            return Err(err(EXIT_TARGET, "错误: 无法识别 device_id(LBA7 两候选均未解出 EDPF); 可插好盘重试"))
        }
    };
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = diskio::lba4_label_id_from(lba4);
    let facts = DiskFacts { disk, total_sectors: secs, vid, pid, label_id };
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;

    let read = |lba: u32| -> NopwdResult<Vec<u8>> {
        Ok(img[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    let result = convert(&read, &did, size_gb, true)?;

    let baks = find_backups(&ctx.backup_dir, &facts, Some(&did), Some(tag16));
    if !baks.is_empty() {
        println!("\n{}  本盘已有 {} 份(写入时会自动再备份):", crate::ui::bold("备份"), baks.len());
        let entries: Vec<(String, bool)> = baks
            .iter()
            .map(|b| {
                (
                    diskio::backup_display_time(b, diskio::mtime_epoch(b)),
                    backup_is_nopwd(b, &did),
                )
            })
            .collect();
        print!("{}", backup_menu_str(&entries));
    } else {
        println!("\n{}  尚无; 写入时自动创建首个备份", crate::ui::bold("备份"));
    }

    let already = looks_nopwd(&read, &did)?;
    if already {
        println!("\n{}", crate::ui::yellow("提示: 该盘已是改造后的免密盘 — 再次写入只会重写相同内容(实测幂等)。"));
    }
    if !apply {
        let tail = if already { " (该盘已是免密盘, 须加 --force)" } else { "" };
        println!(
            "{}",
            crate::ui::dim(&format!(
                "操作  以上为预览(dry-run), 未写盘。执行写入: nopwd apply --disk {}{}",
                disk, tail
            ))
        );
        return Ok(EXIT_OK);
    }
    if already && !force {
        return Err(err(
            EXIT_ALREADY_NOPWD,
            format!(
                "错误: 该盘已是免密盘, 拒绝重复写入(重写内容相同, 实测幂等无害)。确需重写: nopwd apply --disk {} --force",
                disk
            ),
        ));
    }
    if already {
        println!(
            "{}",
            crate::ui::yellow("--force: 继续重写。本次自动备份将标记为免密状态(文件名含 _nopwd); 加密原盘备份是更早时间戳那份。")
        );
    }

    let (bpath, _nopwd) = backup_disk(&facts, &img, &did, &ctx.backup_dir, ctx.clock)?;
    println!("{}  nopwd restore \"{}\" --disk {} --yes", crate::ui::bold("还原"), bpath.display(), disk);

    if !ctx.prompt.confirm_yes(&crate::ui::bold(&format!("将改写 disk{} LBA0/6/7/12/9。输入 YES: ", disk))) {
        return Err(err(EXIT_CANCELLED, "已取消(未写盘)"));
    }
    sysinfo::unmount_disk(ctx.runner, disk);
    // 卸载后才切 O_RDWR(挂载态打开读写会撞 EBUSY); 写序由 atomic_write_sectors
    // 保证: LBA0(唯一改 MBR 的扇区)最后写, 单 fd 全程持有到写完校验完
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e)))?;
    let mut writes: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    writes.insert(6, result.lba6);
    writes.insert(7, result.lba7);
    writes.insert(12, result.lba12);
    if let Some(l9) = result.lba9 {
        writes.insert(9, l9);
    }
    writes.insert(0, result.lba0);
    diskio::atomic_write_sectors(dev, &writes)?;
    println!(
        "{}",
        crate::ui::green("已写入, 读回校验通过。请拔出 U 盘重新插入, 数据区格式化 exFAT/NTFS 即得免密可写区。")
    );
    Ok(EXIT_OK)
}

/// restore 主流程: bin=None 时交互列出本盘备份并选择。
pub fn restore_flow(
    bin: Option<String>,
    disk: u32,
    ctx: &mut Ctx,
    dev: &mut dyn SectorDev,
) -> NopwdResult<i32> {
    guard_system_disk(disk)?;
    let runner = ctx.runner;

    let img = read_image(dev)?;
    let id = identify(runner, disk, &img[7 * SECTOR..8 * SECTOR]);
    let did = id.device_id;
    let lba4 = &img[4 * SECTOR..5 * SECTOR];
    let label_id = diskio::lba4_label_id_from(lba4);
    let tag16 = diskio::lba4_tag16_from(lba4)
        .ok_or_else(|| err(EXIT_IO, "错误: LBA4 缺少 16B 身份标签"))?;
    if tag16.iter().all(|&b| b == 0) {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前盘 LBA4 身份标签为空，无法确认备份归属，拒绝还原",
        ));
    }

    let path: PathBuf = match bin {
        Some(p) => PathBuf::from(p),
        None => {
            let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
            let facts = DiskFacts {
                disk,
                total_sectors: sysinfo::disk_total_sectors(runner, disk),
                vid,
                pid,
                label_id: label_id.clone(),
            };
            let baks = find_backups(&ctx.backup_dir, &facts, did.as_deref(), Some(tag16));
            if baks.is_empty() {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: 备份目录未找到本盘备份; 可 nopwd restore <备份.bin> 显式指定",
                ));
            }
            println!("disk{} 匹配备份 {} 个(新→旧):", disk, baks.len());
            let entries: Vec<(String, bool)> = baks
                .iter()
                .map(|b| {
                    (
                        diskio::backup_display_time(b, diskio::mtime_epoch(b)),
                        did.as_ref().map(|d| backup_is_nopwd(b, d)).unwrap_or(false),
                    )
                })
                .collect();
            print!("{}", backup_menu_str(&entries));
            let sel = loop {
                let c = ctx.prompt.prompt_line(&crate::ui::bold(&format!("选择 [1-{}] (回车取消): ", baks.len())));
                let c = c.trim();
                if c.is_empty() {
                    return Err(err(EXIT_CANCELLED, "已取消"));
                }
                if let Ok(n) = c.parse::<usize>() {
                    if (1..=baks.len()).contains(&n) {
                        break baks[n - 1].clone();
                    }
                }
                println!("{}", crate::ui::yellow("无效输入"));
            };
            sel
        }
    };

    // 显式备份文件的预检 + 写入
    let data = std::fs::read(&path)
        .map_err(|e| err(EXIT_BACKUP, format!("错误: 无法读取备份 {}: {}", path.display(), e)))?;
    if data.len() != 14 * SECTOR {
        return Err(err(EXIT_BACKUP, format!("错误: 备份大小 {} ≠ {}", data.len(), 14 * SECTOR)));
    }
    let md5_path = diskio::md5_sidecar_path(&path);
    let want = match diskio::read_backup_md5(&path) {
        Ok(Some(expected)) => expected,
        Ok(None) => {
            return Err(err(
                EXIT_BACKUP,
                format!("错误: 备份缺少校验文件 {}，拒绝还原", md5_path.display()),
            ));
        }
        Err(e) => {
            return Err(err(
                EXIT_BACKUP,
                format!("错误: 无法读取有效校验 {}: {}", md5_path.display(), e),
            ));
        }
    };
    let got = crate::md5::md5_hex(&data);
    if want != got {
        return Err(err(
            EXIT_BACKUP,
            format!("错误: 备份 MD5 不符(期望 {}, 实际 {}) — 文件损坏?", want, got),
        ));
    }
    println!("{}  {}", crate::ui::green("MD5 校验通过"), got);

    // 显式路径也必须执行与交互选择相同的“同一物理盘”终验。device_id/容量/VID/PID
    // 对同型号盘并不唯一，LBA4 前 16B 才是现有备份体系使用的最终身份标签。
    let backup_lba4 = &data[4 * SECTOR..5 * SECTOR];
    let backup_tag16 = diskio::lba4_tag16_from(backup_lba4)
        .ok_or_else(|| err(EXIT_BACKUP, "错误: 备份 LBA4 缺少 16B 身份标签"))?;
    if tag16.iter().any(|&b| b != 0) && backup_tag16 != tag16 {
        let current_id = label_id.as_deref().unwrap_or("未知");
        let backup_id = diskio::lba4_label_id_from(backup_lba4).unwrap_or_else(|| "未知".into());
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: 备份属于另一块盘(current onlyid={}, backup onlyid={})，拒绝还原",
                current_id, backup_id
            ),
        ));
    }
    let backup_meta = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(diskio::parse_backup_name);
    let tagged_nopwd = backup_meta
        .as_ref()
        .map(|meta| meta.tagged_nopwd)
        .unwrap_or(false);
    let detection_did = did
        .as_deref()
        .or_else(|| backup_meta.as_ref().map(|meta| meta.device_id.as_str()));
    let nopwd_snap = if tagged_nopwd {
        true
    } else {
        let Some(device_id) = detection_did else {
            return Err(err(
                EXIT_BACKUP,
                "错误: 当前盘与备份文件名都无法提供 device_id，无法确认备份是否为免密状态，拒绝还原",
            ));
        };
        diskio::image_is_nopwd(&data, device_id)
    };
    if nopwd_snap {
        println!(
            "{}",
            crate::ui::yellow("注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。")
        );
        println!(
            "{}",
            crate::ui::dim(&format!(
                "[dry-run] 将还原 {} → disk{} LBA0-13 ({}B) — 未写入(免密快照不作还原)。",
                path.display(),
                disk,
                data.len()
            ))
        );
        return Ok(EXIT_OK);
    }
    println!(
        "{}  {}",
        crate::ui::bold("还原"),
        crate::ui::truncate_mid(&path.display().to_string(), 64)
    );
    if !ctx.prompt.confirm_yes(&crate::ui::bold(&format!("  → disk{} LBA0-13? 输入 YES: ", disk))) {
        return Err(err(EXIT_CANCELLED, "已取消"));
    }
    sysinfo::unmount_disk(ctx.runner, disk);
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e)))?;
    let writes: BTreeMap<u32, Vec<u8>> = (0..14u32)
        .map(|lba| (lba, data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec()))
        .collect();
    diskio::atomic_write_sectors(dev, &writes)?;
    println!("{}", crate::ui::green("已还原, 读回校验通过。请拔出重插。"));
    Ok(EXIT_OK)
}

/// 离线转换(不碰真盘)。
pub fn convert_flow(dir: String, id: Option<String>, size: Option<f64>, out: Option<String>) -> i32 {
    let Some(id) = id else {
        eprintln!("错误: 离线模式需 --id <device_id>");
        return EXIT_USAGE;
    };
    let d = Path::new(&dir);
    let read = |lba: u32| -> NopwdResult<Vec<u8>> { Ok(diskio::read_lba_file(d, lba)) };
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
        for (lba, data) in
            [(0u32, &result.lba0), (6, &result.lba6), (7, &result.lba7), (12, &result.lba12)]
        {
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
// 5. 备份管理(永不提权)
// ══════════════════════════════════════════════════════════════════

fn backup_model_name(meta: &BackupMeta) -> String {
    let mut vendor = None;
    let mut product = None;
    let mut revision = None;
    for part in meta.device_id.split('&') {
        if let Some(v) = part.strip_prefix("ven_") {
            vendor = Some(v.replace('_', " "));
        } else if let Some(v) = part.strip_prefix("prod_") {
            product = Some(v.replace('_', " "));
        } else if let Some(v) = part.strip_prefix("rev_") {
            revision = Some(v.replace('_', " "));
        }
    }
    let mut out = match (vendor, product) {
        (Some(v), Some(p)) => format!("{} {}", v, p),
        (Some(v), None) => v,
        _ => meta.device_id.clone(),
    };
    if let Some(r) = revision {
        out.push_str(&format!(" ({})", r));
    }
    out
}

fn backup_capacity(meta: &BackupMeta) -> String {
    meta.secs
        .and_then(|s| s.checked_mul(SECTOR as u64))
        .map(crate::common::fmt_gb)
        .unwrap_or_else(|| "容量未知".into())
}

fn backup_kind(entry: &BackupEntry) -> &'static str {
    if entry.is_nopwd || entry.meta.as_ref().map(|m| m.tagged_nopwd).unwrap_or(false) {
        "[免密状态]"
    } else {
        "[加密原盘]"
    }
}

fn backup_health(entry: &BackupEntry) -> String {
    if !entry.size_ok {
        return crate::ui::red(&format!("大小 ✗ (应为 {}B)", 14 * SECTOR));
    }
    match entry.md5_ok {
        Md5Status::Ok => crate::ui::green("MD5 ✓"),
        Md5Status::Mismatch => crate::ui::red("MD5 ✗ 损坏"),
        Md5Status::NoSidecar => crate::ui::yellow("(缺 .md5)"),
    }
}

fn print_numbered_backup_entries(entries: &[&BackupEntry]) {
    let width = entries.len().max(1).to_string().len();
    for (idx, entry) in entries.iter().enumerate() {
        let time = diskio::backup_display_time(&entry.path, entry.mtime);
        println!(
            "  [{}] {}   {}   {}",
            crate::ui::pad_left(&(idx + 1).to_string(), width),
            crate::ui::pad_to(&time, 16),
            crate::ui::pad_to(backup_kind(entry), 12),
            backup_health(entry)
        );
        println!("      └─ {}", crate::ui::dim(backup_catalog::file_name(entry)));
    }
}

fn print_onlyid_backup_choices(id: &str, group: &[&BackupEntry]) {
    if let Some(meta) = group.first().and_then(|e| e.meta.as_ref()) {
        println!(
            "{} · {} · onlyid={} · {} 份",
            backup_model_name(meta),
            backup_capacity(meta),
            id,
            group.len()
        );
    } else {
        println!("onlyid={} · {} 份", id, group.len());
    }
    print_numbered_backup_entries(group);
}

fn print_inspect_backup_sources(entries: &[BackupEntry]) -> bool {
    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    for entry in entries {
        let Some(id) = entry.meta.as_ref().and_then(|m| m.onlyid.as_ref()) else {
            continue;
        };
        groups.entry(id.clone()).or_default().push(entry);
    }
    if groups.is_empty() {
        return false;
    }
    let mut groups: Vec<(String, Vec<&BackupEntry>)> = groups.into_iter().collect();
    for (_, group) in &mut groups {
        backup_catalog::sort_newest_first(group);
    }
    groups.sort_by(|a, b| {
        match (a.1.first(), b.1.first()) {
            (Some(ae), Some(be)) => {
                diskio::cmp_backup_newest_first(ae, be).then_with(|| a.0.cmp(&b.0))
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.0.cmp(&b.0),
        }
    });
    println!("{}", crate::ui::bold("可查看的备份盘:"));
    for (id, group) in groups {
        let latest = group
            .first()
            .map(|e| diskio::backup_display_time(&e.path, e.mtime))
            .unwrap_or_default();
        let model = group
            .first()
            .and_then(|e| e.meta.as_ref())
            .map(backup_model_name)
            .unwrap_or_else(|| "未知型号".into());
        println!(
            "  {}  {} · {} 份 · 最新 {}",
            crate::ui::bold_cyan(&format!("onlyid={}", id)),
            model,
            group.len(),
            latest
        );
    }
    println!();
    println!("{}", crate::ui::dim("查看某盘: nopwd inspect --onlyid <ID>"));
    true
}

fn parse_backup_selection_tokens(tokens: &[String], max: usize) -> Result<Vec<usize>, String> {
    let mut selected = std::collections::BTreeSet::new();
    for token in tokens {
        for raw in token.split(',') {
            let part = raw.trim();
            if part.is_empty() {
                return Err("备份编号不能为空".into());
            }
            if let Some((left, right)) = part.split_once('-') {
                if right.contains('-') {
                    return Err(format!("无法解析备份范围: {}", part));
                }
                let start = left
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", left))?;
                let end = right
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", right))?;
                if start == 0 || end == 0 || start > end || end > max {
                    return Err(format!("备份范围超出 1-{}: {}", max, part));
                }
                selected.extend(start..=end);
            } else {
                let idx = part
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", part))?;
                if idx == 0 || idx > max {
                    return Err(format!("备份编号超出 1-{}: {}", max, part));
                }
                selected.insert(idx);
            }
        }
    }
    if selected.is_empty() {
        return Err("至少选择一份备份".into());
    }
    Ok(selected.into_iter().collect())
}

pub fn backup_list(backup_dir: &Path, onlyid: Option<&str>) -> i32 {
    let catalog = BackupCatalog::load(backup_dir);
    let selected: Vec<&BackupEntry> = if let Some(id) = onlyid {
        match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else {
        catalog.entries().iter().collect()
    };
    println!("备份目录 {} · {} 份", backup_dir.display(), selected.len());
    if selected.is_empty() {
        return EXIT_OK;
    }

    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    let mut unknown = Vec::new();
    for entry in selected {
        if let Some(key) = diskio::backup_group_key(entry) {
            groups.entry(key).or_default().push(entry);
        } else {
            unknown.push(entry);
        }
    }
    let mut grouped: Vec<Vec<&BackupEntry>> = groups.into_values().collect();
    for group in &mut grouped {
        backup_catalog::sort_newest_first(group);
    }
    grouped.sort_by(|a, b| {
        match (a.first(), b.first()) {
            (Some(ae), Some(be)) => diskio::cmp_backup_newest_first(ae, be),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    for group in grouped {
        let Some(meta) = group.first().and_then(|entry| entry.meta.as_ref()) else {
            continue;
        };
        let identity = match &meta.onlyid {
            Some(id) => format!("onlyid={}", id),
            None => crate::ui::yellow("未知盘"),
        };
        println!();
        println!(
            "{} · {} · {} · {} 份",
            backup_model_name(meta),
            backup_capacity(meta),
            identity,
            group.len()
        );
        print_numbered_backup_entries(&group);
    }
    if !unknown.is_empty() {
        println!();
        for entry in unknown {
            let name = entry.path.file_name().and_then(|n| n.to_str()).unwrap_or("<无效文件名>");
            println!("  └─ {}   {}", crate::ui::dim(name), crate::ui::dim("未识别(非本工具命名)"));
        }
    }
    EXIT_OK
}

pub fn backup_verify(backup_dir: &Path, onlyid: Option<&str>, target: Option<&str>) -> i32 {
    backup_verify_select(backup_dir, onlyid, target, None)
}

fn backup_verify_select(
    backup_dir: &Path,
    onlyid: Option<&str>,
    target: Option<&str>,
    index: Option<usize>,
) -> i32 {
    let catalog = BackupCatalog::load(backup_dir);
    let selected: Vec<&BackupEntry> = if let Some(target) = target {
        match catalog.resolve_target(target) {
            Ok(entry) => vec![entry],
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else if let Some(id) = onlyid {
        if let Some(idx) = index {
            match catalog.onlyid_index(id, idx) {
                Ok(entry) => vec![entry],
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    if let Ok(group) = catalog.onlyid_group(id) {
                        println!();
                        print_onlyid_backup_choices(id, &group);
                    }
                    return EXIT_BACKUP;
                }
            }
        } else {
            match catalog.onlyid_group(id) {
                Ok(group) => group,
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_BACKUP;
                }
            }
        }
    } else {
        if !backup_dir.is_dir() {
            eprintln!("{}", crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display())));
            return EXIT_BACKUP;
        }
        catalog.entries().iter().collect()
    };

    if selected.is_empty() {
        println!("没有可校验的 .bin 备份。");
        return EXIT_OK;
    }
    let mut bad = 0usize;
    for entry in selected {
        let name = entry.path.file_name().and_then(|n| n.to_str()).unwrap_or("<无效文件名>");
        if backup_catalog::is_healthy(entry) {
            println!("{}  {}", crate::ui::green("✓"), name);
        } else {
            bad += 1;
            println!("{}  {}  {}", crate::ui::red("✗"), name, backup_health(entry));
        }
    }
    if bad == 0 {
        println!("校验通过。");
        EXIT_OK
    } else {
        eprintln!("{}", crate::ui::red(&format!("校验失败: {} 份备份异常。", bad)));
        EXIT_BACKUP
    }
}

fn delete_backup_pair(path: &Path) -> Result<(), String> {
    if let Err(e) = fs::remove_file(path) {
        let suffix = if e.kind() == io::ErrorKind::PermissionDenied {
            "；备份目录可能由 root 持有且不可写，可检查目录属主/权限，必要时使用 sudo rm 手动删除"
        } else {
            ""
        };
        return Err(format!("删除失败 {}: {}{}", path.display(), e, suffix));
    }
    let sidecar = PathBuf::from(format!("{}.md5", path.display()));
    if sidecar.exists() {
        if let Err(e) = fs::remove_file(&sidecar) {
            let suffix = if e.kind() == io::ErrorKind::PermissionDenied {
                "；备份目录可能由 root 持有且不可写，可检查目录属主/权限，必要时使用 sudo rm 手动删除"
            } else {
                ""
            };
            return Err(format!("已删除 .bin，但删除校验文件失败 {}: {}{}", sidecar.display(), e, suffix));
        }
    }
    Ok(())
}

pub fn backup_prune(backup_dir: &Path, onlyid: Option<&str>, keep: usize, yes: bool) -> i32 {
    if !backup_dir.is_dir() {
        eprintln!("{}", crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display())));
        return EXIT_BACKUP;
    }
    let catalog = BackupCatalog::load(backup_dir);
    let selected_refs: Vec<&BackupEntry> = if let Some(id) = onlyid {
        match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else {
        catalog.entries().iter().collect()
    };
    let selected: Vec<BackupEntry> = selected_refs.into_iter().cloned().collect();
    let candidates = diskio::prune_candidates(&selected, keep);
    let candidate_set: std::collections::BTreeSet<PathBuf> = candidates
        .iter()
        .map(|p| backup_catalog::canonical_entry_path(p))
        .collect();

    let originals = selected.iter().filter(|e| e.meta.is_some() && !e.is_nopwd).count();
    let snapshots = selected.iter().filter(|e| e.meta.is_some() && e.is_nopwd).count();
    let keep_snapshots = snapshots.saturating_sub(candidates.len());
    if candidates.is_empty() {
        println!("无需清理：当前策略不会删除任何备份。");
        println!("保留: 加密原盘 {} 份 · 免密快照 {} 份", originals, keep_snapshots);
        return EXIT_OK;
    }

    println!(
        "将删除 {} 个免密状态快照(每盘保留最新 {} 份, 加密原盘永不自动删除):",
        candidates.len(), keep
    );
    for path in &candidates {
        let model = selected
            .iter()
            .find(|e| {
                backup_catalog::canonical_entry_path(&e.path)
                    == backup_catalog::canonical_entry_path(path)
            })
            .and_then(|e| e.meta.as_ref())
            .map(backup_model_name)
            .unwrap_or_else(|| "未知盘".into());
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("<无效文件名>");
        println!("  {}   {}", name, model);
    }
    println!();
    println!("保留: 加密原盘 {} 份 · 免密快照 {} 份", originals, keep_snapshots);
    if !yes {
        match onlyid {
            Some(id) => println!("确认执行: nopwd backup prune --onlyid {} --keep {} --yes", id, keep),
            None => println!("确认执行: nopwd backup prune --keep {} --yes", keep),
        }
        return EXIT_OK;
    }

    let mut failed = 0usize;
    for entry in &selected {
        if candidate_set.contains(&backup_catalog::canonical_entry_path(&entry.path)) {
            if let Err(msg) = delete_backup_pair(&entry.path) {
                failed += 1;
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
            }
        }
    }
    if failed == 0 {
        println!("{}", crate::ui::green(&format!("已删除 {} 份免密状态快照。", candidates.len())));
        EXIT_OK
    } else {
        EXIT_BACKUP
    }
}

pub fn backup_rm(
    backup_dir: &Path,
    onlyid: Option<&str>,
    targets: &[String],
    yes: bool,
    prompt: &mut dyn Prompter,
) -> i32 {
    if !backup_dir.is_dir() {
        eprintln!("{}", crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display())));
        return EXIT_BACKUP;
    }
    let catalog = BackupCatalog::load(backup_dir);
    let entries = catalog.entries();
    let mut numbered_index: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut selected_group_len = None;
    let mut resolved = if let Some(id) = onlyid {
        let group = match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        };
        selected_group_len = Some(group.len());
        for (idx, entry) in group.iter().enumerate() {
            numbered_index.insert(backup_catalog::canonical_entry_path(&entry.path), idx + 1);
        }

        let indices = if targets.is_empty() {
            print_onlyid_backup_choices(id, &group);
            loop {
                let input = prompt.prompt_line("选择要删除的备份 [如 2 / 1,3 / 2-3，回车取消]: ");
                let input = input.trim();
                if input.is_empty() {
                    eprintln!("已取消");
                    return EXIT_CANCELLED;
                }
                match parse_backup_selection_tokens(&[input.to_string()], group.len()) {
                    Ok(v) => break v,
                    Err(msg) => eprintln!("{}", crate::ui::red(&format!("错误: {}", msg))),
                }
            }
        } else {
            match parse_backup_selection_tokens(targets, group.len()) {
                Ok(v) => v,
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_USAGE;
                }
            }
        };
        indices
            .into_iter()
            .map(|idx| backup_catalog::canonical_entry_path(&group[idx - 1].path))
            .collect::<Vec<_>>()
    } else {
        if targets.is_empty() {
            eprintln!("{}", crate::ui::red("错误: backup rm 至少需要一个路径或文件名"));
            return EXIT_USAGE;
        }
        let mut paths = Vec::new();
        for target in targets {
            match catalog.resolve_target(target) {
                Ok(entry) => {
                    let path = backup_catalog::canonical_entry_path(&entry.path);
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_BACKUP;
                }
            }
        }
        paths
    };
    resolved.dedup();

    let mut total_per_group: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleting_per_group: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        if let Some(key) = diskio::backup_group_key(entry) {
            *total_per_group.entry(key.clone()).or_default() += 1;
            let ep = backup_catalog::canonical_entry_path(&entry.path);
            if resolved.contains(&ep) {
                *deleting_per_group.entry(key).or_default() += 1;
            }
        }
    }
    for (key, deleting) in &deleting_per_group {
        let total = total_per_group.get(key).copied().unwrap_or(0);
        if *deleting >= total && total > 0 {
            eprintln!(
                "{}",
                crate::ui::red("错误: 安全保护拒绝删除——该盘将被清到零份备份；至少保留 1 份。")
            );
            return EXIT_BACKUP;
        }
    }

    println!("将删除 {} 份备份:", resolved.len());
    for path in &resolved {
        let entry = entries
            .iter()
            .find(|e| backup_catalog::canonical_entry_path(&e.path) == *path);
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("<无效文件名>");
        match entry {
            Some(e) if onlyid.is_some() => {
                let idx = numbered_index.get(path).copied().unwrap_or(0);
                let time = diskio::backup_display_time(&e.path, e.mtime);
                println!(
                    "  [{}] {}   {}   {}",
                    idx,
                    time,
                    backup_kind(e),
                    backup_health(e)
                );
                println!("      └─ {}", crate::ui::dim(name));
            }
            Some(e) => println!("  {}   {}   {}", name, backup_kind(e), backup_health(e)),
            None => println!("  {}   {}", name, crate::ui::yellow("未识别")),
        }
    }
    if let Some(total) = selected_group_len {
        println!("删除后该盘仍保留 {} 份备份。", total.saturating_sub(resolved.len()));
    }
    if !yes && !prompt.confirm_yes("输入 YES 确认删除: ") {
        eprintln!("已取消");
        return EXIT_CANCELLED;
    }

    let mut failed = 0usize;
    for path in &resolved {
        if let Err(msg) = delete_backup_pair(path) {
            failed += 1;
            eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
        }
    }
    if failed == 0 {
        println!("{}", crate::ui::green(&format!("已删除 {} 份备份。", resolved.len())));
        EXIT_OK
    } else {
        EXIT_BACKUP
    }
}

// ══════════════════════════════════════════════════════════════════
// 6. 扇区检查器（物理盘只读 / 备份永不提权）
// ══════════════════════════════════════════════════════════════════

fn resolve_inspect_file(backup_dir: &Path, target: &str) -> Result<PathBuf, String> {
    let raw = Path::new(target);
    let candidate = if raw.components().count() == 1 {
        backup_dir.join(raw)
    } else if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(raw)
    };
    let path = fs::canonicalize(&candidate)
        .map_err(|e| format!("文件不存在或不可访问 {}: {}", candidate.display(), e))?;
    if !path.is_file() {
        return Err(format!("不是普通文件: {}", path.display()));
    }
    Ok(path)
}

fn plain_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("+0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&b) = line.get(i) {
                out.push_str(&format!("{b:02X} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for &b in line {
            out.push(if (0x20..=0x7e).contains(&b) { b as char } else { '.' });
        }
        out.push('\n');
    }
    out
}

fn export_inspect_view(dir: &Path, view: &inspect::SectorView) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let base = format!("LBA{:02}", view.lba);
    fs::write(dir.join(format!("{base}_raw.bin")), &view.raw)?;
    fs::write(dir.join(format!("{base}_decoded.bin")), &view.decoded)?;
    fs::write(dir.join(format!("{base}_raw.hex")), plain_hex(&view.raw))?;
    fs::write(dir.join(format!("{base}_decoded.hex")), plain_hex(&view.decoded))?;
    Ok(())
}

fn print_inspect_meta(meta: &InspectMeta) {
    let mut parts = Vec::new();
    if let Some(id) = &meta.onlyid {
        parts.push(format!("onlyid={id}"));
    }
    if let (Some(v), Some(p)) = (&meta.vid, &meta.pid) {
        parts.push(format!("USB {v}:{p}"));
    }
    if let Some(size) = meta.size_bytes {
        parts.push(crate::common::fmt_gb(size));
    }
    if !parts.is_empty() {
        println!("{}  {}", crate::ui::bold("设备"), parts.join(" · "));
    }
    if let Some(did) = &meta.device_id {
        println!("{}  {}", crate::ui::bold("device_id"), did);
    } else {
        println!(
            "{}",
            crate::ui::yellow("device_id 未识别：LBA7/8/9/12 只能显示 RAW；可用 --id 手动指定")
        );
    }
}

fn render_inspect_source<F>(
    source_label: &str,
    meta: &InspectMeta,
    opts: &InspectOpts,
    mut read: F,
) -> i32
where
    F: FnMut(u32) -> io::Result<Vec<u8>>,
{
    println!("{}  {}", crate::ui::bold("来源"), source_label);
    print_inspect_meta(meta);

    let export_dir = opts.export.as_deref().map(PathBuf::from);
    if opts.lbas.is_empty() && !opts.raw && !opts.hex {
        println!();
        println!("{}", crate::ui::bold("LBA 0-13 概览:"));
        for lba in 0..14u32 {
            match read(lba) {
                Ok(raw) => {
                    let view = inspect::analyze_sector(lba, &raw, meta);
                    println!("  {}", inspect::overview_line(&view));
                    if let Some(dir) = &export_dir {
                        if let Err(e) = export_inspect_view(dir, &view) {
                            eprintln!("{}", crate::ui::red(&format!("错误: 导出 LBA{lba} 失败: {e}")));
                            return EXIT_IO;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: 读取 LBA{lba} 失败: {e}")));
                    return EXIT_IO;
                }
            }
        }
        println!();
        println!(
            "{}",
            crate::ui::dim("指定 LBA 可展开结构化字段，例如: nopwd inspect 6 7 12 --disk N --hex")
        );
        if let Some(dir) = &export_dir {
            println!("{}  {}", crate::ui::green("已导出"), dir.display());
        }
        return EXIT_OK;
    }

    // 用户显式要求 --hex / --raw 时不能悄悄忽略旗标：未给 LBA 就展开全部 0-13。
    let detailed_lbas: Vec<u32> = if opts.lbas.is_empty() {
        (0..14u32).collect()
    } else {
        opts.lbas.clone()
    };
    for &lba in &detailed_lbas {
        let raw = match read(lba) {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!("{}", crate::ui::red(&format!("错误: 读取 LBA{lba} 失败: {e}")));
                return EXIT_IO;
            }
        };
        let view = inspect::analyze_sector(lba, &raw, meta);
        println!();
        println!("{}  {}", crate::ui::bold(&format!("LBA{lba}")), view.method);
        let fields = inspect::render_fields(&view);
        if !fields.is_empty() {
            print!("{}", fields);
        }
        for note in &view.notes {
            println!("  {} {}", crate::ui::dim("└─"), crate::ui::dim(note));
        }
        if opts.raw {
            print!("{}", inspect::render_hex(&view, true));
        } else if opts.hex || view.fields.is_empty() {
            print!("{}", inspect::render_hex(&view, false));
        }
        if let Some(dir) = &export_dir {
            if let Err(e) = export_inspect_view(dir, &view) {
                eprintln!("{}", crate::ui::red(&format!("错误: 导出 LBA{lba} 失败: {e}")));
                return EXIT_IO;
            }
        }
    }
    if let Some(dir) = &export_dir {
        println!();
        println!("{}  {}", crate::ui::green("已导出"), dir.display());
    }
    EXIT_OK
}

fn inspect_backup_flow(opts: InspectOpts) -> i32 {
    let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let (path, parsed_meta, source_label) = if let Some(id) = opts.onlyid.as_deref() {
        let catalog = BackupCatalog::load(&bak);
        let group = match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                if print_inspect_backup_sources(catalog.entries()) {
                    println!();
                }
                return EXIT_BACKUP;
            }
        };
        let Some(idx) = opts.index else {
            print_onlyid_backup_choices(id, &group);
            println!();
            println!(
                "{}",
                crate::ui::bold(&format!(
                    "继续查看: nopwd inspect --onlyid {} --index N [LBA...] [--hex]",
                    id
                ))
            );
            println!(
                "{}",
                crate::ui::dim(&format!(
                    "例如最新一份: nopwd inspect --onlyid {} --index 1",
                    id
                ))
            );
            return EXIT_OK;
        };
        let entry = match catalog.onlyid_index(id, idx) {
            Ok(entry) => entry,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                println!();
                print_onlyid_backup_choices(id, &group);
                return EXIT_BACKUP;
            }
        };
        (
            entry.path.clone(),
            entry.meta.clone(),
            format!(
                "backup onlyid={id} [{idx}] · {}",
                backup_catalog::file_name(entry)
            ),
        )
    } else {
        let Some(target) = opts.backup.as_deref() else {
            eprintln!("{}", crate::ui::red("错误: inspect 缺少备份文件来源"));
            return EXIT_USAGE;
        };
        let path = match resolve_inspect_file(&bak, target) {
            Ok(p) => p,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                return EXIT_BACKUP;
            }
        };
        let meta = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(diskio::parse_backup_name);
        let label = path.display().to_string();
        (path, meta, label)
    };
    let mut meta = parsed_meta
        .as_ref()
        .map(InspectMeta::from_backup_meta)
        .unwrap_or_default();
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }
    let path_s = path.to_string_lossy().into_owned();
    render_inspect_source(&source_label, &meta, &opts, |lba| diskio::read_lba(&path_s, lba))
}

fn inspect_disk_flow(runner: &SysRunner, mut opts: InspectOpts) -> i32 {
    if let Some(n) = opts.disk {
        if let Err(e) = guard_system_disk(n) {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    }
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        if opts.disk.is_none() {
            let mut prompt = StdPrompter;
            let n = match auto_pick_disk(runner, &mut prompt) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&e.msg));
                    return e.code;
                }
            };
            argv.push("--disk".into());
            argv.push(n.to_string());
        }
        elevate::ensure_elevated(&argv);
        unreachable!();
    }
    let n = match opts.disk {
        Some(n) => n,
        None => {
            let mut prompt = StdPrompter;
            match auto_pick_disk(runner, &mut prompt) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&e.msg));
                    return e.code;
                }
            }
        }
    };
    opts.disk = Some(n);
    let path = raw_path(n);
    let raw7 = diskio::read_lba(&path, 7).ok();
    let id = raw7
        .as_deref()
        .and_then(|r| identify(runner, n, r).device_id);
    let (vid, pid) = sysinfo::usb_vid_pid(runner, n);
    let size_bytes = sysinfo::disk_total_sectors(runner, n).and_then(|s| s.checked_mul(SECTOR as u64));
    let onlyid = diskio::read_lba(&path, 4)
        .ok()
        .and_then(|b| diskio::lba4_label_id_from(&b[..b.len().min(32)]));
    let mut meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes,
        onlyid,
    };
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }
    render_inspect_source(&format!("物理盘 disk{n} ({path})"), &meta, &opts, |lba| {
        diskio::read_lba(&path, lba)
    })
}

fn inspect_flow(runner: &SysRunner, opts: InspectOpts) -> i32 {
    if opts.backup.is_some() || opts.onlyid.is_some() {
        inspect_backup_flow(opts)
    } else {
        if opts.disk.is_none() && sysinfo::list_usb_disks(runner).is_empty() {
            let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
            let entries = diskio::scan_backup_dir(&bak);
            if print_inspect_backup_sources(&entries) {
                println!(
                    "{}",
                    crate::ui::yellow("未检测到外接 USB 盘；上面是当前可离线查看的备份。")
                );
                return EXIT_OK;
            }
            eprintln!("{}", crate::ui::red("错误: 未检测到外接 USB 盘，备份目录中也没有可查看的备份。"));
            return EXIT_TARGET;
        }
        inspect_disk_flow(runner, opts)
    }
}

// ══════════════════════════════════════════════════════════════════
// 7. 入口
// ══════════════════════════════════════════════════════════════════
pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
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
                        "list" | "run" | "apply" | "restore" | "backup" | "inspect" | "convert" | "completion"
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
        Parsed::InternalComplete { kind, onlyid, backup_dir } => {
            for value in completion::dynamic_values(
                &kind,
                onlyid.as_deref(),
                backup_dir.as_deref(),
                &runner,
            ) {
                println!("{}", value);
            }
            EXIT_OK
        }
        Parsed::Version => {
            println!("nopwd {}", env!("CARGO_PKG_VERSION"));
            EXIT_OK
        }
        Parsed::List { backup_dir } => {
            let bak = diskio::resolve_backup_dir(backup_dir.as_deref());
            let read_disk = |disk: u32, lba: u32| diskio::read_lba(&raw_path(disk), lba);
            print!("{}", print_disk_table(&scan_disks(&runner, &bak, &read_disk)));
            EXIT_OK
        }
        Parsed::Backup { action, keep, yes, onlyid, backup_dir } => {
            let bak = diskio::resolve_backup_dir(backup_dir.as_deref());
            match action {
                BackupAction::List => backup_list(&bak, onlyid.as_deref()),
                BackupAction::Verify { target, index } => {
                    backup_verify_select(&bak, onlyid.as_deref(), target.as_deref(), index)
                }
                BackupAction::Prune => backup_prune(&bak, onlyid.as_deref(), keep, yes),
                BackupAction::Rm { targets } => {
                    let mut prompt = StdPrompter;
                    backup_rm(&bak, onlyid.as_deref(), &targets, yes, &mut prompt)
                }
            }
        }
        Parsed::Inspect(opts) => inspect_flow(&runner, opts),
        Parsed::Convert { dir, id, size, out } => match dir {
            Some(d) => convert_flow(d, id, size, out),
            None => {
                eprintln!("错误: convert 需 --dir <快照目录>");
                EXIT_USAGE
            }
        },
        Parsed::Run(opts) => real_flow(
            &runner,
            opts.disk,
            opts.size,
            opts.backup_dir,
            FlowKind::Dry,
        ),
        Parsed::Apply { opts, force, yes } => real_flow(
            &runner,
            opts.disk,
            opts.size,
            opts.backup_dir,
            FlowKind::Apply { force, yes },
        ),
        Parsed::Restore { bin, disk, yes, backup_dir } => real_flow(
            &runner,
            disk,
            None,
            backup_dir,
            FlowKind::Restore { bin, yes },
        ),
    }
}

enum FlowKind {
    Dry,
    Apply { force: bool, yes: bool },
    Restore { bin: Option<String>, yes: bool },
}

/// run/apply/restore 的公共外壳:
///   1) 显式盘号的系统盘拒绝不需要 root, 提权前先判;
///   2) 非 root 且未给 --disk: 先以用户身份选盘(只查 diskutil, 无需权限),
///      把选定盘号并入 sudo 重执行参数, 子进程不再重复选盘;
///   3) root 路径: (必要时交互选盘)→ 以读写打开 rdisk → 执行流程。
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
        if let Err(e) = guard_system_disk(n) {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    }
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        // sudo 清环境变量: $NOPWD_BACKUP_DIR 转显式旗标随 argv 过界(未显式给旗标时)
        if backup_dir_flag.is_none() {
            argv.extend(diskio::backup_dir_argv_suffix(std::env::var("NOPWD_BACKUP_DIR").ok()));
        }
        if disk_opt.is_none() {
            let mut sp = StdPrompter;
            match auto_pick_disk(runner, &mut sp) {
                Ok(n) => {
                    argv.push("--disk".into());
                    argv.push(n.to_string());
                }
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&e.msg));
                    return e.code;
                }
            }
        }
        elevate::ensure_elevated(&argv); // 内部以子进程退出码结束, 不返回
        unreachable!();
    }
    let bak = diskio::resolve_backup_dir(backup_dir_flag.as_deref());
    // 手动 sudo 提醒: shell 环境已被 sudo 剥掉(env 过不了界), 且旗标/配置
    // 都没命中时, 明确告知备份去向与两种正确做法。自动提权的子进程带哨兵, 不提示。
    let has_sentinel = std::env::args().any(|a| a == ELEVATED_FLAG);
    if !has_sentinel
        && diskio::sudo_user().is_some()
        && backup_dir_flag.is_none()
        && std::env::var("NOPWD_BACKUP_DIR").unwrap_or_default().is_empty()
        && diskio::conf_backup_dir().is_none()
    {
        let cwd_bak = std::env::current_dir().unwrap_or_default().join("backup");
        eprintln!(
            "{}",
            crate::ui::yellow(&format!(
                "注意: 手动 sudo 会丢失 shell 环境变量($NOPWD_BACKUP_DIR 未生效), 备份将落在 {}。建议直接 nopwd <子命令>(自动提权), 或在 ~/.nopwd.conf 写 backup_dir 固定目录",
                cwd_bak.display()
            ))
        );
    }
    let mut std_prompter = StdPrompter;
    let mut always = AlwaysYes(StdPrompter); // 无状态, 独立实例
    let prompter: &mut dyn Prompter = if yes { &mut always } else { &mut std_prompter };
    let mut ctx = Ctx { runner, clock: &SystemClock, prompt: prompter, backup_dir: bak };
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
    if let Err(e) = guard_system_disk(n) {
        eprintln!("{}", crate::ui::red(&e.msg));
        return e.code;
    }
    let mut dev = match FileDev::open_rdonly(&raw_path(n)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("错误: 无法打开 {}: {} (加 sudo?)", raw_path(n), e);
            return EXIT_IO;
        }
    };
    let r = match kind {
        FlowKind::Dry => apply_flow(false, false, n, size, &mut ctx, &mut dev),
        FlowKind::Apply { force, .. } => apply_flow(true, force, n, size, &mut ctx, &mut dev),
        FlowKind::Restore { bin, .. } => restore_flow(bin, n, &mut ctx, &mut dev),
    };
    finish(r)
}

fn finish(r: NopwdResult<i32>) -> i32 {
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

    #[test]
    fn parse_bare_and_subcommands() {
        // 裸 nopwd = 打印用法, 不进入任何需要提权的流程
        assert!(matches!(parse_args(&[]).unwrap(), Parsed::Help { topic: None }));
        assert!(matches!(parse_args(&["help".into()]).unwrap(), Parsed::Help { topic: None }));
        assert!(matches!(parse_args(&["version".into()]).unwrap(), Parsed::Version));
        match parse_args(&["apply".into(), "--disk".into(), "6".into(), "--force".into(), "--yes".into()]).unwrap() {
            Parsed::Apply { opts, force, yes } => {
                assert_eq!(opts.disk, Some(6));
                assert!(force && yes);
            }
            _ => panic!("应解析为 Apply"),
        }
        // --disk=4 与 /dev/rdisk4 形式
        match parse_args(&["run".into(), "--disk=4".into()]).unwrap() {
            Parsed::Run(o) => assert_eq!(o.disk, Some(4)),
            _ => panic!(),
        }
        match parse_args(&["run".into(), "--disk".into(), "/dev/rdisk4".into()]).unwrap() {
            Parsed::Run(o) => assert_eq!(o.disk, Some(4)),
            _ => panic!(),
        }
        match parse_args(&["restore".into(), "b.bin".into(), "--disk".into(), "6".into(), "--yes".into()]).unwrap() {
            Parsed::Restore { bin, disk, yes, .. } => {
                assert_eq!(bin.as_deref(), Some("b.bin"));
                assert_eq!(disk, Some(6));
                assert!(yes);
            }
            _ => panic!(),
        }
        match parse_args(&["convert".into(), "--dir".into(), "d".into(), "--id".into(), "i".into()]).unwrap() {
            Parsed::Convert { dir, id, .. } => {
                assert_eq!(dir.as_deref(), Some("d"));
                assert_eq!(id.as_deref(), Some("i"));
            }
            _ => panic!(),
        }
        match parse_args(&[
            "inspect".into(),
            "6".into(),
            "7".into(),
            "12".into(),
            "--onlyid".into(),
            "1987718388".into(),
            "--index".into(),
            "2".into(),
            "--hex".into(),
            "--backup-dir".into(),
            "/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.lbas, vec![6, 7, 12]);
                assert_eq!(opts.onlyid.as_deref(), Some("1987718388"));
                assert_eq!(opts.index, Some(2));
                assert!(opts.hex);
                assert_eq!(opts.backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 inspect"),
        }
        match parse_args(&[
            "inspect".into(),
            "9".into(),
            "--backup".into(),
            "x.bin".into(),
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
            "--onlyid".into(),
            "1402259934".into(),
            "--keep".into(),
            "0".into(),
            "--yes".into(),
            "--backup-dir=/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Backup { action: BackupAction::Prune, keep, yes, backup_dir, onlyid } => {
                assert_eq!(keep, 0);
                assert!(yes);
                assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
                assert_eq!(onlyid.as_deref(), Some("1402259934"));
            }
            _ => panic!("应解析为 backup prune"),
        }
        match parse_args(&[
            "backup".into(),
            "verify".into(),
            "--onlyid=-1833210541".into(),
            "--index".into(),
            "2".into(),
        ])
        .unwrap()
        {
            Parsed::Backup { action: BackupAction::Verify { target, index }, keep, yes, onlyid, .. } => {
                assert_eq!(target, None);
                assert_eq!(index, Some(2));
                assert_eq!(keep, 2);
                assert!(!yes);
                assert_eq!(onlyid.as_deref(), Some("-1833210541"));
            }
            _ => panic!("应解析为 backup verify"),
        }
        match parse_args(&[
            "backup".into(),
            "rm".into(),
            "--onlyid".into(),
            "1402259934".into(),
            "2".into(),
            "3-4".into(),
            "--yes".into(),
            "--backup-dir".into(),
            "/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Backup { action: BackupAction::Rm { targets }, yes, onlyid, backup_dir, .. } => {
                assert_eq!(targets, vec!["2", "3-4"]);
                assert!(yes);
                assert_eq!(onlyid.as_deref(), Some("1402259934"));
                assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 backup rm"),
        }
        match parse_args(&[
            "backup".into(),
            "list".into(),
            "--onlyid".into(),
            "1987718388".into(),
        ])
        .unwrap()
        {
            Parsed::Backup { action: BackupAction::List, onlyid, .. } => {
                assert_eq!(onlyid.as_deref(), Some("1987718388"));
            }
            _ => panic!("应解析为 backup list"),
        }
    }

    #[test]
    fn parse_usage_errors() {
        assert!(parse_args(&["bogus".into()]).is_err());
        assert!(parse_args(&["run".into(), "--nope".into()]).is_err());
        assert!(parse_args(&["run".into(), "--disk".into()]).is_err()); // 缺值
        assert!(parse_args(&["run".into(), "--disk".into(), "x".into()]).is_err()); // 非数字
        assert!(parse_args(&["run".into(), "--force".into()]).is_err()); // force 仅 apply
        assert!(parse_args(&["apply".into(), "--size".into(), "-3".into()]).is_err()); // 负 size
        assert!(parse_args(&["restore".into(), "a.bin".into(), "b.bin".into()]).is_err()); // 两个位置参数
        assert!(matches!(
            parse_args(&["backup".into()]).unwrap(),
            Parsed::Backup { action: BackupAction::List, .. }
        )); // 裸 backup = list
        assert!(parse_args(&["backup".into(), "rm".into()]).is_err()); // 无 onlyid 时 rm 缺目标
        assert!(parse_args(&[
            "backup".into(),
            "rm".into(),
            "--onlyid".into(),
            "1402259934".into(),
            "--yes".into(),
        ])
        .is_err()); // --yes 不能在无编号时进入交互选择
        assert!(parse_args(&["backup".into(), "prune".into(), "--keep".into(), "-1".into()]).is_err());
        assert!(parse_args(&["backup".into(), "verify".into(), "a.bin".into(), "b.bin".into()]).is_err());
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
            "--backup".into(),
            "x.bin".into(),
        ])
        .is_err());
        assert!(matches!(parse_args(&[
            "inspect".into(),
            "--onlyid".into(),
            "1402259934".into(),
        ]).unwrap(), Parsed::Inspect(_))); // 缺 index 时进入备份选择视图
        assert!(parse_args(&[
            "inspect".into(),
            "--backup".into(),
            "x.bin".into(),
            "--index".into(),
            "1".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "inspect".into(),
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
            parse_args(&["apply".into(), "--disk".into(), "6".into(), "--_elevated".into()]).unwrap(),
            Parsed::Apply { .. }
        ));
    }

    #[test]
    fn backup_selection_accepts_numbers_commas_and_ranges() {
        assert_eq!(
            parse_backup_selection_tokens(&["1,3".into(), "2-4".into()], 5).unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            parse_backup_selection_tokens(&["2".into(), "2".into()], 3).unwrap(),
            vec![2]
        );
        for bad in ["0", "4", "3-2", "1-4", "x", "1--2", "1,"] {
            assert!(
                parse_backup_selection_tokens(&[bad.into()], 3).is_err(),
                "应拒绝 {bad}"
            );
        }
    }

    #[test]
    fn disk_table_rendering() {
        let parts = vec![
            EdpfPartition { ptype: 1, active: 1, enc: 0, start_lba: 32, size_bytes: 16_384 },
            EdpfPartition { ptype: 2, active: 1, enc: 1, start_lba: 63, size_bytes: 59_750_819_680 },
            EdpfPartition { ptype: 4, active: 0, enc: 1, start_lba: 116_707_328, size_bytes: 3_143_761_920 },
        ];
        let rows = vec![
            Row {
                disk: 4, size: 64_000_000_000, vid: "0951".into(), pid: "1666".into(),
                proto: "USB".into(), device_id: None, onlyid: None, n_baks: 0, denied: false,
                probe_error: None,
                is_nopwd: false, partitions: None,
            },
            Row {
                disk: 6, size: 62_914_560_000, vid: "0dd8".into(), pid: "2005".into(),
                proto: "USB".into(), device_id: Some("disk&ven_netac&prod_onlydisk".into()),
                onlyid: Some("1402259934".into()), n_baks: 3, denied: false,
                probe_error: None,
                is_nopwd: true, partitions: Some(parts),
            },
            Row {
                disk: 7, size: 500_107_862_016, vid: "xxxx".into(), pid: "xxxx".into(),
                proto: "Thunderbolt".into(), device_id: None, onlyid: None, n_baks: 0, denied: false,
                probe_error: None,
                is_nopwd: false, partitions: None,
            },
        ];
        let out = print_disk_table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "外接盘 3 个:");
        let disk4_line = lines.iter().find(|l| l.contains("disk4")).unwrap();
        assert!(disk4_line.contains("非cems盘"));
        let disk6_line = lines.iter().find(|l| l.contains("disk6")).unwrap();
        assert!(disk6_line.contains("cems盘") && disk6_line.contains("[免密]"));
        // EDPF 明细行: 类型 + 大小 + LBA 范围
        let edpf = lines.iter().find(|l| l.contains("EDPF")).unwrap();
        assert!(edpf.contains("Share 59.75GB (LBA 63~116,700,881)"), "{}", edpf);
        assert!(edpf.contains("Encrypt 3.14GB (LBA 116,707,328~122,847,487)"), "{}", edpf);
        assert!(edpf.contains("Boot 0.00GB (LBA 32~63)"), "{}", edpf);
        let meta = lines.iter().find(|l| l.contains("onlyid")).unwrap();
        assert!(meta.contains("onlyid=1402259934") && meta.contains("备份 3 份"));
        let disk7_line = lines.iter().find(|l| l.contains("disk7")).unwrap();
        assert!(disk7_line.contains("非USB"));
        assert_eq!(print_disk_table(&[]).trim(), "未检测到外接盘。");
    }

    #[test]
    fn menus_are_numbered() {
        use crate::sysinfo::ExtDisk;
        let disks = vec![
            ExtDisk { n: 4, size: 64_000_000_000, vid: "0951".into(), pid: "1666".into(), proto: "USB".into() },
            ExtDisk { n: 6, size: 62_914_560_000, vid: "0dd8".into(), pid: "2005".into(), proto: "USB".into() },
        ];
        let m = disk_menu_str(&disks);
        assert!(m.contains("1)") && m.contains("2)"), "{}", m);
        assert!(m.contains("disk4") && m.contains("64.00GB") && m.contains("0951:1666"));

        let b = backup_menu_str(&[("2026-09-16 23:36".into(), true), ("2026-08-27 22:25".into(), false)]);
        assert!(b.contains("1)  2026-09-16 23:36"), "{}", b);
        assert!(b.contains("[免密状态]"));
        assert!(b.contains("2)  2026-08-27 22:25"));
        assert!(b.contains("[加密原盘]"));
    }
}
