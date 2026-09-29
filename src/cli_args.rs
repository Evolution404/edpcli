//! 命令行参数模型与零依赖解析器。
//!
//! 只负责 argv → 结构化命令，不做 I/O、不提权、不访问磁盘；所有歧义参数在这里
//! 统一拒绝，避免执行层出现“后一个覆盖前一个”或布尔 flag 带值的危险语义。

mod backup;
mod info;
mod inspect;
mod provision;

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

#[derive(Clone, PartialEq, Eq)]
pub struct ProvisionNewOpts {
    pub disk: Option<u32>,
    pub target: crate::provision::ProvisionTarget,
    pub plain_partitions: Vec<crate::application::provision::PlainPartitionRequest>,
    pub boot_start_lba: Option<u64>,
    pub share_start_lba: Option<u64>,
    pub encrypt_start_lba: Option<u64>,
    pub boot_mib: Option<u64>,
    pub boot_sectors: Option<u64>,
    pub share_mib: Option<u64>,
    pub share_sectors: Option<u64>,
    pub encrypt_mib: Option<u64>,
    pub encrypt_sectors: Option<u64>,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub share_source_password: String,
    pub share_target_password: String,
    pub encrypt_source_password: String,
    pub encrypt_target_password: String,
    pub volume_label: String,
    pub format_boot: bool,
    pub format_share: bool,
    pub format_encrypt: bool,
    pub boot_label: String,
    pub share_label: String,
    pub encrypt_label: String,
    pub boot_fs: crate::provision::OfficialFilesystemFormat,
    pub share_fs: crate::provision::OfficialFilesystemFormat,
    pub encrypt_fs: crate::provision::OfficialFilesystemFormat,
    pub force_change_password: Option<bool>,
    pub cancel_password_complexity_check: Option<bool>,
    pub max_share_password_errors: Option<u8>,
    pub max_encrypt_password_errors: Option<u8>,
}

