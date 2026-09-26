//! 命令行入口: 子命令解析、自动提权、交互提示、各处理器。
//!
//! 用法:
//!   edpcli                                     交互式 TTY 默认进入 TUI；非 TTY 等价于 list
//!   edpcli list                                列出外接盘(只读)
//!   edpcli info [备份.edpb] [--disk N]          查看设备/备份详情
//!   edpcli backup restore [备份] [--disk N]    还原
//!
//! 历史备份可通过 edpcli backup restore 还原。

use std::io::{self, IsTerminal, Write};
use std::path::Path;

pub use crate::backup_cli::{backup_delete, backup_list, backup_prune, backup_verify};
use crate::cli_args::print_help;
pub use crate::cli_args::{
    parse_args, print_usage, BackupAction, DiskOpts, InfoOpts, InspectOpts, Parsed,
    ProvisionAction, ProvisionNewOpts,
};
use crate::common::*;
use crate::completion;
use crate::elevate::{self, ELEVATED_FLAG};
use crate::inspect_cli::inspect_flow;
use crate::metainfo_cli::info_flow;
use crate::selectors::DeviceSelector;
use crate::sysinfo::{ReadProbeCache, SysRunner};

// ══════════════════════════════════════════════════════════════════
// 1. 交互提示抽象(测试注入)
// ══════════════════════════════════════════════════════════════════
pub use crate::application::write::Prompter;
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
    fn confirm_write_yes(&mut self, msg: &str) -> bool {
        self.prompt_line(&crate::ui::bold(msg)).trim() == "YES"
    }
    fn write_event(&mut self, event: crate::application::WriteEvent) {
        print!("{}", crate::ui::render_write_event(&event));
        let _ = io::stdout().flush();
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
    fn confirm_write_yes(&mut self, _msg: &str) -> bool {
        true
    }
    fn write_event(&mut self, event: crate::application::WriteEvent) {
        self.0.write_event(event);
    }
}

// ══════════════════════════════════════════════════════════════════
// 3. 外接盘一览
// ══════════════════════════════════════════════════════════════════
pub use crate::disk_scan::{print_disk_table, scan_disks, Row};

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
                        "list" | "tui" | "info" | "backup" | "inspect" | "provision" | "completion"
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
            let bak = crate::application::resolve_backup_dir(backup_dir.as_deref());
            match action {
                BackupAction::Create { disk, deep } => {
                    backup_create_real_flow(&runner, disk, backup_dir, deep)
                }
                BackupAction::List => backup_list(&bak),
                BackupAction::Verify { target } => backup_verify(&bak, target.as_deref()),
                BackupAction::Restore { target, disk } => {
                    real_flow(&runner, disk, backup_dir, target, yes)
                }
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
        Parsed::Provision(action) => provision_flow(&runner, action),
    }
}

