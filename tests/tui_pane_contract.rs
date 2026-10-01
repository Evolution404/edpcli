use edpcli::application::inspect::{AdvancedInspectMode, AdvancedInspectWorkspace};
use edpcli::inspect::InspectMeta;
use edpcli::tui::disk_layout::{DiskLayoutModel, DiskRegionKind};
use edpcli::tui::pane::{PaneFocus, PaneId};
use edpcli::tui::render;
use edpcli::tui::state::{
    AdvancedInspectSource, AppState, NavCommand, ProvisionKind, ProvisionStage, WizardStage,
    Workspace, WriteKind,
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
        backup_manifest: None,
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
        hardware_model: None,
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
            vid: u16::from_str_radix(&row.vid, 16).ok(),
            pid: u16::from_str_radix(&row.pid, 16).ok(),
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

fn backup(
    index: usize,
    kind: Option<edpcli::provision::DiskProvisionKind>,
) -> edpcli::application::BackupWorkspaceItem {
    edpcli::application::BackupWorkspaceItem {
        index,
        path: format!("backup-{index}.edpb").into(),
        file_name: format!("backup-{index}.edpb"),
        display_time: "2026-09-29 10:00".into(),
        size_bytes: Some(64_000_000_000),
        vid: Some("1234".into()),
        pid: Some("5678".into()),
        device_id: Some("disk&ven_test&prod_test".into()),
        onlyid: Some(format!("700{index}")),
        identity: None,
        user: Some("测试用户".into()),
        dept: Some("输电运检中心".into()),
        provision_kind: kind,
        integrity_status: edpcli::application::BackupIntegrityStatus::Verified,
        size_ok: true,
        content_sha256: Some("a".repeat(64)),
        coverage: None,
        restore_preview: None,
    }
}

fn related_backup(
    index: usize,
    row: &edpcli::disk_scan::Row,
) -> edpcli::application::BackupWorkspaceItem {
    let mut item = backup(index, Some(row.provision_kind));
    item.identity = row.identity_pin.as_ref().map(|pin| pin.snapshot.clone());
    item
}

fn edp_device_with_layout() -> edpcli::disk_scan::Row {
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
    row
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
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
    state.provision_enter_form_workspace();
    state
}

fn plain_provision_state() -> AppState {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_select_scheme_index(4));
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Plain);
    state.provision_enter_form_workspace();
    state
}

#[test]
fn inspect_tab_cycle_is_tree_overview_detail_bytes() {
    let mut state = inspect_state();
    assert_eq!(
        state.advanced_inspect_focused_pane(),
        Some(PaneId::InspectTree)
    );
    for expected in [
        PaneId::InspectOverview,
        PaneId::InspectDetail,
        PaneId::InspectBytes,
        PaneId::InspectTree,
    ] {
        state.advanced_inspect_shift_panel(false);
        assert_eq!(state.advanced_inspect_focused_pane(), Some(expected));
    }
    state.advanced_inspect_shift_panel(true);
    assert_eq!(
        state.advanced_inspect_focused_pane(),
        Some(PaneId::InspectBytes)
    );
}

#[test]
fn contextual_help_advertises_shared_table_copy_without_a_permanent_footer() {
    let mut devices = AppState::new();
    devices.replace_devices(vec![device()]);
    let device_text = render_text(&devices, 240, 60);
    assert!(!device_text.contains("y单元格·Y整行"));
    assert!(device_text.contains("?帮助"));

    devices.navigate(NavCommand::Help, 20);
    let help = render_text(&devices, 240, 60);
    assert!(help.contains("快捷键·设备"), "{help}");
    assert!(help.contains("复制单元格/整行"), "{help}");
}

#[test]
fn top_navigation_is_the_only_persistent_help_prompt() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    let normal = render_text(&state, 120, 32);
    assert_eq!(normal.matches("?帮助").count(), 1, "{normal}");

    state.begin_provision_for_selected_device().unwrap();
    let picker = render_text(&state, 120, 32);
    assert_eq!(picker.matches("?帮助").count(), 1, "{picker}");
    assert!(!picker.contains("制盘方案："), "{picker}");
}

