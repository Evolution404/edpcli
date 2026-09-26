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
            println!("create 可加 --deep 执行只读文件系统分析；backup 无动作时等价于 list。");
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
            println!("密码域: [--share-source-password PASSWORD] [--share-target-password PASSWORD]");
            println!("        [--encrypt-source-password PASSWORD] [--encrypt-target-password PASSWORD]");
            println!(
                "标签默认值: {}；可通过 --label 自定义。",
                crate::provision::DEFAULT_SAFE6_LABEL
            );
            println!("来源密码未指定表示 Unknown；存在的目标密码域默认 0000aaaa；卷标默认值: 启动区。");
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

fn parse_provision_mode(s: &str) -> Result<crate::provision::ProvisionTarget, String> {
    let mode = match s {
        "0" => 0,
        "1" => 1,
        "2" => 2,
        "3" => 3,
        _ => return Err(format!("错误: --mode 只接受 0/1/2/3，得到 {s}")),
    };
    crate::provision::ProvisionTarget::from_mode_number(mode)
        .ok_or_else(|| format!("错误: 无效官方模式 {mode}"))
}

fn parse_provision_target(s: &str) -> Result<crate::provision::ProvisionTarget, String> {
    match s.to_ascii_lowercase().as_str() {
        "plain" => Ok(crate::provision::ProvisionTarget::Plain),
        "mode0" => parse_provision_mode("0"),
        "mode1" => parse_provision_mode("1"),
        "mode2" => parse_provision_mode("2"),
        "mode3" => parse_provision_mode("3"),
        _ => Err(format!(
            "错误: --target 只接受 mode0/mode1/mode2/mode3/plain，得到 {s}"
        )),
    }
}

fn parse_provision_filesystem(
    value: &str,
) -> Result<crate::provision::OfficialFilesystemFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "fat16" => Ok(crate::provision::OfficialFilesystemFormat::Fat16),
        "exfat" => Ok(crate::provision::OfficialFilesystemFormat::ExFat),
        _ => Err(format!(
            "错误: 当前仅支持 fat16/exfat 文件系统，得到 {value}"
        )),
    }
}

