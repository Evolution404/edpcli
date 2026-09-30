use std::process::{Command, Stdio};

use edpcli::common::EXIT_USAGE;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand, ProvisionStage, Workspace, WriteKind},
};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

fn usb_device() -> edpcli::disk_scan::Row {
    let mut row = edpcli::disk_scan::Row {
        disk: 6,
        size: 64_000_000_000,
        vid: "0dd8".into(),
        pid: "2005".into(),
        proto: "USB".into(),
        serial: None,
        hardware_model: None,
        device_id: None,
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
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };
    crate::common::confirm_row_identity(&mut row);
    row
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
fn device_list_shows_model_default_capacity_and_help_only_hint() {
    let mut state = AppState::new();
    let mut row = usb_device();
    row.device_id = Some("disk&ven_aigo&prod_u335".into());
    row.onlyid = Some("1987718388".into());
    state.replace_devices(vec![row]);
    let backend = TestBackend::new(180, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.replace(' ', "").contains("型号"), "{text}");
    assert!(text.contains("aigo_u335"), "{text}");
    assert!(
        text.replace(' ', "").contains("无法建立可靠容量布局"),
        "default device detail must be capacity layout: {text}"
    );
    assert!(text.replace(' ', "").contains("?帮助"), "{text}");
    assert!(!text.replace(' ', "").contains("Enter设备信息"), "{text}");
    assert!(!text.replace(' ', "").contains("当前设备·"), "{text}");
    assert!(
        text.contains("▌"),
        "focused device row must render ▌: {text}"
    );
    assert!(!text.contains("Apply"), "{text}");
}

#[test]
fn active_department_column_expands_fully_without_ellipsis_and_sort_keeps_disk_selection() {
    use edpcli::tui::table_layout::TableKind;

    let mut state = AppState::new();
    let mut first = usb_device();
    first.disk = 4;
    first.dept = Some("江苏省电力有限公司/南京供电公司".into());
    crate::common::confirm_row_identity(&mut first);

    let mut second = usb_device();
    second.disk = 5;
    second.dept = Some("国网南京供电公司".into());
    crate::common::confirm_row_identity(&mut second);

    state.replace_devices(vec![first, second]);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(5));

    assert!(state.move_table_column(TableKind::Devices, false)); // 容量
    assert!(state.move_table_column(TableKind::Devices, false)); // 部门
    assert_eq!(state.table_active_column(TableKind::Devices), 2);

    let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .chunks(140)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join(
            "
",
        );

    let compact = text.replace(' ', "");
    assert!(
        compact.contains("江苏省电力有限公司/南京供电公司"),
        "{text}"
    );
    assert!(
        !compact.contains("江苏省电力有限公司/南京供电公…"),
        "{text}"
    );
    assert!(!compact.contains("h/l激活"), "{text}");
    assert!(!compact.contains("H/L视口"), "{text}");
    assert!(compact.contains("?帮助"), "{text}");

    state.toggle_table_sort(TableKind::Devices);
    assert_eq!(state.selected_device_disk(), Some(5));
    state.toggle_table_sort(TableKind::Devices);
    assert_eq!(state.selected_device_disk(), Some(5));
    assert!(state.clear_table_sort(TableKind::Devices));
    assert_eq!(state.selected_device_disk(), Some(5));
}

