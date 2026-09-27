use edpcli::application::inspect::{AdvancedInspectMode, AdvancedInspectWorkspace};
use edpcli::inspect::InspectMeta;
use edpcli::inspect_target::InspectDiskContext;
use edpcli::tui::disk_layout::{DiskLayoutModel, DiskRegionKind};
use edpcli::tui::pane::{PaneFocus, PaneId};
use edpcli::tui::render;
use edpcli::tui::state::{
    AdvancedInspectSource, AppState, NavCommand, ProvisionKind, ProvisionStage,
};
use ratatui::{backend::TestBackend, Terminal};

fn inspect_workspace() -> AdvancedInspectWorkspace {
    let context =
        InspectDiskContext::new(vec![0; edpcli::common::METADATA_IMAGE_LEN], None, 16_384);
    AdvancedInspectWorkspace {
        source: "pane-contract".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: Vec::new(),
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    }
}

fn device() -> edpcli::disk_scan::Row {
    edpcli::disk_scan::Row {
        disk: 6,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        serial: Some("SERIAL-D0-1234".into()),
        device_id: Some("disk&ven_test&prod_test".into()),
        identity_pin: None,
        onlyid: Some("1402259934".into()),
        dept: Some("输电运检中心".into()),
        user: Some("测试用户".into()),
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
        n_possible_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
    }
}

fn inspect_state() -> AppState {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(inspect_workspace()));
    state
}

fn provision_state() -> AppState {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
    state
}

fn plain_provision_state() -> AppState {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Plain);
    state
}

#[test]
fn inspect_tab_cycle_is_tree_overview_detail_disk_layout() {
    let mut state = inspect_state();
    assert_eq!(
        state.advanced_inspect_focused_pane(),
        Some(PaneId::InspectTree)
    );
    for expected in [
        PaneId::InspectOverview,
        PaneId::InspectDetail,
        PaneId::InspectDiskLayout,
        PaneId::InspectTree,
    ] {
        state.advanced_inspect_shift_panel(false);
        assert_eq!(state.advanced_inspect_focused_pane(), Some(expected));
    }
    state.advanced_inspect_shift_panel(true);
    assert_eq!(
        state.advanced_inspect_focused_pane(),
        Some(PaneId::InspectDiskLayout)
    );
}

#[test]
fn inspect_tree_jk_changes_tree_selection_only() {
    let mut state = inspect_state();
    state.advanced_inspect_focus_pane(PaneId::InspectTree);
    let before_scroll = state.pane_viewport(PaneId::InspectTree).scroll_y.offset;
    state.advanced_inspect_move_focused_vertical(1, 8, 100);
    assert_eq!(state.advanced_inspect().unwrap().tree_selected, 1);
    assert_eq!(
        state.pane_viewport(PaneId::InspectTree).scroll_y.offset,
        before_scroll
    );
}

#[test]
fn inspect_non_tree_jk_never_changes_tree_selection() {
    let mut state = inspect_state();
    state.advanced_inspect_focus_pane(PaneId::InspectTree);
    state.advanced_inspect_move_focused_vertical(1, 8, 100);
    let selected = state.advanced_inspect().unwrap().tree_selected;

    for pane in [
        PaneId::InspectDiskLayout,
        PaneId::InspectOverview,
        PaneId::InspectDetail,
    ] {
        state.advanced_inspect_focus_pane(pane);
        let before = state.pane_viewport(pane).scroll_y.offset;
        state.advanced_inspect_move_focused_vertical(1, 8, 100);
        assert_eq!(
            state.advanced_inspect().unwrap().tree_selected,
            selected,
            "{pane:?}"
        );
        assert_eq!(
            state.pane_viewport(pane).scroll_y.offset,
            before + 1,
            "{pane:?}"
        );
    }
}

#[test]
fn provision_parameters_jk_changes_field_selection() {
    let mut state = provision_state();
    state.provision_focus_pane(PaneId::ProvisionParameters);
    let before = state.provision().field_selected;
    state.provision_move_focused_vertical(1, 8, 100);
    assert!(state.provision().field_selected > before);
}

#[test]
fn provision_disk_layout_jk_scrolls_without_changing_field_selection() {
    let mut state = provision_state();
    state.provision_focus_pane(PaneId::ProvisionDiskLayout);
    let selected = state.provision().field_selected;
    let before = state
        .pane_viewport(PaneId::ProvisionDiskLayout)
        .scroll_y
        .offset;
    state.provision_move_focused_vertical(1, 8, 100);
    assert_eq!(state.provision().field_selected, selected);
    assert_eq!(
        state
            .pane_viewport(PaneId::ProvisionDiskLayout)
            .scroll_y
            .offset,
        before + 1
    );
}

