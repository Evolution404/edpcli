use edpcli::application::inspect::{AdvancedInspectMode, AdvancedInspectWorkspace};
use edpcli::inspect::InspectMeta;
use edpcli::tui::disk_layout::{DiskLayoutModel, DiskRegionKind};
use edpcli::tui::pane::{PaneFocus, PaneId};
use edpcli::tui::render;
use edpcli::tui::state::{
    AdvancedInspectSource, AppState, NavCommand, ProvisionKind, ProvisionStage,
};
use ratatui::{backend::TestBackend, Terminal};

fn inspect_workspace() -> AdvancedInspectWorkspace {
    let context = crate::common::edp_inspect_context(16_384);
    let disk_layout =
        edpcli::application::disk_layout::DiskLayoutModel::canonical_inspect_context(&context)
            .unwrap();
    AdvancedInspectWorkspace {
        source: "pane-contract".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: Vec::new(),
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
        disk_layout: Some(disk_layout),
        disk_layout_issue: None,
    }
}

fn device() -> edpcli::disk_scan::Row {
    let mut row = edpcli::disk_scan::Row {
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
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };
    confirm_kind(&mut row, edpcli::provision::DiskProvisionKind::Plain);
    row
}

fn confirm_kind(row: &mut edpcli::disk_scan::Row, kind: edpcli::provision::DiskProvisionKind) {
    use edpcli::application::media_identity::{
        DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentityPin,
        MediaIdentitySnapshot, ProtocolIdentityEvidence,
    };
    let snapshot = MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            total_sectors: Some(row.size / 512),
            logical_sector_size: Some(512),
            ..HardwareIdentityEvidence::default()
        },
        protocol: ProtocolIdentityEvidence {
            device_id: (kind != edpcli::provision::DiskProvisionKind::Plain)
                .then(|| row.device_id.clone())
                .flatten(),
            onlyid: (kind != edpcli::provision::DiskProvisionKind::Plain)
                .then(|| row.onlyid.clone())
                .flatten(),
            provision_kind: Some(kind),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    };
    row.identity_pin = Some(MediaIdentityPin::new(
        snapshot,
        &vec![0; edpcli::common::METADATA_IMAGE_LEN],
    ));
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
fn table_footer_advertises_cell_and_row_copy_across_workspaces() {
    let mut devices = AppState::new();
    devices.replace_devices(vec![device()]);
    assert!(render_text(&devices, 240, 60).contains("y单元格·Y整行"));

    devices.navigate(NavCommand::WorkspaceBackups, 20);
    assert!(render_text(&devices, 240, 60).contains("y单元格·Y整行"));

    devices.navigate(NavCommand::WorkspaceProvision, 20);
    assert!(render_text(&devices, 240, 60).contains("y单元格·Y整行"));
    assert_eq!(devices.provision_select_disk(), Some(6));
    assert!(render_text(&devices, 240, 60).contains("y单元格·Y整行"));
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

#[test]
fn provision_context_tab_walks_fields_then_layout_and_wraps() {
    let mut state = provision_state();
    let count = state.provision_visible_fields().len();
    assert!(count > 1);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    assert_eq!(state.provision().field_selected, 0);

    for expected in 1..count {
        state.provision_tab_focus(false);
        assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
        assert_eq!(state.provision().field_selected, expected);
    }
    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    assert_eq!(state.provision().field_selected, 0);

    state.provision_tab_focus(true);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    state.provision_tab_focus(true);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    assert_eq!(state.provision().field_selected, count - 1);
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
fn same_edp_fixture_has_identical_devices_inspect_and_provision_source_geometry() {
    let context = crate::common::edp_inspect_context(2_000_000);
    let mut row = device();
    row.size = context.total_sectors * edpcli::common::SECTOR as u64;
    row.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut row, edpcli::provision::DiskProvisionKind::Mode0);
    row.partitions = Some(
        context
            .partitions
            .iter()
            .map(|part| edpcli::sectors::EdpfPartition {
                ptype: part.partition_type,
                active: 1,
                enc: u32::from(part.partition_type != 1),
                start_lba: part.start_sector,
                size_bytes: part.sector_count * edpcli::common::SECTOR as u64,
            })
            .collect(),
    );
    row.lce = context.lce.clone();
    let device_layout = row.canonical_layout().unwrap();
    let inspect_layout = DiskLayoutModel::canonical_inspect_context(&context).unwrap();
    assert_eq!(device_layout, inspect_layout);

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    let provision_source = state.selected_device().unwrap().canonical_layout().unwrap();
    assert_eq!(provision_source, inspect_layout);
    assert_complete_layout(&provision_source);
}

#[test]
fn inspect_disk_layout_is_complete_and_semantically_distinct() {
    let workspace = inspect_workspace();
    let model = workspace.disk_layout.as_ref().unwrap();
    assert_complete_layout(model);
    let kinds = model
        .segments
        .iter()
        .map(|segment| segment.kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&DiskRegionKind::Protocol));
    assert!(kinds.contains(&DiskRegionKind::Free));
    assert!(kinds.contains(&DiskRegionKind::Lce));
    assert!(kinds.contains(&DiskRegionKind::BackupMirror));
    assert!(kinds.contains(&DiskRegionKind::RestoreNode));
    assert!(!kinds.contains(&DiskRegionKind::Unknown));
}

