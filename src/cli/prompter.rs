use std::io::{self, Write};

use crate::application::write::Prompter;

pub struct StdPrompter;

impl Prompter for StdPrompter {
    fn prompt_line(&mut self, msg: &str) -> String {
        print!("{}", msg);
        let _ = io::stdout().flush();
        let mut s = String::new();
        let _ = io::stdin().read_line(&mut s);
        s
    }
    fn prompt_secret(&mut self, msg: &str) -> crate::provision::SecretBytes {
        use std::io::IsTerminal;
        if !io::stdin().is_terminal() {
            let mut input = self.prompt_line(msg).into_bytes();
            while input
                .last()
                .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
            {
                input.pop();
            }
            let secret = crate::provision::SecretBytes::new(&input);
            input.fill(0);
            return secret;
        }
        if crossterm::terminal::enable_raw_mode().is_err() {
            eprintln!("无法安全关闭密码输入回显，已取消输入");
            return crate::provision::SecretBytes::default();
        }
        struct RawModeGuard;
        impl Drop for RawModeGuard {
            fn drop(&mut self) {
                let _ = crossterm::terminal::disable_raw_mode();
            }
        }
        let _guard = RawModeGuard;
        print!("{msg}");
        let _ = io::stdout().flush();
        let mut input = String::new();
        loop {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key))
                    if matches!(
                        key.kind,
                        crossterm::event::KeyEventKind::Press
                            | crossterm::event::KeyEventKind::Repeat
                    ) =>
                {
                    match key.code {
                        crossterm::event::KeyCode::Enter => break,
                        crossterm::event::KeyCode::Backspace => {
                            input.pop();
                        }
                        crossterm::event::KeyCode::Char(c)
                            if !key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            input.push(c);
                        }
                        crossterm::event::KeyCode::Esc => {
                            input.clear();
                            break;
                        }
                        crossterm::event::KeyCode::Char('c')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            input.clear();
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    input.clear();
                    break;
                }
            }
        }
        println!();
        let secret = crate::provision::SecretBytes::new(input.as_bytes());
        let mut bytes = input.into_bytes();
        bytes.fill(0);
        secret
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
    fn prompt_secret(&mut self, msg: &str) -> crate::provision::SecretBytes {
        self.0.prompt_secret(msg)
    }
    fn confirm_yes(&mut self, _msg: &str) -> bool {
        true
    }
    fn confirm_write_yes(&mut self, _msg: &str) -> bool {
        true
    }
    fn confirm_post_restore_format_yes(&mut self, msg: &str) -> bool {
        self.0.confirm_write_yes(msg)
    }
    fn confirm_reinitialize_yes(&mut self, msg: &str) -> bool {
        self.0.confirm_reinitialize_yes(msg)
    }
    fn write_event(&mut self, event: crate::application::WriteEvent) {
        self.0.write_event(event);
    }
}
