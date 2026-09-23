use std::process::{Command, Stdio};

use edpcli::common::EXIT_USAGE;
use edpcli::tui::{render, state::AppState};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn tui_fails_closed_without_an_interactive_tty() {
    let output = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .arg("tui")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run edpcli tui");
    assert_eq!(output.status.code(), Some(EXIT_USAGE));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("TTY"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn redraw_handles_small_and_large_terminal_sizes_without_panicking() {
    let state = AppState::new();
    for (width, height) in [(20, 6), (40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render::draw(frame, &state))
            .unwrap_or_else(|error| panic!("{width}x{height}: {error}"));
    }
}

#[test]
fn terminal_lifecycle_has_raii_restore_for_error_and_unwind_paths() {
    let source = include_str!("../src/tui/mod.rs");
    for required in [
        "impl Drop for TerminalSession",
        "disable_raw_mode",
        "LeaveAlternateScreen",
        "Show",
    ] {
        assert!(
            source.contains(required),
            "terminal lifecycle missing restore primitive: {required}"
        );
    }
}

#[test]
fn large_navigation_stays_state_only_and_bounded() {
    let mut state = AppState::new();
    state.set_item_count(100_000);
    for _ in 0..20_000 {
        state.navigate(edpcli::tui::state::NavCommand::Down, 40);
    }
    assert_eq!(state.selected(), 20_000);
    for _ in 0..30_000 {
        state.navigate(edpcli::tui::state::NavCommand::Up, 40);
    }
    assert_eq!(state.selected(), 0);
}

#[test]
fn repository_locks_tui_dependencies_and_ci_checks_locked_tree() {
    let lock = include_str!("../Cargo.lock");
    assert!(lock.contains("name = \"ratatui\""));
    assert!(lock.contains("name = \"crossterm\""));

    let ci = include_str!("../.github/workflows/ci.yml");
    let test_wrapper = include_str!("../scripts/ci/run-cargo-test-ci.py");
    assert!(ci.contains("cargo fmt --all -- --check"));
    assert!(ci.contains("python scripts/ci/run-cargo-test-ci.py"));
    assert!(test_wrapper.contains(r#"COMMAND = ["cargo", "test", "--all-targets", "--locked"]"#));
    assert!(ci.contains("cargo clippy --all-targets --locked -- -D warnings"));
}

#[test]
fn background_workers_convert_panics_into_results_instead_of_hanging_ui() {
    let task = include_str!("../src/tui/task.rs");
    assert!(task.contains("catch_unwind"));
    assert!(task.contains("AssertUnwindSafe"));
}