#[test]
fn provision_form_tab_changes_focus_without_changing_field_selection() {
    let mut state = provision_state();
    let selected = state.provision().field_selected;
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    state.provision_shift_pane(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    assert_eq!(state.provision().field_selected, selected);
    state.provision_shift_pane(true);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    assert_eq!(state.provision().field_selected, selected);
}

fn assert_complete_layout(model: &DiskLayoutModel) {
    assert!(model.total_sectors > 0);
    assert_eq!(model.segments.first().unwrap().start_lba, 0);
    for pair in model.segments.windows(2) {
        assert_eq!(pair[0].end_exclusive().unwrap(), pair[1].start_lba);
    }
    assert_eq!(
        model.segments.last().unwrap().end_exclusive().unwrap(),
        model.total_sectors
    );
    assert!(model
        .segments
        .iter()
        .all(|segment| segment.sector_count > 0));
    model.validate_complete().unwrap();
}

#[test]
fn inspect_disk_layout_is_complete_and_semantically_distinct() {
    let workspace = inspect_workspace();
    let model = DiskLayoutModel::from_topology(&workspace.topology);
    assert_complete_layout(&model);
    let kinds = model
        .segments
        .iter()
        .map(|segment| segment.kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&DiskRegionKind::Protocol));
    assert!(kinds.contains(&DiskRegionKind::Unknown));
    assert!(kinds.contains(&DiskRegionKind::Tail));
}

#[test]
fn official_provision_disk_layout_covers_the_whole_physical_disk() {
    let state = provision_state();
    let model = state.provision_layout_model();
    assert_complete_layout(&model);
    assert_eq!(
        model.total_sectors,
        device().size / edpcli::common::SECTOR as u64
    );
    let kinds = model
        .segments
        .iter()
        .map(|segment| segment.kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&DiskRegionKind::Protocol));
    assert!(kinds.contains(&DiskRegionKind::Reserved));
    assert!(kinds.contains(&DiskRegionKind::Lce));
    assert!(kinds.contains(&DiskRegionKind::Tail));
}

#[test]
fn plain_provision_disk_layout_covers_mbr_free_and_partitions_to_last_sector() {
    let state = plain_provision_state();
    let model = state.provision_layout_model();
    assert_complete_layout(&model);
    assert_eq!(
        model.total_sectors,
        device().size / edpcli::common::SECTOR as u64
    );
    assert_eq!(model.segments[0].kind, DiskRegionKind::Reserved);
    assert_eq!(model.segments[0].start_lba, 0);
    assert_eq!(model.segments[0].sector_count, 1);
    assert!(model
        .segments
        .iter()
        .any(|segment| segment.kind == DiskRegionKind::Free));
    assert!(model
        .segments
        .iter()
        .any(|segment| segment.kind == DiskRegionKind::Plain));
}

fn render_text(state: &AppState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "")
}

#[test]
fn provision_review_tab_cycle_and_vertical_scroll_are_pane_local() {
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = PaneFocus::provision_review();
    let selected = state.provision().field_selected;

    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionSummary);
    for expected in [
        PaneId::ProvisionDiskLayout,
        PaneId::ProvisionChanges,
        PaneId::ProvisionSummary,
    ] {
        state.provision_shift_pane(false);
        assert_eq!(state.provision_focused_pane(), expected);
    }

    for pane in [
        PaneId::ProvisionSummary,
        PaneId::ProvisionDiskLayout,
        PaneId::ProvisionChanges,
    ] {
        state.provision_focus_pane(pane);
        let before = state.pane_viewport(pane).scroll_y.offset;
        state.provision_move_focused_vertical(1, 1, 100);
        assert_eq!(state.provision().field_selected, selected, "{pane:?}");
        assert_eq!(
            state.pane_viewport(pane).scroll_y.offset,
            before + 1,
            "{pane:?}"
        );
    }
}

