//! 命令行参数模型与零依赖解析器。
//!
//! 只负责 argv → 结构化命令，不做 I/O、不提权、不访问磁盘；所有歧义参数在这里
//! 统一拒绝，避免执行层出现“后一个覆盖前一个”或布尔 flag 带值的危险语义。

mod backup;
mod help;
mod info;
mod inspect;
mod parse_support;
mod provision;

use crate::completion::Shell;
use crate::elevate::ELEVATED_FLAG;
pub(crate) use help::print_help;
pub use help::{print_usage, usage_text};
use parse_support::{
    flag_name, parse_disk_spec, parse_positive_u64, set_once, set_switch, take_value,
};

// ══════════════════════════════════════════════════════════════════
// 2. 参数解析(手写, 零依赖)
// ══════════════════════════════════════════════════════════════════
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
    pub share_source_password: crate::domain::secret::SecretText,
    pub share_target_password: crate::domain::secret::SecretText,
    pub encrypt_source_password: crate::domain::secret::SecretText,
    pub encrypt_target_password: crate::domain::secret::SecretText,
    pub prompt_passwords: bool,
    pub format_boot: bool,
    pub format_share: bool,
    pub format_encrypt: bool,
    pub boot_label: String,
    pub share_label: String,
    pub encrypt_label: String,
    pub boot_fs: crate::filesystem::FilesystemKind,
    pub share_fs: crate::filesystem::FilesystemKind,
    pub encrypt_fs: crate::filesystem::FilesystemKind,
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
            .field("prompt_passwords", &self.prompt_passwords)
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

pub fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let args: Vec<&String> = argv
        .iter()
        .filter(|a| a.as_str() != ELEVATED_FLAG)
        .collect();
    let Some(first) = args.first() else {
        return Ok(Parsed::List { backup_dir: None });
    };
    let rest =
        crate::domain::secret::SecretArguments(args[1..].iter().map(|s| (*s).clone()).collect());
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