#[test]
fn devices_and_backups_share_overview_layout_and_hide_zero_kind_counts() {
    let mut plain = device();
    plain.disk = 4;
    let mut mode1 = device();
    mode1.disk = 5;
    mode1.provision_kind = edpcli::provision::DiskProvisionKind::Mode1;
    confirm_kind(&mut mode1, edpcli::provision::DiskProvisionKind::Mode1);

    let mut state = AppState::new();
    state.replace_devices(vec![plain, mode1]);
    let devices = render_text(&state, 160, 45);
    assert!(devices.contains("设备概览"), "{devices}");
    assert!(devices.contains("总计2·普通盘1·mode11"), "{devices}");
    assert!(
        devices.contains("/搜索设备、部门、姓名、型号、盘型"),
        "{devices}"
    );
    for zero_kind in ["mode00", "mode20", "mode30", "未知0"] {
        assert!(!devices.contains(zero_kind), "{devices}");
    }

    state.replace_backups(vec![
        backup(1, Some(edpcli::provision::DiskProvisionKind::Plain)),
        backup(2, Some(edpcli::provision::DiskProvisionKind::Mode2)),
        backup(3, Some(edpcli::provision::DiskProvisionKind::Mode2)),
    ]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.toggle_selected_backup();
    let backups = render_text(&state, 160, 45);
    assert!(backups.contains("备份概览"), "{backups}");
    assert!(backups.contains("总计3·普通盘1·mode22"), "{backups}");
    assert!(
        backups.contains("/搜索身份、容量、型号、文件名"),
        "{backups}"
    );
    assert!(backups.contains("备份列表(3)·已选1·当前列"), "{backups}");
    for zero_kind in ["mode00", "mode10", "mode30", "未知0"] {
        assert!(!backups.contains(zero_kind), "{backups}");
    }
    for old_hint in [
        "h/l激活",
        "</>移列",
        "0/$首尾列",
        "H/L视口",
        "s排序",
        "S默认",
    ] {
        assert!(!backups.contains(old_hint), "{backups}");
    }
}

#[test]
fn backup_device_tree_filters_without_renumbering_and_search_stays_scoped() {
    let mut first = device();
    first.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut first, edpcli::provision::DiskProvisionKind::Mode0);

    let mut second = device();
    second.disk = 7;
    second.device_id = Some("disk&ven_test&prod_second".into());
    second.onlyid = Some("2402259934".into());
    second.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut second, edpcli::provision::DiskProvisionKind::Mode0);

    let mut first_old = related_backup(1, &first);
    first_old.file_name = "first-old.edpb".into();
    let mut first_new = related_backup(3, &first);
    first_new.file_name = "first-new.edpb".into();
    let mut second_backup = related_backup(8, &second);
    second_backup.file_name = "second.edpb".into();
    let mut unresolved = backup(11, Some(edpcli::provision::DiskProvisionKind::Plain));
    unresolved.file_name = "unknown.edpb".into();

    let mut state = AppState::new();
    state.replace_backups(vec![first_old, first_new, second_backup, unresolved]);
    state.navigate(NavCommand::WorkspaceBackups, 20);

    let nodes = state.backup_device_tree_nodes();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].label, "全部备份");
    assert_eq!(nodes[0].count, 4);
    assert_eq!(nodes[1].count, 2);
    assert_eq!(nodes[2].count, 1);
    assert_eq!(nodes[3].label, "身份未确认");
    assert_eq!(nodes[3].count, 1);

    state.focus_backups_pane(PaneId::BackupDevices);
    state.backup_device_tree_move(1);
    assert!(state.backup_device_filter_active());
    assert_eq!(state.visible_backup_count(), 2);
    assert_eq!(state.backup_at_visible(0).unwrap().index, 1);
    assert_eq!(state.backup_at_visible(1).unwrap().index, 3);

    use edpcli::tui::table_layout::TableKind;
    state.focus_backups_pane(PaneId::BackupsList);
    assert!(state.move_table_column_for_viewport(TableKind::Backups, false, 160, 45));
    state.toggle_table_sort(TableKind::Backups);
    let mut sorted_group = (0..state.visible_backup_count())
        .map(|position| state.backup_at_visible(position).unwrap().index)
        .collect::<Vec<_>>();
    sorted_group.sort_unstable();
    assert_eq!(sorted_group, vec![1, 3]);
    let selected_path = state.selected_backup().unwrap().path.clone();
    state.toggle_selected_backup();
    assert_eq!(
        state.selected_backup_batch_targets(),
        vec![(selected_path, "a".repeat(64))]
    );
    assert!(state.clear_table_sort(TableKind::Backups));
    state.focus_backups_pane(PaneId::BackupDevices);

    state.navigate(NavCommand::Search, 20);
    for ch in "first-new".chars() {
        state.push_input_char(ch);
    }
    state.submit_search();
    assert_eq!(state.visible_backup_count(), 1);
    assert_eq!(state.backup_at_visible(0).unwrap().index, 3);
    assert!(state.backup_device_filter_active());

    state.navigate(NavCommand::Search, 20);
    for _ in 0.."first-new".chars().count() {
        state.backspace_input();
    }
    state.submit_search();
    assert_eq!(state.visible_backup_count(), 2);
    assert!(state.backup_device_filter_active());

    state.backup_device_tree_toggle();
    assert!(!state.backup_device_tree_expanded());
    assert_eq!(
        (0..state.visible_backup_count())
            .map(|position| state.backup_at_visible(position).unwrap().index)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    state.backup_device_tree_jump(true);
    assert_eq!(state.visible_backup_count(), 2);

    state.backup_device_tree_toggle();
    assert!(state.backup_device_tree_expanded());
    assert_eq!(state.backup_device_tree_selected(), 1);

    state.backup_device_tree_jump(true);
    assert_eq!(state.visible_backup_count(), 1);
    assert_eq!(state.backup_at_visible(0).unwrap().index, 11);
    state.backup_device_tree_jump(false);
    assert_eq!(state.visible_backup_count(), 4);
}

#[test]
fn backup_device_tree_disambiguates_distinct_identity_groups_with_same_visible_label() {
    let mut first = device();
    first.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    first.onlyid = Some("1111111111".into());
    confirm_kind(&mut first, edpcli::provision::DiskProvisionKind::Mode0);

    let mut second = device();
    second.disk = 7;
    second.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    second.onlyid = Some("2222222222".into());
    confirm_kind(&mut second, edpcli::provision::DiskProvisionKind::Mode0);

    let mut first_backup = related_backup(1, &first);
    let mut second_backup = related_backup(2, &second);
    first_backup.onlyid = Some("SAME-DISPLAY".into());
    second_backup.onlyid = Some("SAME-DISPLAY".into());

    let mut state = AppState::new();
    state.replace_backups(vec![first_backup, second_backup]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    let nodes = state.backup_device_tree_nodes();
    assert_eq!(nodes.len(), 3);
    assert_ne!(nodes[1].label, nodes[2].label);
    assert!(nodes[1].label.starts_with('#'));
    assert!(nodes[2].label.starts_with('#'));
}

#[test]
fn devices_to_backups_follows_the_selected_physical_device_group() {
    let mut first = device();
    first.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut first, edpcli::provision::DiskProvisionKind::Mode0);

    let mut second = device();
    second.disk = 7;
    second.device_id = Some("disk&ven_test&prod_second".into());
    second.onlyid = Some("2402259934".into());
    second.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut second, edpcli::provision::DiskProvisionKind::Mode0);

    let backups = vec![related_backup(1, &first), related_backup(2, &second)];
    let mut state = AppState::new();
    state.replace_devices(vec![first, second]);
    state.replace_backups(backups);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(7));

    state.navigate(NavCommand::WorkspaceBackups, 20);

    assert!(state.backup_device_filter_active());
    assert_eq!(state.visible_backup_count(), 1);
    assert_eq!(state.backup_at_visible(0).unwrap().index, 2);
    assert_eq!(state.backup_device_tree_selected(), 2);
}

#[test]
fn backup_device_tree_is_a_sidebar_and_enter_target_is_the_backup_list() {
    let mut confirmed = device();
    confirmed.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut confirmed, edpcli::provision::DiskProvisionKind::Mode0);
    let mut state = AppState::new();
    state.replace_backups(vec![
        related_backup(5, &confirmed),
        backup(9, Some(edpcli::provision::DiskProvisionKind::Plain)),
    ]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupDevices);

    let rendered = render_text(&state, 160, 45).replace(' ', "");
    for expected in ["设备", "全部备份2", "身份未确认1", "备份列表"] {
        assert!(
            rendered.contains(expected),
            "missing {expected}: {rendered}"
        );
    }

    state.backup_device_tree_focus_list();
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}