fn provision_request(opts: &ProvisionNewOpts) -> crate::application::provision::ProvisionRequest {
    match opts.target {
        crate::provision::ProvisionTarget::Plain => {
            crate::application::provision::ProvisionRequest::Plain(
                crate::application::provision::PlainProvisionRequest {
                    partitions: opts.plain_partitions.clone(),
                },
            )
        }
        crate::provision::ProvisionTarget::Official(_) => {
            crate::application::provision::ProvisionRequest::Official(Box::new(
                crate::application::provision::OfficialProvisionRequest {
                    target: opts.target,
                    boot_start_lba: opts.boot_start_lba,
                    share_start_lba: opts.share_start_lba,
                    encrypt_start_lba: opts.encrypt_start_lba,
                    boot_mib: opts.boot_mib,
                    boot_sectors: opts.boot_sectors,
                    share_mib: opts.share_mib,
                    share_sectors: opts.share_sectors,
                    encrypt_mib: opts.encrypt_mib,
                    encrypt_sectors: opts.encrypt_sectors,
                    label_id: opts.label_id.clone(),
                    user: opts.user.clone(),
                    dept: opts.dept.clone(),
                    label: opts.label.clone(),
                    key_domains: crate::provision::KeyDomainSecrets::new(
                        crate::provision::KeyDomainSecretPair::new(
                            (!opts.share_source_password.is_empty())
                                .then_some(opts.share_source_password.as_bytes()),
                            Some(opts.share_target_password.as_bytes()),
                        ),
                        crate::provision::KeyDomainSecretPair::new(
                            (!opts.encrypt_source_password.is_empty())
                                .then_some(opts.encrypt_source_password.as_bytes()),
                            Some(opts.encrypt_target_password.as_bytes()),
                        ),
                    ),
                    volume_label: opts.volume_label.clone(),
                    format: crate::application::provision::FormatOptions {
                        boot: opts.format_boot,
                        share: opts.format_share,
                        encrypt: opts.format_encrypt,
                        boot_label: opts.boot_label.clone(),
                        share_label: opts.share_label.clone(),
                        encrypt_label: opts.encrypt_label.clone(),
                        boot_fs: opts.boot_fs,
                        share_fs: opts.share_fs,
                        encrypt_fs: opts.encrypt_fs,
                    },
                    force_change_password: opts.force_change_password,
                    cancel_password_complexity_check: opts.cancel_password_complexity_check,
                    max_share_password_errors: opts.max_share_password_errors,
                    max_encrypt_password_errors: opts.max_encrypt_password_errors,
                },
            ))
        }
    }
}

fn provision_resolve_disk(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    prompt: &mut dyn Prompter,
) -> EdpCliResult<u32> {
    DeviceSelector::new(disk_opt).resolve(runner, prompt)
}

fn target_plan_summary_lines(plan: &crate::provision::TargetProvisionPlan) -> Vec<String> {
    use crate::provision::{RegionDisposition, SourcePasswordKnowledge, TargetPasswordPolicy};

    let mut lines = Vec::with_capacity(plan.partitions.len() * 2 + 1);
    for partition in &plan.partitions {
        let geometry = &partition.geometry;
        let end_lba = geometry
            .start_lba
            .checked_add(geometry.sector_count)
            .and_then(|end| end.checked_sub(1))
            .unwrap_or(u64::MAX);
        let fate = match partition.disposition {
            RegionDisposition::PreserveOpaque => {
                "PreserveOpaque · 原 key material 逐字段透传 · data extent 0 写入"
            }
            RegionDisposition::PreserveVerified => {
                "PreserveVerified · K_old/wrapper 保持 · data extent 0 写入"
            }
            RegionDisposition::RewrapVerified => {
                "RewrapVerified · K_old 保持 · 仅重包 wrapper · data extent 0 写入"
            }
            RegionDisposition::Migrate => "Migrate · 当前版本 unsupported",
            RegionDisposition::Rebuild => {
                "Rebuild · K_new + 完整 filesystem initialization · 原数据不可原样保留"
            }
            RegionDisposition::Drop => "Drop · 来源区域不进入目标",
        };
        let password = match partition.source_password_knowledge {
            Some(SourcePasswordKnowledge::DefaultVerified) => "source=DefaultVerified",
            Some(SourcePasswordKnowledge::UserVerified) => "source=UserVerified",
            Some(SourcePasswordKnowledge::Unknown) => "source=Unknown",
            None => "source=no-key-domain",
        };
        let target_policy = match partition.target_password_policy {
            Some(TargetPasswordPolicy::PreserveOpaque) => "target=disabled(opaque)",
            Some(TargetPasswordPolicy::ReuseVerified) => "target=reuse-verified",
            Some(TargetPasswordPolicy::ReplaceVerified) => "target=replace/rewrap",
            Some(TargetPasswordPolicy::InitializeNew) => "target=initialize-new",
            None => "target=no-key-domain",
        };
        lines.push(format!(
            "  {} type{} start={} end={} sectors={} · {}",
            geometry.role.label(),
            geometry.partition_type.raw(),
            geometry.start_lba,
            end_lba,
            geometry.sector_count,
            fate
        ));
        lines.push(format!(
            "    {} · {} · {}",
            password, target_policy, partition.reason
        ));
    }
    lines.push(format!(
        "  unallocated={} sectors",
        plan.unallocated_sectors
    ));
    lines
}

