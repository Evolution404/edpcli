use edpcli::application::inspect::{
    AbsoluteByteRange, AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    InspectField, InspectFieldStatus, InspectFieldType,
};
use edpcli::inspect::{FieldChild, FieldStyle, InspectFieldKey, InspectMeta, InspectParseState};
use edpcli::tui::{
    render,
    state::{AdvancedInspectSource, AppState},
};
use ratatui::{backend::TestBackend, Terminal};

fn rendered_lines(state: &AppState, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, state)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn lba8_state() -> AppState {
    let children = (0..17)
        .map(|index| FieldChild {
            label: match index {
                0 => "Dept".into(),
                1 => "User".into(),
                _ => format!("字段 {}", index + 1),
            },
            value: match index {
                0 => "输电运检中心".into(),
                1 => "测试用户".into(),
                _ => format!("value-{index}"),
            },
            relative_range: None,
        })
        .collect();
    let sector = AdvancedInspectItem {
        lba: 8,
        regions: vec!["protocol".into()],
        raw: vec![0; 512],
        raw_sha256: "test-raw".into(),
        raw_nonzero: 0,
        decoded: None,
        decoded_sha256: None,
        method: None,
        decode_error: None,
        parse_state: InspectParseState::Parsed,
        diagnostics: Vec::new(),
        fields: vec![InspectField {
            key: InspectFieldKey::Lba8Elabel,
            range: AbsoluteByteRange {
                start: 8 * 512,
                end_exclusive: 8 * 512 + 256,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![0; 256],
            decoded: vec![0; 256],
            field_logical: None,
            transform: None,
            status: InspectFieldStatus::Known,
            label: "renamed raw field".into(),
            value: "verified".into(),
            style: FieldStyle::Identity,
            group: Some("LBA8".into()),
            children,
        }],
        notes: Vec::new(),
        meta_text: None,
    };
    let context = crate::common::edp_inspect_context(16_384);
    let workspace = AdvancedInspectWorkspace {
        source: "chapter-16-fixture".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: vec![sector],
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
        disk_layout: None,
        disk_layout_issue: None,
    };
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace));
    state.advanced_inspect_jump_lba(8).unwrap();
    state
}

fn device() -> edpcli::disk_scan::Row {
    let mut row = edpcli::disk_scan::Row {
        disk: 6,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        serial: None,
        device_id: Some("disk&ven_demo&prod_u335".into()),
        identity_pin: None,
        onlyid: Some("ABCDEF0123456789".into()),
        dept: Some("输电运检中心".into()),
        user: Some("张三".into()),
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 3,
        n_possible_baks: 1,
        denied: false,
        probe_error: None,
        provision_kind: edpcli::provision::DiskProvisionKind::Mode0,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };
    crate::common::confirm_row_identity(&mut row);
    row
}

fn provision_state() -> AppState {
    use edpcli::tui::state::NavCommand;
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    state.provision_begin_selected();
    state
}