#[test]
fn backup_device_tree_hl_scroll_reveals_full_active_identity_without_ellipsis() {
    let mut confirmed = device();
    confirmed.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
    confirm_kind(&mut confirmed, edpcli::provision::DiskProvisionKind::Mode0);
    let mut item = related_backup(5, &confirmed);
    item.device_id =
        Some("disk&ven_vendorco&prod_productcode_with_a_very_long_model_WRAP_SENTINEL_TAIL".into());
    item.identity
        .as_mut()
        .expect("EDP identity")
        .protocol
        .device_id = item.device_id.clone();

    let mut state = AppState::new();
    state.replace_backups(vec![item]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupDevices);
    state.backup_device_tree_move(1);

    let initial = render_lines(&state, 160, 45);
    let initial_sidebar = initial
        .iter()
        .filter_map(|line| line.split_once("┃│").map(|(left, _)| left))
        .collect::<Vec<_>>();
    assert!(
        !initial_sidebar
            .iter()
            .any(|line| line.contains("WRAP_SENTINEL_TAIL")),
        "long active identity should initially be clipped by the viewport, not wrapped: {initial_sidebar:#?}"
    );
    assert!(
        !initial_sidebar.iter().any(|line| line.contains('…')),
        "device-tree clipping must not replace hidden content with ellipsis: {initial_sidebar:#?}"
    );
    let initial_active = initial_sidebar
        .iter()
        .find(|line| line.contains('▌'))
        .expect("active device row");
    assert!(
        initial_active.trim_end().ends_with('1'),
        "backup count must stay pinned at the right edge before horizontal scrolling: {initial_active}"
    );

    while state.scroll_backup_device_tree(false, 160) {}
    assert!(state.backup_device_tree_scroll_offset() > 0);
    let scrolled = render_lines(&state, 160, 45);
    let scrolled_sidebar = scrolled
        .iter()
        .filter_map(|line| line.split_once("┃│").map(|(left, _)| left))
        .collect::<Vec<_>>();
    assert!(
        scrolled_sidebar
            .iter()
            .any(|line| line.contains("SENTINEL_TAIL")),
        "H/L must reach the real unabridged right tail of the active device identity: {scrolled_sidebar:#?}"
    );
    assert!(
        !scrolled_sidebar.iter().any(|line| line.contains('…')),
        "active device content must remain real text after horizontal scrolling: {scrolled_sidebar:#?}"
    );
    let scrolled_active = scrolled_sidebar
        .iter()
        .find(|line| line.contains('▌'))
        .expect("active device row after H/L");
    assert!(
        scrolled_sidebar
            .iter()
            .any(|line| line.replace(' ', "").contains("全部备份")),
        "root label must stay anchored while child device information scrolls: {scrolled_sidebar:#?}"
    );
    assert!(
        scrolled_active.trim_end().ends_with('1'),
        "backup count must remain visible and fixed while device information scrolls: {scrolled_active}"
    );

    while state.scroll_backup_device_tree(true, 160) {}
    assert_eq!(state.backup_device_tree_scroll_offset(), 0);
}

#[test]
fn backup_device_tree_keeps_selected_device_visible_when_moving_beyond_viewport() {
    let mut backups = Vec::new();
    for index in 0..32usize {
        let mut row = device();
        row.disk = 10 + index as u32;
        row.device_id = Some(format!("disk&ven_vendor{index:02}&prod_model{index:02}"));
        row.onlyid = Some(format!("ID{index:04}"));
        row.provision_kind = edpcli::provision::DiskProvisionKind::Mode0;
        confirm_kind(&mut row, edpcli::provision::DiskProvisionKind::Mode0);
        let mut item = related_backup(index + 1, &row);
        item.device_id = row.device_id.clone();
        item.onlyid = row.onlyid.clone();
        backups.push(item);
    }

    let mut state = AppState::new();
    state.replace_backups(backups);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupDevices);
    for _ in 0..26 {
        state.backup_device_tree_move(1);
    }

    let selected = state.backup_device_tree_selected();
    let selected_label = state.backup_device_tree_nodes()[selected].label.clone();
    let lines = render_lines(&state, 160, 32);
    let sidebar = lines
        .iter()
        .filter_map(|line| line.split_once("┃│").map(|(left, _)| left))
        .collect::<Vec<_>>();

    let selected_prefix = selected_label
        .split(" · ")
        .next()
        .unwrap_or(selected_label.as_str());
    assert!(
        sidebar
            .iter()
            .any(|line| line.contains('▌') && line.contains(selected_prefix)),
        "selected device must remain visible after automatic tree scrolling: {selected_label}\n{sidebar:#?}"
    );
    assert!(
        !sidebar.iter().any(|line| line.contains("全部备份")),
        "tree should have scrolled away from the root once selection moves far below the viewport: {sidebar:#?}"
    );
}

#[test]
fn backup_horizontal_scroll_reaches_real_right_edge_with_device_sidebar() {
    use edpcli::tui::table_layout::TableKind;

    let mut item = backup(1, Some(edpcli::provision::DiskProvisionKind::Mode1));
    item.dept = Some("江苏省电力有限公司/南京供电公司/输电运检中心/超长部门字段".into());
    item.user = Some("测试用户姓名很长".into());
    item.onlyid = Some("19877183881234567890".into());
    item.file_name =
        "disk5_245760000_vid3535_pid6300_extremely_long_backup_filename_for_scroll.edpb".into();

    let mut state = AppState::new();
    state.replace_backups(vec![item]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.focus_backups_pane(PaneId::BackupsList);

    let terminal_width = 200u16;
    let terminal_height = 45usize;
    while state.scroll_table_for_viewport(
        TableKind::Backups,
        false,
        terminal_width,
        terminal_height,
    ) {}

    let view = state
        .table_view_data(TableKind::Backups)
        .expect("backup table view");
    let layout = state.table_visual_layout(TableKind::Backups);
    let widths = state.table_visual_widths(TableKind::Backups, &view.content_widths);
    let active = state.table_interaction(TableKind::Backups).active_column();
    let expected_viewport = terminal_width
        .saturating_sub(
            edpcli::tui::ui::ViewportClass::for_width(terminal_width)
                .backup_device_sidebar_width()
                .unwrap(),
        )
        .saturating_sub(4);
    let expected_max = layout.max_scroll(&widths, Some(active), expected_viewport);

    assert_eq!(
        state.table_scroll_offset(TableKind::Backups),
        expected_max,
        "H/L viewport movement must use the actual right-table width after subtracting the device tree"
    );

    let rendered = render_text(&state, terminal_width, terminal_height as u16);
    assert!(
        rendered.contains("名称"),
        "rightmost backup column must become reachable at the final horizontal viewport: {rendered}"
    );
}

#[test]
fn provision_scheme_picker_is_centered_over_devices_before_entering_form() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(state.provision_scheme_picker_open());

    let text = render_text(&state, 120, 32);
    assert!(text.contains("选择制盘方案"), "{text}");
    assert!(text.contains("选择方案后直接进入参数表单"), "{text}");
    assert!(text.contains("j/k"), "{text}");
    assert!(text.contains("Enter"), "{text}");
    assert!(text.contains("Esc"), "{text}");

    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.provision_scheme_selected(), 1);
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.provision().kind, ProvisionKind::Mode1);
}

