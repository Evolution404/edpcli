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
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub use crate::backup_cli::{backup_list, backup_prune, backup_rm, backup_verify};
use crate::backup_cli::backup_verify_select;
use crate::common::*;
use crate::completion::{self, Shell};
use crate::diskio::{self, backup_disk, backup_is_nopwd, find_backups, raw_path, DiskFacts,
                    FileDev, SectorDev, Clock, SystemClock};
use crate::elevate::{self, ELEVATED_FLAG};
use crate::identify::identify;
use crate::inspect_cli::inspect_flow;
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
                            let n = v.parse::<usize>().map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
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
                                    let n = v
                                        .parse::<usize>()
                                        .map_err(|_| format!("错误: --index 须为正整数, 得到 {}", v))?;
                                    if n == 0 {
                                        return Err("错误: --index 从 1 开始".into());
                                    }
                                    set_once(&mut index, n, "--index")?;
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
                                "--yes" => set_switch(&mut yes, a, "--yes")?,
                                "--backup-dir" => {
                                    let v = take_value(tail, &mut i, "--backup-dir")?;
                                    set_once(&mut backup_dir, v, "--backup-dir")?;
                                }
                                "--onlyid" => {
                                    let v = take_value(tail, &mut i, "--onlyid")?;
                                    set_once(&mut onlyid, parse_onlyid(&v)?, "--onlyid")?;
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
    // 镜像布局必须严格保持 LBA0→13 的连续顺序；后续所有固定偏移都依赖此契约。
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

/// `reopen_rdwr` 会重新打开 `/dev/rdiskN`。确认期间既可能换盘，也可能有别的程序
/// 改动同一块盘的元数据。自动备份保存的是确认前 LBA0-13，因此第一笔写入前必须
/// 再读一次并逐扇区比对，保证“当前状态 == 刚刚备份的状态”。
fn verify_reopened_snapshot(dev: &mut dyn SectorDev, expected: &[u8]) -> NopwdResult<()> {
    if expected.len() != 14 * SECTOR {
        return Err(err(EXIT_IO, "错误: 内部预写快照长度异常"));
    }
    // 先核身份，再核其余元数据；换盘时不要被 LBA0 的差异抢先掩盖诊断。
    for lba in std::iter::once(4u32).chain((0..14u32).filter(|&lba| lba != 4)) {
        let actual = dev
            .read_sector(lba)
            .map_err(|e| err(EXIT_IO, format!("错误: 重开后读取 LBA{} 失败: {}", lba, e)))?;
        if actual.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: 重开后 LBA{} 读取 {}B，预期完整扇区 {}B",
                    lba,
                    actual.len(),
                    SECTOR
                ),
            ));
        }
        let start = lba as usize * SECTOR;
        let before = &expected[start..start + SECTOR];
        if actual != before {
            if lba == 4 {
                let expected_id =
                    diskio::lba4_label_id_from(before).unwrap_or_else(|| "未知".into());
                let actual_id =
                    diskio::lba4_label_id_from(&actual).unwrap_or_else(|| "未知".into());
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备身份在确认/卸载期间发生变化(expected onlyid={}, actual onlyid={})，疑似换盘或重枚举，拒绝写入",
                        expected_id, actual_id
                    ),
                ));
            }
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备元数据在备份/确认期间发生变化(LBA{})，拒绝基于过期快照写入",
                    lba
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn guard_system_disk(disk: u32) -> NopwdResult<()> {
    if disk < 2 {
        return Err(err(EXIT_TARGET, format!("错误: 拒绝系统盘 disk{}(须 disk2+)", disk)));
    }
    Ok(())
}

pub(crate) fn auto_pick_disk(
    runner: &dyn CmdRunner,
    prompt: &mut dyn Prompter,
) -> NopwdResult<u32> {
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
    sysinfo::unmount_disk(ctx.runner, disk)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法卸载 disk{}: {}", disk, e)))?;
    // 卸载后才切 O_RDWR(挂载态打开读写会撞 EBUSY); 写序由 atomic_write_sectors
    // 保证: LBA0(唯一改 MBR 的扇区)最后写, 单 fd 全程持有到写完校验完
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e)))?;
    verify_reopened_snapshot(dev, &img)?;
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
    sysinfo::unmount_disk(ctx.runner, disk)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法卸载 disk{}: {}", disk, e)))?;
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|e| err(EXIT_IO, format!("错误: 无法以读写打开 {}: {}", raw_path(disk), e)))?;
    verify_reopened_snapshot(dev, &img)?;
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
    fn read_image_preserves_lba_zero_to_thirteen_order() {
        let image = read_image(&mut PatternSectorDev).unwrap();
        assert_eq!(image.len(), 14 * SECTOR);
        for lba in 0..14usize {
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
    fn boolean_flags_reject_inline_values() {
        for argv in [
            vec!["apply", "--yes=no"],
            vec!["apply", "--force=false"],
            vec!["backup", "prune", "--yes=0"],
            vec!["backup", "rm", "--onlyid", "1402259934", "1", "--yes=no"],
            vec!["restore", "backup.bin", "--yes=false"],
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
            vec!["backup", "prune", "--yes", "--yes"],
            vec!["restore", "backup.bin", "--yes", "--yes"],
            vec!["inspect", "--raw", "--raw"],
            vec!["inspect", "--hex", "--hex"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("布尔旗标重复必须报错");
            assert!(err.contains("重复"), "{err}");
        }
    }

    #[test]
    fn inspect_lba_is_limited_to_zero_through_thirteen() {
        for bad in ["14", "99", "4294967295"] {
            let args = vec!["inspect".to_string(), bad.to_string()];
            let err = parse_args(&args).err().expect("inspect 不应接受 LBA0-13 之外的扇区");
            assert!(err.contains("LBA") && err.contains("0-13"), "{err}");
        }
        for good in ["0", "4", "13"] {
            let args = vec!["inspect".to_string(), good.to_string()];
            assert!(parse_args(&args).is_ok(), "LBA{good} 应被接受");
        }
    }

    #[test]
    fn single_value_flags_reject_duplicates() {
        for argv in [
            vec!["apply", "--disk", "4", "--disk", "6"],
            vec!["apply", "--size", "10", "--size", "20"],
            vec!["restore", "--disk=4", "--disk=6"],
            vec!["backup", "--onlyid", "1", "--onlyid", "2"],
            vec!["backup", "prune", "--keep", "1", "--keep", "2"],
            vec!["inspect", "--onlyid", "1", "--onlyid", "2"],
            vec!["inspect", "--index", "1", "--index", "2"],
            vec!["convert", "--dir", "a", "--dir", "b", "--id", "x"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("单值旗标重复必须报错");
            assert!(err.contains("重复"), "{err}");
        }
    }

    #[test]
    fn backup_selection_accepts_numbers_commas_and_ranges() {
        assert_eq!(
            crate::backup_cli::parse_backup_selection_tokens(&["1,3".into(), "2-4".into()], 5)
                .unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            crate::backup_cli::parse_backup_selection_tokens(&["2".into(), "2".into()], 3)
                .unwrap(),
            vec![2]
        );
        for bad in ["0", "4", "3-2", "1-4", "x", "1--2", "1,"] {
            assert!(
                crate::backup_cli::parse_backup_selection_tokens(&[bad.into()], 3).is_err(),
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
