//! 命令行参数模型与零依赖解析器。
//!
//! 只负责 argv → 结构化命令，不做 I/O、不提权、不访问磁盘；所有歧义参数在这里
//! 统一拒绝，避免执行层出现“后一个覆盖前一个”或布尔 flag 带值的危险语义。

use std::collections::HashSet;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectMode {
    Raw,
    Decode,
    Meta,
}

impl InspectMode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "raw" => Some(Self::Raw),
            "decode" => Some(Self::Decode),
            "meta" => Some(Self::Meta),
            _ => None,
        }
    }
}

pub struct InspectOpts {
    pub mode: InspectMode,
    pub disk: Option<u32>,
    pub backup: Option<String>,
    pub lbas: Vec<u64>,
    pub count: Option<u64>,
    pub export: Option<String>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

impl Default for InspectOpts {
    fn default() -> Self {
        Self {
            mode: InspectMode::Meta,
            disk: None,
            backup: None,
            lbas: Vec::new(),
            count: None,
            export: None,
            device_id: None,
            backup_dir: None,
        }
    }
}

#[derive(Default)]
pub struct InfoOpts {
    pub disk: Option<u32>,
    pub backup: Option<String>,
    pub device_id: Option<String>,
    pub backup_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionNewOpts {
    pub disk: Option<u32>,
    pub mode: u8,
    pub boot_mib: Option<u64>,
    pub share_mib: Option<u64>,
    pub encrypt_mib: Option<u64>,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub password: String,
    pub volume_label: String,
    pub force_change_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionAction {
    Plan(ProvisionNewOpts),
    Image {
        opts: ProvisionNewOpts,
        out: String,
    },
    Write {
        opts: ProvisionNewOpts,
        yes: bool,
    },
    Convert {
        disk: Option<u32>,
        write: bool,
        yes: bool,
        backup_dir: Option<String>,
    },
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
    Provision(ProvisionAction),
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
  provision 制盘：官方四模式新盘与现有盘免密改造\n\
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
            println!("{}", bold("用法: edpcli inspect <raw|decode|meta> [备份.edpb] [--disk N] [--lba 列表或范围] [--count N] [--export DIR]"));
            println!("raw=物理原始字节；decode=按已验证区域算法解码；meta=结构化区域与字段语义。");
            println!("--lba 支持 7,12,240250283 或 240250283-240250288；--count 仅能与单个起始 LBA 同用。");
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
        "provision" => {
            println!(
                "{}",
                bold("用法: edpcli provision <plan|image|write|convert> [选项]")
            );
            println!("  provision plan  --disk N --mode 0|1|2|3 <身份/分区参数>");
            println!("  provision image --disk N --mode 0|1|2|3 <身份/分区参数> --out FILE");
            println!("  provision write --disk N --mode 0|1|2|3 <身份/分区参数> [--yes]");
            println!("  provision convert [--disk N] [--write] [--yes] [--backup-dir D]");
            println!("新盘身份参数: --label-id ID --user USER --dept DEPT [--label LABEL] [--password PASSWORD]");
            println!(
                "标签默认值: {}；可通过 --label 自定义。",
                crate::provision::DEFAULT_SAFE6_LABEL
            );
            println!("密码默认值: 0000aaaa；卷标默认值: 启动区。");
            println!("密码策略: --force-change-password 表示首次插入时强制修改密码；默认关闭。");
            println!("分区参数: --boot-mib N --share-mib N --encrypt-mib N；仅当前模式实际使用的项必填。");
            println!("当前产品写入固定使用已验证的 exFAT + SM4(mode2) 路线。");
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

fn parse_positive_u64(s: &str, flag: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: {flag} 须为正整数，得到 {s}"));
    }
    match s.parse::<u64>() {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(format!("错误: {flag} 须为正整数，得到 {s}")),
    }
}

fn parse_provision_mode(s: &str) -> Result<u8, String> {
    match s {
        "0" => Ok(0),
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        _ => Err(format!("错误: --mode 只接受 0/1/2/3，得到 {s}")),
    }
}

fn parse_new_provision_opts(
    rest: &[String],
) -> Result<(ProvisionNewOpts, Option<String>, bool), String> {
    let mut disk = None;
    let mut mode = None;
    let mut boot_mib = None;
    let mut share_mib = None;
    let mut encrypt_mib = None;
    let mut label_id = None;
    let mut user = None;
    let mut dept = None;
    let mut label = None;
    let mut password = None;
    let mut volume_label = None;
    let mut force_change_password = false;
    let mut out = None;
    let mut yes = false;
    let mut i = 0usize;
    while i < rest.len() {
        match flag_name(&rest[i]) {
            "--disk" => {
                let value = take_value(rest, &mut i, "--disk")?;
                set_once(&mut disk, parse_disk_spec(&value)?, "--disk")?;
            }
            "--mode" => {
                let value = take_value(rest, &mut i, "--mode")?;
                set_once(&mut mode, parse_provision_mode(&value)?, "--mode")?;
            }
            "--boot-mib" => {
                let value = take_value(rest, &mut i, "--boot-mib")?;
                set_once(
                    &mut boot_mib,
                    parse_positive_u64(&value, "--boot-mib")?,
                    "--boot-mib",
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
            "--encrypt-mib" => {
                let value = take_value(rest, &mut i, "--encrypt-mib")?;
                set_once(
                    &mut encrypt_mib,
                    parse_positive_u64(&value, "--encrypt-mib")?,
                    "--encrypt-mib",
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
            "--password" => {
                let value = take_value(rest, &mut i, "--password")?;
                set_once(&mut password, value, "--password")?;
            }
            "--volume-label" => {
                let value = take_value(rest, &mut i, "--volume-label")?;
                set_once(&mut volume_label, value, "--volume-label")?;
            }
            "--force-change-password" => {
                set_switch(
                    &mut force_change_password,
                    &rest[i],
                    "--force-change-password",
                )?;
            }
            "--out" => {
                let value = take_value(rest, &mut i, "--out")?;
                set_once(&mut out, value, "--out")?;
            }
            "--yes" => set_switch(&mut yes, &rest[i], "--yes")?,
            other => return Err(format!("错误: provision 不认识选项 {other}")),
        }
        i += 1;
    }

    let mode = mode.ok_or("错误: provision 新盘操作必须指定 --mode 0|1|2|3")?;
    let require = |value: Option<u64>, flag: &str| {
        value.ok_or_else(|| format!("错误: mode{mode} 必须指定 {flag}"))
    };
    match mode {
        0 => {
            require(boot_mib, "--boot-mib")?;
            require(share_mib, "--share-mib")?;
            require(encrypt_mib, "--encrypt-mib")?;
        }
        1 => {
            require(share_mib, "--share-mib")?;
            require(encrypt_mib, "--encrypt-mib")?;
        }
        2 => {
            require(encrypt_mib, "--encrypt-mib")?;
        }
        3 => {
            require(boot_mib, "--boot-mib")?;
            require(share_mib, "--share-mib")?;
        }
        _ => unreachable!(),
    }

    Ok((
        ProvisionNewOpts {
            disk,
            mode,
            boot_mib,
            share_mib,
            encrypt_mib,
            label_id: label_id.ok_or("错误: provision 新盘操作必须指定 --label-id")?,
            user: user.ok_or("错误: provision 新盘操作必须指定 --user")?,
            dept: dept.ok_or("错误: provision 新盘操作必须指定 --dept")?,
            label: label.unwrap_or_else(|| crate::provision::DEFAULT_SAFE6_LABEL.into()),
            password: password.unwrap_or_else(|| "0000aaaa".into()),
            volume_label: volume_label.unwrap_or_else(|| "启动区".into()),
            force_change_password,
        },
        out,
        yes,
    ))
}

fn parse_keep(s: &str) -> Result<usize, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: --keep 须为大于等于 0 的整数, 得到 {}", s));
    }
    s.parse::<usize>()
        .map_err(|_| format!("错误: --keep 超出范围: {}", s))
}

const MAX_INSPECT_SECTORS: usize = 65_536;

fn parse_lba_value(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("错误: LBA 须为非负十进制整数，得到 {value}"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("错误: LBA 超出 u64 范围: {value}"))
}

fn push_unique_lba(out: &mut Vec<u64>, seen: &mut HashSet<u64>, lba: u64) -> Result<(), String> {
    if seen.insert(lba) {
        if out.len() >= MAX_INSPECT_SECTORS {
            return Err(format!(
                "错误: 单次 inspect 最多读取 {MAX_INSPECT_SECTORS} 个扇区"
            ));
        }
        out.push(lba);
    }
    Ok(())
}

fn parse_lbas(s: &str) -> Result<Vec<u64>, String> {
    if s.is_empty() {
        return Err("错误: --lba 缺少 LBA 值".into());
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for token in s.split(',') {
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_lba_value(start)?;
            let end = parse_lba_value(end)?;
            if start > end {
                return Err(format!("错误: LBA 范围起点大于终点: {token}"));
            }
            let span = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| format!("错误: LBA 范围溢出: {token}"))?;
            if span > MAX_INSPECT_SECTORS as u64 {
                return Err(format!(
                    "错误: 单个 LBA 范围最多包含 {MAX_INSPECT_SECTORS} 个扇区"
                ));
            }
            for lba in start..=end {
                push_unique_lba(&mut out, &mut seen, lba)?;
            }
        } else {
            push_unique_lba(&mut out, &mut seen, parse_lba_value(token)?)?;
        }
    }
    Ok(out)
}

fn parse_inspect_count(s: &str) -> Result<u64, String> {
    let count = parse_lba_value(s)?;
    if count == 0 || count > MAX_INSPECT_SECTORS as u64 {
        return Err(format!(
            "错误: --count 须为 1..={MAX_INSPECT_SECTORS}，得到 {s}"
        ));
    }
    Ok(count)
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
            let Some(mode_text) = rest.first() else {
                return Err("错误: inspect 需要模式 raw / decode / meta".into());
            };
            let mode = InspectMode::parse(mode_text).ok_or_else(|| {
                format!(
                    "错误: inspect 模式必须是 raw / decode / meta，得到 {}",
                    mode_text
                )
            })?;
            let rest = rest[1..].to_vec();
            let mut opts = InspectOpts {
                mode,
                ..InspectOpts::default()
            };
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
                        "--count" => {
                            let v = take_value(&rest, &mut i, "--count")?;
                            set_once(&mut opts.count, parse_inspect_count(&v)?, "--count")?;
                        }
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
                        "错误: inspect 不接受裸 LBA {}。请使用 --lba {}",
                        a, a
                    ));
                } else if opts.backup.is_none() {
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
            if let Some(count) = opts.count {
                if opts.lbas.len() != 1 {
                    return Err("错误: --count 只能与单个 --lba 起点同时使用".into());
                }
                let start = opts.lbas[0];
                opts.lbas.clear();
                for offset in 0..count {
                    let lba = start
                        .checked_add(offset)
                        .ok_or_else(|| "错误: --count 产生的 LBA 范围溢出".to_string())?;
                    opts.lbas.push(lba);
                }
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
                            "--deep" => set_switch(&mut deep, &tail[i], "--deep")?,
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
        "provision" => {
            if rest.iter().any(|a| a == "-h" || a == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("provision".into()),
                });
            }
            let Some(action) = rest.first().map(String::as_str) else {
                return Err("错误: provision 需要动作 plan / image / write / convert".into());
            };
            let tail = &rest[1..];
            match action {
                "plan" | "image" | "write" => {
                    let (opts, out, yes) = parse_new_provision_opts(tail)?;
                    match action {
                        "plan" => {
                            if out.is_some() || yes {
                                return Err("错误: provision plan 不接受 --out 或 --yes".into());
                            }
                            Ok(Parsed::Provision(ProvisionAction::Plan(opts)))
                        }
                        "image" => {
                            if yes {
                                return Err("错误: provision image 不接受 --yes".into());
                            }
                            let out = out.ok_or("错误: provision image 必须指定 --out FILE")?;
                            Ok(Parsed::Provision(ProvisionAction::Image { opts, out }))
                        }
                        "write" => {
                            if out.is_some() {
                                return Err("错误: provision write 不接受 --out".into());
                            }
                            Ok(Parsed::Provision(ProvisionAction::Write { opts, yes }))
                        }
                        _ => unreachable!(),
                    }
                }
                "convert" => {
                    let mut disk = None;
                    let mut write = false;
                    let mut yes = false;
                    let mut backup_dir = None;
                    let mut i = 0usize;
                    while i < tail.len() {
                        match flag_name(&tail[i]) {
                            "--disk" => {
                                let value = take_value(tail, &mut i, "--disk")?;
                                set_once(&mut disk, parse_disk_spec(&value)?, "--disk")?;
                            }
                            "--backup-dir" => {
                                let value = take_value(tail, &mut i, "--backup-dir")?;
                                set_once(&mut backup_dir, value, "--backup-dir")?;
                            }
                            "--write" => set_switch(&mut write, &tail[i], "--write")?,
                            "--yes" => set_switch(&mut yes, &tail[i], "--yes")?,
                            other => {
                                return Err(format!("错误: provision convert 不认识选项 {other}"))
                            }
                        }
                        i += 1;
                    }
                    if yes && !write {
                        return Err("错误: provision convert --yes 只能与 --write 同用".into());
                    }
                    Ok(Parsed::Provision(ProvisionAction::Convert {
                        disk,
                        write,
                        yes,
                        backup_dir,
                    }))
                }
                other => Err(format!(
                    "错误: 未知 provision 动作: {other} (可用 plan / image / write / convert)"
                )),
            }
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