#[test]
fn provision_scheme_picker_uses_shared_modal_surface_instead_of_terminal_black() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.begin_provision_for_selected_device().unwrap();

    let width = 120;
    let height = 32;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let content_area = ratatui::layout::Rect::new(0, 2, width, height - 2);
    let popup = edpcli::tui::ui::centered_modal_rect(content_area, 78, 17);
    let expected = edpcli::tui::theme::current()
        .modal_surface()
        .bg
        .expect("modal surface background");
    // Sample an interior blank cell, not the continuation cell of a full-width CJK glyph.
    let sample = (popup.right().saturating_sub(3), popup.y + 2);
    assert_eq!(
        terminal.backend().buffer()[sample].style().bg,
        Some(expected),
        "modal interior must be painted by the shared modal surface"
    );
}

#[test]
fn backup_confirm_is_overlay_and_escape_preserves_device_selection() {
    let mut state = AppState::new();
    let row = device();
    let identity = row
        .identity_pin
        .as_ref()
        .map(edpcli::tui::state::ExpectedIdentity::from_pin)
        .expect("test device identity pin");
    state.replace_devices(vec![row]);
    assert_eq!(state.selected_device_disk(), Some(6));

    let width = 120;
    let height = 32;
    let content_area = ratatui::layout::Rect::new(0, 2, width, height - 3);
    let viewport = ratatui::layout::Rect::new(0, 0, width, height);
    let popup = edpcli::tui::ui::centered_modal_rect(viewport, 76, 11);

    let mut before_terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    before_terminal
        .draw(|frame| render::draw(frame, &state))
        .unwrap();
    let before = before_terminal.backend().buffer().clone();

    assert!(state.begin_write_wizard_for_identity(
        WriteKind::BackupCreate,
        6,
        None,
        Some(identity),
    ));
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Confirm);

    let mut modal_terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    modal_terminal
        .draw(|frame| render::draw(frame, &state))
        .unwrap();
    let modal = modal_terminal.backend().buffer();
    for y in content_area.y..content_area.bottom() {
        for x in content_area.x..content_area.right() {
            if x >= popup.x && x < popup.right() && y >= popup.y && y < popup.bottom() {
                continue;
            }
            assert_eq!(
                modal[(x, y)].symbol(),
                before[(x, y)].symbol(),
                "backup confirmation must preserve workspace symbol at ({x},{y})"
            );
            assert_eq!(
                modal[(x, y)].style(),
                before[(x, y)].style(),
                "backup confirmation must preserve workspace style at ({x},{y})"
            );
        }
    }

    assert_eq!(
        state.navigate(NavCommand::Escape, 20),
        edpcli::tui::state::StateEffect::None
    );
    assert!(state.wizard().is_none());
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.selected_device_disk(), Some(6));
}

