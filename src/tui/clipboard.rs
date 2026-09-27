//! Confirmable native clipboard writes with an explicitly unconfirmed OSC52 fallback.

use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};

use crossterm::{clipboard::CopyToClipboard, execute};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardOutcome {
    Confirmed,
    TerminalRequestSent,
    Unsupported,
    Failed(String),
}

pub trait ClipboardBackend {
    fn copy(&mut self, content: &str) -> ClipboardOutcome;
}

pub fn copy_with(backend: &mut dyn ClipboardBackend, content: &str) -> ClipboardOutcome {
    backend.copy(content)
}

pub fn copy_text(content: &str) -> ClipboardOutcome {
    copy_with(&mut ClipboardService, content)
}

pub fn outcome_message(outcome: &ClipboardOutcome, whole_row: bool) -> String {
    match outcome {
        ClipboardOutcome::Confirmed if whole_row => "已复制当前整行。".into(),
        ClipboardOutcome::Confirmed => "已复制当前单元格。".into(),
        ClipboardOutcome::TerminalRequestSent => {
            "已发送终端剪贴板请求；是否生效取决于终端支持。".into()
        }
        ClipboardOutcome::Unsupported => "当前环境不支持系统剪贴板。".into(),
        ClipboardOutcome::Failed(error) => format!("复制失败：{error}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipboardPlatform {
    MacOs,
    Linux,
    Windows,
    Other,
}

impl ClipboardPlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

trait ClipboardCommandRunner {
    fn run(&mut self, program: &str, args: &[&str], content: &str) -> io::Result<bool>;
}

struct ProcessCommandRunner;

impl ClipboardCommandRunner for ProcessCommandRunner {
    fn run(&mut self, program: &str, args: &[&str], content: &str) -> io::Result<bool> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("clipboard process stdin unavailable"))?
            .write_all(content.as_bytes());
        let status = child.wait()?;
        write_result?;
        Ok(status.success())
    }
}

fn copy_native_with(
    platform: ClipboardPlatform,
    runner: &mut dyn ClipboardCommandRunner,
    content: &str,
) -> Option<ClipboardOutcome> {
    const POWERSHELL_STDIN: &str = "[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); Set-Clipboard -Value ([Console]::In.ReadToEnd())";
    let candidates: Vec<(&str, &[&str])> = match platform {
        ClipboardPlatform::MacOs => vec![("pbcopy", &[])],
        ClipboardPlatform::Linux => linux_candidates(
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
            std::env::var_os("DISPLAY").is_some(),
        ),
        ClipboardPlatform::Windows => vec![(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                POWERSHELL_STDIN,
            ],
        )],
        ClipboardPlatform::Other => Vec::new(),
    };
    for (program, args) in candidates {
        match runner.run(program, args, content) {
            Ok(true) => return Some(ClipboardOutcome::Confirmed),
            Ok(false) => {
                return Some(ClipboardOutcome::Failed(format!(
                    "{program} exited unsuccessfully"
                )))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Some(ClipboardOutcome::Failed(format!("{program}: {error}"))),
        }
    }
    None
}

fn linux_candidates(wayland: bool, x11: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut candidates: Vec<(&str, &[&str])> = Vec::new();
    if wayland {
        candidates.push(("wl-copy", &[]));
    }
    if x11 {
        candidates.push(("xclip", &["-selection", "clipboard"]));
        candidates.push(("xsel", &["--clipboard", "--input"]));
    }
    candidates
}

pub fn copy_osc52_to(writer: &mut impl Write, content: &str) -> ClipboardOutcome {
    match execute!(writer, CopyToClipboard::to_clipboard_from(content)) {
        Ok(()) => ClipboardOutcome::TerminalRequestSent,
        Err(error) => ClipboardOutcome::Failed(error.to_string()),
    }
}

pub struct ClipboardService;

