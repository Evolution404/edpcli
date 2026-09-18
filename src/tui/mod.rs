//! Interactive ratatui/crossterm frontend.
//!
//! Business work is delegated to `crate::application`; this module owns only terminal lifecycle,
//! event dispatch and rendering.

pub mod event;
pub mod render;
pub mod state;

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
use state::{AppState, StateEffect};

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

    loop {
        session.terminal.draw(|frame| render::draw(frame, &state))?;
        if !ct_event::poll(Duration::from_millis(100))? {
            continue;
        }

        match ct_event::read()? {
            ct_event::Event::Key(key) => {
                if let Some(command) = keys.map(key) {
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