fn print_new_provision_summary(prepared: &crate::application::provision::PreparedNewProvision) {
    let mode = crate::provision::ProvisionTarget::Official(prepared.mode)
        .mode_number()
        .expect("official mode always has a mode number");
    println!(
        "制盘计划: disk{} mode{}  device_id={}",
        prepared.disk, mode, prepared.device_id
    );
    println!(
        "目标扇区={}  LCE=LBA{}  计划写入={}扇区",
        prepared.write_image.total_sectors,
        prepared.lce_start_lba,
        prepared.write_image.touched_sector_count()
    );
    if let Some(target_plan) = &prepared.target_plan {
        println!("最终精确分区（TargetProvisionPlan）:");
        for line in target_plan_summary_lines(target_plan) {
            println!("{line}");
        }
    } else {
        println!("最终精确分区:");
        for choice in &prepared.format_targets {
            let geometry = &choice.target.geometry;
            let end_lba = geometry
                .start_sector
                .saturating_add(geometry.sector_count())
                .saturating_sub(1);
            println!(
                "  {} type{} start={} end={} sectors={}",
                choice.target.role.label(),
                geometry.partition_type.raw(),
                geometry.start_sector,
                end_lba,
                geometry.sector_count()
            );
        }
    }
    println!("制盘后格式化:");
    for choice in &prepared.format_targets {
        let target = &choice.target;
        println!(
            "  {}  {} type{} {} {}{} 卷标={}",
            if !target.format_capable {
                "—"
            } else if choice.selected {
                "☑"
            } else {
                "☐"
            },
            target.role.label(),
            target.geometry.partition_type.raw(),
            if !target.format_capable {
                "不可格式化"
            } else if target.physically_encrypted {
                "加密"
            } else {
                "明文"
            },
            choice
                .filesystem
                .map(|format| format.windows_format_name())
                .unwrap_or("—"),
            target
                .visible_mbr_type
                .map(|mbr| format!(" / MBR 0x{mbr:02X}"))
                .unwrap_or_default(),
            if target.format_capable {
                choice.volume_label.as_str()
            } else {
                "—"
            }
        );
    }
}

fn print_plain_provision_summary(prepared: &crate::application::provision::PreparedPlainProvision) {
    println!(
        "制盘计划: disk{} Plain  device_id={}",
        prepared.disk, prepared.device_id
    );
    println!(
        "目标扇区={}  分区={}  计划写入={}扇区",
        prepared.plan.total_sectors,
        prepared.plan.partitions.len(),
        prepared.write_plan.writes.len()
    );
    for (index, partition) in prepared.plan.partitions.iter().enumerate() {
        let end = partition
            .start_lba
            .saturating_add(partition.sector_count)
            .saturating_sub(1);
        println!(
            "  P{} start={} end={} sectors={} fs={} label={}",
            index + 1,
            partition.start_lba,
            end,
            partition.sector_count,
            partition.filesystem.windows_format_name(),
            partition.volume_label
        );
    }
    for gap in &prepared.plan.gaps {
        println!("  gap start={} sectors={}", gap.start_lba, gap.sector_count);
    }
    println!(
        "来源状态={}  来源 LCE cleanup={}",
        prepared.source_kind.short_name(),
        prepared
            .source_lce_start_lba
            .map(|lba| format!("LBA{lba}"))
            .unwrap_or_else(|| "无".into())
    );
}

fn print_provision_summary(prepared: &crate::application::provision::PreparedProvision) {
    match prepared {
        crate::application::provision::PreparedProvision::Official(prepared) => {
            print_new_provision_summary(prepared)
        }
        crate::application::provision::PreparedProvision::Plain(prepared) => {
            print_plain_provision_summary(prepared)
        }
    }
}