impl ClipboardBackend for ClipboardService {
    fn copy(&mut self, content: &str) -> ClipboardOutcome {
        if let Some(outcome) = copy_native_with(
            ClipboardPlatform::current(),
            &mut ProcessCommandRunner,
            content,
        ) {
            return outcome;
        }
        let mut stdout = io::stdout();
        if stdout.is_terminal() {
            copy_osc52_to(&mut stdout, content)
        } else {
            ClipboardOutcome::Unsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeCommandRunner {
        calls: Vec<(String, Vec<String>, String)>,
    }

    impl ClipboardCommandRunner for FakeCommandRunner {
        fn run(&mut self, program: &str, args: &[&str], content: &str) -> io::Result<bool> {
            self.calls.push((
                program.into(),
                args.iter().map(|arg| (*arg).into()).collect(),
                content.into(),
            ));
            Ok(true)
        }
    }

    struct FakeBackend {
        outcome: ClipboardOutcome,
        payloads: Vec<String>,
    }

    impl ClipboardBackend for FakeBackend {
        fn copy(&mut self, content: &str) -> ClipboardOutcome {
            self.payloads.push(content.into());
            self.outcome.clone()
        }
    }

    #[test]
    fn injectable_backend_preserves_payload_and_typed_outcomes() {
        for outcome in [
            ClipboardOutcome::Confirmed,
            ClipboardOutcome::TerminalRequestSent,
            ClipboardOutcome::Unsupported,
            ClipboardOutcome::Failed("failure".into()),
        ] {
            let mut backend = FakeBackend {
                outcome: outcome.clone(),
                payloads: Vec::new(),
            };
            assert_eq!(copy_with(&mut backend, "部门\t张三\n🙂"), outcome);
            assert_eq!(backend.payloads, ["部门\t张三\n🙂"]);
        }
    }

    #[test]
    fn osc52_request_is_never_reported_as_confirmed() {
        let mut output = Vec::new();
        assert_eq!(
            copy_osc52_to(&mut output, "中文🙂"),
            ClipboardOutcome::TerminalRequestSent
        );
        assert!(!output.is_empty());
    }

    #[test]
    fn macos_native_uses_pbcopy_and_stdin_only() {
        let mut runner = FakeCommandRunner::default();
        let result = copy_native_with(
            ClipboardPlatform::MacOs,
            &mut runner,
            "中文\n$(echo unsafe)",
        );
        assert_eq!(result, Some(ClipboardOutcome::Confirmed));
        assert_eq!(
            runner.calls,
            [("pbcopy".into(), Vec::new(), "中文\n$(echo unsafe)".into())]
        );
    }

    #[test]
    fn native_failure_and_missing_program_have_distinct_outcomes() {
        struct Runner(bool);
        impl ClipboardCommandRunner for Runner {
            fn run(&mut self, _: &str, _: &[&str], _: &str) -> io::Result<bool> {
                if self.0 {
                    Ok(false)
                } else {
                    Err(io::Error::from(io::ErrorKind::NotFound))
                }
            }
        }
        assert!(matches!(
            copy_native_with(ClipboardPlatform::MacOs, &mut Runner(true), "x"),
            Some(ClipboardOutcome::Failed(_))
        ));
        assert_eq!(
            copy_native_with(ClipboardPlatform::MacOs, &mut Runner(false), "x"),
            None
        );
        assert_eq!(
            copy_native_with(ClipboardPlatform::Other, &mut Runner(true), "x"),
            None
        );
    }

    #[test]
    fn linux_native_candidates_follow_available_display_servers() {
        assert!(linux_candidates(false, false).is_empty());
        assert_eq!(linux_candidates(true, false)[0].0, "wl-copy");
        assert_eq!(
            linux_candidates(false, true)
                .iter()
                .map(|(program, _)| *program)
                .collect::<Vec<_>>(),
            ["xclip", "xsel"]
        );
        assert_eq!(linux_candidates(true, true).len(), 3);
    }

    #[test]
    fn windows_native_command_reads_content_from_stdin() {
        let mut runner = FakeCommandRunner::default();
        assert_eq!(
            copy_native_with(ClipboardPlatform::Windows, &mut runner, "中文🙂"),
            Some(ClipboardOutcome::Confirmed)
        );
        assert_eq!(runner.calls.len(), 1);
        assert_eq!(runner.calls[0].0, "powershell.exe");
        assert_eq!(runner.calls[0].2, "中文🙂");
        assert!(runner.calls[0].1.last().unwrap().contains("ReadToEnd"));
        assert!(!runner.calls[0].1.iter().any(|arg| arg.contains("中文🙂")));
    }

    #[test]
    fn notices_never_claim_confirmation_for_unconfirmed_outcomes() {
        assert_eq!(
            outcome_message(&ClipboardOutcome::Confirmed, false),
            "已复制当前单元格。"
        );
        assert_eq!(
            outcome_message(&ClipboardOutcome::Confirmed, true),
            "已复制当前整行。"
        );
        assert!(
            outcome_message(&ClipboardOutcome::TerminalRequestSent, false)
                .contains("是否生效取决于终端支持")
        );
        assert!(!outcome_message(&ClipboardOutcome::TerminalRequestSent, false).contains("已复制"));
        assert!(outcome_message(&ClipboardOutcome::Unsupported, false).contains("不支持"));
        assert!(outcome_message(&ClipboardOutcome::Failed("fail".into()), false).contains("fail"));
    }
}
