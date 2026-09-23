use std::process::{Command, Stdio};

use edpcli::common::EXIT_USAGE;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, ProvisionStage, Workspace},
};
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
fn three_workspaces_cycle_and_new_overlays_render_at_all_terminal_sizes() {
    let mut state = AppState::new();
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);

    state.provision_begin_selected();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }

    state.provision_reset();
    state.navigate(NavCommand::WorkspaceBackups, 20);
    assert!(state.begin_backup_prune());
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }
}

#[test]
fn apply_and_offline_convert_states_render_and_enforce_preview_before_write() {
    use edpcli::application::WriteEvent;
    use edpcli::tui::state::{ApplyStage, ExpectedIdentity, OfflineConvertView, ProvisionKind};

    let mut apply = AppState::new();
    assert!(apply.begin_apply(
        7,
        ExpectedIdentity {
            onlyid: None,
            device_id: None,
        },
    ));
    assert_eq!(apply.apply().unwrap().stage, ApplyStage::Setup);
    apply.apply_start_preview();
    apply.apply_finish_preview(Ok(vec![WriteEvent::DryRunPreview {
        disk: 7,
        needs_force: false,
    }]));
    assert_eq!(apply.apply().unwrap().stage, ApplyStage::Review);
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &apply)).unwrap();
    }
    apply.apply_begin_confirm();
    assert_eq!(apply.apply().unwrap().stage, ApplyStage::Confirm);
    assert!(apply.apply_take_for_write().is_none(), "YES is mandatory");
    for ch in ['Y', 'E', 'S'] {
        apply.apply_push_char(ch);
    }
    assert!(apply.apply_take_for_write().is_some());
    assert_eq!(apply.apply().unwrap().stage, ApplyStage::Running);
    apply.apply_finish_write(Ok(()));
    assert_eq!(apply.apply().unwrap().stage, ApplyStage::Result);

    let mut force_gate = AppState::new();
    assert!(force_gate.begin_apply(
        8,
        ExpectedIdentity {
            onlyid: None,
            device_id: None,
        },
    ));
    force_gate.apply_start_preview();
    force_gate.apply_finish_preview(Ok(vec![WriteEvent::DryRunPreview {
        disk: 8,
        needs_force: true,
    }]));
    force_gate.apply_begin_confirm();
    assert_eq!(
        force_gate.apply().unwrap().stage,
        ApplyStage::Review,
        "needs_force preview must block confirmation when force is off"
    );
    assert!(force_gate
        .apply()
        .unwrap()
        .message
        .as_deref()
        .is_some_and(|message| message.contains("force")));
    force_gate.apply_back_to_setup();
    force_gate.apply_toggle_force();
    force_gate.apply_start_preview();
    force_gate.apply_finish_preview(Ok(vec![WriteEvent::DryRunPreview {
        disk: 8,
        needs_force: true,
    }]));
    force_gate.apply_begin_confirm();
    assert_eq!(force_gate.apply().unwrap().stage, ApplyStage::Confirm);

    let mut offline = AppState::new();
    offline.navigate(NavCommand::WorkspaceProvision, 20);
    offline.navigate(NavCommand::Bottom, 20);
    assert_eq!(
        ProvisionKind::ALL[offline.selected()],
        ProvisionKind::Offline,
        "offline tool must remain the sixth provision entry"
    );
    assert_eq!(offline.provision_begin_selected(), ProvisionKind::Offline);
    assert_eq!(offline.provision().stage, ProvisionStage::OfflineForm);
    assert!(
        offline.selected_device_disk().is_none(),
        "offline conversion must not require a physical disk"
    );
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render::draw(frame, &offline))
            .unwrap();
    }

    offline.offline_finish(Ok(OfflineConvertView {
        reports: vec![edpcli::sectors::ConvertReport::SectorPlan {
            share: 100,
            clears_lba9: true,
        }],
        share: 100,
        enc_start: 200,
        enc_size: 300,
        crc: 0x1234_5678,
        k0: 0x9abc,
        output_dir: None,
    }));
    assert_eq!(offline.provision().stage, ProvisionStage::OfflineResult);
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render::draw(frame, &offline))
            .unwrap();
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