#[test]
fn backup_management_input_is_overlay_on_backups_workspace() {
    let mut state = AppState::new();
    state.navigate(NavCommand::WorkspaceBackups, 20);
    assert_eq!(state.workspace(), Workspace::Backups);

    let width = 120;
    let height = 32;
    let content_area = ratatui::layout::Rect::new(0, 2, width, height - 3);
    let viewport = ratatui::layout::Rect::new(0, 0, width, height);

    let mut before_terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    before_terminal
        .draw(|frame| render::draw(frame, &state))
        .unwrap();
    let before = before_terminal.backend().buffer().clone();

    assert!(state.begin_backup_prune());
    let popup = edpcli::tui::ui::centered_modal_rect(viewport, 78, 5);
    let mut modal_terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    modal_terminal
        .draw(|frame| render::draw(frame, &state))
        .unwrap();
    let modal = modal_terminal.backend().buffer();

    for y in content_area.y..content_area.bottom() {
        for x in content_area.x..content_area.right() {
            if x >= popup.x && x < popup.right() && y >= popup.y && y < popup.bottom() {
                continue;
            }
            assert_eq!(
                modal[(x, y)].symbol(),
                before[(x, y)].symbol(),
                "backup management overlay must preserve workspace symbol at ({x},{y})"
            );
            assert_eq!(
                modal[(x, y)].style(),
                before[(x, y)].style(),
                "backup management overlay must preserve workspace style at ({x},{y})"
            );
        }
    }

    let rendered = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| modal[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.replace(' ', "").contains("备份清理·keep-N"));
    assert!(rendered.replace(' ', "").contains("备份概览"));
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

    for pane in [PaneId::InspectOverview, PaneId::InspectDetail] {
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
fn provision_disk_layout_jk_moves_selection_before_viewport_scrolls() {
    let mut state = provision_state();
    state.provision_focus_pane(PaneId::ProvisionDiskLayout);
    let field_selected = state.provision().field_selected;
    let first_region = state.disk_layout_selected();

    state.provision_move_focused_vertical(1, 20, 100);
    assert_eq!(state.provision().field_selected, field_selected);
    assert!(state.disk_layout_selected() > first_region);
    assert_eq!(
        state
            .pane_viewport(PaneId::ProvisionDiskLayout)
            .scroll_y
            .offset,
        0,
        "selection that remains visible must not move the whole layout page"
    );

    for _ in 0..8 {
        state.provision_move_focused_vertical(1, 7, 100);
    }
    assert!(
        state
            .pane_viewport(PaneId::ProvisionDiskLayout)
            .scroll_y
            .offset
            > 0,
        "viewport should follow only after the selected row reaches the visible edge"
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
fn provision_context_tab_cycles_panes_without_touching_field_selection() {
    let mut state = provision_state();
    state.provision_move_field(3);
    let selected = state.provision().field_selected;
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);

    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    assert_eq!(state.provision().field_selected, selected);

    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
    assert_eq!(state.provision().field_selected, selected);

    state.provision_tab_focus(true);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
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
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
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
    assert!(!collapsed.contains("盘尾恢复节点"));
    state.toggle_disk_layout_tail();
    let expanded = render_text(&state, 160, 45);
    assert!(expanded.contains("盘尾恢复节点"));
    assert!(state
        .disk_layout_detail(&canonical)
        .unwrap()
        .contains("EDP 主协议区"));
    state.disk_layout_move_selection(1, canonical.segments.len());
    assert!(state
        .disk_layout_detail(&canonical)
        .unwrap()
        .contains("保留区域"));
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
    assert!(kinds.contains(&DiskRegionKind::Reserved));
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

fn render_lines(state: &AppState, width: u16, height: u16) -> Vec<String> {
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

#[test]
fn plain_device_row_uses_native_hardware_model_when_protocol_device_id_is_absent() {
    let mut row = device();
    row.device_id = None;
    row.hardware_model = Some("HIKSEMI Portable SSD".into());
    let mut state = AppState::new();
    state.replace_devices(vec![row]);

    let text = render_text(&state, 200, 40);
    assert!(
        text.contains("HIKSEMIPortableSSD"),
        "plain device model should come from native inquiry: {text}"
    );
}

#[test]
fn standard_width_device_tree_keeps_region_label_and_capacity_on_one_line() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    let lines = render_lines(&state, 80, 24);
    let normalized = lines
        .iter()
        .map(|line| line.replace(' ', ""))
        .collect::<Vec<_>>();
    let protocol = normalized
        .iter()
        .find(|line| line.contains("EDP主协议区"))
        .unwrap_or_else(|| panic!("EDP protocol row visible at 80 columns: {normalized:#?}"));
    assert!(
        protocol.contains("6.66KB"),
        "Standard-width device tree must keep region label and capacity together: {protocol:?}"
    );
}

#[test]
fn provision_review_focus_defaults_to_partition_plan_and_navigation_is_pane_local() {
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = PaneFocus::provision_review();
    let field_selected = state.provision().field_selected;

    assert_eq!(
        state.provision_focused_pane(),
        PaneId::ProvisionPartitionPlan
    );
    for expected in [
        PaneId::ProvisionExecutionSummary,
        PaneId::ProvisionDiskLayout,
        PaneId::ProvisionPartitionPlan,
    ] {
        state.provision_shift_pane(false);
        assert_eq!(state.provision_focused_pane(), expected);
    }

    state.provision_focus_pane(PaneId::ProvisionPartitionPlan);
    state.provision_move_focused_vertical(1, 1, 100);
    assert_eq!(state.provision().field_selected, field_selected);
    assert_eq!(state.provision_review_selected_region(), 0);
    assert_eq!(
        state
            .pane_viewport(PaneId::ProvisionPartitionPlan)
            .scroll_y
            .offset,
        0
    );

    for pane in [
        PaneId::ProvisionDiskLayout,
        PaneId::ProvisionExecutionSummary,
    ] {
        state.provision_focus_pane(pane);
        let before = state.pane_viewport(pane).scroll_y.offset;
        state.provision_move_focused_vertical(1, 1, 100);
        assert_eq!(state.provision().field_selected, field_selected, "{pane:?}");
        assert_eq!(
            state.pane_viewport(pane).scroll_y.offset,
            before + 1,
            "{pane:?}"
        );
    }
}

#[test]
fn provision_review_spatial_focus_matches_top_plus_lower_pair_geometry() {
    let mut focus = PaneFocus::provision_review();
    assert_eq!(focus.focused(), PaneId::ProvisionPartitionPlan);
    focus.spatial_provision_review(0, -1);
    assert_eq!(focus.focused(), PaneId::ProvisionDiskLayout);
    focus.spatial_provision_review(0, 1);
    assert_eq!(focus.focused(), PaneId::ProvisionPartitionPlan);
    focus.spatial_provision_review(1, 0);
    assert_eq!(focus.focused(), PaneId::ProvisionExecutionSummary);
    focus.spatial_provision_review(-1, 0);
    assert_eq!(focus.focused(), PaneId::ProvisionPartitionPlan);
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
fn provision_review_without_prepared_snapshot_fails_closed() {
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = PaneFocus::provision_review();

    for (width, height) in [(160, 36), (80, 24)] {
        let rendered = render_text(&state, width, height);
        assert!(rendered.contains("计划确认不可用"), "{rendered}");
        assert!(!rendered.contains("需要勾选格式化"), "{rendered}");
        assert!(!rendered.contains("尚未获得格式化授权"), "{rendered}");
    }
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
        vec![
            "设备",
            "容量",
            "部门",
            "姓名",
            "盘型",
            "状态",
            "备份",
            "身份可靠性",
            "型号",
            "VID:PID",
            "序列号",
        ]
    );
}

#[test]
fn d0_device_info_tree_is_semantic_and_capacity_is_expandable_in_place() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    assert_eq!(
        state.device_info_selected_key(),
        DeviceInfoNodeKey::Capacity
    );
    assert!(state
        .device_info_tree_rows()
        .iter()
        .find(|row| row.key == DeviceInfoNodeKey::Capacity)
        .is_some_and(|row| row.expanded));
    state.device_info_toggle_selected();
    assert!(state
        .device_info_tree_rows()
        .iter()
        .find(|row| row.key == DeviceInfoNodeKey::Capacity)
        .is_some_and(|row| !row.expanded));
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
fn d0_current_device_capacity_uses_thick_full_disk_map() {
    let row = edp_device_with_layout();

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Down, 12);
    let text = render_text(&state, 160, 36);
    assert!(text.contains("容量布局"), "{text}");
    assert!(text.contains("全盘容量地图"), "{text}");
    assert!(!text.contains("当前设备·disk6"), "{text}");
    assert!(text.contains('▄'), "{text}");
    assert!(text.contains('▀'), "{text}");
    assert!(text.contains("启动区"), "{text}");
    assert!(text.contains("交换区"), "{text}");
    assert!(text.contains("保密区"), "{text}");
    assert!(!text.contains("极小区域使用最小可视宽度"), "{text}");
    let presentation = include_str!("../src/tui/devices/presentation.rs");
    assert!(!presentation.contains("let left = \"LBA 0\""));
    assert!(!presentation.contains("LBA {last_lba} · {}"));
}

#[test]
fn device_capacity_tail_expands_in_place_to_real_canonical_children() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let row = edp_device_with_layout();
    let canonical = row.canonical_layout().expect("canonical EDP layout");
    let tail = canonical.tail_group().expect("tail group");
    let expected_children = tail
        .children
        .iter()
        .map(|child| (child.start_lba, child.kind))
        .collect::<Vec<_>>();

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.focus_devices_pane(PaneId::DevicesTree);

    while state.device_info_selected_key() != DeviceInfoNodeKey::TailGroup {
        state.device_info_move_tree(1);
    }
    let collapsed = state.device_info_tree_rows();
    assert!(!collapsed.iter().any(|row| row.depth == 2));

    state.device_info_toggle_selected();
    let expanded = state.device_info_tree_rows();
    let actual_children = expanded
        .iter()
        .filter_map(|row| match row.key {
            DeviceInfoNodeKey::LayoutSegment { start_lba, kind } if row.depth == 2 => {
                Some((start_lba, kind))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_children, expected_children);
}

#[test]
fn switching_devices_falls_back_from_missing_dynamic_segment_to_capacity() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let first = edp_device_with_layout();
    let mut second = device();
    second.disk = 7;
    second.partition_table = None;

    let mut state = AppState::new();
    state.replace_devices(vec![first, second]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.device_info_move_tree(2);
    assert!(matches!(
        state.device_info_selected_key(),
        DeviceInfoNodeKey::LayoutSegment { .. }
    ));

    state
        .pane_viewport_mut(PaneId::DevicesDetail)
        .scroll_y
        .offset = 5;
    state.focus_devices_pane(PaneId::DevicesList);
    state.navigate(NavCommand::Down, 20);

    assert_eq!(state.selected_device_disk(), Some(7));
    assert_eq!(
        state.device_info_selected_key(),
        DeviceInfoNodeKey::Capacity
    );
    assert_eq!(
        state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset,
        0
    );
}

#[test]
fn devices_renderer_has_no_direct_disk_or_filesystem_io() {
    let source = concat!(
        include_str!("../src/tui/devices/render.rs"),
        include_str!("../src/tui/devices/list_render.rs"),
        include_str!("../src/tui/devices/tree_render.rs"),
        include_str!("../src/tui/devices/detail_render.rs"),
        include_str!("../src/tui/devices/presentation.rs"),
    );
    for forbidden in [
        "std::fs",
        "File::open",
        "OpenOptions",
        "scan_disks(",
        "read_disk(",
        "find_backups(",
    ] {
        assert!(
            !source.contains(forbidden),
            "device renderer must stay presentation-only: {forbidden}"
        );
    }
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
            volume_label: Some("DATA".into()),
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
        edpcli::tui::state::DeviceInfoNodeKey::Capacity
    );
    let selected = state.device_info_selected_key();
    state.device_info_focus_detail();
    state.navigate(NavCommand::Down, 2);
    assert_eq!(state.device_info_selected_key(), selected);
    assert_eq!(
        state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset,
        1
    );
}

#[test]
fn device_tree_gg_and_g_jump_to_first_and_last_visible_nodes() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.device_info_move_tree(3);
    assert_ne!(
        state.device_info_selected_key(),
        DeviceInfoNodeKey::Identity
    );

    state.navigate(NavCommand::Top, 12);
    assert_eq!(
        state.device_info_selected_key(),
        DeviceInfoNodeKey::Capacity
    );

    let last = state
        .device_info_tree_rows()
        .last()
        .expect("device info tree has rows")
        .key;
    state.navigate(NavCommand::Bottom, 12);
    assert_eq!(state.device_info_selected_key(), last);
    assert_eq!(last, DeviceInfoNodeKey::Status);
}

#[test]
fn device_status_backup_table_omits_zero_counts_and_selects_restore_source() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut row = edp_device_with_layout();
    row.n_baks = 2;
    row.n_possible_baks = 0;
    let backups = vec![related_backup(1, &row), related_backup(2, &row)];

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.replace_backups(backups);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.device_info_selected_key(), DeviceInfoNodeKey::Status);
    state.device_info_focus_detail();

    assert_eq!(
        state.selected_restore_backup_path(),
        Some("backup-1.edpb".into())
    );
    let text = render_text(&state, 180, 46);
    assert!(text.contains("●2份确认"), "{text}");
    assert!(!text.contains("▲0份疑似"), "{text}");
    for heading in [
        "关系", "时间", "容量", "部门", "姓名", "型号", "盘型", "VID:PID",
    ] {
        assert!(text.contains(heading), "missing {heading}: {text}");
    }
    assert!(text.contains("R恢复当前备份"), "{text}");

    state.navigate(NavCommand::Down, 20);
    assert_eq!(
        state.selected_restore_backup_path(),
        Some("backup-2.edpb".into())
    );
    state.navigate(NavCommand::Top, 20);
    assert_eq!(
        state.selected_restore_backup_path(),
        Some("backup-1.edpb".into())
    );
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(
        state.selected_restore_backup_path(),
        Some("backup-2.edpb".into())
    );
}

