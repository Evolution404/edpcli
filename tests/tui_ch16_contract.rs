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
#[ignore = "U2: red AppShell contract"]
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
#[ignore = "U1/U7: red centralized responsive contract"]
fn ch16_responsive_breakpoints_have_one_source() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("src/tui/ui/responsive.rs"))
        .expect("central responsive module");
    for token in ["Compact", "Standard", "Wide", "UltraWide"] {
        assert!(source.contains(token), "missing viewport class {token}");
    }
}