#[test]
fn table_column_reorder_moves_whole_column_without_changing_h_l_or_sort_identity() {
    use edpcli::tui::table_layout::TableKind;

    let mut state = AppState::new();
    let mut row = usb_device();
    row.dept = Some("江苏省电力有限公司/南京供电公司".into());
    crate::common::confirm_row_identity(&mut row);
    state.replace_devices(vec![row]);

    let original = state.table_column_order(TableKind::Devices);
    assert_eq!(original, (0..11).collect::<Vec<_>>());

    // h/l keeps its existing meaning: change active column only.
    assert!(state.move_table_column(TableKind::Devices, false)); // 容量
    assert!(state.move_table_column(TableKind::Devices, false)); // 部门
    assert_eq!(state.table_active_column(TableKind::Devices), 2);
    assert_eq!(state.table_column_order(TableKind::Devices), original);

    // Sort binds to the logical Department column (2).
    state.toggle_table_sort(TableKind::Devices);
    assert_eq!(state.table_sort(TableKind::Devices).unwrap().column, 2);

    // '<' semantics: move the whole active Department column left.
    assert!(state.reorder_table_column_for_viewport(TableKind::Devices, true, 140, 24));
    assert_eq!(state.table_active_column(TableKind::Devices), 1);
    assert_eq!(
        state.table_column_order(TableKind::Devices),
        vec![0, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(
        state.table_sort(TableKind::Devices).unwrap().column,
        2,
        "sort must travel with the logical Department column"
    );
    assert_eq!(state.table_logical_column(TableKind::Devices, 1), 2);

    // '>' moves the same whole column back to its original place.
    assert!(state.reorder_table_column_for_viewport(TableKind::Devices, false, 140, 24));
    assert_eq!(state.table_active_column(TableKind::Devices), 2);
    assert_eq!(state.table_column_order(TableKind::Devices), original);
    assert_eq!(state.table_sort(TableKind::Devices).unwrap().column, 2);
}

#[test]
fn permanent_message_bar_keeps_content_geometry_stable_when_notice_disappears() {
    let mut state = AppState::new();
    state.set_notice("批量选择只在备份页可用。");
    let render_lines = |state: &AppState| {
        let backend = TestBackend::new(130, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render::draw(frame, state)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(130)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
    };
    let lines = render_lines(&state);
    assert!(
        lines.last().unwrap().replace(' ', "").contains("批量选择"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.replace(' ', "").contains("?帮助")),
        "{lines:?}"
    );
    state.clear_notice();
    let idle = render_lines(&state);
    assert!(idle.last().unwrap().replace(' ', "").contains("就绪"));
    assert_eq!(
        &lines[..23],
        &idle[..23],
        "notice presence must only change the permanent bottom message row"
    );
}

#[test]
fn two_top_level_tabs_and_nested_provision_render_at_all_terminal_sizes() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    assert_eq!(
        Workspace::TOP_LEVEL,
        [Workspace::Devices, Workspace::Backups]
    );
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(state.provision_scheme_picker_open());

    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    assert_eq!(state.workspace(), Workspace::Provision);
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
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;

    let width = 160u16;
    let backend = TestBackend::new(width, 60);
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

    assert!(compact_text.contains("磁盘布局"), "{text}");
    assert!(compact_text.contains("EDP主协议区"), "{text}");
    assert!(
        text.contains("尾") && text.contains("区") && text.contains("域"),
        "{text}"
    );
    assert!(
        compact_rows
            .iter()
            .any(|row| row.contains("交换区容量") && row.contains("交换区起点LBA")),
        "{text}"
    );

    let palette = edpcli::tui::theme::current().palette();
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
                            && cell.style().bg != Some(palette.selection)
                    })
                    .map(|(x, _)| x)
            })
            .expect("internal group separator")
    };
    let identity_separator = internal_separator_x("标签标识");
    assert_eq!(identity_separator, internal_separator_x("部门"));
    let key_domain_separator = internal_separator_x("交换区来源密码");
    assert_eq!(key_domain_separator, internal_separator_x("交换区目标密码"));
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
        key_domain_separator,
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

    use edpcli::tui::disk_layout::DiskRegionKind;
    let theme = edpcli::tui::theme::current();
    let boot_bg = theme.disk_region_fill(DiskRegionKind::Boot, false).bg;
    let share_bg = theme.disk_region_fill(DiskRegionKind::Share, false).bg;
    let encrypt_bg = theme.disk_region_fill(DiskRegionKind::Encrypt, false).bg;
    let capacity_row = cells
        .chunks(width as usize)
        .find(|row| {
            row.iter().any(|cell| cell.style().bg == share_bg)
                && row.iter().any(|cell| cell.style().bg == encrypt_bg)
        })
        .expect("shared compact capacity-map row");
    assert!(capacity_row.iter().any(|cell| cell.style().bg == boot_bg));
    assert!(capacity_row.iter().any(|cell| cell.style().bg == share_bg));
    assert!(capacity_row
        .iter()
        .any(|cell| cell.style().bg == encrypt_bg));
}