impl std::fmt::Debug for ProvisionNewOpts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProvisionNewOpts")
            .field("disk", &self.disk)
            .field("target", &self.target)
            .field("plain_partitions", &self.plain_partitions)
            .field("label_id", &self.label_id)
            .field("user", &self.user)
            .field("dept", &self.dept)
            .field("label", &self.label)
            .field("share_source_password", &"[REDACTED]")
            .field("share_target_password", &"[REDACTED]")
            .field("encrypt_source_password", &"[REDACTED]")
            .field("encrypt_target_password", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionAction {
    Plan(Box<ProvisionNewOpts>),
    Image {
        opts: Box<ProvisionNewOpts>,
        out: String,
    },
    Write {
        opts: Box<ProvisionNewOpts>,
        yes: bool,
        backup_dir: Option<String>,
    },
}

pub enum Parsed {
    List {
        backup_dir: Option<String>,
    },
    Tui,
    Demo {
        scene: Option<String>,
        list_scenes: bool,
    },
    Backup {
        action: BackupAction,
        keep: usize,
        yes: bool,
        backup_dir: Option<String>,
    },
    Inspect(InspectOpts),
    Info(InfoOpts),
    Provision(ProvisionAction),
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
    use std::fmt::Write as _;

    let mut out = format!(
        "edpcli — EDP/cems U 盘管理 CLI v{}\n\n用法: edpcli [命令] [选项]\n\n",
        env!("CARGO_PKG_VERSION")
    );
    for spec in crate::command_spec::top_level_specs() {
        let _ = writeln!(out, "  {:<10} {}", spec.name, spec.summary);
    }
    out.push_str(
        "\n交互式终端中无参数 edpcli 默认进入 TUI；管道/重定向等非 TTY 环境仍等价于 edpcli list。\n",
    );
    out
}

pub fn print_usage() {
    print!("{}", usage_text());
}

fn print_topic_help(topic: &str) {
    use crate::ui::bold;

    let Some(spec) = crate::command_spec::command(topic) else {
        print_usage();
        return;
    };
    println!("{}", bold(&format!("用法: {}", spec.usage)));
    if !spec.actions.is_empty() {
        for action in spec.actions {
            println!("  {:<10} {}", action.name, action.summary);
        }
    }

    match topic {
        "info" => {
            println!("未指定来源且只有一个可用目标盘时自动选择；多盘时交互选择。");
        }
        "inspect" => {
            println!("raw=物理原始字节；decode=按已验证区域算法解码；meta=结构化区域与字段语义。");
            println!("--lba 支持 7,12,240250283 或 240250283-240250288；--count 仅能与单个起始 LBA 同用。");
        }
        "backup" => {
            println!("create 创建元数据备份。backup 无动作时等价于 list。");
        }
        "provision" => {
            println!("目标: --target mode0|mode1|mode2|mode3|plain");
            println!("    兼容输入: --mode 0|1|2|3；Plain 不是 mode4，--mode 4 永远非法。");
            println!("    Plain 分区: 可重复 --partition START:SIZE:fat16|exfat[:LABEL]；SIZE 支持 sectors/MiB/GiB/fill。");
            println!("    Plain 未指定 --partition 时默认 P1 从 LBA2048 占满至盘尾。");
            println!("    mode1 若识别到现有 mode0，将保留原 type4 位置/密钥并让 type2 扩满前部。");
            println!("    可选格式化: --format-boot --format-share --format-encrypt");
            println!("    文件系统: --boot-fs fat16|exfat --share-fs fat16|exfat --encrypt-fs fat16|exfat");
            println!("    各区卷标: --boot-label LABEL --share-label LABEL --encrypt-label LABEL");
            println!("新盘身份参数: [--label-id ID] --user USER --dept DEPT [--label LABEL]");
            println!(
                "密码域: [--share-source-password PASSWORD] [--share-target-password PASSWORD]"
            );
            println!(
                "        [--encrypt-source-password PASSWORD] [--encrypt-target-password PASSWORD]"
            );
            println!(
                "标签默认值: {}；可通过 --label 自定义。",
                crate::provision::DEFAULT_SAFE6_LABEL
            );
            println!(
                "来源密码未指定表示 Unknown；存在的目标密码域默认 0000aaaa；卷标默认值: 启动区。"
            );
            println!("标签标识未指定时自动生成一个合法 onlyid 候选；可通过 --label-id 手动覆盖。");
            println!("密码策略: 未指定时继承注册盘可靠 PassInfo；普通盘默认 强制改密=否、取消复杂性验证=否、两区最大错误次数=255。");
            println!("    --force-change-password / --no-force-change-password");
            println!(
                "    --cancel-password-complexity-check / --enforce-password-complexity-check"
            );
            println!(
                "    --share-max-password-errors N --encrypt-max-password-errors N   (0..255)"
            );
            println!("分区参数: --boot-mib N / --boot-sectors N、--share-mib N、--encrypt-mib N；mode0 未指定启动区时默认 20417 扇区。");
            println!("当前可写文件系统为 FAT16/exFAT；加密分区使用已验证的 SM4(mode2) 扇区变换。");
        }
        "completion" => {
            println!("zsh : eval \"$(edpcli completion zsh)\"");
            println!("bash: eval \"$(edpcli completion bash)\"");
            println!("fish: edpcli completion fish | source");
        }
        _ => {}
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

fn parse_positive_u64(s: &str, flag: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("错误: {flag} 须为正整数，得到 {s}"));
    }
    match s.parse::<u64>() {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(format!("错误: {flag} 须为正整数，得到 {s}")),
    }
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
        "demo" => {
            if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
                return Ok(Parsed::Help {
                    topic: Some("demo".into()),
                });
            }
            let mut scene = None;
            let mut list_scenes = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--scene" => {
                        let value = take_value(&rest, &mut i, "--scene")?;
                        set_once(&mut scene, value, "--scene")?;
                    }
                    "--list-scenes" if !list_scenes => list_scenes = true,
                    "--list-scenes" => return Err("错误: --list-scenes 重复指定".into()),
                    other => return Err(format!("错误: demo 不认识参数 {other}")),
                }
                i += 1;
            }
            if scene.is_some() && list_scenes {
                return Err("错误: --scene 与 --list-scenes 不能同时使用".into());
            }
            Ok(Parsed::Demo { scene, list_scenes })
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
        "inspect" => inspect::parse_inspect(&rest),
        "info" => info::parse_info(&rest),
        "backup" => backup::parse_backup(&rest),
        "provision" => provision::parse_provision(&rest),
        "run" => Err("错误: v2 已取消 run。请使用: edpcli provision plan".into()),
        "restore" => Err("错误: v2 已取消顶层 restore。请使用: edpcli backup restore".into()),
        "meta" | "metainfo" => Err("错误: v2 已取消 meta/metainfo。请使用: edpcli info".into()),
        other if other.starts_with('-') => {
            Err(format!("错误: 未知选项 {} (首个参数应为子命令)", other))
        }
        other => Err(format!(
            "错误: 未知子命令: {} (edpcli help 查看用法)",
            other
        )),
    }
}
