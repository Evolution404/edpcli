use std::process::{Command, Stdio};

use edpcli::common::EXIT_USAGE;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, ProvisionStage, Workspace},
};
use ratatui::{backend::TestBackend, style::Color, Terminal};

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
fn device_list_shows_ven_prod_and_onlyid_and_enter_shortcut() {
    let mut state = AppState::new();
    let mut row = usb_device();
    row.device_id = Some("disk&ven_aigo&prod_u335".into());
    row.onlyid = Some("1987718388".into());
    state.replace_devices(vec![row]);
    let backend = TestBackend::new(130, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("ven_prod"), "{text}");
    assert!(text.contains("aigo_u335"), "{text}");
    assert!(text.contains("1987718388"), "{text}");
    assert!(text.replace(' ', "").contains("Enter制盘"), "{text}");
    assert!(
        text.contains("▶"),
        "focused device row must render ▶: {text}"
    );
    assert!(!text.contains("Apply"), "{text}");
}

#[test]
fn transient_notice_has_its_own_area_and_expires() {
    let mut state = AppState::new();
    state.set_notice("批量选择只在备份页可用。");
    let backend = TestBackend::new(130, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let rows = terminal.backend().buffer().content().chunks(130);
    let lines = rows
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    assert!(lines[19].replace(' ', "").contains("批量选择"), "{lines:?}");
    assert!(lines[22].replace(' ', "").contains("页面"), "{lines:?}");
    std::thread::sleep(std::time::Duration::from_millis(4_050));
    assert_eq!(state.notice(), None);
}

#[test]
fn two_tabs_cycle_and_provision_flow_renders_at_all_terminal_sizes() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.workspace(), Workspace::Provision);

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
                    .find(|(x, cell)| {
                        *x > 8
                            && *x < 80
                            && cell.symbol() == "│"
                            && cell.style().bg != Some(Color::Cyan)
                    })
                    .map(|(x, _)| x)
            })
            .expect("internal group separator")
    };
    let identity_separator = internal_separator_x("标签标识");
    assert_eq!(identity_separator, internal_separator_x("部门"));
    let layout_separator = internal_separator_x("启动区容量");
    assert_eq!(layout_separator, internal_separator_x("交换区容量"));
    assert_eq!(layout_separator, internal_separator_x("保密区容量"));
    let format_separator = internal_separator_x("启动区格式化");
    assert_eq!(format_separator, internal_separator_x("交换区格式化"));
    assert_eq!(format_separator, internal_separator_x("保密区格式化"));
    let password_separator = internal_separator_x("初始化密码强制修改");
    assert_eq!(
        password_separator,
        internal_separator_x("交换区密码最大错误次数")
    );
    let distinct = [
        identity_separator,
        layout_separator,
        format_separator,
        password_separator,
    ]
    .into_iter()
    .collect::<std::collections::HashSet<_>>();
    assert!(
        distinct.len() > 1,
        "group separators must be independently aligned"
    );

    let ratio_row = cells
        .chunks(width as usize)
        .find(|row| row.iter().any(|cell| cell.symbol() == "比"))
        .expect("ratio row");
    assert!(ratio_row
        .iter()
        .any(|cell| cell.style().fg == Some(Color::Cyan)));
    assert!(ratio_row
        .iter()
        .any(|cell| cell.style().fg == Some(Color::Green)));
    assert!(ratio_row
        .iter()
        .any(|cell| cell.style().fg == Some(Color::Magenta)));
}

#[test]
fn provision_selection_highlights_only_value_and_long_values_scroll_with_cursor() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    state.provision_begin_selected();
    state.provision_mut().form.label_id = "3164177653".into();
    state.provision_mut().field_selected = 0;
    state.provision_cursor_end();

    let width = 160u16;
    let backend = TestBackend::new(width, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let cells = terminal.backend().buffer().content();
    let row = cells
        .chunks(width as usize)
        .find(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .replace(' ', "")
                .contains("标签标识")
        })
        .expect("label id row");
    let label_cell = row
        .iter()
        .find(|cell| cell.symbol() == "标")
        .expect("label cell");
    assert_ne!(label_cell.style().bg, Some(Color::Cyan));
    let highlighted = row
        .iter()
        .filter(|cell| cell.style().bg == Some(Color::Cyan))
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        highlighted.chars().any(|ch| ch.is_ascii_digit()),
        "selected value should contain highlighted input content: {highlighted}"
    );
    assert!(!highlighted.contains('标'));

    let dept_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "部门")
        .expect("dept field");
    state.provision_mut().field_selected = dept_index;
    state.provision_mut().form.dept =
        "江苏省电力有限公司/南京供电公司/输电运检中心/超长部门名称".into();
    state.provision_cursor_end();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        text.contains('‹'),
        "long active input should scroll from the left: {text}"
    );

    state.provision_cursor_home();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        text.contains('›'),
        "long active input should expose right overflow: {text}"
    );
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
fn advanced_inspect_tree_browser_renders_and_navigates_across_terminal_sizes() {
    use edpcli::application::inspect::{
        AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    };
    use edpcli::inspect::InspectMeta;
    use edpcli::tui::state::{AdvancedInspectPanel, AdvancedInspectSource, AdvancedInspectStage};

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(9)));
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Running
    );

    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }

    let items = [0u64, 7, 12]
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
            decode_error: None,
            fields: Vec::new(),
            notes: Vec::new(),
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
        topology: edpcli::application::inspect_tree::build_inspect_topology(
            &edpcli::inspect_target::InspectDiskContext::new(
                vec![0; edpcli::common::METADATA_IMAGE_LEN],
                None,
                24_025_029,
            ),
        ),
    }));
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Browser
    );

    let initial_rows = state.advanced_inspect_tree_rows();
    assert!(initial_rows.len() > 1);
    assert_eq!(initial_rows[0].id, "device");
    state.advanced_inspect_move_tree(1);
    assert_eq!(state.advanced_inspect().unwrap().tree_selected, 1);

    let collapsed_len = state.advanced_inspect_tree_rows().len();
    state.advanced_inspect_toggle_selected();
    let expanded_len = state.advanced_inspect_tree_rows().len();
    assert!(expanded_len > collapsed_len);

    state.advanced_inspect_shift_panel(false);
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::Overview
    );
    state.advanced_inspect_shift_panel(false);
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::Detail
    );
    state.advanced_inspect_shift_panel(true);
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::Overview
    );
    state.advanced_inspect_shift_panel(false);
    state.advanced_inspect_scroll_detail(10);
    assert_eq!(state.advanced_inspect().unwrap().detail_scroll, 10);

    for (width, height) in [(40, 10), (80, 24), (160, 60)] {
        let backend = TestBackend::new(width, height);
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
        assert!(compact.contains("结构树"), "{text}");
        assert!(compact.contains("节点概览"), "{text}");
        assert!(compact.contains("节点详情"), "{text}");
    }

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
