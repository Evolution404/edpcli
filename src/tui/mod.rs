//! Interactive ratatui/crossterm frontend.
//!
//! Business work is delegated to `crate::application`; this module owns only terminal lifecycle,
//! event dispatch and rendering.

pub mod event;
pub mod render;
pub mod state;
pub mod task;

use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::{
    cursor::{Hide, Show},
    event as ct_event,
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
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
        },
        RESUME_DISK_FLAG.to_string(),
        crate::platform::disk_selector_value(intent.disk),
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
                    _ => return Err(format!("错误: 非法 TUI resume kind: {value}")),
                });
            }
            RESUME_DISK_FLAG => {
                if disk.is_some() {
                    return Err(format!("错误: {RESUME_DISK_FLAG} 重复指定"));
                }
                saw_resume = true;
                let value = take(RESUME_DISK_FLAG)?;
                disk = Some(
                    crate::platform::parse_disk_selector(&value)
                        .map_err(|error| format!("错误: resume disk {value}: {error}"))?,
                );
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
        state::WriteKind::Apply if backup.is_some() => {
            Err("错误: apply resume 不允许携带备份路径".into())
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

fn run_loop() -> io::Result<()> {
    let mut session = TerminalSession::enter()?;
    let mut state = AppState::new();
    let mut keys = KeyMapper::new();
    let mut tasks = TaskHub::new();
    let backup_dir = crate::diskio::resolve_backup_dir(None);
    tasks.request_device_scan(backup_dir.clone());
    tasks.request_backup_scan(backup_dir.clone());
    state.set_device_scan_pending(true);
    state.set_backup_scan_pending(true);

    loop {
        let updates = tasks.poll();
        if let Some(rows) = updates.devices {
            state.replace_devices(rows);
        }
        if let Some(rows) = updates.backups {
            state.replace_backups(rows);
        }
        session.terminal.draw(|frame| render::draw(frame, &state))?;
        if !ct_event::poll(Duration::from_millis(100))? {
            continue;
        }

        match ct_event::read()? {
            ct_event::Event::Key(key) => {
                if let Some(command) = keys.map(key) {
                    if command == NavCommand::Refresh {
                        match state.workspace() {
                            state::Workspace::Devices => {
                                tasks.request_device_scan(backup_dir.clone());
                                state.set_device_scan_pending(true);
                            }
                            state::Workspace::Backups => {
                                tasks.request_backup_scan(backup_dir.clone());
                                state.set_backup_scan_pending(true);
                            }
                        }
                        continue;
                    }
                    let viewport_height = session.terminal.size()?.height.saturating_sub(5) as usize;
                    match state.navigate(command, viewport_height) {
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
    Ok(())
}

/// Run the interactive TUI only when both stdin and stdout are terminals.
pub fn run() -> i32 {
    if !is_interactive_terminal() {
        eprintln!("错误: edpcli tui 需要交互式 TTY；脚本请继续使用 CLI v2 子命令。");
        return EXIT_USAGE;
    }

    match run_loop() {
        Ok(()) => EXIT_OK,
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
}
