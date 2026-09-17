//! 命令行入口: 子命令解析、自动提权、交互提示、各处理器。
//!
//! 用法:
//!   nopwd list                                列出外接盘(不写入, 不提权)
//!   nopwd run    [--disk N] [--size GB]       预览 dry-run(裸 nopwd 同义; 自动提权)
//!   nopwd apply  [--disk N] [--size GB] [--force] [--yes]   实际写入
//!   nopwd restore [<备份.bin>] [--disk N] [--yes]           还原(缺省交互选择)
//!   nopwd convert --dir <快照目录> --id <device_id> [--size GB] [--out <目录>]
//!
//! 实测记录(2026-08-27, 均内网免密成功): aigo U335 128G / aigo U320 32G /
//! Kingston DT3.0 64G (每盘改前自动备份, 可随时 nopwd restore 还原)。

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::common::*;
use crate::diskio::{self, backup_disk, backup_is_nopwd, find_backups, raw_path, DiskFacts,
                    FileDev, SectorDev, Clock, SystemClock};
use crate::elevate::{self, ELEVATED_FLAG};
use crate::identify::identify;
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

pub enum Parsed {
    List { backup_dir: Option<String> },
    Run(DiskOpts),
    Apply { opts: DiskOpts, force: bool, yes: bool },
    Restore { bin: Option<String>, disk: Option<u32>, yes: bool, backup_dir: Option<String> },
    Convert { dir: Option<String>, id: Option<String>, size: Option<f64>, out: Option<String> },
    Version,
    Help,
}