#[test]
fn ch16_provision_has_shared_stepper_and_card_surfaces() {
    let state = provision_state();
    let text = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    for value in [
        "选择设备",
        "制盘配置",
        "分区预览",
        "计划确认",
        "执行",
        "完成",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    assert!(text.contains("固定目标"));
}

#[test]
fn ch16_provision_running_separates_progress_phase_step_log_and_safety() {
    use edpcli::application::progress::{Phase, ProgressEvent, Step};
    use edpcli::tui::state::ProvisionStage;
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Running;
    state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_running();
    let now = std::time::Instant::now();
    state.provision_mut().run = Some(edpcli::tui::state::ProvisionRunState {
        started_at: now,
        last_activity_at: now,
        latest: None,
        log: std::collections::VecDeque::new(),
    });
    state.provision_push_progress(ProgressEvent::new(
        Phase::Transaction,
        Step::ProtocolReadback,
        7,
        10,
    ));
    let text = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    for value in [
        "总体进度",
        "70%",
        "当前阶段",
        "事务写入",
        "当前步骤",
        "协议读回校验",
        "运行日志",
        "安全提示",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    for (width, height) in [(40, 10), (80, 24), (120, 36), (240, 60)] {
        let compact = rendered_lines(&state, width, height)
            .join("\n")
            .replace(' ', "");
        assert!(
            compact.contains("安全事务"),
            "running fallback missing at {width}x{height}"
        );
    }
}

#[test]
fn ch16_provision_result_uses_typed_outcome_badges() {
    use edpcli::application::provision::ProvisionExecutionStatus as Status;
    use edpcli::tui::state::ProvisionStage;
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Result;
    for (status, label) in [
        (Status::Success, "[制盘成功]"),
        (Status::CompletedWithWarnings, "[制盘完成，存在警告]"),
        (Status::PartialFormatFailure, "[部分完成：格式化失败]"),
        (Status::FatalFailure, "[制盘失败]"),
    ] {
        state.provision_mut().result_status = Some(status);
        let text = rendered_lines(&state, 120, 36).join("\n").replace(' ', "");
        assert!(text.contains(label), "missing {label}");
    }
}

#[test]
fn ch16_shell_exposes_four_top_level_workspaces() {
    let lines = rendered_lines(&AppState::new(), 120, 36);
    let navigation = lines
        .iter()
        .take(6)
        .cloned()
        .collect::<String>()
        .replace(' ', "");
    for title in ["设备", "Inspect", "制盘", "备份"] {
        assert!(
            navigation.contains(title),
            "missing {title} in {navigation}"
        );
    }
}

#[test]
fn ch16_inspect_is_a_workspace_with_a_real_return_target() {
    use edpcli::tui::state::{NavCommand, Workspace};

    let mut state = AppState::new();
    assert_eq!(
        Workspace::ALL,
        [
            Workspace::Devices,
            Workspace::Inspect,
            Workspace::Provision,
            Workspace::Backups
        ]
    );
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    assert_eq!(state.workspace(), Workspace::Inspect);
    let lines = rendered_lines(&state, 120, 36).join("\n").replace(' ', "");
    assert!(lines.contains("Inspect"));
    state.advanced_inspect_finish(Err("test".into()));
    state.navigate(NavCommand::Escape, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
}

#[test]
fn ch16_devices_and_backups_have_independent_pane_focus_and_viewports() {
    use edpcli::tui::pane::PaneId;

    let mut state = AppState::new();
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
    state.focus_devices_pane(PaneId::DevicesSummary);
    state
        .pane_viewport_mut(PaneId::DevicesSummary)
        .scroll_y
        .offset = 7;
    state.focus_backups_pane(PaneId::BackupCoverage);
    state.pane_viewport_mut(PaneId::BackupCoverage).scroll_x = 3;
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesSummary);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupCoverage);
    assert_eq!(
        state.pane_viewport(PaneId::DevicesSummary).scroll_y.offset,
        7
    );
    assert_eq!(state.pane_viewport(PaneId::BackupCoverage).scroll_x, 3);
}

#[test]
fn ch16_devices_wide_shows_current_identity_stats_and_no_animation_sidebar() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    let text = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    for value in [
        "设备列表",
        "当前设备·disk6",
        "张三",
        "输电运检中心",
        "容量布局",
        "设备状态",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    assert!(!text.contains("EDPCORE·LIVE"));
}

#[test]
fn ch16_devices_compact_enter_opens_detail_and_escape_returns_to_list() {
    use edpcli::tui::{pane::PaneId, state::NavCommand};

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(state.activate_device_for_viewport(40).unwrap(), None);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesSummary);
    let text = rendered_lines(&state, 40, 10).join("\n").replace(' ', "");
    for value in ["当前设备·disk6", "身份信息", "onlyid"] {
        assert!(text.contains(value), "missing {value} at 40x10");
    }
    state.navigate(NavCommand::Escape, 7);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
}

#[test]
fn ch16_device_secondary_pane_remains_reachable_at_standard_width() {
    use edpcli::tui::pane::PaneId;

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.focus_devices_pane(PaneId::DevicesStats);
    let text = rendered_lines(&state, 100, 30).join("\n").replace(' ', "");
    assert!(text.contains("设备状态"));
    assert!(text.contains("总设备1"));
}

