//! Interactive ratatui/crossterm frontend.
//!
//! Business work is delegated to `crate::application`; this module owns only terminal lifecycle,
//! event dispatch and rendering.

pub mod animation;
pub mod command;
pub mod event;
pub mod render;
pub mod state;
pub mod task;

use std::io::{self, IsTerminal, Stdout};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event as ct_event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::common::{EXIT_IO, EXIT_OK, EXIT_USAGE};
use event::KeyMapper;
use state::{AppState, NavCommand, StateEffect};
use task::TaskHub;

const RESUME_KIND_FLAG: &str = "--_resume-kind";
const RESUME_DISK_FLAG: &str = "--_resume-disk";
const RESUME_BACKUP_FLAG: &str = "--_resume-backup";

/// Serialize a confirmed write intent for an elevated TUI restart.
///
/// The disk is converted to the platform-native selector before crossing the privilege boundary;
/// restore additionally pins the exact backup path. The resumed TUI requires a second explicit YES.
pub fn resume_argv(intent: &state::WriteIntent) -> Vec<String> {
    let mut argv = vec![
        "tui".to_string(),
        RESUME_KIND_FLAG.to_string(),
        match intent.kind {
            state::WriteKind::Apply => "apply".to_string(),
            state::WriteKind::Restore => "restore".to_string(),
            state::WriteKind::BackupCreate => "backup-create".to_string(),
        },
        RESUME_DISK_FLAG.to_string(),
        crate::application::pin_disk_selector(intent.disk),
    ];
    if let Some(path) = &intent.backup {
        argv.push(RESUME_BACKUP_FLAG.to_string());
        argv.push(path.to_string_lossy().into_owned());
    }
    argv
}