pub fn print_usage() {
    print!(
        "cems 加密 U 盘 → 无密码盘(纯 Rust 标准库, 零依赖) v{}

用法: nopwd <子命令> [选项]

子命令:
  list                              列出外接盘: 编号/容量/接口/cems识别/备份(无需 sudo)
  run    [--disk N] [--size GB]     真盘预览 dry-run(缺省子命令; 需管理员, 自动 sudo)
  apply  [--disk N] [--size GB] [--force] [--yes]
                                   真盘实际写入(自动备份 LBA0-13 → 备份目录)
  restore [<备份.bin>] [--disk N] [--yes]
                                   从备份还原 LBA0-13(缺省交互选择本盘备份)
  convert --dir <快照目录> --id <device_id> [--size GB] [--out <目录>]
                                   离线转换(不碰真盘)
  version | help

选项:
  --disk <N|/dev/diskN|/dev/rdiskN>  真盘号(缺省自动检测外部 USB 盘; 须 disk2+)
  --size <GB>                        Share 大小(默认占满到 Encrypt 前)
  --force                            已改造(免密)盘仍强制重写(默认拒绝)
  --yes                              免交互(自动确认一切 YES 提示)
  --backup-dir <目录>                备份目录(默认 $NOPWD_BACKUP_DIR 或 ./backup)
",
        env!("CARGO_PKG_VERSION")
    );
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

fn flag_name(a: &str) -> &str {
    a.split('=').next().unwrap_or(a)
}

pub fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let args: Vec<&String> = argv.iter().filter(|a| a.as_str() != ELEVATED_FLAG).collect();
    let Some(first) = args.first() else {
        return Ok(Parsed::Run(DiskOpts::default())); // 裸 nopwd = run
    };
    let rest: Vec<String> = args[1..].iter().map(|s| (*s).clone()).collect();
    match first.as_str() {
        "-h" | "--help" | "help" => Ok(Parsed::Help),
        "-V" | "--version" | "version" => Ok(Parsed::Version),
        "list" => {
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
        "run" | "apply" => {
            let is_apply = first.as_str() == "apply";
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
pub struct Row {
    pub disk: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub n_baks: usize,
    pub denied: bool,
}

/// 外接盘一览数据: 编号/容量/接口; USB 盘再尽力识别 cems 身份与备份份数。
/// 未 sudo 时 raw 设备无读权限 → denied=true(基本列仍可显示)。
pub fn scan_disks(
    runner: &dyn CmdRunner,
    backup_dir: &Path,
    read_disk: &dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for d in sysinfo::list_external_disks(runner) {
        let mut row = Row {
            disk: d.n,
            size: d.size,
            vid: d.vid.clone(),
            pid: d.pid.clone(),
            proto: d.proto.clone(),
            device_id: None,
            onlyid: None,
            n_baks: 0,
            denied: false,
        };
        if d.proto == "USB" {
            let probe = (|| -> io::Result<()> {
                let lba7 = read_disk(d.n, 7)?;
                let id = identify(runner, d.n, &lba7);
                row.device_id = id.device_id.clone();
                let lba4 = read_disk(d.n, 4)?;
                row.onlyid = diskio::lba4_label_id_from(&lba4[..32]);
                if let Some(did) = &id.device_id {
                    let tag: [u8; 16] = lba4[..16].try_into().unwrap();
                    let facts = DiskFacts {
                        disk: d.n,
                        total_sectors: sysinfo::disk_total_sectors(runner, d.n),
                        vid: d.vid.clone(),
                        pid: d.pid.clone(),
                        label_id: row.onlyid.clone(),
                    };
                    row.n_baks = find_backups(backup_dir, &facts, Some(did), Some(tag)).len();
                }
                Ok(())
            })();
            if probe.is_err() {
                row.denied = true;
            }
        }
        rows.push(row);
    }
    rows
}

pub fn print_disk_table(rows: &[Row]) -> String {
    let mut out = String::new();
    if rows.is_empty() {
        out.push_str("未检测到外接盘。\n");
        return out;
    }
    out.push_str(&format!("外接盘 {} 个:\n", rows.len()));
    let w = rows.iter().map(|r| r.disk.to_string().len()).max().unwrap_or(1);
    for r in rows {
        let head = format!("  disk{:<w$}  {:>8}  {}", r.disk, fmt_gb(r.size), r.proto, w = w);
        if r.proto != "USB" {
            out.push_str(&format!("{}  (非USB, 本工具不支持)\n", head));
        } else if r.denied {
            out.push_str(&format!("{} {}:{}  (加 sudo 可识别 cems 盘/备份)\n", head, r.vid, r.pid));
        } else if r.device_id.is_none() {
            out.push_str(&format!("{} {}:{}  非cems盘\n", head, r.vid, r.pid));
        } else {
            let oid = r.onlyid.as_ref().map(|o| format!("  onlyid={}", o)).unwrap_or_default();
            let baks = if r.n_baks > 0 { format!("  备份{}份", r.n_baks) } else { "  无备份".to_string() };
            out.push_str(&format!("{} {}:{}  cems盘{}{}\n", head, r.vid, r.pid, oid, baks));
        }
    }
    out
}

// ══════════════════════════════════════════════════════════════════
// 4. 真盘流程
// ══════════════════════════════════════════════════════════════════
fn read_image(dev: &mut dyn SectorDev) -> NopwdResult<Vec<u8>> {
    let mut img = Vec::with_capacity(14 * SECTOR);
    for lba in 0..14u32 {
        img.extend_from_slice(&dev.read_sector(lba).map_err(|e| err(EXIT_IO, format!("错误: {}", e)))?);
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
    for (i, d) in disks.iter().enumerate() {
        println!("  {}) disk{}  {}  {}:{}", i + 1, d.n, fmt_gb(d.size), d.vid, d.pid);
    }
    loop {
        let c = prompt.prompt_line(&format!("选择 [1-{}] (回车取消): ", disks.len()));
        let c = c.trim();
        if c.is_empty() {
            return Err(err(EXIT_CANCELLED, "已取消"));
        }
        if let Ok(n) = c.parse::<usize>() {
            if (1..=disks.len()).contains(&n) {
                return Ok(disks[n - 1].n);
            }
        }
        println!("无效输入");
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
    println!("盘   : disk{}  {}  USB {}:{}", disk, sz, vid, pid);

    let img = read_image(dev)?;
    let id = identify(runner, disk, &img[7 * SECTOR..8 * SECTOR]);
    let did = match id.device_id {
        Some(d) => d,
        None => {
            return Err(err(EXIT_TARGET, "错误: 无法识别 device_id(LBA7 两候选均未解出 EDPF); 可插好盘重试"))
        }
    };
    let label_id = diskio::lba4_label_id_from(&img[4 * SECTOR..4 * SECTOR + 32]);
    let facts = DiskFacts { disk, total_sectors: secs, vid, pid, label_id };
    let tag16: [u8; 16] = img[4 * SECTOR..4 * SECTOR + 16].try_into().unwrap();

    let read = |lba: u32| -> NopwdResult<Vec<u8>> {
        Ok(img[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    let result = convert(&read, &did, size_gb, true)?;

    let baks = find_backups(&ctx.backup_dir, &facts, Some(&did), Some(tag16));
    if !baks.is_empty() {
        println!("\n备份 : 本盘已有 {} 份(写入时会自动再备份):", baks.len());
        for b in &baks {
            let t = ctx.clock.fmt_human(diskio::mtime_epoch(b));
            let base = b.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            println!("  {}  {}", t, base);
        }
    } else {
        println!("\n备份 : 尚无; 写入时自动创建首个备份");
    }

    let already = looks_nopwd(&read, &did)?;
    if already {
        println!("\n提示: 该盘已是改造后的免密盘 — 再次写入只会重写相同内容(实测幂等)。");
    }
    if !apply {
        let tail = if already { " (该盘已是免密盘, 须加 --force)" } else { "" };
        println!("操作 : 以上为预览(dry-run), 未写盘。执行写入: nopwd apply --disk {}{}", disk, tail);
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
        println!("--force: 继续重写。本次自动备份将标记为免密状态(文件名含 _nopwd); 加密原盘备份是更早时间戳那份。");
    }

    let (bpath, _nopwd) = backup_disk(&facts, &img, &did, &ctx.backup_dir, ctx.clock)?;
    println!("还原: nopwd restore \"{}\" --disk {} --yes", bpath.display(), disk);

    if !ctx.prompt.confirm_yes(&format!("将改写 disk{} LBA0/6/7/12/9。输入 YES: ", disk)) {
        return Err(err(EXIT_CANCELLED, "已取消(未写盘)"));
    }
    sysinfo::unmount_disk(ctx.runner, disk);
    // 写序由 atomic_write_sectors 保证: LBA0(唯一改 MBR 的扇区)最后写 —
    // 写它才触发 macOS 重扫/挂载; 且单 fd 全程持有, 不再存在中途重开窗口
    let mut writes: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    writes.insert(6, result.lba6);
    writes.insert(7, result.lba7);
    writes.insert(12, result.lba12);
    if let Some(l9) = result.lba9 {
        writes.insert(9, l9);
    }
    writes.insert(0, result.lba0);
    diskio::atomic_write_sectors(dev, &writes)?;
    println!("已写入, 读回校验通过。请拔出 U 盘重新插入, 数据区格式化 exFAT/NTFS 即得免密可写区。");
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
    let label_id = diskio::lba4_label_id_from(&img[4 * SECTOR..4 * SECTOR + 32]);
    let tag16: [u8; 16] = img[4 * SECTOR..4 * SECTOR + 16].try_into().unwrap();

    let path: PathBuf = match bin {
        Some(p) => PathBuf::from(p),
        None => {
            let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
            let facts = DiskFacts {
                disk,
                total_sectors: sysinfo::disk_total_sectors(runner, disk),
                vid,
                pid,
                label_id,
            };
            let baks = find_backups(&ctx.backup_dir, &facts, did.as_deref(), Some(tag16));
            if baks.is_empty() {
                return Err(err(
                    EXIT_BACKUP,
                    "错误: 备份目录未找到本盘备份; 可 nopwd restore <备份.bin> 显式指定",
                ));
            }
            println!("disk{} 匹配备份 {} 个(新→旧):", disk, baks.len());
            for b in &baks {
                let mt = ctx.clock.fmt_human(diskio::mtime_epoch(b));
                let tag = match &did {
                    Some(d) if backup_is_nopwd(b, d) => "  [免密状态]",
                    _ => "",
                };
                println!("  {}{}  {}", mt, tag, b.display());
            }
            let sel = loop {
                let c = ctx.prompt.prompt_line(&format!("选择 [1-{}] (回车取消): ", baks.len()));
                let c = c.trim();
                if c.is_empty() {
                    return Err(err(EXIT_CANCELLED, "已取消"));
                }
                if let Ok(n) = c.parse::<usize>() {
                    if (1..=baks.len()).contains(&n) {
                        break baks[n - 1].clone();
                    }
                }
                println!("无效输入");
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
    let md5_path = PathBuf::from(format!("{}.md5", path.display()));
    if md5_path.exists() {
        let want = std::fs::read_to_string(&md5_path)
            .map_err(|e| err(EXIT_BACKUP, format!("错误: 无法读取 {}: {}", md5_path.display(), e)))?
            .trim()
            .to_string();
        let got = crate::md5::md5_hex(&data);
        if want != got {
            return Err(err(EXIT_BACKUP, format!("错误: 备份 MD5 不符(期望 {}, 实际 {}) — 文件损坏?", want, got)));
        }
        println!("MD5 校验通过: {}", got);
    }
    let nopwd_snap = did.as_ref().map(|d| backup_is_nopwd(&path, d)).unwrap_or(false);
    if nopwd_snap {
        println!("注意: 该备份为【免密状态】快照 — 还原后仍是免密盘, 不会回到加密原盘。");
        println!(
            "[dry-run] 将还原 {} → disk{} LBA0-13 ({}B) — 未写入(免密快照不作还原)。",
            path.display(),
            disk,
            data.len()
        );
        return Ok(EXIT_OK);
    }
    if !ctx.prompt.confirm_yes(&format!("还原 {} → disk{} LBA0-13? 输入 YES: ", path.display(), disk)) {
        return Err(err(EXIT_CANCELLED, "已取消"));
    }
    sysinfo::unmount_disk(ctx.runner, disk);
    let writes: BTreeMap<u32, Vec<u8>> = (0..14u32)
        .map(|lba| (lba, data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec()))
        .collect();
    diskio::atomic_write_sectors(dev, &writes)?;
    println!("已还原, 读回校验通过。请拔出重插。");
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
            eprintln!("{}", e.msg);
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
// 5. 入口
// ══════════════════════════════════════════════════════════════════
pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&argv) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{}", msg);
            eprintln!();
            print_usage();
            return EXIT_USAGE;
        }
    };
    let runner = SysRunner;
    match parsed {
        Parsed::Help => {
            print_usage();
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
            eprintln!("{}", e.msg);
            return e.code;
        }
    }
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        if disk_opt.is_none() {
            let mut sp = StdPrompter;
            match auto_pick_disk(runner, &mut sp) {
                Ok(n) => {
                    argv.push("--disk".into());
                    argv.push(n.to_string());
                }
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return e.code;
                }
            }
        }
        elevate::ensure_elevated(&argv); // 内部以子进程退出码结束, 不返回
        unreachable!();
    }
    let bak = diskio::resolve_backup_dir(backup_dir_flag.as_deref());
    let mut std_prompter = StdPrompter;
    let mut always = AlwaysYes(StdPrompter); // 无状态, 独立实例
    let prompter: &mut dyn Prompter = if yes { &mut always } else { &mut std_prompter };
    let mut ctx = Ctx { runner, clock: &SystemClock, prompt: prompter, backup_dir: bak };
    let n = match disk_opt {
        Some(n) => n,
        None => match auto_pick_disk(runner, &mut *ctx.prompt) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("{}", e.msg);
                return e.code;
            }
        },
    };
    if let Err(e) = guard_system_disk(n) {
        eprintln!("{}", e.msg);
        return e.code;
    }
    let mut dev = match FileDev::open_rdwr(&raw_path(n), OPEN_WAIT) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("错误: 无法以读写打开 {}: {} (加 sudo?)", raw_path(n), e);
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
            eprintln!("{}", e.msg);
            e.code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_and_subcommands() {
        assert!(matches!(parse_args(&[]).unwrap(), Parsed::Run(_)));
        assert!(matches!(parse_args(&["help".into()]).unwrap(), Parsed::Help));
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
        // 哨兵旗标被剥离
        assert!(matches!(
            parse_args(&["apply".into(), "--disk".into(), "6".into(), "--_elevated".into()]).unwrap(),
            Parsed::Apply { .. }
        ));
    }

    #[test]
    fn disk_table_rendering() {
        let rows = vec![
            Row {
                disk: 4, size: 64_000_000_000, vid: "0951".into(), pid: "1666".into(),
                proto: "USB".into(), device_id: None, onlyid: None, n_baks: 0, denied: false,
            },
            Row {
                disk: 6, size: 62_914_560_000, vid: "0dd8".into(), pid: "2005".into(),
                proto: "USB".into(), device_id: Some("disk&ven_netac&prod_onlydisk".into()),
                onlyid: Some("1402259934".into()), n_baks: 3, denied: false,
            },
            Row {
                disk: 7, size: 500_107_862_016, vid: "xxxx".into(), pid: "xxxx".into(),
                proto: "Thunderbolt".into(), device_id: None, onlyid: None, n_baks: 0, denied: false,
            },
        ];
        let out = print_disk_table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "外接盘 3 个:");
        assert!(lines[1].contains("disk4") && lines[1].contains("非cems盘"));
        assert!(lines[2].contains("disk6") && lines[2].contains("cems盘"));
        assert!(lines[2].contains("onlyid=1402259934") && lines[2].contains("备份3份"));
        assert!(lines[3].contains("disk7") && lines[3].contains("非USB"));
        assert_eq!(print_disk_table(&[]).trim(), "未检测到外接盘。");
    }
}