#[test]
fn restore_confirmation_prioritizes_backup_device_match_and_basic_identity() {
    use edpcli::application::media_identity::SerialQuality;

    let mut row = edp_device_with_layout();
    row.hardware_model = Some("HIKSEMI".into());
    row.device_id = None;
    if let Some(pin) = row.identity_pin.as_mut() {
        pin.snapshot.hardware.vid = Some(0x1234);
        pin.snapshot.hardware.pid = Some(0x5678);
        pin.snapshot.hardware.serial = row.serial.clone();
        pin.snapshot.hardware.serial_sha256 = Some("11".repeat(32));
        pin.snapshot.hardware.serial_quality = SerialQuality::Usable;
    }
    row.n_baks = 1;
    let mut item = related_backup(1, &row);
    item.display_time = "2026-09-29 14:48".into();
    let path = item.path.clone();

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.replace_backups(vec![item]);
    assert!(state.begin_write_wizard(WriteKind::Restore, 6, Some(path)));

    let text = render_text(&state, 180, 46);
    for expected in [
        "恢复写入确认",
        "匹配度强",
        "物理介质一致",
        "当前设备",
        "HIKSEMI",
        "1234:5678",
        "SERIAL-D0-1234",
        "备份",
        "2026-09-2914:48",
        "输电运检中心",
        "测试用户",
        "身份对照",
        "序列号",
        "VID:PID",
        "容量",
        "onlyid",
        "部门",
        "姓名",
        "✓一致",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("写入前"), "{text}");
    assert!(!text.contains("写入链"), "{text}");
    assert!(!text.contains("恢复内容"), "{text}");
}

#[test]
fn restore_overlay_escape_consumes_visible_layer_before_device_panes() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.device_info_selected_key(), DeviceInfoNodeKey::Status);
    state.device_info_focus_detail();
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesDetail);

    assert!(state.begin_write_wizard(WriteKind::Restore, 6, Some("backup-1.edpb".into())));
    assert_eq!(state.wizard().unwrap().stage, WizardStage::Confirm);
    assert_eq!(state.input_mode(), edpcli::tui::state::InputMode::Confirm);
    state.navigate(NavCommand::Escape, 20);
    assert!(state.wizard().is_none());
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesDetail);
    assert_eq!(state.input_mode(), edpcli::tui::state::InputMode::Normal);
}

#[test]
fn device_tree_g_keeps_context_visible_instead_of_scrolling_to_one_line() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Bottom, 12);

    let text = render_text(&state, 150, 30);
    for label in ["容量布局", "身份与协议", "状态与备份"] {
        assert!(
            text.contains(label),
            "G must keep surrounding tree rows visible; missing {label}: {text}"
        );
    }
}