#[test]
fn provision_form_narrow_renders_only_the_focused_pane() {
    let mut state = provision_state();
    state.provision_focus_pane(PaneId::ProvisionParameters);
    let parameters = render_text(&state, 80, 24);
    assert!(parameters.contains("参数"), "{parameters}");
    assert!(!parameters.contains("磁盘布局"), "{parameters}");

    state.provision_focus_pane(PaneId::ProvisionDiskLayout);
    let layout = render_text(&state, 80, 24);
    assert!(layout.contains("磁盘布局"), "{layout}");
    assert!(!layout.contains("参数"), "{layout}");
}

#[test]
fn provision_review_wide_has_three_panes_and_narrow_uses_focus() {
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = PaneFocus::provision_review();

    let wide = render_text(&state, 160, 36);
    assert!(wide.contains("计划摘要"), "{wide}");
    assert!(wide.contains("磁盘布局"), "{wide}");
    assert!(wide.contains("变更明细"), "{wide}");

    state.provision_focus_pane(PaneId::ProvisionSummary);
    let summary = render_text(&state, 80, 24);
    assert!(summary.contains("计划摘要"), "{summary}");
    assert!(!summary.contains("磁盘布局"), "{summary}");
    assert!(!summary.contains("变更明细"), "{summary}");

    state.provision_focus_pane(PaneId::ProvisionChanges);
    let changes = render_text(&state, 80, 24);
    assert!(changes.contains("变更明细"), "{changes}");
    assert!(!changes.contains("计划摘要"), "{changes}");
    assert!(!changes.contains("磁盘布局"), "{changes}");
}

#[test]
fn d0_device_table_uses_user_approved_column_order() {
    use edpcli::tui::table_layout::{table_column_schema, TableKind};
    let headings = table_column_schema(TableKind::Devices)
        .unwrap()
        .into_iter()
        .map(|column| column.heading)
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        vec!["设备", "容量", "部门", "姓名", "盘型", "状态", "备份", "型号"]
    );
}

#[test]
fn d0_device_summary_sections_are_interactive_and_default_identity_capacity_open() {
    use edpcli::tui::state::DeviceSummarySection;

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.focus_devices_pane(PaneId::DevicesSummary);

    assert_eq!(
        state.device_summary_selected_section(),
        DeviceSummarySection::Identity
    );
    assert!(state.device_summary_section_expanded(DeviceSummarySection::Identity));
    assert!(state.device_summary_section_expanded(DeviceSummarySection::Capacity));
    assert!(!state.device_summary_section_expanded(DeviceSummarySection::Status));

    state.device_summary_move_section(1);
    assert_eq!(
        state.device_summary_selected_section(),
        DeviceSummarySection::Capacity
    );
    state.device_summary_toggle_selected_section();
    assert!(!state.device_summary_section_expanded(DeviceSummarySection::Capacity));
}

#[test]
fn d0_three_pane_focus_cycle_never_changes_selected_device() {
    let mut state = AppState::new();
    let first = device();
    let mut second = device();
    second.disk = 7;
    state.replace_devices(vec![first, second]);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(7));

    for expected in [
        PaneId::DevicesSummary,
        PaneId::DevicesStats,
        PaneId::DevicesList,
    ] {
        state.shift_workspace_pane(false);
        assert_eq!(state.devices_focused_pane(), expected);
        assert_eq!(state.selected_device_disk(), Some(7));
    }
}

#[test]
fn d0_current_device_summary_renders_capacity_layout_bar() {
    use edpcli::sectors::EdpfPartition;

    let mut row = device();
    row.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    row.partitions = Some(vec![
        EdpfPartition {
            ptype: 1,
            active: 1,
            enc: 0,
            start_lba: 63,
            size_bytes: 20_417 * 512,
        },
        EdpfPartition {
            ptype: 2,
            active: 1,
            enc: 1,
            start_lba: 20_480,
            size_bytes: 80_000_000,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: 176_730,
            size_bytes: 120_000_000,
        },
    ]);

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.focus_devices_pane(PaneId::DevicesSummary);
    let text = render_text(&state, 160, 36);
    assert!(text.contains("SERIAL-D0-1234"), "{text}");
    assert!(text.contains("身份依据"), "{text}");
    assert!(!text.contains("物理介质一致"), "{text}");
    assert!(text.contains("容量布局"), "{text}");
    assert!(
        text.matches('━').count() >= 80,
        "capacity bar should use the pane width: {text}"
    );
    assert!(text.contains("启动区"), "{text}");
    assert!(text.contains("交换区"), "{text}");
    assert!(text.contains("保密区"), "{text}");
    assert!(text.contains("━"), "{text}");
}
