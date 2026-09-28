//! Interactive ratatui/crossterm frontend.
//!
//! Business work is delegated to `crate::application`; this module owns only terminal lifecycle,
//! event dispatch and rendering.

pub mod animation;
pub mod clipboard;
pub mod command;
mod controller;
pub mod demo;
pub mod disk_layout;
mod dispatch;
pub mod event;
pub mod execution;
pub mod keymap;
pub mod pane;
pub mod render;
mod runtime_input;
mod runtime_updates;
pub mod shell;
pub mod state;
mod table_dispatch;
pub mod table_layout;
pub mod task;
pub mod theme;
pub mod ui;

use std::io::{self, IsTerminal, Stdout};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event as ct_event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::common::{EXIT_IO, EXIT_OK, EXIT_USAGE};
use dispatch::*;
use event::KeyMapper;
use runtime_updates::apply_task_updates;
use state::{AppState, NavCommand, StateEffect};
use task::TaskHub;

const RESUME_KIND_FLAG: &str = "--_resume-kind";
const RESUME_DISK_FLAG: &str = "--_resume-disk";
const RESUME_BACKUP_FLAG: &str = "--_resume-backup";
const RESUME_IDENTITY_PIN_FLAG: &str = "--_resume-identity-pin";

/// Serialize a confirmed write intent for an elevated TUI restart.
///
/// The disk is converted to the platform-native selector before crossing the privilege boundary;
/// restore additionally pins the exact backup path. The resumed TUI requires a second explicit YES.
pub fn resume_argv(intent: &state::WriteIntent) -> Vec<String> {
    let mut argv = vec![
        "tui".to_string(),
        RESUME_KIND_FLAG.to_string(),
        match intent.kind {
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
    if let Some(identity) = &intent.expected_identity {
        argv.push(RESUME_IDENTITY_PIN_FLAG.to_string());
        argv.push(serde_json::to_string(identity).unwrap_or_else(|_| "{}".into()));
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
    let mut identity_pin = None;
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
            RESUME_IDENTITY_PIN_FLAG => {
                if identity_pin.is_some() {
                    return Err(format!("错误: {RESUME_IDENTITY_PIN_FLAG} 重复指定"));
                }
                saw_resume = true;
                let raw = take(RESUME_IDENTITY_PIN_FLAG)?;
                let pin: state::ExpectedIdentity = serde_json::from_str(&raw)
                    .map_err(|error| format!("错误: TUI resume identity pin 无效: {error}"))?;
                pin.validate().map_err(|error| format!("错误: {error}"))?;
                identity_pin = Some(pin);
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
    let identity_pin =
        identity_pin.ok_or_else(|| format!("错误: TUI resume 缺少 {RESUME_IDENTITY_PIN_FLAG}"))?;
    match kind {
        state::WriteKind::BackupCreate if backup.is_some() => {
            Err("错误: 非 Restore resume 不允许携带备份路径".into())
        }
        state::WriteKind::Restore if backup.is_none() => {
            Err(format!("错误: restore resume 缺少 {RESUME_BACKUP_FLAG}"))
        }
        _ => Ok(Some(state::WriteIntent {
            kind,
            disk,
            backup,
            expected_identity: Some(identity_pin),
        })),
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

fn run_loop(resume: Option<state::WriteIntent>) -> io::Result<LoopExit> {
    let mut session = TerminalSession::enter()?;
    let mut state = AppState::new();
    let mut last_animation_tick = Instant::now();
    let motion_mode = animation::MotionMode::from_env();
    let mut redraw_requested = true;
    let mut notice_was_visible = false;
    let mut last_render_at: Option<Instant> = None;
    if let Some(intent) = resume {
        state.begin_write_wizard(intent.kind, intent.disk, intent.backup);
    }
    let mut keys = KeyMapper::new();
    let mut tasks = TaskHub::new();
    let backup_dir = crate::application::resolve_backup_dir(None);
    tasks.request_device_scan(backup_dir.clone());
    tasks.request_backup_scan(backup_dir.clone());
    state.set_device_scan_pending(true);
    state.set_backup_scan_pending(true);

    let result = (|| -> io::Result<LoopExit> {
        loop {
            let now = Instant::now();
            if let Some(interval) = motion_mode.tick_interval() {
                if now.duration_since(last_animation_tick) >= interval {
                    state.advance_animation();
                    last_animation_tick = now;
                    redraw_requested = true;
                }
            }

            let updates = tasks.poll();
            redraw_requested |= updates.has_updates();
            apply_task_updates(&mut state, &mut tasks, updates, &backup_dir);
            if state.take_deferred_exit() == StateEffect::ExitRequested {
                break;
            }

            let notice_visible = state.notice().is_some();
            if notice_visible != notice_was_visible {
                notice_was_visible = notice_visible;
                redraw_requested = true;
            }

            let now = Instant::now();
            const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);
            if redraw_requested
                && last_render_at.is_none_or(|last| now.duration_since(last) >= MIN_RENDER_INTERVAL)
            {
                session.terminal.draw(|frame| render::draw(frame, &state))?;
                redraw_requested = false;
                last_render_at = Some(now);
            }
            let poll_timeout = if redraw_requested {
                last_render_at
                    .map(|last| MIN_RENDER_INTERVAL.saturating_sub(now.duration_since(last)))
                    .unwrap_or_default()
                    .min(Duration::from_millis(animation::TICK_INTERVAL_MS))
            } else {
                Duration::from_millis(animation::TICK_INTERVAL_MS)
            };
            if !ct_event::poll(poll_timeout)? {
                continue;
            }

            redraw_requested = true;
            match ct_event::read()? {
                ct_event::Event::Key(key) => {
                    if !event::is_actionable_key(&key) {
                        continue;
                    }
                    let terminal_size = session.terminal.size()?;
                    match runtime_input::handle_key(
                        &mut state,
                        &mut tasks,
                        &mut keys,
                        key,
                        &backup_dir,
                        terminal_size,
                    ) {
                        runtime_input::KeyOutcome::Handled => {}
                        runtime_input::KeyOutcome::NextIteration => continue,
                        runtime_input::KeyOutcome::Exit => break,
                        runtime_input::KeyOutcome::Elevate(intent) => {
                            return Ok(LoopExit::Elevate(intent));
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
    })();

    if result.is_err() && tasks.active_operation().is_some() {
        // Restore raw mode/alternate screen before waiting. The transaction keeps
        // running without its UI and is allowed to complete rollback/readback.
        drop(session);
        tasks.wait_for_critical_operation();
    }
    result
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
    fn provision_form_uses_explicit_insert_mode_for_text_editing() {
        let mut state = AppState::new();
        state.navigate(NavCommand::WorkspaceProvision, 20);
        state.provision_mut().stage = state::ProvisionStage::Form;
        state.provision_mut().field_selected = 0;
        state.provision_mut().form.label_id = "12345".into();

        assert!(state.provision_begin_insert());
        assert_eq!(state.input_mode(), state::InputMode::Insert);
        assert_eq!(state.workspace(), state::Workspace::Provision);

        state.provision_move_cursor(-1);
        assert_eq!(state.provision_field_cursor(), 4);
        state.provision_end_insert();
        assert_eq!(state.input_mode(), state::InputMode::Normal);
        assert_eq!(state.workspace(), state::Workspace::Provision);
    }

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