#[test]
fn provision_selection_highlights_only_value_and_long_values_scroll_with_cursor() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
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
    let selection = edpcli::tui::theme::current().palette().selection;
    let label_cell = row
        .iter()
        .find(|cell| cell.symbol() == "标")
        .expect("label cell");
    assert_ne!(label_cell.style().bg, Some(selection));
    let highlighted = row
        .iter()
        .filter(|cell| cell.style().bg == Some(selection))
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
        text.contains("[NORMAL]"),
        "normal mode badge missing: {text}"
    );
    assert!(
        !text.contains('‹') && !text.contains('›'),
        "Normal selection must not show input overflow markers: {text}"
    );

    assert!(state.provision_begin_insert());
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
        text.contains("[INSERT]"),
        "insert mode badge missing: {text}"
    );
    assert!(
        text.contains('‹'),
        "long Insert input should scroll from the left: {text}"
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
        "long Insert input should expose right overflow: {text}"
    );
}

#[test]
fn empty_secret_field_renders_input_placeholder_instead_of_black_value() {
    let mut state = AppState::new();
    state.replace_devices(vec![usb_device()]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    state.provision_mut().form.share_target_password.clear();

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
            parse_state: edpcli::inspect::InspectParseState::Parsed,
            diagnostics: Vec::new(),
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
            &crate::common::edp_inspect_context(24_025_029),
        ),
        disk_layout: None,
        disk_layout_issue: None,
    }));
    assert_eq!(
        state.advanced_inspect().unwrap().stage,
        AdvancedInspectStage::Browser
    );

    state.advanced_inspect_focus_pane(edpcli::tui::pane::PaneId::InspectTree);
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let text = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    let palette = edpcli::tui::theme::current().palette();
    let selection = palette.selection;
    let active_tab = buffer
        .content()
        .iter()
        .filter(|cell| {
            cell.style().fg == Some(palette.accent)
                && cell.style().add_modifier.contains(Modifier::UNDERLINED)
        })
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        active_tab.replace(' ', "").contains("1业务字段"),
        "active Inspect panel tab should use active-tab style: {active_tab}"
    );
    let pane_title_markers = buffer
        .content()
        .iter()
        .filter(|cell| {
            cell.symbol() == "▌"
                && cell.style().fg == Some(palette.accent)
                && cell.style().bg != Some(selection)
                && cell.style().add_modifier.contains(Modifier::BOLD)
        })
        .count();
    assert_eq!(
        pane_title_markers, 1,
        "focused tree pane title should carry one accent/bold pane marker: {text}"
    );
    let focused_tree_rows = buffer
        .content()
        .chunks(120)
        .filter(|row| {
            row.iter()
                .any(|cell| cell.symbol() == "▌" && cell.style().bg == Some(selection))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        focused_tree_rows.len(),
        1,
        "exactly one tree row should carry the focused selection marker"
    );
    let focus_row = focused_tree_rows[0];
    let last_highlighted = focus_row
        .iter()
        .rposition(|cell| cell.style().bg == Some(selection))
        .expect("selected content cells");
    let border = focus_row
        .iter()
        .enumerate()
        .skip(last_highlighted + 1)
        .find(|(_, cell)| cell.symbol() == "│")
        .map(|(x, _)| x)
        .expect("tree right border");
    assert!(
        border > last_highlighted + 1,
        "selection should not extend to the panel border: highlighted={last_highlighted} border={border} row={}",
        focus_row.iter().map(|cell| cell.symbol()).collect::<String>()
    );
    assert!(
        focus_row[last_highlighted + 1..border]
            .iter()
            .all(|cell| cell.style().bg != Some(selection)),
        "selected background must not paint unrelated layout padding"
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

    state.advanced_inspect_focus_pane(edpcli::tui::pane::PaneId::InspectTree);
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
    state.advanced_inspect_move_focused_vertical(10, 1, 100);
    assert_eq!(
        state
            .pane_viewport(edpcli::tui::pane::PaneId::InspectDetail)
            .scroll_y
            .offset,
        10
    );

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
        assert!(compact.contains("字段详情"), "{text}");
        if width >= 80 {
            assert!(compact.contains("结构树"), "{text}");
            assert!(compact.contains("对象快照"), "{text}");
            assert!(compact.contains("磁盘概览"), "{text}");
        }
        let active_tab = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| {
                cell.style().fg == Some(palette.accent)
                    && cell.style().add_modifier.contains(Modifier::UNDERLINED)
            })
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(
            state.advanced_inspect().unwrap().panel,
            AdvancedInspectPanel::Detail
        );
        if width >= 80 {
            assert!(
                active_tab.replace(' ', "").contains("2原始字段"),
                "Detail focus must be visible in the shared Inspect tabs: {active_tab}"
            );
        } else {
            assert!(
                !active_tab.trim().is_empty(),
                "narrow Inspect tabs must still expose the active-tab style"
            );
        }
    }

    state.advanced_inspect_shift_panel(true);
    state.advanced_inspect_shift_panel(true);
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::Tree
    );
    let selected_before = state.advanced_inspect().unwrap().tree_selected;
    let selected_label = state.advanced_inspect_tree_rows()[selected_before]
        .label
        .clone();
    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).expect("narrow test terminal");
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        text.replace(' ', "")
            .contains(&selected_label.replace(' ', "")),
        "narrow layout lost selected tree node {selected_label}: {text}"
    );
    let narrow_buffer = terminal.backend().buffer();
    let narrow_pane_markers = narrow_buffer
        .content()
        .iter()
        .filter(|cell| {
            cell.symbol() == "▌"
                && cell.style().fg == Some(palette.accent)
                && cell.style().bg != Some(selection)
                && cell.style().add_modifier.contains(Modifier::BOLD)
        })
        .count();
    assert_eq!(narrow_pane_markers, 1, "{text}");
    let narrow_row_markers = narrow_buffer
        .content()
        .chunks(60)
        .filter(|row| {
            row.iter()
                .any(|cell| cell.symbol() == "▌" && cell.style().bg == Some(selection))
        })
        .count();
    assert_eq!(narrow_row_markers, 1, "{text}");
    assert_eq!(
        state.advanced_inspect().unwrap().tree_selected,
        selected_before,
        "narrow rendering must not mutate selection"
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
    let full_runner = include_str!("../scripts/test-full.py");
    assert!(ci.contains("cargo fmt --all -- --check"));
    assert!(ci.contains("python scripts/test-full.py --profile full"));
    assert!(full_runner.contains("--message-format=json"));
    assert!(full_runner.contains("ThreadPoolExecutor"));
    assert!(!full_runner.contains("cargo test --all-targets"));
    assert!(ci.contains("cargo clippy --all-targets --locked -- -D warnings"));
}

