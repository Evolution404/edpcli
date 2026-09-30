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
mod help_overlay;
pub mod keymap;
mod operation_progress_status;
mod overview;
pub mod pane;
mod progress_transport;
pub mod render;
mod resume;
mod runtime_input;
mod runtime_updates;
pub mod shell;
pub mod state;
mod status;
mod table_dispatch;
pub mod table_layout;
pub mod task;
pub mod theme;
pub mod ui;
pub use resume::{parse_resume_args, resume_argv};

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
        state.begin_write_wizard_for_identity(
            intent.kind,
            intent.disk,
            intent.backup,
            intent.expected_identity,
        );
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
            // Long-running operations may produce one progress snapshot per sector.
            // Rendering at 20 Hz keeps gauges responsive without tying terminal paints
            // to storage event frequency.
            const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(50);
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

    fn provision_test_device() -> crate::disk_scan::Row {
        use crate::application::media_identity::{MediaIdentityPin, MediaIdentitySnapshot};
        use crate::common::{METADATA_IMAGE_LEN, SECTOR};
        use crate::provision::DiskProvisionKind;

        let mut row = crate::disk_scan::Row {
            disk: 6,
            size: 64_000_000_000,
            vid: "1234".into(),
            pid: "5678".into(),
            proto: "USB".into(),
            serial: None,
            hardware_model: None,
            device_id: Some("disk&ven_test&prod_test".into()),
            identity_pin: None,
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
            provision_kind: DiskProvisionKind::Plain,
            partitions: None,
            partition_table: None,
            partition_table_error: None,
            lce: None,
        };
        let mut snapshot = MediaIdentitySnapshot::default();
        snapshot.hardware.total_sectors = Some(row.size / SECTOR as u64);
        snapshot.hardware.logical_sector_size = Some(SECTOR as u32);
        snapshot.protocol.provision_kind = Some(row.provision_kind);
        row.identity_pin = Some(MediaIdentityPin::new(
            snapshot,
            &vec![0; METADATA_IMAGE_LEN],
        ));
        row
    }

    #[test]
    fn provision_form_uses_explicit_insert_mode_for_text_editing() {
        let mut state = AppState::new();
        state.replace_devices(vec![provision_test_device()]);
        assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
        state.provision_begin_selected();
        state.provision_enter_form_workspace();
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