fn provision_flow(runner: &SysRunner, action: ProvisionAction) -> i32 {
    match action {
        ProvisionAction::Plan(opts) => {
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv: Vec<String> = std::env::args().skip(1).collect();
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            match crate::application::provision::prepare_provision_on_disk(runner, disk, &request) {
                Ok(prepared) => {
                    print_provision_summary(&prepared);
                    EXIT_OK
                }
                Err(error) => finish(Err(error)),
            }
        }
        ProvisionAction::Image { opts, out } => {
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv: Vec<String> = std::env::args().skip(1).collect();
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            let prepared = match crate::application::provision::prepare_provision_on_disk(
                runner, disk, &request,
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            print_provision_summary(&prepared);
            match crate::application::provision::export_provision_image(Path::new(&out), &prepared)
            {
                Ok(()) => {
                    println!("稀疏制盘镜像已写入 {}（已保留目标盘原始 LBA3）", out);
                    EXIT_OK
                }
                Err(error) => finish(Err(error)),
            }
        }
        ProvisionAction::Write {
            opts,
            yes,
            backup_dir,
        } => {
            if let Some(disk) = opts.disk {
                if let Err(error) = guard_usb_disk(runner, disk) {
                    return finish(Err(error));
                }
            }
            if !elevate::is_root() {
                let mut prompt = StdPrompter;
                let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                    Ok(value) => value,
                    Err(error) => return finish(Err(error)),
                };
                let mut argv = argv_with_backup_dir_for_elevation(backup_dir.as_deref());
                DeviceSelector::new(opts.disk).pin_argv(&mut argv, disk);
                elevate::ensure_elevated(&argv);
                unreachable!();
            }

            let mut prompt = StdPrompter;
            let disk = match provision_resolve_disk(runner, opts.disk, &mut prompt) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            let request = provision_request(&opts);
            let prepared = match crate::application::provision::prepare_provision_on_disk(
                runner, disk, &request,
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            print_provision_summary(&prepared);
            let confirmed = if yes {
                true
            } else {
                prompt.confirm_yes(&crate::ui::bold(&format!(
                    "将按上述制盘计划写入 disk{}（{}）；将保留制造商 LBA3，并按共享 application 安全事务执行。输入 YES: ",
                    disk, opts.target.full_name()
                )))
            };
            if !confirmed {
                return finish(Err(EdpCliError::new(EXIT_CANCELLED, "已取消(未写盘)")));
            }
            let write = match crate::application::provision::commit_provision_with_backup_on_disk(
                runner,
                &prepared,
                crate::application::resolve_backup_dir(backup_dir.as_deref()),
                &mut prompt,
            ) {
                Ok(value) => value,
                Err(error) => return finish(Err(error)),
            };
            println!("制盘前自动备份：{}", write.backup.path.display());
            for warning in &write.warnings {
                println!("{}", crate::ui::yellow(&warning.message()));
            }
            match write.commit {
                crate::application::provision::ProvisionCommitOutcome::Official(report) => {
                    println!(
                        "{}",
                        crate::ui::green("制盘：成功，协议与几何读回校验通过。")
                    );
                    if report.formats.is_empty() {
                        println!("格式化：未选择任何分区");
                    }
                    for item in &report.formats {
                        match &item.result {
                            Ok(()) => println!("格式化：✓ {}（读回验证通过）", item.role.label()),
                            Err(message) => {
                                println!("格式化：✗ {}：{}", item.role.label(), message)
                            }
                        }
                    }
                    if report.formats.iter().any(|item| item.result.is_err()) {
                        EXIT_IO
                    } else {
                        EXIT_OK
                    }
                }
                crate::application::provision::ProvisionCommitOutcome::Plain {
                    partition_count,
                } => {
                    println!(
                        "{}",
                        crate::ui::green(&format!(
                            "普通盘恢复：成功，{} 个 MBR 主分区与文件系统读回校验通过。",
                            partition_count
                        ))
                    );
                    EXIT_OK
                }
            }
        }
    }
}

pub(crate) fn argv_with_backup_dir_for_elevation(backup_dir_flag: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if backup_dir_flag.is_none() {
        argv.extend(crate::application::backup_dir_argv_suffix(
            std::env::var("EDPCLI_BACKUP_DIR").ok(),
        ));
    }
    argv
}

fn list_needs_elevation(rows: &[Row], elevated: bool, has_sentinel: bool) -> bool {
    crate::application::device_scan_needs_elevation(rows, elevated, has_sentinel)
}

fn list_flow(runner: &SysRunner, backup_dir_flag: Option<String>) -> i32 {
    let bak = crate::application::resolve_backup_dir(backup_dir_flag.as_deref());
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
    deep: bool,
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
    finish(
        crate::application::write::backup_create_on_disk(
            runner,
            disk,
            crate::application::resolve_backup_dir(backup_dir_flag.as_deref()),
            &mut prompt,
            None,
            None,
            deep,
        )
        .map(|_| EXIT_OK),
    )
}

/// backup restore 的公共外壳:
///   1) 显式目标的系统盘拒绝无需管理员权限，提权前先判；
///   2) 未提权时把目标统一固定为平台原生选择器；未给 --disk 时先以用户身份选盘；
///   3) 提权路径：（必要时交互选盘）→ 打开平台裸盘设备 → 执行流程。
fn real_flow(
    runner: &SysRunner,
    disk_opt: Option<u32>,
    backup_dir_flag: Option<String>,
    bin: Option<String>,
    yes: bool,
) -> i32 {
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
    let bak = crate::application::resolve_backup_dir(backup_dir_flag.as_deref());
    // 手动管理员会话且旗标/配置都未命中时，明确告知备份去向。
    // 自动提权的子进程带哨兵，不重复提示。
    let has_sentinel = std::env::args().any(|a| a == ELEVATED_FLAG);
    if !has_sentinel
        && crate::platform::has_elevation_origin()
        && backup_dir_flag.is_none()
        && std::env::var("EDPCLI_BACKUP_DIR")
            .unwrap_or_default()
            .is_empty()
        && !crate::application::has_configured_backup_dir()
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
    let n = match disk_opt {
        Some(n) => n,
        None => match auto_pick_disk(runner, &mut *prompter) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("{}", crate::ui::red(&e.msg));
                return e.code;
            }
        },
    };
    let r = crate::application::write::restore_on_disk(runner, bin, n, bak, prompter, None, None);
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

    #[test]
    fn finish_preserves_business_error_exit_code() {
        assert_eq!(
            finish(Err(EdpCliError::new(EXIT_IO, "expected failure"))),
            EXIT_IO
        );
    }

    #[test]
    fn target_plan_summary_reports_exact_geometry_and_data_fate() {
        use crate::protocol::edpf::EdpPartitionType;
        use crate::provision::{
            OfficialFilesystemFormat, OfficialPartitionMode, PartitionAction, PartitionRole,
            RegionDisposition, SourcePasswordKnowledge, TargetPartitionGeometry,
            TargetPartitionPlan, TargetPasswordPolicy, TargetProvisionPlan,
        };

        let plan = TargetProvisionPlan {
            mode: OfficialPartitionMode::BootShareCombined,
            partitions: vec![
                TargetPartitionPlan {
                    geometry: TargetPartitionGeometry {
                        role: PartitionRole::BootShareCombined,
                        partition_type: EdpPartitionType::Share,
                        start_lba: 63,
                        sector_count: 100,
                        physically_encrypted: false,
                        filesystem: Some(OfficialFilesystemFormat::ExFat),
                    },
                    action: PartitionAction::Rebuild,
                    disposition: RegionDisposition::Rebuild,
                    source_password_knowledge: None,
                    target_password_policy: Some(TargetPasswordPolicy::InitializeNew),
                    reason: "geometry changed".into(),
                    preserved_record: None,
                },
                TargetPartitionPlan {
                    geometry: TargetPartitionGeometry {
                        role: PartitionRole::Encrypt,
                        partition_type: EdpPartitionType::Encrypt,
                        start_lba: 1000,
                        sector_count: 200,
                        physically_encrypted: true,
                        filesystem: Some(OfficialFilesystemFormat::ExFat),
                    },
                    action: PartitionAction::PreserveExact,
                    disposition: RegionDisposition::PreserveOpaque,
                    source_password_knowledge: Some(SourcePasswordKnowledge::Unknown),
                    target_password_policy: Some(TargetPasswordPolicy::PreserveOpaque),
                    reason: "exact source match".into(),
                    preserved_record: None,
                },
            ],
            unallocated_sectors: 737,
        };

        let lines = target_plan_summary_lines(&plan);
        assert!(lines
            .iter()
            .any(|line| line.contains("start=63 end=162 sectors=100")));
        assert!(lines
            .iter()
            .any(|line| line.contains("Rebuild") && line.contains("原数据不可原样保留")));
        assert!(lines
            .iter()
            .any(|line| line.contains("start=1000 end=1199 sectors=200")));
        assert!(lines.iter().any(|line| line.contains("PreserveOpaque")
            && line.contains("原 key material")
            && line.contains("0 写入")));
        assert!(lines.iter().any(
            |line| line.contains("source=Unknown") && line.contains("target=disabled(opaque)")
        ));
        assert!(lines
            .iter()
            .any(|line| line.contains("unallocated=737 sectors")));
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
        assert!(parse_args(&["apply".into()]).is_err());
        match parse_args(&["info".into(), "backup.bin".into()]).unwrap() {
            Parsed::Info(opts) => {
                assert_eq!(opts.backup.as_deref(), Some("backup.bin"));
            }
            _ => panic!("应解析为 info"),
        }
        assert!(
            parse_args(&["convert".into()]).is_err(),
            "removed top-level offline convert command must stay absent"
        );
        match parse_args(&[
            "inspect".into(),
            "meta".into(),
            "--lba".into(),
            "6,7,12".into(),
            "--backup-dir".into(),
            "/tmp/bak".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.mode, crate::cli_args::InspectMode::Meta);
                assert_eq!(opts.lbas, vec![6, 7, 12]);
                assert_eq!(opts.backup_dir.as_deref(), Some("/tmp/bak"));
            }
            _ => panic!("应解析为 inspect meta"),
        }
        match parse_args(&[
            "inspect".into(),
            "raw".into(),
            "x.bin".into(),
            "--lba".into(),
            "9".into(),
            "--id".into(),
            "disk&ven_x&prod_y".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.mode, crate::cli_args::InspectMode::Raw);
                assert_eq!(opts.backup.as_deref(), Some("x.bin"));
                assert_eq!(opts.lbas, vec![9]);
                assert_eq!(opts.device_id.as_deref(), Some("disk&ven_x&prod_y"));
            }
            _ => panic!("应解析为 inspect raw backup"),
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
                action: BackupAction::Create {
                    disk: Some(4),
                    deep: false
                },
                ..
            }
        ));
    }

    #[test]
    fn elevation_reexec_pins_explicit_disk_to_platform_selector() {
        let selector = crate::platform::disk_selector_value(6);

        let mut split = vec![
            "backup".to_string(),
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
            "backup".to_string(),
            "restore".to_string(),
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
            label: None,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
            n_baks: 0,
            n_possible_baks: 0,
            denied: false,
            probe_error: None,
            is_nopwd: false,
            provision_kind: crate::provision::DiskProvisionKind::Plain,
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
            "raw".into(),
            "--disk".into(),
            "4".into(),
            "x.bin".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "inspect".into(),
            "meta".into(),
            "--onlyid".into(),
            "1402259934".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "inspect".into(),
            "raw".into(),
            "--lba".into(),
            "7".into(),
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
                "backup".into(),
                "restore".into(),
                "--disk".into(),
                "6".into(),
                "--_elevated".into()
            ])
            .unwrap(),
            Parsed::Backup { .. }
        ));
    }

    #[test]
    fn boolean_flags_reject_inline_values() {
        for argv in [
            vec!["backup", "prune", "--yes=0"],
            vec!["backup", "delete", "1", "--yes=no"],
            vec!["backup", "restore", "backup.bin", "--yes=false"],
            vec!["backup", "create", "--deep=false"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("布尔旗标带值必须报错");
            assert!(err.contains("不接受参数值"), "{err}");
        }
    }

    #[test]
    fn boolean_flags_reject_duplicates() {
        for argv in [
            vec!["backup", "prune", "--yes", "--yes"],
            vec!["backup", "restore", "backup.bin", "--yes", "--yes"],
        ] {
            let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
            let err = parse_args(&args).err().expect("布尔旗标重复必须报错");
            assert!(err.contains("重复"), "{err}");
        }
    }

    #[test]
    fn inspect_accepts_arbitrary_u64_lbas_ranges_and_count() {
        match parse_args(&[
            "inspect".into(),
            "decode".into(),
            "--lba".into(),
            "240250283-240250288".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => {
                assert_eq!(opts.mode, crate::cli_args::InspectMode::Decode);
                assert_eq!(
                    opts.lbas,
                    vec![240250283, 240250284, 240250285, 240250286, 240250287, 240250288]
                );
            }
            _ => panic!("应解析为 inspect decode"),
        }

        match parse_args(&[
            "inspect".into(),
            "raw".into(),
            "--lba".into(),
            "4294967296".into(),
            "--count".into(),
            "2".into(),
        ])
        .unwrap()
        {
            Parsed::Inspect(opts) => assert_eq!(opts.lbas, vec![4_294_967_296, 4_294_967_297]),
            _ => panic!("应解析为 inspect raw"),
        }

        assert!(parse_args(&["inspect".into(), "--lba".into(), "7".into()]).is_err());
        assert!(parse_args(&[
            "inspect".into(),
            "meta".into(),
            "--lba".into(),
            "12-7".into(),
        ])
        .is_err());
    }

    #[test]
    fn single_value_flags_reject_duplicates() {
        for argv in [
            vec!["backup", "restore", "--disk=4", "--disk=6"],
            vec!["backup", "create", "--disk=4", "--disk=6"],
            vec!["backup", "prune", "--keep", "1", "--keep", "2"],
            vec!["info", "--disk", "4", "--disk", "6"],
            vec!["inspect", "meta", "--lba", "1", "--lba", "2"],
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
                label: None,
                force_change_password: None,
                cancel_password_complexity_check: None,
                max_share_password_errors: None,
                max_encrypt_password_errors: None,
                n_baks: 0,
                n_possible_baks: 0,
                denied: false,
                probe_error: None,
                is_nopwd: false,
                provision_kind: crate::provision::DiskProvisionKind::Plain,
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
                label: Some("江苏电力!SAFE6".into()),
                force_change_password: Some(false),
                cancel_password_complexity_check: Some(false),
                max_share_password_errors: Some(255),
                max_encrypt_password_errors: Some(255),
                n_baks: 3,
                n_possible_baks: 2,
                denied: false,
                probe_error: None,
                is_nopwd: true,
                provision_kind: crate::provision::DiskProvisionKind::Mode0,
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
                label: None,
                force_change_password: None,
                cancel_password_complexity_check: None,
                max_share_password_errors: None,
                max_encrypt_password_errors: None,
                n_baks: 0,
                n_possible_baks: 0,
                denied: false,
                probe_error: None,
                is_nopwd: false,
                provision_kind: crate::provision::DiskProvisionKind::Plain,
                partitions: None,
            },
        ];
        let out = print_disk_table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "外接盘 3 个:");
        let disk4_line = lines.iter().find(|l| l.contains("disk4")).unwrap();
        assert!(disk4_line.contains("普通盘"));
        let disk6_line = lines.iter().find(|l| l.contains("disk6")).unwrap();
        assert!(
            disk6_line.contains("mode0 · 缺省三分区")
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
        assert!(
            meta.contains("onlyid=1402259934")
                && meta.contains("备份 3 份")
                && meta.contains("可能相关 2 份")
        );
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