#[test]
fn ch16_lba8_first_screen_shows_verified_department_user_and_label() {
    let lines = rendered_lines(&lba8_state(), 160, 45);
    let first_screen = lines.join("\n").replace(' ', "");
    for value in ["部门", "输电运检中心", "用户", "测试用户", "E_LABEL", "17"] {
        assert!(
            first_screen.contains(value),
            "missing {value} in LBA8 first screen"
        );
    }
    assert!(!first_screen.contains("EDPCORE·LIVE"));
}

#[test]
fn ch16_enter_views_lba8_without_toggling_tree_expansion() {
    let mut state = lba8_state();
    let before = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert!(before[selected].expandable);
    let expanded = before[selected].expanded;
    state.advanced_inspect_enter_selected();
    let after = state.advanced_inspect_tree_rows();
    assert_eq!(after[selected].expanded, expanded);
}

#[test]
fn ch16_elabel_o_expands_all_seventeen_typed_children_and_enter_does_not() {
    use edpcli::tui::pane::PaneId;

    let mut state = lba8_state();
    state.advanced_inspect_toggle_selected(); // LBA8 -> its fields
    let rows = state.advanced_inspect_tree_rows();
    let field_index = rows
        .iter()
        .position(|row| row.label == "E_LABEL [17]")
        .expect("typed E_LABEL tree node");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(field_index as isize - current as isize);
    let rows = state.advanced_inspect_tree_rows();
    let field_id = rows[field_index].id.clone();
    assert!(rows[field_index].expandable);
    state.advanced_inspect_enter_selected();
    assert!(!state.advanced_inspect_tree_rows()[field_index].expanded);
    state.advanced_inspect_focus_pane(PaneId::InspectTree);
    state.advanced_inspect_toggle_selected(); // o on E_LABEL
    let expanded = state.advanced_inspect_tree_rows();
    assert_eq!(
        expanded
            .iter()
            .filter(|row| row.id.starts_with(&format!("{field_id}/child.")))
            .count(),
        17
    );
    assert!(expanded
        .iter()
        .any(|row| row.label.contains("部门") && row.label.contains("输电运检中心")));
    assert!(expanded
        .iter()
        .any(|row| row.label.contains("用户") && row.label.contains("测试用户")));
    assert!(state.advanced_inspect_view_selected_field());
    assert_eq!(state.advanced_inspect_detail_rows().len(), 1);
    state.advanced_inspect_detail_toggle_selected(); // o in Fields Pane
    assert_eq!(state.advanced_inspect_detail_rows().len(), 18);
}

#[test]
fn ch16_inspect_defaults_to_compact_layout_strip_and_object_snapshot() {
    let state = lba8_state();
    let text = rendered_lines(&state, 120, 36).join("\n").replace(' ', "");
    assert!(text.contains("磁盘概览"));
    assert!(text.contains("对象快照"));
    assert!(text.contains("部门输电运检中心"));
    assert!(
        !text.contains("LBA范围"),
        "full disk layout displaced the snapshot"
    );
}

#[test]
fn ch16_inspect_field_evidence_keeps_every_typed_layer() {
    let mut state = lba8_state();
    state.advanced_inspect_toggle_selected();
    let rows = state.advanced_inspect_tree_rows();
    let field_index = rows
        .iter()
        .position(|row| row.label == "E_LABEL [17]")
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(field_index as isize - current as isize);
    let text = rendered_lines(&state, 240, 60).join("\n").replace(' ', "");
    for value in [
        "字段详情/Evidence",
        "Value:",
        "SourceLBA:",
        "Group:",
        "Offset:",
        "Length:",
        "Raw:",
        "Decoded:",
        "FieldLogical:",
        "Transform:",
        "Type:",
        "Status:",
    ] {
        assert!(text.contains(value), "missing {value} in field evidence");
    }
}

#[test]
fn ch16_inspect_lba8_renders_at_all_required_sizes() {
    let state = lba8_state();
    for (width, height) in [
        (40, 10),
        (60, 18),
        (80, 24),
        (120, 36),
        (160, 45),
        (240, 60),
    ] {
        let text = rendered_lines(&state, width, height)
            .join("\n")
            .replace(' ', "");
        assert!(
            text.contains("Inspect"),
            "missing workspace at {width}x{height}"
        );
    }
}