#[test]
fn background_workers_convert_panics_into_results_instead_of_hanging_ui() {
    let task = include_str!("../src/tui/task.rs");
    assert!(task.contains("catch_unwind"));
    assert!(task.contains("AssertUnwindSafe"));
}

#[test]
fn restore_workspace_has_visual_hierarchy_and_inline_post_restore_action() {
    use edpcli::application::post_restore::{
        MetadataRestoreOutcome, MetadataRestoreReport, PostRestoreAssessment, PostRestorePartition,
        PostRestorePartitionState,
    };
    use edpcli::edpb::ManifestPartition;
    use ratatui::style::Color;

    fn rendered_text(state: &AppState) -> (String, Vec<Color>) {
        let width = 180u16;
        let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
        terminal.draw(|frame| render::draw(frame, state)).unwrap();
        let cells = terminal.backend().buffer().content();
        (
            cells.iter().map(|cell| cell.symbol()).collect::<String>(),
            cells
                .iter()
                .filter(|cell| !cell.symbol().trim().is_empty())
                .map(|cell| cell.fg)
                .collect(),
        )
    }

    let mut state = AppState::new();
    state.begin_write_wizard(
        WriteKind::Restore,
        4,
        Some(
            "/Users/test/.edpcli-backup/disk5_245760000_vid2bdf_pid0300_plain_20260929_073104.edpb"
                .into(),
        ),
    );

    let (review, review_colors) = rendered_text(&state);
    let compact = review.replace(' ', "");
    assert!(compact.contains("恢复写入确认"), "{review}");
    assert!(compact.contains("确认后将直接开始向disk4写入"), "{review}");
    assert!(compact.contains("匹配度未知"), "{review}");
    assert!(compact.contains("备份身份详情未加载"), "{review}");
    assert!(compact.contains("当前设备disk4"), "{review}");
    assert!(compact.contains("备份"), "{review}");
    assert!(compact.contains("/Users/test/.edpcli-backup"), "{review}");
    assert!(!compact.contains("写入前"), "{review}");
    assert!(!compact.contains("写入链"), "{review}");
    assert!(compact.contains("输入YES确认写入"), "{review}");
    assert!(
        review_colors
            .iter()
            .copied()
            .fold(Vec::new(), |mut unique, color| {
                if !unique.contains(&color) {
                    unique.push(color);
                }
                unique
            })
            .len()
            >= 4,
        "restore confirmation modal must use semantic color hierarchy"
    );

    for ch in ['Y', 'E', 'S'] {
        state.push_wizard_confirmation(ch);
    }
    let _ = state.submit_wizard_confirmation();
    state.finish_restore(Ok(MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.partition_table.mbr".into()],
        },
        assessment: PostRestoreAssessment {
            partitions: vec![PostRestorePartition {
                index: 1,
                role: Some("plain".into()),
                start_lba: 2_048,
                sector_count: 245_757_952,
                filesystem_hint: Some("exfat".into()),
                detected_filesystem: None,
                requires_original_key: false,
                state: PostRestorePartitionState::NeedsFormat,
                detail: "文件系统引导区无效".into(),
            }],
            issues: Vec::new(),
        },
        partitions: vec![ManifestPartition {
            index: 1,
            role: Some("plain".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba: 2_048,
            sector_count: 245_757_952,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: Some("普通卷".into()),
        }],
        device_state: "plain".into(),
        device_id: String::new(),
        total_sectors: 245_760_000,
        format_target_pin: None,
    }));

    let (post_restore, colors) = rendered_text(&state);
    let compact = post_restore.replace(' ', "");
    assert!(compact.contains("元数据恢复成功✓"), "{post_restore}");
    assert!(compact.contains("恢复后分区状态"), "{post_restore}");
    assert!(compact.contains("需要格式化"), "{post_restore}");
    assert!(compact.contains("Enter处理选中分区"), "{post_restore}");
    assert!(!compact.contains("请使用CLI"), "{post_restore}");
    assert!(
        colors
            .iter()
            .copied()
            .fold(Vec::new(), |mut unique, color| {
                if !unique.contains(&color) {
                    unique.push(color);
                }
                unique
            })
            .len()
            >= 4,
        "post-restore screen must retain semantic color hierarchy"
    );

    state.begin_selected_post_restore_action();
    let (label_view, _) = rendered_text(&state);
    let compact_label = label_view.replace(' ', "");
    assert!(compact_label.contains("恢复后的卷标"), "{label_view}");
    assert!(compact_label.contains("普通卷"), "{label_view}");
    assert!(compact_label.contains("备份中的原卷标"), "{label_view}");
    assert!(
        compact_label.contains("不会自动生成占位名称"),
        "{label_view}"
    );

    let mut password_state = AppState::new();
    password_state.begin_write_wizard(WriteKind::Restore, 4, Some("edp.edpb".into()));
    for ch in ['Y', 'E', 'S'] {
        password_state.push_wizard_confirmation(ch);
    }
    let _ = password_state.submit_wizard_confirmation();
    let mut encrypted = state
        .wizard()
        .unwrap()
        .restore_outcome
        .as_ref()
        .unwrap()
        .clone();
    encrypted.device_state = "edp".into();
    encrypted.device_id = "disk&ven_test&prod_edp".into();
    encrypted.partitions[0].role = Some("encrypt".into());
    encrypted.assessment.partitions[0].role = Some("encrypt".into());
    encrypted.assessment.partitions[0].requires_original_key = true;
    encrypted.assessment.partitions[0].state = PostRestorePartitionState::PasswordRequired;
    password_state.finish_restore(Ok(encrypted));
    password_state.begin_selected_post_restore_action();
    for ch in "visible-secret".chars() {
        password_state.push_wizard_secret_char(ch);
    }
    let (password_view, _) = rendered_text(&password_state);
    assert!(!password_view.contains("visible-secret"), "{password_view}");
    assert!(
        password_view.matches('•').count() >= "visible-secret".chars().count(),
        "{password_view}"
    );
}