/// Parse the private state used only for an elevated TUI restart.
///
/// Any partial, duplicated or contradictory state fails closed. Public TUI invocations with no
/// private flags return `Ok(None)`.
pub fn parse_resume_args(argv: &[String]) -> Result<Option<state::WriteIntent>, String> {
    let mut kind = None;
    let mut disk = None;
    let mut backup = None;
    let mut saw_resume = false;

    let mut i = usize::from(argv.first().is_some_and(|arg| arg == "tui"));
    while i < argv.len() {
        let arg = &argv[i];
        if arg == crate::elevate::ELEVATED_FLAG {
            i += 1;
            continue;
        }
        let mut take = |flag: &str| -> Result<String, String> {
            i += 1;
            if i >= argv.len() {
                return Err(format!("错误: {flag} 缺少参数值"));
            }
            Ok(argv[i].clone())
        };
        match arg.as_str() {
            RESUME_KIND_FLAG => {
                if kind.is_some() {
                    return Err(format!("错误: {RESUME_KIND_FLAG} 重复指定"));
                }
                saw_resume = true;
                let value = take(RESUME_KIND_FLAG)?;
                kind = Some(match value.as_str() {
                    "apply" => state::WriteKind::Apply,
                    "restore" => state::WriteKind::Restore,
                    "backup-create" => state::WriteKind::BackupCreate,
                    _ => return Err(format!("错误: 非法 TUI resume kind: {value}")),
                });
            }
            RESUME_DISK_FLAG => {
                if disk.is_some() {
                    return Err(format!("错误: {RESUME_DISK_FLAG} 重复指定"));
                }
                saw_resume = true;
                let value = take(RESUME_DISK_FLAG)?;
                disk = Some(crate::application::parse_pinned_disk_selector(&value)?);
            }
            RESUME_BACKUP_FLAG => {
                if backup.is_some() {
                    return Err(format!("错误: {RESUME_BACKUP_FLAG} 重复指定"));
                }
                saw_resume = true;
                backup = Some(std::path::PathBuf::from(take(RESUME_BACKUP_FLAG)?));
            }
            other => return Err(format!("错误: tui 不认识内部 resume 参数 {other}")),
        }
        i += 1;
    }

    if !saw_resume {
        return Ok(None);
    }
    let kind = kind.ok_or_else(|| format!("错误: 缺少 {RESUME_KIND_FLAG}"))?;
    let disk = disk.ok_or_else(|| format!("错误: 缺少 {RESUME_DISK_FLAG}"))?;
    match kind {
        state::WriteKind::Apply | state::WriteKind::BackupCreate if backup.is_some() => {
            Err("错误: 非 Restore resume 不允许携带备份路径".into())
        }
        state::WriteKind::Restore if backup.is_none() => {
            Err(format!("错误: restore resume 缺少 {RESUME_BACKUP_FLAG}"))
        }
        _ => Ok(Some(state::WriteIntent { kind, disk, backup })),
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                if let Err(error) = terminal.clear() {
                    let _ = disable_raw_mode();
                    let _ = execute!(terminal.backend_mut(), Show, LeaveAlternateScreen);
                    return Err(error);
                }
                Ok(Self { terminal })
            }
            Err(error) => {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, Show, LeaveAlternateScreen);
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn startup_elevation_argv(argv: &[String], elevated: bool) -> Option<Vec<String>> {
    if elevated {
        return None;
    }
    if argv.first().is_some_and(|arg| arg == "tui") {
        Some(argv.to_vec())
    } else {
        // bare `edpcli` 由 cli 层路由到 TUI；跨越 sudo/UAC 边界时显式补上
        // `tui`，避免 elevated child 因只剩内部哨兵而回落到普通 CLI 解析。
        Some(vec!["tui".to_string()])
    }
}

fn ensure_elevated_before_tui(argv: &[String]) {
    if let Some(elevation_argv) = startup_elevation_argv(argv, crate::elevate::is_root()) {
        crate::elevate::ensure_elevated(&elevation_argv);
        unreachable!();
    }
}

enum LoopExit {
    Done,
    Elevate(state::WriteIntent),
}

fn dispatch_nav_command(
    state: &mut AppState,
    tasks: &mut TaskHub,
    command: NavCommand,
    backup_dir: &std::path::Path,
    viewport_height: usize,
) -> StateEffect {
    match command {
        NavCommand::Refresh => {
            match state.workspace() {
                state::Workspace::Devices => {
                    tasks.request_device_scan(backup_dir.to_path_buf());
                    state.set_device_scan_pending(true);
                }
                state::Workspace::Backups => {
                    tasks.request_backup_scan(backup_dir.to_path_buf());
                    state.set_backup_scan_pending(true);
                }
            }
            StateEffect::None
        }
        NavCommand::OpenInspect => {
            match state.workspace() {
                state::Workspace::Devices => {
                    if let Some(disk) = state.selected_device_disk() {
                        tasks.request_inspect_disk(disk);
                        state.set_inspect_pending(true);
                    }
                }
                state::Workspace::Backups => {
                    if let Some(path) = state.selected_backup_path() {
                        tasks.request_inspect_backup(path);
                        state.set_inspect_pending(true);
                    }
                }
            }
            StateEffect::None
        }
        NavCommand::BeginApply => {
            if let Some(disk) = state.selected_device_disk() {
                state.begin_write_wizard(state::WriteKind::Apply, disk, None);
            }
            StateEffect::None
        }
        NavCommand::BeginRestore => {
            if let (Some(disk), Some(backup)) =
                (state.selected_device_disk(), state.selected_backup_path())
            {
                state.begin_write_wizard(state::WriteKind::Restore, disk, Some(backup));
            } else {
                state.set_notice("恢复需要先在设备页选定目标 U 盘，再进入备份页选择备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupCreate => {
            if let Some(disk) = state.selected_device_disk() {
                state.begin_write_wizard(state::WriteKind::BackupCreate, disk, None);
            } else {
                state.set_notice("创建备份需要先在设备页选定 U 盘。");
            }
            StateEffect::None
        }
        NavCommand::VerifyBackup => {
            if let Some(path) = state.selected_backup_path() {
                state.set_notice("正在后台校验当前备份…");
                tasks.request_backup_verify(path, backup_dir.to_path_buf());
            } else {
                state.set_notice("当前没有可校验的备份。");
            }
            StateEffect::None
        }
        NavCommand::BeginBackupDelete => {
            if let Some((path, expected_sha256)) = state.selected_backup_delete_target() {
                state.begin_backup_delete(path, expected_sha256);
            } else {
                state.set_notice("当前备份缺少可固定的内容摘要，拒绝删除。");
            }
            StateEffect::None
        }
        _ => state.navigate(command, viewport_height),
    }
}

fn palette_action_to_nav(action: command::PaletteAction) -> NavCommand {
    match action {
        command::PaletteAction::Devices => NavCommand::Left,
        command::PaletteAction::Backups => NavCommand::Right,
        command::PaletteAction::Inspect => NavCommand::OpenInspect,
        command::PaletteAction::Apply => NavCommand::BeginApply,
        command::PaletteAction::Restore => NavCommand::BeginRestore,
        command::PaletteAction::BackupCreate => NavCommand::BeginBackupCreate,
        command::PaletteAction::BackupVerify => NavCommand::VerifyBackup,
        command::PaletteAction::BackupDelete => NavCommand::BeginBackupDelete,
        command::PaletteAction::Refresh => NavCommand::Refresh,
        command::PaletteAction::Help => NavCommand::Help,
        command::PaletteAction::Quit => NavCommand::Quit,
    }
}

fn run_loop(resume: Option<state::WriteIntent>) -> io::Result<LoopExit> {
    let mut session = TerminalSession::enter()?;
    let mut state = AppState::new();
    let mut last_animation_tick = Instant::now();
    if let Some(intent) = resume {
        state.begin_write_wizard(intent.kind, intent.disk, intent.backup);
    }
    let mut keys = KeyMapper::new();
    let mut tasks = TaskHub::new();
    let backup_dir = crate::diskio::resolve_backup_dir(None);
    tasks.request_device_scan(backup_dir.clone());
    tasks.request_backup_scan(backup_dir.clone());
    state.set_device_scan_pending(true);
    state.set_backup_scan_pending(true);

    loop {
        let now = Instant::now();
        if now.duration_since(last_animation_tick)
            >= Duration::from_millis(animation::TICK_INTERVAL_MS)
        {
            state.advance_animation();
            last_animation_tick = now;
        }

        let updates = tasks.poll();
        if let Some(rows) = updates.devices {
            state.replace_devices(rows);
        }
        if let Some(rows) = updates.backups {
            state.replace_backups(rows);
        }
        if let Some(message) = updates.device_error {
            state.set_device_scan_pending(false);
            state.set_notice(message);
        }
        if let Some(message) = updates.backup_error {
            state.set_backup_scan_pending(false);
            state.set_notice(message);
        }
        if let Some(message) = updates.write_progress {
            state.set_write_progress(message);
        }
        if let Some(result) = updates.write {
            let refresh_backups = result.is_ok()
                && state
                    .wizard()
                    .is_some_and(|wizard| wizard.kind == state::WriteKind::BackupCreate);
            state.finish_write(result);
            if refresh_backups {
                tasks.request_backup_scan(backup_dir.clone());
                state.set_backup_scan_pending(true);
            }
        }
        if let Some(result) = updates.backup_verify {
            match result {
                Ok(()) => state.set_notice("当前备份校验通过：大小与 SHA-256 正常。"),
                Err(message) => state.set_notice(message),
            }
        }
        if let Some(result) = updates.backup_delete {
            let refresh_backups = result.is_ok();
            state.finish_backup_delete(result);
            if refresh_backups {
                tasks.request_backup_scan(backup_dir.clone());
                state.set_backup_scan_pending(true);
            }
        }
        if let Some(result) = updates.inspect {
            match result {
                Ok(workspace) => state.replace_inspect(workspace),
                Err(message) => {
                    state.set_inspect_pending(false);
                    state.set_notice(message);
                }
            }
        }
        session.terminal.draw(|frame| render::draw(frame, &state))?;
        if !ct_event::poll(Duration::from_millis(animation::TICK_INTERVAL_MS))? {
            continue;
        }

        match ct_event::read()? {
            ct_event::Event::Key(key) => {
                if state
                    .backup_delete()
                    .is_some_and(|delete| delete.stage == state::WizardStage::Confirm)
                {
                    match key.code {
                        ct_event::KeyCode::Char(ch)
                            if !key.modifiers.contains(ct_event::KeyModifiers::CONTROL) =>
                        {
                            state.push_backup_delete_confirmation(ch);
                            continue;
                        }
                        ct_event::KeyCode::Backspace => {
                            state.backspace_backup_delete_confirmation();
                            continue;
                        }
                        ct_event::KeyCode::Enter => {
                            if let Some((path, expected_sha256)) =
                                state.submit_backup_delete_confirmation()
                            {
                                tasks.request_backup_delete(
                                    path,
                                    expected_sha256,
                                    backup_dir.clone(),
                                );
                            }
                            continue;
                        }
                        ct_event::KeyCode::Esc => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                            continue;
                        }
                        _ => {}
                    }
                }

                if state
                    .wizard()
                    .is_some_and(|wizard| wizard.stage == state::WizardStage::Confirm)
                {
                    match key.code {
                        ct_event::KeyCode::Char(ch)
                            if !key.modifiers.contains(ct_event::KeyModifiers::CONTROL) =>
                        {
                            state.push_wizard_confirmation(ch);
                            continue;
                        }
                        ct_event::KeyCode::Backspace => {
                            state.backspace_wizard_confirmation();
                            continue;
                        }
                        ct_event::KeyCode::Enter => {
                            if let Some(intent) = state.submit_wizard_confirmation() {
                                if !crate::elevate::is_root() {
                                    return Ok(LoopExit::Elevate(intent));
                                }
                                if intent.kind == state::WriteKind::BackupCreate {
                                    tasks.request_backup_create(intent.disk, backup_dir.clone());
                                } else {
                                    tasks.request_write(intent, backup_dir.clone());
                                }
                            }
                            continue;
                        }
                        ct_event::KeyCode::Esc => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                            continue;
                        }
                        _ => {}
                    }
                }

                if matches!(
                    state.input_mode(),
                    state::InputMode::Search | state::InputMode::Command
                ) {
                    match key.code {
                        ct_event::KeyCode::Char(ch)
                            if !key.modifiers.contains(ct_event::KeyModifiers::CONTROL) =>
                        {
                            state.push_input_char(ch);
                            continue;
                        }
                        ct_event::KeyCode::Backspace => {
                            state.backspace_input();
                            continue;
                        }
                        ct_event::KeyCode::Esc => {
                            let _ = state.navigate(NavCommand::Escape, 1);
                            continue;
                        }
                        ct_event::KeyCode::Enter => {
                            if state.input_mode() == state::InputMode::Search {
                                state.submit_search();
                            } else {
                                let input = state.take_input();
                                state.cancel_input();
                                match command::parse_command(&input) {
                                    Ok(action) => {
                                        let viewport_height =
                                            session.terminal.size()?.height.saturating_sub(5)
                                                as usize;
                                        let effect = dispatch_nav_command(
                                            &mut state,
                                            &mut tasks,
                                            palette_action_to_nav(action),
                                            &backup_dir,
                                            viewport_height,
                                        );
                                        if effect == StateEffect::ExitRequested {
                                            break;
                                        }
                                    }
                                    Err(message) => state.set_notice(message),
                                }
                            }
                            continue;
                        }
                        _ => {}
                    }
                }

                if let Some(command) = keys.map(key) {
                    let viewport_height =
                        session.terminal.size()?.height.saturating_sub(5) as usize;
                    match dispatch_nav_command(
                        &mut state,
                        &mut tasks,
                        command,
                        &backup_dir,
                        viewport_height,
                    ) {
                        StateEffect::ExitRequested => break,
                        StateEffect::ExitDeferred | StateEffect::None => {}
                    }
                }
            }
            ct_event::Event::Resize(_, _) => {}
            _ => {}
        }

        if state.take_deferred_exit() == StateEffect::ExitRequested {
            break;
        }
    }
    Ok(LoopExit::Done)
}

/// Run the interactive TUI only when both stdin and stdout are terminals.
pub fn run() -> i32 {
    if !is_interactive_terminal() {
        eprintln!("错误: edpcli tui 需要交互式 TTY；脚本请继续使用 CLI v2 子命令。");
        return EXIT_USAGE;
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();

    // 在进入 raw mode / alternate screen 之前获取管理员权限。这样 macOS/Linux
    // 直接在普通终端显示 sudo 密码提示，Windows 直接走 UAC；授权后一次进入
    // 完整能力 TUI，不再等到写盘确认时退出界面再重启。
    ensure_elevated_before_tui(&argv);

    let resume = match parse_resume_args(&argv) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("{message}");
            return EXIT_USAGE;
        }
    };

    match run_loop(resume) {
        Ok(LoopExit::Done) => EXIT_OK,
        Ok(LoopExit::Elevate(intent)) => {
            let argv = resume_argv(&intent);
            crate::elevate::ensure_elevated(&argv);
            unreachable!()
        }
        Err(error) => {
            eprintln!("错误: TUI 终端初始化或事件循环失败: {error}");
            EXIT_IO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tty_gate_is_purely_a_terminal_capability_check() {
        let _ = is_interactive_terminal();
    }

    #[test]
    fn bare_tui_route_becomes_explicit_across_the_elevation_boundary() {
        assert_eq!(
            startup_elevation_argv(&[], false),
            Some(vec!["tui".to_string()])
        );
        assert_eq!(
            startup_elevation_argv(&["tui".to_string()], false),
            Some(vec!["tui".to_string()])
        );
        assert_eq!(startup_elevation_argv(&[], true), None);
    }
}