#[test]
fn ch16_inspect_view_shortcuts_are_explicit() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use edpcli::tui::{
        keymap::{KeyMapper, TuiAction},
        state::InputMode,
    };

    let mut mapper = KeyMapper::new();
    for (digit, expected) in [
        ('1', TuiAction::InspectBusiness),
        ('2', TuiAction::InspectRawFields),
        ('3', TuiAction::InspectHex),
        ('4', TuiAction::InspectDiskLayout),
    ] {
        assert_eq!(
            mapper.map(
                InputMode::Normal,
                KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE)
            ),
            Some(expected)
        );
    }
}

#[test]
fn ch16_responsive_breakpoints_have_one_source() {
    use edpcli::tui::ui::ViewportClass;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("src/tui/ui/responsive.rs"))
        .expect("central responsive module");
    for token in ["Compact", "Standard", "Wide", "UltraWide"] {
        assert!(source.contains(token), "missing viewport class {token}");
    }
    for (width, class) in [
        (40, ViewportClass::Compact),
        (79, ViewportClass::Compact),
        (80, ViewportClass::Standard),
        (119, ViewportClass::Standard),
        (120, ViewportClass::Wide),
        (159, ViewportClass::Wide),
        (160, ViewportClass::UltraWide),
        (240, ViewportClass::UltraWide),
    ] {
        assert_eq!(ViewportClass::for_width(width), class, "width={width}");
    }
}

#[test]
fn ch16_renderers_use_only_central_responsive_breakpoints() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/tui/render.rs",
        "src/tui/shell/mod.rs",
        "src/tui/disk_layout.rs",
        "src/tui/devices/render.rs",
        "src/tui/inspect/render.rs",
        "src/tui/inspect/sector_render.rs",
        "src/tui/provision/render.rs",
        "src/tui/backups/render.rs",
    ] {
        let source = std::fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        for (index, line) in source.lines().enumerate() {
            assert!(
                ![".width <", ".width >", ".width <=", ".width >="]
                    .iter()
                    .any(|needle| line.contains(needle)),
                "{path}:{} contains a private responsive breakpoint: {}",
                index + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn ch16_business_layouts_do_not_use_legacy_animation_or_workspace_sidebars() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let shell =
        std::fs::read_to_string(root.join("src/tui/render.rs")).expect("top-level renderer");
    let provision = std::fs::read_to_string(root.join("src/tui/provision/render.rs"))
        .expect("provision renderer");
    for forbidden in [
        "animation_area",
        "animation::draw(",
        "workspace_sidebar_layout",
    ] {
        assert!(
            !shell.contains(forbidden),
            "top-level business renderer still contains legacy sidebar token {forbidden}"
        );
    }
    assert!(
        !provision.contains("workspace_sidebar_layout"),
        "Provision must own its responsive context layout instead of using the legacy workspace sidebar helper"
    );
}

#[test]
fn ch16_design_primitives_share_theme_and_render_at_compact_size() {
    use edpcli::tui::ui::{
        card, data_table, key_hints, notice_banner, panel, status_badge, BadgeTone, BannerTone,
    };
    use ratatui::{
        layout::Constraint,
        text::Line,
        widgets::{Paragraph, Row},
    };

    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(status_badge("正常", BadgeTone::Success)).block(card("设备", true)),
                ratatui::layout::Rect::new(0, 0, 20, 4),
            );
            frame.render_widget(
                data_table(
                    "列表",
                    Row::new(["名称"]),
                    [Row::new(["disk4"])],
                    [Constraint::Min(1)],
                    false,
                ),
                ratatui::layout::Rect::new(20, 0, 20, 4),
            );
            frame.render_widget(
                notice_banner(Line::from("扫描完成"), BannerTone::Info),
                ratatui::layout::Rect::new(0, 4, 40, 1),
            );
            frame.render_widget(
                Paragraph::new(key_hints(&[("r", "刷新"), ("?", "帮助")]))
                    .block(panel("操作", false)),
                ratatui::layout::Rect::new(0, 5, 40, 4),
            );
        })
        .unwrap();
    let text = (0..10)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .map(|position| terminal.backend().buffer()[position].symbol().to_owned())
        .collect::<String>()
        .replace(' ', "");
    for value in ["设备", "正常", "disk4", "扫描完成", "刷新"] {
        assert!(text.contains(value), "missing {value}");
    }
}
