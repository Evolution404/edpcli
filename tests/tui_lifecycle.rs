use std::process::{Command, Stdio};

use edpcli::common::EXIT_USAGE;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, ProvisionStage, Workspace},
};
use ratatui::{backend::TestBackend, Terminal};

fn usb_device() -> edpcli::disk_scan::Row {
    edpcli::disk_scan::Row {
        disk: 6,
        size: 64_000_000_000,
        vid: "0dd8".into(),
        pid: "2005".into(),
        proto: "USB".into(),
        device_id: None,
        onlyid: None,
        dept: None,
        user: None,
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
    }
}

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
    state.replace_devices(vec![usb_device()]);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);

    state.provision_select_disk();
    state.provision_skip_backup();
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
fn wide_provision_form_uses_two_columns_and_compact_partition_rows() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    state.provision_begin_selected();
    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;

    let width = 160u16;
    let backend = TestBackend::new(width, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let cells = terminal.backend().buffer().content();
    let rows = cells
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    let text = rows.join("\n");
    let compact_text = text.replace(' ', "");
    let compact_rows = rows
        .iter()
        .map(|row| row.replace(' ', ""))
        .collect::<Vec<_>>();

    assert!(compact_text.contains("实时布局"), "{text}");
    assert!(compact_text.contains("最大可设"), "{text}");
    assert!(compact_text.contains("还能增加"), "{text}");
    assert!(
        compact_rows
            .iter()
            .any(|row| row.contains("交换区容量") && row.contains("交换区起点LBA")),
        "{text}"
    );

    let internal_separator_x = |needle: &str| {
        cells
            .chunks(width as usize)
            .find(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .replace(' ', "")
                    .contains(needle)
            })
            .and_then(|row| {
                row.iter()
                    .enumerate()
                    .find(|(x, cell)| *x > 8 && *x < 80 && cell.symbol() == "│")
                    .map(|(x, _)| x)
            })
            .expect("internal group separator")
    };
    let identity_separator = internal_separator_x("标签标识");
    let layout_separator = internal_separator_x("交换区容量");
    let format_separator = internal_separator_x("交换区格式化");
    assert_eq!(identity_separator, layout_separator);
    assert_eq!(layout_separator, format_separator);
}

#[test]
fn empty_secret_field_renders_input_placeholder_instead_of_black_value() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    state.provision_begin_selected();
    state.provision_mut().form.password.clear();

    let backend = TestBackend::new(100, 28);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    let compact = text.replace(' ', "");
    assert!(compact.contains("密码〈请输入〉"), "{text}");
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
    assert_eq!(offline.provision().stage, ProvisionStage::SelectDisk);
    offline.provision_begin_offline();
    assert_eq!(offline.provision().kind, ProvisionKind::Offline);
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
fn advanced_inspect_form_and_result_render_across_terminal_sizes() {
    use edpcli::application::inspect::{
        AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    };
    use edpcli::inspect::InspectMeta;
    use edpcli::tui::state::{AdvancedInspectSource, AdvancedInspectStage};

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(9)));
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Form
    );
    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }

    let items = [7u64, 12, 24_025_028]
        .into_iter()
        .map(|lba| AdvancedInspectItem {
            lba,
            regions: vec![format!("测试区域 LBA{lba}")],
            raw: vec![0x5a; 512],
            raw_sha256: format!("raw-{lba}"),
            raw_nonzero: 512,
            decoded: None,
            decoded_sha256: None,
            method: None,
            meta_text: Some(format!(
                "LBA: {lba}\n区域:\n  - 测试区域\n物理数据状态: 测试\ndecode 策略: fail-closed\n"
            )),
        })
        .collect();
    state.advanced_inspect_finish(Ok(AdvancedInspectWorkspace {
        source: "测试物理盘 disk9".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items,
        export_dir: None,
    }));
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Result
    );
    state.advanced_inspect_move_result(1);
    state.advanced_inspect_scroll(10);
    assert_eq!(state.advanced_inspect().unwrap().selected, 1);
    assert_eq!(state.advanced_inspect().unwrap().scroll, 10);

    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }

    state.advanced_inspect_back_to_form();
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Form
    );
    state.close_advanced_inspect();
    assert!(state.advanced_inspect().is_none());
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