#[test]
fn device_capacity_map_stays_visible_and_tracks_selected_region() {
    use edpcli::application::disk_layout::DiskRegionKind;
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    let select = |state: &mut AppState, target: DeviceInfoNodeKey| {
        let index = state
            .device_info_tree_rows()
            .iter()
            .position(|row| row.key == target)
            .expect("target is visible");
        state.navigate(NavCommand::Top, 20);
        state.device_info_move_tree(index as isize);
        assert_eq!(state.device_info_selected_key(), target);
    };

    select(&mut state, DeviceInfoNodeKey::Capacity);
    let capacity = render_lines(&state, 180, 46);
    let capacity_text = capacity.join("\n");
    let compact_capacity = capacity_text.replace(' ', "");
    assert!(compact_capacity.contains("全盘容量地图"), "{capacity_text}");
    assert!(capacity_text.contains('▄') && capacity_text.contains('▀'));
    assert!(!capacity_text.contains('▲'));
    let presentation_source = include_str!("../src/tui/devices/presentation.rs");
    assert!(!presentation_source.contains("尾部区域可直接"));
    assert!(!presentation_source.contains("极小区域使用最小可视宽度"));
    assert!(!presentation_source.contains("let left = \"LBA 0\""));

    let boot = state
        .device_info_tree_rows()
        .iter()
        .find_map(|row| match row.key {
            DeviceInfoNodeKey::LayoutSegment { start_lba, kind }
                if kind == DiskRegionKind::Boot =>
            {
                Some(DeviceInfoNodeKey::LayoutSegment { start_lba, kind })
            }
            _ => None,
        })
        .expect("boot segment");
    select(&mut state, boot);
    let boot_lines = render_lines(&state, 180, 46);
    let boot_marker = boot_lines
        .iter()
        .find(|line| line.contains('▲'))
        .expect("boot map marker");
    let boot_col = boot_marker.find('▲').expect("boot marker column");
    assert!(boot_lines
        .join("\n")
        .replace(' ', "")
        .contains("当前区域：启动区"));

    let encrypt = state
        .device_info_tree_rows()
        .iter()
        .find_map(|row| match row.key {
            DeviceInfoNodeKey::LayoutSegment { start_lba, kind }
                if kind == DiskRegionKind::Encrypt =>
            {
                Some(DeviceInfoNodeKey::LayoutSegment { start_lba, kind })
            }
            _ => None,
        })
        .expect("encrypt segment");
    select(&mut state, encrypt);
    let encrypt_lines = render_lines(&state, 180, 46);
    let encrypt_marker = encrypt_lines
        .iter()
        .find(|line| line.contains('▲'))
        .expect("encrypt map marker");
    let encrypt_col = encrypt_marker.find('▲').expect("encrypt marker column");
    assert!(encrypt_lines
        .join("\n")
        .replace(' ', "")
        .contains("当前区域：保密区"));
    assert!(
        encrypt_col > boot_col,
        "marker must track the selected region: boot={boot_col}, encrypt={encrypt_col}"
    );
}

#[test]
fn device_tail_children_keep_the_same_full_disk_map_and_move_marker() {
    use edpcli::application::disk_layout::DiskRegionKind;
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    let tail_index = state
        .device_info_tree_rows()
        .iter()
        .position(|row| row.key == DeviceInfoNodeKey::TailGroup)
        .expect("tail group visible");
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(tail_index as isize);
    state.device_info_toggle_selected();

    let tail_lines = render_lines(&state, 180, 46);
    assert!(tail_lines.iter().any(|line| line.contains('▄')));
    assert!(tail_lines.iter().any(|line| line.contains('▀')));
    let tail_marker = tail_lines
        .iter()
        .find(|line| line.contains('▲'))
        .and_then(|line| line.find('▲'))
        .expect("tail marker");

    let lce = state
        .device_info_tree_rows()
        .iter()
        .find_map(|row| match row.key {
            DeviceInfoNodeKey::LayoutSegment { start_lba, kind } if kind == DiskRegionKind::Lce => {
                Some(DeviceInfoNodeKey::LayoutSegment { start_lba, kind })
            }
            _ => None,
        })
        .expect("LCE child");
    let lce_index = state
        .device_info_tree_rows()
        .iter()
        .position(|row| row.key == lce)
        .expect("LCE child visible");
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(lce_index as isize);

    let lce_lines = render_lines(&state, 180, 46);
    assert!(lce_lines.iter().any(|line| line.contains('▄')));
    assert!(lce_lines.iter().any(|line| line.contains('▀')));
    let lce_marker = lce_lines
        .iter()
        .find(|line| line.contains('▲'))
        .and_then(|line| line.find('▲'))
        .expect("LCE marker");
    assert!(lce_lines
        .join("\n")
        .replace(' ', "")
        .contains("当前区域：LCE"));
    assert!(
        lce_marker <= tail_marker,
        "LCE child marker should resolve within the aggregate tail: lce={lce_marker}, tail={tail_marker}"
    );
}

#[test]
fn device_capacity_map_uses_axis_three_visual_rows_and_selection_card() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Down, 20);

    let lines = render_lines(&state, 180, 46);
    let joined = lines.join("\n").replace(' ', "");
    for tick in ["0%", "25%", "50%", "75%", "100%"] {
        assert!(joined.contains(tick), "missing disk-map tick {tick}");
    }

    let top = lines
        .iter()
        .position(|line| line.contains('▄'))
        .expect("disk map upper half-band");
    assert!(lines[top].chars().filter(|&ch| ch == '▄').count() > 20);
    assert!(!lines[top + 1].contains(['▐', '▌']));
    assert!(!lines[top + 2].contains(['▐', '▌']));
    assert!(lines[top + 3].chars().filter(|&ch| ch == '▀').count() > 20);
    assert!(
        joined.contains("当前选中") || joined.contains("全盘布局"),
        "selection card missing"
    );
}

#[test]
fn device_capacity_map_uses_shared_semantic_component_without_partition_borders() {
    let devices = include_str!("../src/tui/devices/presentation.rs");
    let layout = include_str!("../src/tui/disk_layout.rs");
    let theme = include_str!("../src/tui/theme.rs");
    assert!(devices.contains("DiskCapacityMapProfile::Full"));
    assert!(layout.contains("pub struct DiskCapacityMap"));
    assert!(layout.contains("capacity_map_half_band_line"));
    assert!(layout.contains("disk_region_fill("));
    assert!(
        layout.contains("'┈'"),
        "full map axis should stay lightweight"
    );
    assert!(theme.contains("pub fn disk_region_fill"));
    for source in [devices, layout] {
        assert!(!source.contains("QUADRANT_INSIDE"));
        assert!(!source.contains("disk_map_internal_boundary_span"));
        assert!(!source.contains("disk_map_outer_vertical_span"));
    }
}

#[test]
fn device_capacity_root_has_no_false_active_glyphs_or_tiny_placeholders() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    assert_eq!(
        state.device_info_selected_key(),
        edpcli::tui::state::DeviceInfoNodeKey::Capacity
    );

    let lines = render_lines(&state, 180, 46);
    let top = lines
        .iter()
        .position(|line| line.contains('▄'))
        .expect("disk map upper half-band");
    let map = lines[top..=top + 3].join("\n");
    assert!(
        !map.contains('●') && !map.contains('▲'),
        "capacity root must not look partially active: {map}"
    );
    assert!(!map.contains('▐') && !map.contains('▌'));
    assert!(
        !lines[top + 2].contains("6.6") && !lines[top + 2].contains("25."),
        "tiny regions must not show clipped numeric fragments: {}",
        lines[top + 2]
    );
}

