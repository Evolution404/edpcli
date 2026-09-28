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

pub use crate::backup_cli::{backup_delete, backup_list, backup_prune, backup_verify};
use crate::cli_args::print_help;
pub use crate::cli_args::{
    parse_args, print_usage, BackupAction, DiskOpts, InfoOpts, InspectOpts, Parsed,
    ProvisionAction, ProvisionNewOpts,
};
use crate::common::*;
use crate::completion;
use crate::elevate::{self, ELEVATED_FLAG};

mod commands;
use crate::inspect_cli::inspect_flow;
use crate::metainfo_cli::info_flow;
use crate::selectors::DeviceSelector;
use crate::sysinfo::{ReadProbeCache, SysRunner};
use commands::{backup_create_real_flow, list_flow, provision_flow, real_flow};
#[cfg(test)]
use commands::{list_needs_elevation, target_plan_summary_lines};

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
pub use crate::disk_scan::{scan_disks, Row};
pub use crate::disk_scan_render::print_disk_table;

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
                        "list"
                            | "tui"
                            | "demo"
                            | "info"
                            | "backup"
                            | "inspect"
                            | "provision"
                            | "completion"
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
        Parsed::Demo { scene, list_scenes } => crate::tui::demo::run(scene.as_deref(), list_scenes),
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

pub(crate) fn argv_with_backup_dir_for_elevation(backup_dir_flag: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if backup_dir_flag.is_none() {
        argv.extend(crate::application::backup_dir_argv_suffix(
            std::env::var("EDPCLI_BACKUP_DIR").ok(),
        ));
    }
    argv
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
#[cfg(test)]
mod tests;