fn parse_plain_partition(
    value: &str,
) -> Result<crate::application::provision::PlainPartitionRequest, String> {
    let mut fields = value.splitn(4, ':');
    let start = fields
        .next()
        .ok_or_else(|| "错误: --partition 缺少 start LBA".to_string())?;
    let size = fields
        .next()
        .ok_or_else(|| "错误: --partition 格式应为 START:SIZE:FS[:LABEL]".to_string())?;
    let filesystem = fields
        .next()
        .ok_or_else(|| "错误: --partition 格式应为 START:SIZE:FS[:LABEL]".to_string())?;
    let volume_label = fields.next().unwrap_or("普通卷").to_string();

    let start_lba = parse_positive_u64(start, "--partition START")?;
    let lower_size = size.to_ascii_lowercase();
    let size = if lower_size == "fill" {
        crate::application::provision::PlainPartitionSize::Fill
    } else if let Some(value) = lower_size.strip_suffix("mib") {
        crate::application::provision::PlainPartitionSize::MiB(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix("gib") {
        crate::application::provision::PlainPartitionSize::GiB(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix("sectors") {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else if let Some(value) = lower_size.strip_suffix('s') {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            value,
            "--partition SIZE",
        )?)
    } else {
        crate::application::provision::PlainPartitionSize::Sectors(parse_positive_u64(
            size,
            "--partition SIZE",
        )?)
    };

    Ok(crate::application::provision::PlainPartitionRequest {
        start_lba,
        size,
        filesystem: parse_provision_filesystem(filesystem)?,
        volume_label,
    })
}

fn parse_new_provision_opts(
    rest: &[String],
    allow_prefill: bool,
) -> Result<(ProvisionNewOpts, Option<String>, bool), String> {
    let mut disk = None;
    let mut target = None;
    let mut plain_partitions = Vec::new();
    let mut boot_mib = None;
    let mut boot_start_lba = None;
    let mut share_start_lba = None;
    let mut encrypt_start_lba = None;
    let mut boot_sectors = None;
    let mut share_mib = None;
    let mut share_sectors = None;
    let mut encrypt_mib = None;
    let mut encrypt_sectors = None;
    let mut label_id = None;
    let mut user = None;
    let mut dept = None;
    let mut label = None;
    let mut share_source_password = None;
    let mut share_target_password = None;
    let mut encrypt_source_password = None;
    let mut encrypt_target_password = None;
    let mut volume_label = None;
    let mut format_boot = false;
    let mut format_share = false;
    let mut format_encrypt = false;
    let mut boot_label = None;
    let mut share_label = None;
    let mut encrypt_label = None;
    let mut boot_fs = None;
    let mut share_fs = None;
    let mut encrypt_fs = None;
    let mut force_change_password = None;
    let mut cancel_password_complexity_check = None;
    let mut max_share_password_errors = None;
    let mut max_encrypt_password_errors = None;
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
                set_once(
                    &mut target,
                    parse_provision_mode(&value)?,
                    "--target/--mode",
                )?;
            }
            "--target" => {
                let value = take_value(rest, &mut i, "--target")?;
                set_once(
                    &mut target,
                    parse_provision_target(&value)?,
                    "--target/--mode",
                )?;
            }
            "--partition" => {
                let value = take_value(rest, &mut i, "--partition")?;
                if plain_partitions.len() >= crate::provision::MAX_PLAIN_PARTITIONS {
                    return Err(format!(
                        "错误: 普通盘最多支持 {} 个 MBR 主分区",
                        crate::provision::MAX_PLAIN_PARTITIONS
                    ));
                }
                plain_partitions.push(parse_plain_partition(&value)?);
            }
            "--boot-mib" => {
                let value = take_value(rest, &mut i, "--boot-mib")?;
                set_once(
                    &mut boot_mib,
                    parse_positive_u64(&value, "--boot-mib")?,
                    "--boot-mib",
                )?;
            }
            "--boot-start-sector" => {
                let value = take_value(rest, &mut i, "--boot-start-sector")?;
                set_once(
                    &mut boot_start_lba,
                    parse_positive_u64(&value, "--boot-start-sector")?,
                    "--boot-start-sector",
                )?;
            }
            "--share-start-sector" => {
                let value = take_value(rest, &mut i, "--share-start-sector")?;
                set_once(
                    &mut share_start_lba,
                    parse_positive_u64(&value, "--share-start-sector")?,
                    "--share-start-sector",
                )?;
            }
            "--encrypt-start-sector" => {
                let value = take_value(rest, &mut i, "--encrypt-start-sector")?;
                set_once(
                    &mut encrypt_start_lba,
                    parse_positive_u64(&value, "--encrypt-start-sector")?,
                    "--encrypt-start-sector",
                )?;
            }
            "--boot-sectors" => {
                let value = take_value(rest, &mut i, "--boot-sectors")?;
                set_once(
                    &mut boot_sectors,
                    parse_positive_u64(&value, "--boot-sectors")?,
                    "--boot-sectors",
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
            "--share-sectors" => {
                let value = take_value(rest, &mut i, "--share-sectors")?;
                set_once(
                    &mut share_sectors,
                    parse_positive_u64(&value, "--share-sectors")?,
                    "--share-sectors",
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
            "--encrypt-sectors" => {
                let value = take_value(rest, &mut i, "--encrypt-sectors")?;
                set_once(
                    &mut encrypt_sectors,
                    parse_positive_u64(&value, "--encrypt-sectors")?,
                    "--encrypt-sectors",
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
            "--share-source-password" => {
                let value = take_value(rest, &mut i, "--share-source-password")?;
                set_once(
                    &mut share_source_password,
                    value,
                    "--share-source-password",
                )?;
            }
            "--share-target-password" => {
                let value = take_value(rest, &mut i, "--share-target-password")?;
                set_once(
                    &mut share_target_password,
                    value,
                    "--share-target-password",
                )?;
            }
            "--encrypt-source-password" => {
                let value = take_value(rest, &mut i, "--encrypt-source-password")?;
                set_once(
                    &mut encrypt_source_password,
                    value,
                    "--encrypt-source-password",
                )?;
            }
            "--encrypt-target-password" => {
                let value = take_value(rest, &mut i, "--encrypt-target-password")?;
                set_once(
                    &mut encrypt_target_password,
                    value,
                    "--encrypt-target-password",
                )?;
            }
            "--volume-label" => {
                let value = take_value(rest, &mut i, "--volume-label")?;
                set_once(&mut volume_label, value, "--volume-label")?;
            }
            "--format-boot" => set_switch(&mut format_boot, &rest[i], "--format-boot")?,
            "--format-share" => set_switch(&mut format_share, &rest[i], "--format-share")?,
            "--format-encrypt" => set_switch(&mut format_encrypt, &rest[i], "--format-encrypt")?,
            "--boot-label" => {
                let value = take_value(rest, &mut i, "--boot-label")?;
                set_once(&mut boot_label, value, "--boot-label")?;
            }
            "--share-label" => {
                let value = take_value(rest, &mut i, "--share-label")?;
                set_once(&mut share_label, value, "--share-label")?;
            }
            "--encrypt-label" => {
                let value = take_value(rest, &mut i, "--encrypt-label")?;
                set_once(&mut encrypt_label, value, "--encrypt-label")?;
            }
            "--boot-fs" => {
                let value = take_value(rest, &mut i, "--boot-fs")?;
                set_once(
                    &mut boot_fs,
                    parse_provision_filesystem(&value)?,
                    "--boot-fs",
                )?;
            }
            "--share-fs" => {
                let value = take_value(rest, &mut i, "--share-fs")?;
                set_once(
                    &mut share_fs,
                    parse_provision_filesystem(&value)?,
                    "--share-fs",
                )?;
            }
            "--encrypt-fs" => {
                let value = take_value(rest, &mut i, "--encrypt-fs")?;
                set_once(
                    &mut encrypt_fs,
                    parse_provision_filesystem(&value)?,
                    "--encrypt-fs",
                )?;
            }
            "--force-change-password" => {
                if rest[i] != "--force-change-password" {
                    return Err(format!(
                        "错误: --force-change-password 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if force_change_password.replace(true).is_some() {
                    return Err("错误: 强制改密策略重复或冲突指定".into());
                }
            }
            "--no-force-change-password" => {
                if rest[i] != "--no-force-change-password" {
                    return Err(format!(
                        "错误: --no-force-change-password 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if force_change_password.replace(false).is_some() {
                    return Err("错误: 强制改密策略重复或冲突指定".into());
                }
            }
            "--cancel-password-complexity-check" => {
                if rest[i] != "--cancel-password-complexity-check" {
                    return Err(format!(
                        "错误: --cancel-password-complexity-check 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if cancel_password_complexity_check.replace(true).is_some() {
                    return Err("错误: 密码复杂性验证策略重复或冲突指定".into());
                }
            }
            "--enforce-password-complexity-check" => {
                if rest[i] != "--enforce-password-complexity-check" {
                    return Err(format!(
                        "错误: --enforce-password-complexity-check 是布尔旗标，不接受参数值: {}",
                        rest[i]
                    ));
                }
                if cancel_password_complexity_check.replace(false).is_some() {
                    return Err("错误: 密码复杂性验证策略重复或冲突指定".into());
                }
            }
            "--share-max-password-errors" => {
                let value = take_value(rest, &mut i, "--share-max-password-errors")?;
                set_once(
                    &mut max_share_password_errors,
                    value.parse::<u8>().map_err(|_| {
                        format!("错误: --share-max-password-errors 须为 0..255，得到 {value}")
                    })?,
                    "--share-max-password-errors",
                )?;
            }
            "--encrypt-max-password-errors" => {
                let value = take_value(rest, &mut i, "--encrypt-max-password-errors")?;
                set_once(
                    &mut max_encrypt_password_errors,
                    value.parse::<u8>().map_err(|_| {
                        format!("错误: --encrypt-max-password-errors 须为 0..255，得到 {value}")
                    })?,
                    "--encrypt-max-password-errors",
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

    let target = target.ok_or(
        "错误: provision 必须指定 --target mode0|mode1|mode2|mode3|plain（兼容 --mode 0|1|2|3）",
    )?;
    if target == crate::provision::ProvisionTarget::Plain {
        let has_official_only = boot_mib.is_some()
            || boot_start_lba.is_some()
            || share_start_lba.is_some()
            || encrypt_start_lba.is_some()
            || boot_sectors.is_some()
            || share_mib.is_some()
            || share_sectors.is_some()
            || encrypt_mib.is_some()
            || encrypt_sectors.is_some()
            || label_id.is_some()
            || user.is_some()
            || dept.is_some()
            || label.is_some()
            || share_source_password.is_some()
            || share_target_password.is_some()
            || encrypt_source_password.is_some()
            || encrypt_target_password.is_some()
            || volume_label.is_some()
            || format_boot
            || format_share
            || format_encrypt
            || boot_label.is_some()
            || share_label.is_some()
            || encrypt_label.is_some()
            || boot_fs.is_some()
            || share_fs.is_some()
            || encrypt_fs.is_some()
            || force_change_password.is_some()
            || cancel_password_complexity_check.is_some()
            || max_share_password_errors.is_some()
            || max_encrypt_password_errors.is_some();
        if has_official_only {
            return Err(
                "错误: --target plain 只接受 --disk/--partition/--out/--yes；官方模式参数不能混用"
                    .into(),
            );
        }
        return Ok((
            ProvisionNewOpts {
                disk,
                target,
                plain_partitions,
                boot_start_lba: None,
                share_start_lba: None,
                encrypt_start_lba: None,
                boot_mib: None,
                boot_sectors: None,
                share_mib: None,
                share_sectors: None,
                encrypt_mib: None,
                encrypt_sectors: None,
                label_id: String::new(),
                user: String::new(),
                dept: String::new(),
                label: String::new(),
                share_source_password: String::new(),
                share_target_password: String::new(),
                encrypt_source_password: String::new(),
                encrypt_target_password: String::new(),
                volume_label: String::new(),
                format_boot: false,
                format_share: false,
                format_encrypt: false,
                boot_label: String::new(),
                share_label: String::new(),
                encrypt_label: String::new(),
                boot_fs: crate::provision::OfficialFilesystemFormat::Fat16,
                share_fs: crate::provision::OfficialFilesystemFormat::ExFat,
                encrypt_fs: crate::provision::OfficialFilesystemFormat::ExFat,
                force_change_password: None,
                cancel_password_complexity_check: None,
                max_share_password_errors: None,
                max_encrypt_password_errors: None,
            },
            out,
            yes,
        ));
    }
    if !plain_partitions.is_empty() {
        return Err("错误: --partition 仅用于 --target plain".into());
    }
    let mode = target
        .mode_number()
        .expect("non-Plain target always has an official mode number");
    if boot_mib.is_some() && boot_sectors.is_some() {
        return Err("错误: --boot-mib 与 --boot-sectors 不能同时指定".into());
    }
    if share_mib.is_some() && share_sectors.is_some() {
        return Err("错误: --share-mib 与 --share-sectors 不能同时指定".into());
    }
    if encrypt_mib.is_some() && encrypt_sectors.is_some() {
        return Err("错误: --encrypt-mib 与 --encrypt-sectors 不能同时指定".into());
    }
    match mode {
        0 => {
            if !allow_prefill && share_mib.is_none() && share_sectors.is_none() {
                return Err("错误: mode0 必须指定 --share-mib 或 --share-sectors".into());
            }
            if !allow_prefill && encrypt_mib.is_none() && encrypt_sectors.is_none() {
                return Err("错误: mode0 必须指定 --encrypt-mib 或 --encrypt-sectors".into());
            }
            if !allow_prefill && boot_mib.is_none() && boot_sectors.is_none() {
                boot_sectors = Some(crate::provision::DEFAULT_MODE0_BOOT_SECTORS);
            }
        }
        1 => {
            if boot_sectors.is_some() {
                return Err("错误: --boot-sectors 仅用于 mode0".into());
            }
            if !allow_prefill && share_mib.is_none() && share_sectors.is_none() {
                return Err("错误: mode1 必须指定 --share-mib 或 --share-sectors".into());
            }
            if !allow_prefill && encrypt_mib.is_none() && encrypt_sectors.is_none() {
                return Err("错误: mode1 必须指定 --encrypt-mib 或 --encrypt-sectors".into());
            }
        }
        2 => {
            if boot_sectors.is_some() {
                return Err("错误: --boot-sectors 仅用于 mode0".into());
            }
            if !allow_prefill && encrypt_mib.is_none() && encrypt_sectors.is_none() {
                return Err("错误: mode2 必须指定 --encrypt-mib 或 --encrypt-sectors".into());
            }
        }
        3 => {
            if !allow_prefill && boot_mib.is_none() && boot_sectors.is_none() {
                return Err("错误: mode3 必须指定 --boot-mib 或 --boot-sectors".into());
            }
            if !allow_prefill && share_mib.is_none() && share_sectors.is_none() {
                return Err("错误: mode3 必须指定 --share-mib 或 --share-sectors".into());
            }
        }
        _ => unreachable!(),
    }

    let shared_label = volume_label.clone();
    let volume_label = volume_label.unwrap_or_else(|| "启动区".into());
    Ok((
        ProvisionNewOpts {
            disk,
            target,
            plain_partitions,
            boot_start_lba,
            share_start_lba,
            encrypt_start_lba,
            boot_mib,
            boot_sectors,
            share_mib,
            share_sectors,
            encrypt_mib,
            encrypt_sectors,
            label_id: match label_id {
                Some(value) => value,
                None if allow_prefill => String::new(),
                None => crate::provision::OnlyId::random_candidate()?
                    .text()
                    .to_string(),
            },
            user: if allow_prefill {
                user.unwrap_or_default()
            } else {
                user.ok_or("错误: provision 新盘操作必须指定 --user")?
            },
            dept: if allow_prefill {
                dept.unwrap_or_default()
            } else {
                dept.ok_or("错误: provision 新盘操作必须指定 --dept")?
            },
            label: label.unwrap_or_else(|| {
                if allow_prefill {
                    String::new()
                } else {
                    crate::provision::DEFAULT_SAFE6_LABEL.into()
                }
            }),
            share_source_password: share_source_password.unwrap_or_default(),
            share_target_password: share_target_password.unwrap_or_else(|| "0000aaaa".into()),
            encrypt_source_password: encrypt_source_password.unwrap_or_default(),
            encrypt_target_password: encrypt_target_password
                .unwrap_or_else(|| "0000aaaa".into()),
            volume_label: volume_label.clone(),
            format_boot,
            format_share,
            format_encrypt,
            boot_label: boot_label.unwrap_or(volume_label),
            share_label: share_label
                .unwrap_or_else(|| shared_label.clone().unwrap_or_else(|| "交换区".into())),
            encrypt_label: encrypt_label
                .unwrap_or_else(|| shared_label.unwrap_or_else(|| "保密区".into())),
            boot_fs: boot_fs.unwrap_or(crate::provision::OfficialFilesystemFormat::Fat16),
            share_fs: share_fs.unwrap_or(crate::provision::OfficialFilesystemFormat::ExFat),
            encrypt_fs: encrypt_fs.unwrap_or(crate::provision::OfficialFilesystemFormat::ExFat),
            force_change_password,
            cancel_password_complexity_check,
            max_share_password_errors,
            max_encrypt_password_errors,
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
                return Err("错误: provision 需要动作 plan / image / write".into());
            };
            let tail = &rest[1..];
            match action {
                "plan" | "image" | "write" => {
                    let (opts, out, yes) = parse_new_provision_opts(tail, true)?;
                    match action {
                        "plan" => {
                            if out.is_some() || yes {
                                return Err("错误: provision plan 不接受 --out 或 --yes".into());
                            }
                            Ok(Parsed::Provision(ProvisionAction::Plan(Box::new(opts))))
                        }
                        "image" => {
                            if yes {
                                return Err("错误: provision image 不接受 --yes".into());
                            }
                            let out = out.ok_or("错误: provision image 必须指定 --out FILE")?;
                            Ok(Parsed::Provision(ProvisionAction::Image {
                                opts: Box::new(opts),
                                out,
                            }))
                        }
                        "write" => {
                            if out.is_some() {
                                return Err("错误: provision write 不接受 --out".into());
                            }
                            Ok(Parsed::Provision(ProvisionAction::Write {
                                opts: Box::new(opts),
                                yes,
                            }))
                        }
                        _ => unreachable!(),
                    }
                }
                other => Err(format!(
                    "错误: 未知 provision 动作: {other} (可用 plan / image / write)"
                )),
            }
        }
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