#[test]
fn device_capacity_active_region_uses_fill_and_semantic_marker_without_borders() {
    use edpcli::application::disk_layout::DiskRegionKind;
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    let target_index = state
        .device_info_tree_rows()
        .iter()
        .position(|row| {
            matches!(
                row.key,
                DeviceInfoNodeKey::LayoutSegment {
                    kind: DiskRegionKind::Encrypt,
                    ..
                }
            )
        })
        .expect("encrypt layout segment");
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(target_index as isize);

    let lines = render_lines(&state, 180, 46);
    let joined = lines.join("\n");
    assert!(joined.contains('▄') && joined.contains('▀'));
    assert!(
        joined.contains('▲'),
        "active region must expose its marker: {joined}"
    );
    let source = include_str!("../src/tui/devices/presentation.rs");
    assert!(!source.contains("QUADRANT_INSIDE"));
    assert!(!source.contains("disk_map_internal_boundary_span"));
    assert!(!source.contains("disk_map_outer_vertical_span"));
}

#[test]
fn device_capacity_active_region_keeps_original_label_without_dot_prefix() {
    use edpcli::tui::state::DeviceInfoNodeKey;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    let target_index = state
        .device_info_tree_rows()
        .iter()
        .position(|row| {
            matches!(row.key, DeviceInfoNodeKey::LayoutSegment { .. }) && row.depth == 1
        })
        .expect("top-level layout segment");
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(target_index as isize);

    assert!(
        !include_str!("../src/tui/devices/presentation.rs")
            .contains("format!(\"● {}\", segment.label)"),
        "active capacity region must not rewrite the partition label"
    );
    let lines = render_lines(&state, 180, 46);
    let top = lines
        .iter()
        .position(|line| line.contains('▄'))
        .expect("disk map upper half-band");
    assert!(
        !lines[top + 1].contains('●'),
        "active map label must not be rewritten with a dot prefix: {}",
        lines[top + 1]
    );
}

#[test]
fn device_capacity_tree_and_region_list_use_partition_semantic_colors() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(1);
    state.device_info_toggle_selected();

    let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let palette = edpcli::tui::theme::current().palette();

    let boot_colored = buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "启" && cell.style().fg == Some(palette.partition_boot))
        .count();
    let share_colored = buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "交" && cell.style().fg == Some(palette.partition_share))
        .count();
    let encrypt_colored = buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "保" && cell.style().fg == Some(palette.partition_encrypt))
        .count();

    let tree_source = include_str!("../src/tui/devices/tree_render.rs");
    assert!(
        tree_source.contains("DeviceInfoNodeKey::LayoutSegment { kind, .. }")
            && tree_source.contains("disk_region_tree(kind, active)"),
        "device capacity tree children must derive normal/active styling from DiskRegionKind"
    );
    let presentation_source = include_str!("../src/tui/devices/presentation.rs");
    let region_list_source = include_str!("../src/tui/disk_region_list/render.rs");
    assert!(
        presentation_source.contains("disk_region_list_lines")
            && region_list_source.contains("disk_region(segment.kind)"),
        "device and result region lists must share DiskRegionKind-derived semantic colors"
    );
    assert!(
        boot_colored >= 1,
        "启动区 should render with boot semantic color; got {boot_colored}"
    );
    assert!(
        share_colored >= 1,
        "交换区 should render with share semantic color; got {share_colored}"
    );
    assert!(
        encrypt_colored >= 1,
        "保密区 should render with encrypt semantic color; got {encrypt_colored}"
    );
}

#[test]
fn device_capacity_tree_selected_partition_uses_brighter_text_without_fill() {
    use ratatui::style::Modifier;

    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);

    let target_index = state
        .device_info_tree_rows()
        .iter()
        .position(|row| {
            matches!(
                row.key,
                edpcli::tui::state::DeviceInfoNodeKey::LayoutSegment {
                    kind: DiskRegionKind::Encrypt,
                    ..
                }
            )
        })
        .expect("encrypt layout segment");
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(target_index as isize);

    let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let expected = edpcli::tui::theme::current().disk_region_tree(DiskRegionKind::Encrypt, true);
    assert_eq!(
        expected.bg, None,
        "selected capacity-tree child must not add a semantic background"
    );
    let selected_encrypt_cells = buffer
        .content()
        .iter()
        .filter(|cell| {
            cell.symbol() == "保"
                && cell.style().fg == expected.fg
                && cell.style().add_modifier.contains(Modifier::BOLD)
        })
        .count();
    assert!(
        selected_encrypt_cells >= 1,
        "selected capacity-tree child must brighten its semantic text without painting the row"
    );
}

#[test]
fn device_capacity_map_half_bands_align_exactly_with_content_fills() {
    let mut state = AppState::new();
    state.replace_devices(vec![edp_device_with_layout()]);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Top, 20);
    state.device_info_move_tree(1);

    let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let surface = edpcli::tui::theme::current()
        .raised_surface()
        .bg
        .expect("raised card surface background");

    let top_y = (0..60)
        .find(|&y| (0..200).filter(|&x| buffer[(x, y)].symbol() == "▄").count() > 20)
        .expect("disk-map upper half-band");
    let band_cells = (0..200)
        .filter(|&x| buffer[(x, top_y)].symbol() == "▄")
        .collect::<Vec<_>>();
    let left = *band_cells.first().expect("half-band start");
    let right = *band_cells.last().expect("half-band end");
    let label_y = top_y + 1;
    let value_y = top_y + 2;
    let bottom_y = top_y + 3;

    for x in left..=right {
        assert_eq!(
            buffer[(x, top_y)].symbol(),
            "▄",
            "upper half-band gap at x={x}"
        );
        assert_eq!(
            buffer[(x, bottom_y)].symbol(),
            "▀",
            "lower half-band gap at x={x}"
        );
        let top_style = buffer[(x, top_y)].style();
        let label_style = buffer[(x, label_y)].style();
        let value_style = buffer[(x, value_y)].style();
        let bottom_style = buffer[(x, bottom_y)].style();
        assert_eq!(
            top_style.bg,
            Some(surface),
            "upper outer half must inherit the enclosing card surface at x={x}"
        );
        assert_eq!(
            bottom_style.bg,
            Some(surface),
            "lower outer half must inherit the enclosing card surface at x={x}"
        );
        assert_eq!(
            top_style.fg, value_style.bg,
            "upper half-band must use the same fill as the stable value row at x={x}"
        );
        assert_eq!(
            value_style.bg, bottom_style.fg,
            "lower half-band must use the same fill as the stable value row at x={x}"
        );
        if label_style.bg != Some(ratatui::style::Color::Reset) {
            assert_eq!(
                label_style.bg, value_style.bg,
                "label row must share the fill except on wide-glyph continuation cells at x={x}"
            );
        }
    }

    let presentation = include_str!("../src/tui/devices/presentation.rs");
    assert!(!presentation.contains("QUADRANT_INSIDE"));
    assert!(!presentation.contains("disk_map_internal_boundary_span"));
    assert!(!presentation.contains("disk_map_outer_vertical_span"));
}