#[test]
fn provision_disk_layout_tail_starts_collapsed_and_expands_without_changing_geometry() {
    let mut state = provision_state();
    state.provision_focus_pane(PaneId::ProvisionDiskLayout);
    let canonical = state.provision_layout_model();
    assert_eq!(
        state.disk_layout_tail_expansion(),
        edpcli::tui::disk_layout::TailExpansion::Collapsed
    );
    let collapsed = render_text(&state, 160, 45);
    assert!(collapsed.contains("尾部区域"));
    assert!(!collapsed.contains("restore-node"));
    state.toggle_disk_layout_tail();
    let expanded = render_text(&state, 160, 45);
    assert!(expanded.contains("restore-node"));
    assert!(state
        .disk_layout_detail(&canonical)
        .unwrap()
        .contains("EDP 主协议区"));
    state.disk_layout_move_selection(1, canonical.segments.len());
    assert!(state
        .disk_layout_detail(&canonical)
        .unwrap()
        .contains("空闲区域"));
    assert_eq!(state.provision_layout_model().segments, canonical.segments);
    state.toggle_disk_layout_tail();
    assert_eq!(
        state.disk_layout_tail_expansion(),
        edpcli::tui::disk_layout::TailExpansion::Collapsed
    );
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
    assert!(kinds.contains(&DiskRegionKind::Free));
    assert!(kinds.contains(&DiskRegionKind::Lce));
    assert!(kinds.contains(&DiskRegionKind::BackupMirror));
    assert!(kinds.contains(&DiskRegionKind::RestoreNode));
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
    assert_eq!(model.segments[0].kind, DiskRegionKind::Metadata);
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
    state.focus_devices_pane(PaneId::DevicesTree);

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
        PaneId::DevicesTree,
        PaneId::DevicesDetail,
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
    confirm_kind(&mut row, edpcli::provision::DiskProvisionKind::Mode0);
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
    let total_sectors = row.size / edpcli::common::SECTOR as u64;
    row.lce = Some(edpcli::backup_metadata::Lba7CompatibilityGeometry {
        start_lba: total_sectors - 2_000,
        sector_count: 6,
        lba7_pointer_entries: Vec::new(),
        official_partition_mode: None,
        chs_expected_start_lba: None,
    });

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Down, 12);
    let text = render_text(&state, 160, 36);
    assert!(text.contains("容量布局"), "{text}");
    assert!(!text.contains("当前设备·disk6"), "{text}");
    assert!(
        text.matches('━').count() >= 80,
        "capacity bar should use the pane width: {text}"
    );
    assert!(text.contains("启动区"), "{text}");
    assert!(text.contains("交换区"), "{text}");
    assert!(text.contains("保密区"), "{text}");
    assert!(text.contains("━"), "{text}");
    assert!(
        text.lines().any(|line| {
            let compact = line.replace(' ', "");
            compact.contains("总容量") && compact.contains("EDP主协议区")
        }),
        "wide capacity legend should place multiple cells on one aligned row: {text}"
    );
}

#[test]
fn d0_plain_mbr_layout_uses_real_partition_table_without_unknown_disk_body() {
    use edpcli::application::partition_table::{
        PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
        PhysicalPartition,
    };

    let total_sectors = 15_728_640u64;
    let mut row = device();
    row.disk = 4;
    row.size = total_sectors * 512;
    row.device_id = None;
    row.onlyid = None;
    row.provision_kind = edpcli::provision::DiskProvisionKind::Plain;
    confirm_kind(&mut row, edpcli::provision::DiskProvisionKind::Plain);
    row.partition_table = Some(PartitionTableSnapshot {
        kind: PartitionTableKind::Mbr,
        partitions: vec![PhysicalPartition {
            index: 1,
            start_lba: 2048,
            sector_count: 15_726_592,
            source: PartitionSource::Mbr {
                partition_type: 0x07,
                primary_slot: Some(1),
            },
            filesystem: Some("exFAT".into()),
        }],
        table_extents: vec![PartitionTableExtent {
            label: "MBR 分区表".into(),
            start_lba: 0,
            sector_count: 1,
        }],
        issues: Vec::new(),
    });

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.focus_devices_pane(PaneId::DevicesTree);
    let text = render_text(&state, 180, 42);

    assert!(text.contains("MBR分区表"), "{text}");
    assert!(text.contains("空闲区域"), "{text}");
    assert!(text.contains("P1exFAT"), "{text}");
    assert!(!text.contains("布局未完整读取"), "{text}");
    assert!(!text.contains("EDP主协议区"), "{text}");
    assert!(!text.contains("未知区域8.05GB"), "{text}");
}

#[test]
fn device_workbench_panes_are_list_tree_detail() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(
        PaneId::DEVICES_ORDER,
        [
            PaneId::DevicesList,
            PaneId::DevicesTree,
            PaneId::DevicesDetail,
        ]
    );
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
    state.shift_workspace_pane(false);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesTree);
    state.shift_workspace_pane(false);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesDetail);
    state.shift_workspace_pane(false);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
}

#[test]
fn device_tree_selection_is_semantic_and_detail_has_independent_scroll() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    assert_eq!(
        state.device_info_selected_key(),
        edpcli::tui::state::DeviceInfoNodeKey::Identity
    );
    state.navigate(NavCommand::Down, 8);
    assert_eq!(
        state.device_info_selected_key(),
        edpcli::tui::state::DeviceInfoNodeKey::Capacity
    );
    let selected = state.device_info_selected_key();
    state.device_info_focus_detail();
    state.navigate(NavCommand::Down, 8);
    assert_eq!(state.device_info_selected_key(), selected);
    assert_eq!(
        state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset,
        1
    );
}
