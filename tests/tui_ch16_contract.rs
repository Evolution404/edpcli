use edpcli::application::inspect::{
    AbsoluteByteRange, AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    InspectField, InspectFieldStatus, InspectFieldType,
};
use edpcli::inspect::{FieldChild, FieldStyle, InspectFieldKey, InspectMeta, InspectParseState};
use edpcli::inspect_target::InspectDiskContext;
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
                0 => "部门".into(),
                1 => "用户".into(),
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
            label: "E_LABEL".into(),
            value: "verified".into(),
            style: FieldStyle::Identity,
            group: Some("LBA8".into()),
            children,
        }],
        notes: Vec::new(),
        meta_text: None,
    };
    let context =
        InspectDiskContext::new(vec![0; edpcli::common::METADATA_IMAGE_LEN], None, 16_384);
    let workspace = AdvancedInspectWorkspace {
        source: "chapter-16-fixture".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: vec![sector],
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    };
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace));
    state.advanced_inspect_jump_lba(8).unwrap();
    state
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
#[ignore = "U4: red LBA8 snapshot contract"]
fn ch16_lba8_first_screen_shows_verified_department_user_and_label() {
    let lines = rendered_lines(&lba8_state(), 120, 36);
    let first_screen = lines.join("\n").replace(' ', "");
    for value in ["部门", "输电运检中心", "用户", "测试用户", "E_LABEL", "17"] {
        assert!(
            first_screen.contains(value),
            "missing {value} in LBA8 first screen"
        );
    }
}

#[test]
#[ignore = "U4: red Enter/open contract"]
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
