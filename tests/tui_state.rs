use edpcli::tui::state::{
    AppState, InputMode, NavCommand, ProvisionForm, ProvisionKind, ProvisionStage, StateEffect,
    Workspace,
};

fn configured_quick_unit_label(unit: edpcli::provision::QuickCapacityUnit) -> &'static str {
    use edpcli::application::support::{CapacityUnitSystem, CAPACITY_UNIT_SYSTEM};
    use edpcli::provision::QuickCapacityUnit;
    match (CAPACITY_UNIT_SYSTEM, unit) {
        (CapacityUnitSystem::Decimal, QuickCapacityUnit::MiB) => "MB",
        (CapacityUnitSystem::Decimal, QuickCapacityUnit::GiB) => "GB",
        (CapacityUnitSystem::Binary, QuickCapacityUnit::MiB) => "MiB",
        (CapacityUnitSystem::Binary, QuickCapacityUnit::GiB) => "GiB",
    }
}

fn configured_quick_unit_bytes(unit: edpcli::provision::QuickCapacityUnit) -> u64 {
    use edpcli::application::support::{CapacityUnitSystem, CAPACITY_UNIT_SYSTEM};
    use edpcli::provision::QuickCapacityUnit;
    match (CAPACITY_UNIT_SYSTEM, unit) {
        (CapacityUnitSystem::Decimal, QuickCapacityUnit::MiB) => 1_000_000,
        (CapacityUnitSystem::Decimal, QuickCapacityUnit::GiB) => 1_000_000_000,
        (CapacityUnitSystem::Binary, QuickCapacityUnit::MiB) => 1_048_576,
        (CapacityUnitSystem::Binary, QuickCapacityUnit::GiB) => 1_073_741_824,
    }
}

fn configured_capacity_3(sectors: u64, unit: edpcli::provision::QuickCapacityUnit) -> String {
    let bytes = sectors as u128 * edpcli::application::support::SECTOR as u128;
    let unit_bytes = configured_quick_unit_bytes(unit) as u128;
    let scaled = (bytes * 1_000 + unit_bytes / 2) / unit_bytes;
    format!("{}.{:03}", scaled / 1_000, scaled % 1_000)
}

fn configured_text_to_sectors(value: &str, unit: edpcli::provision::QuickCapacityUnit) -> u64 {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let denominator = 10u128.pow(fraction.len() as u32);
    let numerator = whole.parse::<u128>().unwrap() * denominator
        + if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u128>().unwrap()
        };
    let scaled_bytes = numerator * configured_quick_unit_bytes(unit) as u128;
    let sector_denominator = denominator * edpcli::application::support::SECTOR as u128;
    let quotient = scaled_bytes / sector_denominator;
    let remainder = scaled_bytes % sector_denominator;
    u64::try_from(quotient + u128::from(remainder * 2 >= sector_denominator)).unwrap()
}

#[test]
fn chapter_11_provision_escape_restores_device_selection() {
    use edpcli::tui::table_layout::TableKind;
    let mut state = AppState::new();
    let first = device(64_000_000_000);
    let mut second = device(128_000_000_000);
    second.disk = 7;
    state.replace_devices(vec![first, second]);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(7));
    state.scroll_table_for_viewport(TableKind::Devices, false, 80);
    state.scroll_table_for_viewport(TableKind::Devices, false, 80);
    let expected_scroll = state.table_scroll_offset(TableKind::Devices);
    assert!(expected_scroll > 0);
    state.begin_provision_for_selected_device().unwrap();
    assert!(state.provision_scheme_picker_open());
    assert_eq!(state.workspace(), Workspace::Devices);
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    assert_eq!(state.navigation().depth(), 1);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.selected_device_disk(), Some(7));
    assert_eq!(
        state.table_scroll_offset(TableKind::Devices),
        expected_scroll
    );
    assert_eq!(state.navigation().depth(), 0);
}

#[test]
fn provision_result_escape_returns_directly_to_originating_devices_workspace() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().stage = ProvisionStage::Result;
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.selected_device_disk(), Some(6));
    assert_eq!(state.navigation().depth(), 0);
}

#[test]
fn provision_result_survives_transient_device_rescan_absence() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().stage = ProvisionStage::Result;
    state.provision_mut().result_plan = Some(plain_result_plan());
    state.provision_initialize_result_workbench();

    assert_eq!(state.provision_target_disk(), Some(6));
    assert_eq!(state.selected_device_disk(), Some(6));

    state.replace_devices(vec![]);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Result);
    assert!(state.provision().result_plan.is_some());
    assert_eq!(state.provision_target_disk(), Some(6));
    assert_eq!(state.selected_device_disk(), None);

    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Result);
    assert!(state.provision().result_plan.is_some());
    assert_eq!(state.provision_target_disk(), Some(6));
    assert_eq!(state.selected_device_disk(), Some(6));
}

fn plain_result_plan() -> edpcli::tui::state::ProvisionResultSnapshot {
    use edpcli::application::filesystem::FilesystemKind;
    use edpcli::tui::state::{ProvisionResultPartition, ProvisionResultSnapshot};

    ProvisionResultSnapshot {
        disk: 6,
        target: edpcli::provision::ProvisionTarget::Plain,
        total_bytes: 20_000 * 512,
        partitions: vec![
            ProvisionResultPartition {
                role: None,
                filesystem: Some(FilesystemKind::ExFat),
                start_lba: 2_048,
                size_bytes: 2_000 * 512,
                selected_for_format: true,
                disposition: None,
            },
            ProvisionResultPartition {
                role: None,
                filesystem: Some(FilesystemKind::Ntfs),
                start_lba: 6_000,
                size_bytes: 3_000 * 512,
                selected_for_format: true,
                disposition: None,
            },
        ],
    }
}

#[test]
fn provision_result_partition_and_region_selection_sync_by_exact_geometry() {
    use edpcli::tui::{
        disk_layout::{DiskCapacitySelection, DiskRegionKind},
        pane::PaneId,
    };

    let mut state = AppState::new();
    state.replace_devices(vec![device(20_000 * 512)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().stage = ProvisionStage::Result;
    state.provision_mut().result_plan = Some(plain_result_plan());
    state.provision_initialize_result_workbench();

    assert_eq!(
        state.provision().result_workbench.selected_partition,
        Some(0)
    );
    let first = state
        .provision()
        .result_plan
        .as_ref()
        .unwrap()
        .partition_selection(0)
        .unwrap();
    assert_eq!(
        state.provision().result_workbench.region_selection(),
        Some(first)
    );

    state.provision_result_move(1, 8);
    assert_eq!(
        state.provision().result_workbench.selected_partition,
        Some(1)
    );
    let second = state
        .provision()
        .result_plan
        .as_ref()
        .unwrap()
        .partition_selection(1)
        .unwrap();
    assert_eq!(
        state.provision().result_workbench.region_selection(),
        Some(second)
    );

    let model = state
        .provision()
        .result_plan
        .as_ref()
        .unwrap()
        .disk_layout_model()
        .unwrap();
    let free = model
        .collapsed_tail_model()
        .segments
        .into_iter()
        .find(|segment| segment.kind == DiskRegionKind::Free)
        .expect("plain fixture must contain free space");
    let free_selection = DiskCapacitySelection::from_segment(&free).unwrap();

    state.provision_focus_pane(PaneId::ResultDiskLayout);
    assert!(state
        .provision_mut()
        .result_workbench
        .select_region_geometry(&model, &free_selection, 8));
    state.provision_result_move(0, 8);

    assert_eq!(state.provision().result_workbench.selected_partition, None);
    assert_eq!(
        state.provision().result_workbench.region_selection(),
        Some(free_selection)
    );
}

#[test]
fn provision_result_cycles_only_result_workbench_panes() {
    use edpcli::tui::pane::PaneId;

    let mut state = AppState::new();
    state.replace_devices(vec![device(20_000 * 512)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().stage = ProvisionStage::Result;

    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);
    let kind = edpcli::tui::table_layout::TableKind::ResultPartitions;
    assert_eq!(state.table_active_column(kind), 0);
    assert!(state.move_table_column_for_viewport(kind, false, 160));
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);
    assert_eq!(state.table_active_column(kind), 1);
    for _ in 0..20 {
        let _ = state.move_table_column_for_viewport(kind, false, 160);
    }
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);
    assert_eq!(state.table_active_column(kind), 5);
    assert!(!state.move_table_column_for_viewport(kind, false, 160));
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);

    state.provision_result_spatial_focus(1, 0);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultDiskLayout);
    state.provision_result_spatial_focus(0, 1);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultVerification);
    state.provision_result_spatial_focus(-1, 0);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);

    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultDiskLayout);
    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultVerification);
    state.provision_tab_focus(false);
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);
}

#[test]
fn provision_result_table_supports_edges_and_visual_order_copy_contract() {
    use edpcli::tui::{pane::PaneId, table_layout::TableKind};

    let mut state = AppState::new();
    state.replace_devices(vec![device(20_000 * 512)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().stage = ProvisionStage::Result;
    state.provision_mut().result_plan = Some(plain_result_plan());
    state.provision_initialize_result_workbench();

    let kind = TableKind::ResultPartitions;
    assert_eq!(state.provision_focused_pane(), PaneId::ResultPartitions);
    assert_eq!(state.active_table_kind(), Some(kind));

    assert!(state.move_table_column_edge_for_viewport(kind, true, 160));
    assert_eq!(state.table_active_column(kind), 5);
    assert!(state.move_table_column_edge_for_viewport(kind, false, 160));
    assert_eq!(state.table_active_column(kind), 0);
    assert_eq!(state.table_copy_payload(kind, false).as_deref(), Some("P1"));

    assert!(state.reorder_table_column_for_viewport(kind, false, 160));
    assert_eq!(state.table_column_order(kind)[..2], [1, 0]);
    let row = state
        .table_copy_payload(kind, true)
        .expect("Result whole-row copy");
    let cells = row.split('\t').collect::<Vec<_>>();
    assert_eq!(cells[0], "普通分区");
    assert_eq!(cells[1], "P1");
    assert_eq!(cells.len(), 6);
}

#[test]
fn provision_review_escape_restores_valid_form_focus_and_insert() {
    use edpcli::tui::pane::{PaneFocus, PaneId};

    for review_pane in [
        PaneId::ProvisionPartitionPlan,
        PaneId::ProvisionExecutionSummary,
    ] {
        let mut state = AppState::new();
        state.replace_devices(vec![device(64_000_000_000)]);
        assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
        state.provision_mut().field_selected = 0;
        state.provision_mut().pane_focus = PaneFocus::provision_review();
        state.provision_focus_pane(review_pane);
        state.provision_mut().stage = ProvisionStage::Review;

        assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
        assert_eq!(state.provision().stage, ProvisionStage::Form);
        assert_eq!(state.provision_focused_pane(), PaneId::ProvisionParameters);
        assert!(
            state.provision_begin_insert(),
            "review pane {review_pane:?} must return to an editable form focus"
        );
        assert_eq!(state.input_mode(), InputMode::Insert);
    }
}

#[test]
fn provision_review_subflows_return_to_exact_review_pane() {
    use edpcli::tui::pane::{PaneFocus, PaneId};

    for stage in [ProvisionStage::Confirm, ProvisionStage::ExportPath] {
        let mut state = AppState::new();
        state.replace_devices(vec![device(64_000_000_000)]);
        assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
        state.provision_mut().pane_focus = PaneFocus::provision_review();
        state.provision_focus_pane(PaneId::ProvisionExecutionSummary);
        state.provision_mut().stage = stage;

        assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
        assert_eq!(state.provision().stage, ProvisionStage::Review);
        assert_eq!(
            state.provision_focused_pane(),
            PaneId::ProvisionExecutionSummary
        );
    }
}

#[test]
fn provision_review_escape_restores_form_layout_snapshot() {
    use edpcli::tui::pane::{PaneFocus, PaneId};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().field_selected = 0;
    state.provision_mut().field_cursor = 2;
    state.provision_focus_pane(PaneId::ProvisionDiskLayout);
    state.provision_set_planning();

    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = PaneFocus::provision_review();
    state.provision_focus_pane(PaneId::ProvisionExecutionSummary);

    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    assert_eq!(state.provision().field_selected, 0);
    assert_eq!(state.provision_field_cursor(), 2);
    assert_eq!(state.input_mode(), InputMode::Normal);
    assert!(state.provision().prepared.is_none());
}

fn device(size: u64) -> edpcli::cli::Row {
    let mut row = edpcli::cli::Row {
        disk: 6,
        size,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        serial: None,
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
    crate::common::confirm_row_identity(&mut row);
    row
}

fn official_device(size: u64, kind: edpcli::provision::DiskProvisionKind) -> edpcli::cli::Row {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::DiskProvisionKind;

    let mut row = device(size);
    row.provision_kind = kind;
    row.partitions = Some(match kind {
        DiskProvisionKind::Mode0 => vec![
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
                size_bytes: 4_000_000 * 512,
            },
            EdpfPartition {
                ptype: 4,
                active: 1,
                enc: 1,
                start_lba: 4_020_480,
                size_bytes: 2_097_153 * 512,
            },
        ],
        DiskProvisionKind::Mode1 => vec![
            EdpfPartition {
                ptype: 2,
                active: 1,
                enc: 0,
                start_lba: 63,
                size_bytes: 6_020_417 * 512,
            },
            EdpfPartition {
                ptype: 4,
                active: 1,
                enc: 1,
                start_lba: 6_020_480,
                size_bytes: 2_097_153 * 512,
            },
        ],
        DiskProvisionKind::Mode2 => vec![
            EdpfPartition {
                ptype: 1,
                active: 1,
                enc: 0,
                start_lba: 63,
                size_bytes: 63 * 512,
            },
            EdpfPartition {
                ptype: 4,
                active: 1,
                enc: 1,
                start_lba: 126,
                size_bytes: 8_000_000 * 512,
            },
        ],
        DiskProvisionKind::Mode3 => vec![
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
                size_bytes: 6_000_000 * 512,
            },
        ],
        DiskProvisionKind::Plain => panic!("official_device requires an EDP mode"),
    });
    crate::common::confirm_row_identity(&mut row);
    row
}

fn mode0_device(size: u64) -> edpcli::cli::Row {
    official_device(size, edpcli::provision::DiskProvisionKind::Mode0)
}

fn authorize_all_rebuilds(state: &mut AppState, kind: ProvisionKind) {
    match kind {
        ProvisionKind::Mode0 => {
            state.provision_mut().form.format_boot = true;
            state.provision_mut().form.format_share = true;
            state.provision_mut().form.format_encrypt = true;
        }
        ProvisionKind::Mode1 => {
            state.provision_mut().form.format_share = true;
            state.provision_mut().form.format_encrypt = true;
        }
        ProvisionKind::Mode2 => {
            state.provision_mut().form.format_encrypt = true;
        }
        ProvisionKind::Mode3 => {
            state.provision_mut().form.format_boot = true;
            state.provision_mut().form.format_share = true;
        }
        ProvisionKind::Plain => {}
    }
}

fn authorize_plain_mode0_rebuild(state: &mut AppState) {
    authorize_all_rebuilds(state, ProvisionKind::Mode0);
}

fn finish_default_key_probe_for_selected_source(state: &mut AppState) {
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};

    let source_kind = state
        .selected_device()
        .and_then(|row| row.confirmed_provision_kind())
        .expect("confirmed source kind");
    let (share, encrypt) = match source_kind {
        DiskProvisionKind::Plain => (None, None),
        DiskProvisionKind::Mode0 | DiskProvisionKind::Mode1 => (
            Some(SourcePasswordKnowledge::DefaultVerified),
            Some(SourcePasswordKnowledge::DefaultVerified),
        ),
        DiskProvisionKind::Mode2 => (None, Some(SourcePasswordKnowledge::DefaultVerified)),
        DiskProvisionKind::Mode3 => (Some(SourcePasswordKnowledge::DefaultVerified), None),
    };
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind,
        share,
        share_opaque_profile: share.is_some(),
        encrypt,
        encrypt_opaque_profile: encrypt.is_some(),
    }));
}

fn enter_provision(state: &mut AppState) -> ProvisionKind {
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    let kind = state.provision_begin_selected();
    state.provision_enter_form_workspace();
    if kind != ProvisionKind::Plain {
        finish_default_key_probe_for_selected_source(state);
    }
    kind
}

fn enter_provision_kind(state: &mut AppState, index: usize) -> ProvisionKind {
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_select_scheme_index(index));
    let kind = state.provision_begin_selected();
    state.provision_enter_form_workspace();
    if kind != ProvisionKind::Plain {
        finish_default_key_probe_for_selected_source(state);
    }
    kind
}

#[test]
fn ch14_partition_layout_has_typed_status_and_concise_selection_summary() {
    use edpcli::tui::disk_layout::DiskLayoutDetailTone;
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .unwrap();
    state.provision_mut().field_selected = encrypt;
    let details = state.provision_layout_editor_details();
    let partitions = details
        .iter()
        .filter_map(|row| row.columns.as_ref())
        .filter(|columns| columns[3] == "⚠ 需重建")
        .collect::<Vec<_>>();
    assert_eq!(partitions.len(), 3);
    assert!(partitions.iter().all(|columns| columns[3] == "⚠ 需重建"));
    assert!(!details
        .iter()
        .filter_map(|row| row.columns.as_ref())
        .any(|columns| { columns[3] == "… 待计划" }));
    assert!(details.iter().any(|row| row.text.starts_with("当前区域  ")));
    assert!(details.iter().any(|row| row.text.starts_with("LBA ")));
    assert!(!details.iter().any(|row| row.text.starts_with("原因      ")));
    assert!(details
        .iter()
        .any(|row| row.tone == DiskLayoutDetailTone::Success));
}

#[test]
fn plain_source_to_every_official_mode_has_no_password_probe_or_pending_region() {
    use edpcli::provision::{
        KeyDomainRole, DEFAULT_KEY_DOMAIN_PASSWORD, DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
    };

    let cases = [
        (0usize, ProvisionKind::Mode0, 3usize, true, true),
        (1, ProvisionKind::Mode1, 2, true, true),
        (2, ProvisionKind::Mode2, 2, false, true),
        (3, ProvisionKind::Mode3, 2, true, false),
    ];

    for (scheme, kind, partition_count, share_active, encrypt_active) in cases {
        let mut state = AppState::new();
        state.replace_devices(vec![device(64_000_000_000)]);
        assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
        assert!(state.provision_select_scheme_index(scheme));
        assert_eq!(state.provision_begin_selected(), kind);
        state.provision_enter_form_workspace();

        let fields = state.provision_visible_fields();
        let source_passwords = fields
            .iter()
            .enumerate()
            .filter(|(_, (label, _, _))| label == "原密码")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            source_passwords.len(),
            usize::from(share_active) + usize::from(encrypt_active),
            "{kind:?} source password rows"
        );
        for index in source_passwords {
            state.provision_mut().field_selected = index;
            assert!(
                !state.provision_selected_field_is_editable(),
                "Plain -> {kind:?} must not edit a non-existent source password"
            );
            assert_eq!(
                state.provision_source_password_verify_request().unwrap(),
                None,
                "Plain -> {kind:?} must never request source password verification"
            );
        }

        for (_, value, secret) in state
            .provision_visible_fields()
            .iter()
            .filter(|(label, _, _)| label == "新密码")
        {
            assert_eq!(
                *value, DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
                "Plain -> {kind:?} default target password"
            );
            assert!(*secret, "target password must remain a secret field");
        }

        let before = state.provision_layout_editor_details();
        assert!(
            !before
                .iter()
                .filter_map(|row| row.columns.as_ref())
                .any(|columns| {
                    matches!(columns[3].as_str(), "… 待计划" | "? 待确认" | "⚠ 计划异常")
                }),
            "Plain -> {kind:?} must have a complete synchronous preflight decision"
        );
        let destructive = before
            .iter()
            .filter_map(|row| row.columns.as_ref())
            .filter(|columns| matches!(columns[3].as_str(), "⚠ 需重建" | "⚠ 重建"))
            .count();
        assert_eq!(
            destructive, partition_count,
            "Plain -> {kind:?} region count"
        );

        let request = state
            .provision_request()
            .unwrap_or_else(|error| panic!("Plain -> {kind:?} request failed: {error}"));
        match kind {
            ProvisionKind::Mode0 => {
                assert!(request.format.boot);
                assert!(request.format.share);
                assert!(request.format.encrypt);
            }
            ProvisionKind::Mode1 => {
                assert!(!request.format.boot);
                assert!(request.format.share);
                assert!(request.format.encrypt);
            }
            ProvisionKind::Mode2 => {
                assert!(!request.format.boot);
                assert!(!request.format.share);
                assert!(request.format.encrypt);
            }
            ProvisionKind::Mode3 => {
                assert!(request.format.boot);
                assert!(request.format.share);
                assert!(!request.format.encrypt);
            }
            ProvisionKind::Plain => unreachable!(),
        }
        let assert_domain = |domain: KeyDomainRole, active: bool| {
            let pair = request.key_domains.pair(domain);
            if active {
                assert!(
                    pair.source_password.is_none(),
                    "Plain -> {kind:?} source secret"
                );
                assert_eq!(
                    pair.target_password
                        .as_ref()
                        .map(|secret| secret.as_bytes()),
                    Some(DEFAULT_KEY_DOMAIN_PASSWORD),
                    "Plain -> {kind:?} target password"
                );
            } else {
                assert!(pair.source_password.is_none());
                assert!(pair.target_password.is_none());
            }
        };
        assert_domain(KeyDomainRole::Share, share_active);
        assert_domain(KeyDomainRole::Encrypt, encrypt_active);
    }
}

#[test]
fn verified_edp_source_target_matrix_has_no_pending_backend_status() {
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};

    #[derive(Clone, Copy)]
    struct Case {
        source: DiskProvisionKind,
        target_scheme: usize,
        target_kind: ProvisionKind,
        preserve: usize,
        passthrough: usize,
        rebuild: usize,
    }

    let cases = [
        Case {
            source: DiskProvisionKind::Mode0,
            target_scheme: 0,
            target_kind: ProvisionKind::Mode0,
            preserve: 1,
            passthrough: 2,
            rebuild: 0,
        },
        Case {
            source: DiskProvisionKind::Mode0,
            target_scheme: 1,
            target_kind: ProvisionKind::Mode1,
            preserve: 0,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode0,
            target_scheme: 2,
            target_kind: ProvisionKind::Mode2,
            preserve: 0,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode0,
            target_scheme: 3,
            target_kind: ProvisionKind::Mode3,
            preserve: 1,
            passthrough: 1,
            rebuild: 0,
        },
        Case {
            source: DiskProvisionKind::Mode1,
            target_scheme: 0,
            target_kind: ProvisionKind::Mode0,
            preserve: 0,
            passthrough: 1,
            rebuild: 2,
        },
        Case {
            source: DiskProvisionKind::Mode1,
            target_scheme: 1,
            target_kind: ProvisionKind::Mode1,
            preserve: 0,
            passthrough: 2,
            rebuild: 0,
        },
        Case {
            source: DiskProvisionKind::Mode1,
            target_scheme: 2,
            target_kind: ProvisionKind::Mode2,
            preserve: 0,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode1,
            target_scheme: 3,
            target_kind: ProvisionKind::Mode3,
            preserve: 0,
            passthrough: 0,
            rebuild: 2,
        },
        Case {
            source: DiskProvisionKind::Mode2,
            target_scheme: 0,
            target_kind: ProvisionKind::Mode0,
            preserve: 0,
            passthrough: 0,
            rebuild: 3,
        },
        Case {
            source: DiskProvisionKind::Mode2,
            target_scheme: 1,
            target_kind: ProvisionKind::Mode1,
            preserve: 0,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode2,
            target_scheme: 2,
            target_kind: ProvisionKind::Mode2,
            preserve: 0,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode2,
            target_scheme: 3,
            target_kind: ProvisionKind::Mode3,
            preserve: 0,
            passthrough: 0,
            rebuild: 2,
        },
        Case {
            source: DiskProvisionKind::Mode3,
            target_scheme: 0,
            target_kind: ProvisionKind::Mode0,
            preserve: 1,
            passthrough: 1,
            rebuild: 1,
        },
        Case {
            source: DiskProvisionKind::Mode3,
            target_scheme: 1,
            target_kind: ProvisionKind::Mode1,
            preserve: 0,
            passthrough: 0,
            rebuild: 2,
        },
        Case {
            source: DiskProvisionKind::Mode3,
            target_scheme: 2,
            target_kind: ProvisionKind::Mode2,
            preserve: 0,
            passthrough: 0,
            rebuild: 2,
        },
        Case {
            source: DiskProvisionKind::Mode3,
            target_scheme: 3,
            target_kind: ProvisionKind::Mode3,
            preserve: 1,
            passthrough: 1,
            rebuild: 0,
        },
    ];

    for case in cases {
        let mut state = AppState::new();
        state.replace_devices(vec![official_device(64_000_000_000, case.source)]);
        assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
        assert!(state.provision_select_scheme_index(case.target_scheme));
        assert_eq!(state.provision_begin_selected(), case.target_kind);
        state.provision_enter_form_workspace();
        let (share, encrypt) = match case.source {
            DiskProvisionKind::Mode0 | DiskProvisionKind::Mode1 => (
                Some(SourcePasswordKnowledge::DefaultVerified),
                Some(SourcePasswordKnowledge::DefaultVerified),
            ),
            DiskProvisionKind::Mode2 => (None, Some(SourcePasswordKnowledge::DefaultVerified)),
            DiskProvisionKind::Mode3 => (Some(SourcePasswordKnowledge::DefaultVerified), None),
            DiskProvisionKind::Plain => unreachable!(),
        };
        state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
            source_kind: case.source,
            share,
            share_opaque_profile: share.is_some(),
            encrypt,
            encrypt_opaque_profile: encrypt.is_some(),
        }));

        let details = state.provision_layout_editor_details();
        let statuses = details
            .iter()
            .filter_map(|row| row.columns.as_ref())
            .map(|columns| columns[3].as_str())
            .collect::<Vec<_>>();
        assert!(
            statuses.iter().all(|status| {
                !matches!(*status, "… 待计划" | "? 待确认" | "⚠ 计划异常")
            }),
            "{:?} -> {:?} must have a complete synchronous preflight decision: {statuses:?}",
            case.source,
            case.target_kind
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| **status == "✓ 候选保留")
                .count(),
            case.preserve,
            "{:?} -> {:?} preserve count",
            case.source,
            case.target_kind
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| **status == "✓ 透传")
                .count(),
            case.passthrough,
            "{:?} -> {:?} passthrough count",
            case.source,
            case.target_kind
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| matches!(**status, "⚠ 需重建" | "⚠ 重建"))
                .count(),
            case.rebuild,
            "{:?} -> {:?} rebuild count: {statuses:?}",
            case.source,
            case.target_kind
        );
    }
}

#[test]
fn unknown_nonopaque_key_profile_is_rebuild_not_pending_backend() {
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_select_scheme_index(0));
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
    state.provision_enter_form_workspace();
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: false,
        encrypt: Some(SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));

    let statuses = state
        .provision_layout_editor_details()
        .into_iter()
        .filter_map(|row| row.columns)
        .map(|columns| (columns[0].clone(), columns[3].clone()))
        .collect::<Vec<_>>();
    assert!(
        statuses
            .iter()
            .any(|(region, status)| region == "交换区" && status == "⚠ 需重建"),
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|(_, status)| {
            !matches!(status.as_str(), "… 待计划" | "? 待确认" | "⚠ 计划异常")
        }),
        "unsupported key profiles must still receive a complete synchronous decision: {statuses:?}"
    );
}

#[test]
fn every_source_kind_to_plain_uses_one_clean_plain_plan_without_password_fields() {
    use edpcli::provision::DiskProvisionKind;

    let sources = [
        DiskProvisionKind::Plain,
        DiskProvisionKind::Mode0,
        DiskProvisionKind::Mode1,
        DiskProvisionKind::Mode2,
        DiskProvisionKind::Mode3,
    ];
    for source in sources {
        let row = if source == DiskProvisionKind::Plain {
            device(64_000_000_000)
        } else {
            official_device(64_000_000_000, source)
        };
        let mut state = AppState::new();
        state.replace_devices(vec![row]);
        assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
        assert!(state.provision_select_scheme_index(4));
        assert_eq!(state.provision_begin_selected(), ProvisionKind::Plain);
        state.provision_enter_form_workspace();

        let fields = state.provision_visible_fields();
        assert!(fields.iter().all(|(label, _, _)| {
            !label.contains("密码") && !label.contains("FileKey") && !label.contains("迁移")
        }));
        let plan = state
            .provision_plain_plan()
            .unwrap_or_else(|error| panic!("{source:?} -> Plain: {error}"));
        assert_eq!(plan.partitions.len(), 1, "{source:?} -> Plain");
        assert_eq!(plan.partitions[0].start_lba, 2_048, "{source:?} -> Plain");
        assert_eq!(
            plan.partitions[0].sector_count,
            64_000_000_000u64 / edpcli::application::support::SECTOR as u64 - 2_048,
            "{source:?} -> Plain"
        );
    }
}

#[test]
fn ch14_write_progress_batch_reaches_tui_state_without_losing_milestones() {
    use edpcli::application::WriteEvent;
    use edpcli::tui::state::WriteKind;
    let mut state = AppState::new();
    assert!(state.begin_write_wizard(WriteKind::BackupCreate, 6, None));
    assert!(state.confirm_backup_create().is_some());
    for event in [
        WriteEvent::BackupCreated {
            path: std::path::PathBuf::from("a.edpb"),
        },
        WriteEvent::RestoreWriteCompleted,
    ] {
        state.set_write_progress(event);
    }
    let log = &state.wizard().unwrap().run.as_ref().unwrap().log;
    assert_eq!(log.len(), 3);
    assert_eq!(
        log[0].step,
        edpcli::application::progress::Step::BackupCreate
    );
    assert_eq!(log[1].phase, edpcli::application::progress::Phase::Complete);
    assert_eq!(log[2].phase, edpcli::application::progress::Phase::Readback);
}

#[test]
fn mode2_preserved_encrypt_geometry_explains_unallocated_space_until_user_edits_it() {
    use edpcli::provision::DiskProvisionKind;

    let mut state = AppState::new();
    state.replace_devices(vec![official_device(
        64_000_000_000,
        DiskProvisionKind::Mode0,
    )]);
    assert_eq!(enter_provision_kind(&mut state, 2), ProvisionKind::Mode2);

    let details = state.provision_layout_editor_details();
    assert!(details.iter().any(|detail| {
        detail.text.contains("保留现有保密区几何") && detail.text.contains("未自动并入")
    }));

    state.provision_mut().form.encrypt_start_lba = "126".into();
    let details = state.provision_layout_editor_details();
    assert!(!details
        .iter()
        .any(|detail| detail.text.contains("保留现有保密区几何")));
}

#[test]
fn registered_mode0_to_mode1_form_keeps_exact_encrypt_geometry() {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::{CapacityInputMode, DiskProvisionKind};
    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
    crate::common::confirm_row_identity(&mut row);
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
            size_bytes: 4_000_000 * 512,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: 4_020_480,
            size_bytes: 2_097_153 * 512,
        },
    ]);
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(enter_provision_kind(&mut state, 1), ProvisionKind::Mode1);
    state.provision_mut().form.format_share = true;
    let form = &state.provision().form;
    assert_eq!(form.share_input_mode, CapacityInputMode::Exact);
    assert_eq!(form.encrypt_input_mode, CapacityInputMode::Exact);
    assert_eq!(form.encrypt_sectors, "2097153");
    assert_eq!(form.encrypt_start_lba, "4020480");
    let request = state.provision_request().unwrap();
    assert_eq!(request.share_sectors, Some(4_020_417));
    assert_eq!(request.encrypt_sectors, Some(2_097_153));
    assert_eq!(request.encrypt_start_lba, Some(4_020_480));
}

#[test]
fn registered_identity_prefills_custom_label_and_force_policy_but_remains_editable() {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::DiskProvisionKind;

    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
    crate::common::confirm_row_identity(&mut row);
    row.label = Some("来源自定义!SAFE6".into());
    row.force_change_password = Some(true);
    row.cancel_password_complexity_check = Some(true);
    row.max_share_password_errors = Some(7);
    row.max_encrypt_password_errors = Some(9);
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
            size_bytes: 4_000_000 * 512,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: 4_020_480,
            size_bytes: 2_097_153 * 512,
        },
    ]);

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    assert_eq!(state.provision().form.label, "来源自定义!SAFE6");
    assert!(state.provision().form.force_change_password);
    assert!(state.provision().form.cancel_password_complexity_check);
    assert_eq!(state.provision().form.max_share_password_errors, "7");
    assert_eq!(state.provision().form.max_encrypt_password_errors, "9");

    let force_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "初始化密码强制修改")
        .unwrap();
    state.provision_mut().field_selected = force_index;
    assert!(state.provision_toggle_selected_option());
    assert!(!state.provision().form.force_change_password);

    let complexity_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "取消密码复杂性验证")
        .unwrap();
    state.provision_mut().field_selected = complexity_index;
    assert!(state.provision_toggle_selected_option());
    assert!(!state.provision().form.cancel_password_complexity_check);

    let request = state.provision_request().unwrap();
    assert_eq!(request.label, "来源自定义!SAFE6");
    assert_eq!(request.force_change_password, Some(false));
    assert_eq!(request.cancel_password_complexity_check, Some(false));
    assert_eq!(request.max_share_password_errors, Some(7));
    assert_eq!(request.max_encrypt_password_errors, Some(9));
}

#[test]
fn provision_has_four_official_modes_plus_plain_after_explicit_disk_selection() {
    assert_eq!(
        ProvisionKind::ALL,
        [
            ProvisionKind::Mode0,
            ProvisionKind::Mode1,
            ProvisionKind::Mode2,
            ProvisionKind::Mode3,
            ProvisionKind::Plain,
        ]
    );

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(state.provision_scheme_picker_open());
    assert_eq!(state.provision_scheme_selected(), 0);
    assert_eq!(state.item_count(), 1);
}

#[test]
fn provision_cannot_enter_an_empty_target_workspace() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(state
        .notice()
        .is_some_and(|message| message.contains("请先在设备列表")));
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(state.provision_scheme_picker_open());
}

#[test]
fn provision_escape_walks_back_one_level_without_exiting() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_scheme_picker_open());
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert!(!state.provision_scheme_picker_open());

    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
}

#[test]
fn provision_flow_is_nested_and_explicit_reentry_preserves_state() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    enter_provision(&mut state);
    state.provision_mut().form.label = "保持当前制盘状态!SAFE6".into();
    assert_eq!(state.provision().stage, ProvisionStage::Form);

    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    state.navigate(NavCommand::WorkspaceDevices, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.selected_device_disk(), Some(6));
    assert_eq!(state.provision().form.label, "保持当前制盘状态!SAFE6");

    state.navigate(NavCommand::WorkspaceDevices, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.selected_device_disk(), Some(6));
    assert_eq!(state.provision().form.label, "保持当前制盘状态!SAFE6");
}

#[test]
fn critical_provision_cannot_leave_nested_workflow_and_blocks_other_commands() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    enter_provision(&mut state);
    state.set_critical_operation(true);

    assert_eq!(
        state.navigate(NavCommand::NextWorkspace, 20),
        StateEffect::None
    );
    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(
        state.navigate(NavCommand::PreviousWorkspace, 20),
        StateEffect::None
    );
    assert_eq!(state.workspace(), Workspace::Provision);

    assert_eq!(state.navigate(NavCommand::Down, 20), StateEffect::None);
    assert_eq!(
        state.notice(),
        Some("关键操作仍在执行，完成前不能执行该命令或启动其他任务。")
    );
}

#[test]
fn escape_never_requests_program_exit_even_during_critical_operation() {
    let mut state = AppState::new();
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert!(!state.exit_pending());

    state.set_critical_operation(true);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert!(!state.exit_pending());

    assert_eq!(
        state.navigate(NavCommand::Quit, 20),
        StateEffect::ExitDeferred
    );
    assert!(state.exit_pending());
}

#[test]
fn provision_label_defaults_to_jiangsu_safe6_and_remains_editable() {
    let mut form = ProvisionForm::default();
    assert_eq!(form.label, "江苏电力!SAFE6");
    assert!(form.share_source_password.is_empty());
    assert_eq!(
        form.share_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
    assert!(!form.share_opaque_profile);
    assert_eq!(form.share_target_password, "0000aaaa");
    assert!(form.encrypt_source_password.is_empty());
    assert_eq!(
        form.encrypt_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
    assert!(!form.encrypt_opaque_profile);
    assert_eq!(form.encrypt_target_password, "0000aaaa");
    assert_eq!(form.volume_label, "启动区");
    assert_eq!(form.boot_sectors, "20417");
    assert_eq!(
        form.encrypt_mib,
        configured_capacity_3(2_097_152, edpcli::provision::QuickCapacityUnit::GiB)
    );
    assert!(edpcli::provision::OnlyId::parse(&form.label_id).is_ok());
    assert!(!form.force_change_password);
    assert!(!form.format_boot && !form.format_share && !form.format_encrypt);
    assert_eq!(
        form.boot_fs,
        edpcli::application::filesystem::FilesystemKind::Fat16
    );
    assert_eq!(
        form.share_fs,
        edpcli::application::filesystem::FilesystemKind::ExFat
    );
    assert_eq!(
        form.encrypt_fs,
        edpcli::application::filesystem::FilesystemKind::ExFat
    );
    form.label = "自定义标签!SAFE6".into();
    form.label_id = "123456789".into();
    assert_eq!(form.label, "自定义标签!SAFE6");
    assert_eq!(form.label_id, "123456789");
}

#[test]
fn provision_key_probe_keeps_default_candidates_and_updates_verification_state() {
    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    enter_provision(&mut state);

    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: edpcli::provision::DiskProvisionKind::Mode0,
        share: Some(edpcli::provision::SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(edpcli::provision::SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));

    assert_eq!(state.provision().form.share_source_password, "0000aaaa");
    assert_eq!(
        state.provision().form.share_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::DefaultVerified
    );
    assert_eq!(state.provision().form.encrypt_source_password, "0000aaaa");
    assert_eq!(
        state.provision().form.encrypt_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
}

#[test]
fn provision_key_probe_never_overwrites_user_entered_source_password() {
    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    enter_provision(&mut state);
    state.provision_mut().form.share_source_password = "ManualOldPass!".into();
    state.provision_mut().form.share_source_knowledge =
        edpcli::provision::SourcePasswordKnowledge::Unknown;

    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: edpcli::provision::DiskProvisionKind::Mode0,
        share: Some(edpcli::provision::SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: None,
        encrypt_opaque_profile: false,
    }));

    assert_eq!(
        state.provision().form.share_source_password,
        "ManualOldPass!"
    );
    assert_eq!(
        state.provision().form.share_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
}

#[test]
fn editing_source_password_invalidates_cached_verification_state() {
    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    enter_provision(&mut state);
    state.provision_mut().form.share_source_password = "0000aaaa".into();
    state.provision_mut().form.share_source_knowledge =
        edpcli::provision::SourcePasswordKnowledge::DefaultVerified;

    let index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "原密码")
        .unwrap();
    state.provision_mut().field_selected = index;
    state.provision_push_char('x');

    assert_eq!(
        state.provision().form.share_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
}

#[test]
fn mode0_to_mode1_unknown_encrypt_requires_explicit_format_for_password_change() {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};

    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
    crate::common::confirm_row_identity(&mut row);
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
            size_bytes: 4_000_000 * 512,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: 4_020_480,
            size_bytes: 2_097_153 * 512,
        },
    ]);

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(enter_provision_kind(&mut state, 1), ProvisionKind::Mode1);
    state.provision_mut().form.format_share = true;
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));

    let fields = state.provision_visible_fields();
    let share_target = fields
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .unwrap();
    let encrypt_target = fields
        .iter()
        .enumerate()
        .filter(|(_, (label, _, _))| label == "新密码")
        .nth(1)
        .map(|(index, _)| index)
        .unwrap();

    assert_eq!(fields[share_target].1, "透传");
    assert_eq!(fields[encrypt_target].1, "透传");

    state.provision_mut().field_selected = encrypt_target;
    assert!(state.provision_selected_field_is_editable());
    assert!(!state.provision().form.format_encrypt);
    assert!(state.provision_begin_insert());
    assert_eq!(
        state.provision_visible_fields()[encrypt_target].1,
        "0000aaaa"
    );
    state.provision_cursor_end();
    state.provision_push_char('x');
    assert!(!state.provision_end_insert());
    assert!(
        !state.provision().form.format_encrypt,
        "raw checkbox state records only an explicit user request"
    );
    let request = state
        .provision_request()
        .expect("unknown source plus edited target password must auto-select required rebuild");
    assert!(request.format.encrypt);
    assert_eq!(
        request
            .key_domains
            .target_password(edpcli::provision::PartitionRole::Encrypt),
        Some(b"0000aaaax".as_slice())
    );
    state.provision_mut().field_selected = share_target;
    assert!(state.provision_selected_field_is_editable());
}

#[test]
fn unknown_source_password_geometry_change_is_blocked_synchronously_by_same_preflight_as_layout() {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::{CapacityInputMode, DiskProvisionKind, SourcePasswordKnowledge};

    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
    crate::common::confirm_row_identity(&mut row);
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
            size_bytes: 4_000_000 * 512,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: 4_020_480,
            size_bytes: 2_097_153 * 512,
        },
    ]);

    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(enter_provision_kind(&mut state, 0), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));

    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors = "3999999".into();
    assert!(!state.provision().form.format_share);

    let details = state.provision_layout_editor_details();
    assert!(details.iter().any(|detail| {
        detail
            .columns
            .as_ref()
            .is_some_and(|columns| columns[0] == "交换区" && columns[3] == "⚠ 需重建")
    }));

    let error = state.provision_request().expect_err(
        "required rebuild still needs an explicit target password when source is unknown",
    );
    assert!(error.contains("交换区"), "{error}");
    assert!(error.contains("设置新密码"), "{error}");

    let target = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .expect("share target password");
    state.provision_mut().field_selected = target;
    assert!(state.provision_toggle_selected_option());
    state.provision_mut().form.share_target_password = "NewSharePass1!".into();
    let request = state
        .provision_request()
        .expect("required rebuild should proceed once the target password is explicit");
    assert!(request.format.share);
    assert!(!state.provision().form.format_share);
}

#[test]
fn target_password_space_and_insert_model_passthrough_explicit_without_format_side_effects() {
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));
    let targets = state
        .provision_visible_fields()
        .iter()
        .enumerate()
        .filter(|(_, (label, _, _))| label == "新密码")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let share = targets[0];
    let encrypt = targets[1];

    assert_eq!(state.provision_visible_fields()[share].1, "透传");
    assert_eq!(state.provision_visible_fields()[encrypt].1, "透传");
    state.provision_mut().field_selected = share;
    assert!(state.provision_toggle_selected_option());
    assert_eq!(state.provision_visible_fields()[share].1, "0000aaaa");
    assert_eq!(
        state.provision_visible_fields()[encrypt].1,
        "透传",
        "share target mode must not leak into encrypt"
    );
    assert!(!state.provision().form.format_share);
    assert!(!state.provision().form.format_encrypt);

    assert!(state.provision_toggle_selected_option());
    assert_eq!(state.provision_visible_fields()[share].1, "透传");
    assert!(!state.provision().form.format_share);

    assert!(state.provision_begin_insert());
    assert_eq!(state.input_mode(), InputMode::Insert);
    assert_eq!(state.provision_visible_fields()[share].1, "0000aaaa");
    assert!(!state.provision().form.format_share);
    assert!(!state.provision_end_insert());
}

#[test]
fn failed_source_with_explicit_target_equal_to_failed_candidate_is_still_blocked() {
    use edpcli::provision::{DiskProvisionKind, KeyDomainRole, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));

    let fields = state.provision_visible_fields();
    let source_index = fields
        .iter()
        .position(|(label, _, _)| label == "原密码")
        .expect("share source password");
    let target_index = fields
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .expect("share target password");

    state.provision_mut().field_selected = source_index;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_backspace();
    assert!(state.provision_end_insert());
    let (_, source_candidate, revision) = state
        .provision_source_password_verify_request()
        .expect("verification request")
        .expect("source password request");
    assert_eq!(source_candidate, "0000aaa");
    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision,
        Err("来源密码验证失败".into()),
    );

    state.provision_mut().field_selected = target_index;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_backspace();
    assert_eq!(state.provision().form.share_target_password, "0000aaa");
    assert!(!state.provision_end_insert());
    assert!(!state.provision().form.format_share);

    let request = state
        .provision_request()
        .expect("failed source verification plus an explicit target password must auto-rebuild");
    assert!(request.format.share);
    assert_eq!(
        request
            .key_domains
            .target_password(edpcli::provision::PartitionRole::Share),
        Some(b"0000aaa".as_slice())
    );
}

#[test]
fn user_target_password_draft_survives_source_reverification_and_passthrough_normalization() {
    use edpcli::provision::{DiskProvisionKind, KeyDomainRole, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::Unknown),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::Unknown),
        encrypt_opaque_profile: true,
    }));
    let source_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "原密码")
        .expect("share source password");
    let target_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .expect("share target password");

    state.provision_mut().field_selected = target_index;
    assert!(state.provision_begin_insert());
    state.provision_mut().form.share_target_password = "UserNew1!".into();
    state.provision_cursor_end();
    state.provision_push_char('x');
    state.provision_backspace();
    assert!(!state.provision_end_insert());
    assert_eq!(state.provision().form.share_target_password, "UserNew1!");

    state.provision_mut().field_selected = source_index;
    assert!(state.provision_begin_insert());
    state.provision_mut().form.share_source_password = "UserNew1!".into();
    state.provision_cursor_end();
    state.provision_push_char('x');
    state.provision_backspace();
    assert!(state.provision_end_insert());
    let (_, _, revision) = state
        .provision_source_password_verify_request()
        .expect("verification request")
        .expect("source password request");
    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision,
        Ok(SourcePasswordKnowledge::UserVerified),
    );
    assert_eq!(
        state.provision_visible_fields()[target_index].1,
        "透传",
        "equal verified source/target should normalize the action to passthrough"
    );
    assert_eq!(
        state.provision().form.share_target_password,
        "UserNew1!",
        "normalization must not destroy the user's target draft"
    );

    state.provision_mut().field_selected = source_index;
    assert!(state.provision_begin_insert());
    state.provision_mut().form.share_source_password = "ActualOld1!".into();
    state.provision_cursor_end();
    state.provision_push_char('x');
    state.provision_backspace();
    assert!(state.provision_end_insert());
    let (_, _, revision) = state
        .provision_source_password_verify_request()
        .expect("second verification request")
        .expect("second source password request");
    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision,
        Ok(SourcePasswordKnowledge::UserVerified),
    );

    state.provision_mut().field_selected = target_index;
    assert!(state.provision_begin_insert());
    assert_eq!(
        state.provision().form.share_target_password,
        "UserNew1!",
        "re-entering target editing after source re-verification must preserve user-owned draft"
    );
}

#[test]
fn verified_equal_target_password_normalizes_back_to_passthrough() {
    use edpcli::provision::{DiskProvisionKind, PartitionRole, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));
    let share_target = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .expect("share target password");
    state.provision_mut().field_selected = share_target;

    assert_eq!(state.provision_visible_fields()[share_target].1, "透传");
    assert!(state.provision_begin_insert());
    assert_eq!(state.provision_visible_fields()[share_target].1, "0000aaaa");
    assert!(!state.provision_end_insert());
    assert_eq!(
        state.provision_visible_fields()[share_target].1,
        "透传",
        "explicit target equal to verified source must normalize to passthrough"
    );
    let request = state.provision_request().expect("passthrough request");
    assert_eq!(
        request.key_domains.target_password(PartitionRole::Share),
        None
    );
    assert!(!request.format.share);
}

#[test]
fn verified_different_target_password_requests_rewrap_without_formatting() {
    use edpcli::provision::{DiskProvisionKind, PartitionRole, SourcePasswordKnowledge};

    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));
    let share_target = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "新密码")
        .expect("share target password");
    state.provision_mut().field_selected = share_target;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_push_char('x');
    assert!(!state.provision_end_insert());
    assert_ne!(state.provision_visible_fields()[share_target].1, "透传");
    let request = state.provision_request().expect("rewrap request");
    assert_eq!(
        request.key_domains.target_password(PartitionRole::Share),
        Some(b"0000aaaax".as_slice())
    );
    assert!(!request.format.share);
}

#[test]
fn source_password_verify_request_is_scoped_to_selected_domain() {
    let mut state = AppState::new();
    state.replace_devices(vec![mode0_device(64_000_000_000)]);
    enter_provision(&mut state);
    state.provision_mut().form.encrypt_source_password = "EncryptOld1!".into();

    let index = state
        .provision_visible_fields()
        .iter()
        .enumerate()
        .filter(|(_, (label, _, _))| label == "原密码")
        .nth(1)
        .map(|(index, _)| index)
        .unwrap();
    state.provision_mut().field_selected = index;
    let request = state
        .provision_source_password_verify_request()
        .unwrap()
        .unwrap();
    assert_eq!(request.0, edpcli::provision::KeyDomainRole::Encrypt);
    assert_eq!(request.1, "EncryptOld1!");
}

#[test]
fn provision_format_controls_follow_current_mode_targets() {
    let mut state = AppState::new();
    state.provision_mut().kind = ProvisionKind::Mode1;
    let fields = state.provision_visible_fields();
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == "启动/交换区格式化"));
    assert!(fields.iter().any(|(label, _, _)| label == "保密区格式化"));
    assert!(!fields.iter().any(|(label, _, _)| label == "启动区格式化"));
    let share_index = fields
        .iter()
        .position(|(label, _, _)| label == "启动/交换区格式化")
        .unwrap();
    let fs_index = fields
        .iter()
        .position(|(label, _, _)| label == "启动/交换区文件系统")
        .unwrap();
    state.provision_mut().field_selected = share_index;
    assert!(state.provision_toggle_selected_option());
    assert!(state.provision().form.format_share);
    state.provision_mut().field_selected = fs_index;
    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_fs,
        edpcli::application::filesystem::FilesystemKind::Fat16
    );
    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_fs,
        edpcli::application::filesystem::FilesystemKind::Fat32
    );
    assert!(state.provision_shift_selected_option(true));
    assert_eq!(
        state.provision().form.share_fs,
        edpcli::application::filesystem::FilesystemKind::Fat16
    );

    state.provision_mut().kind = ProvisionKind::Mode2;
    let fields = state.provision_visible_fields();
    assert!(fields
        .iter()
        .any(|(label, value, _)| label == "模式2兼容区" && *value == "固定 63 sector · 不格式化"));
    assert!(!fields.iter().any(|(label, _, _)| label == "启动区格式化"));
    let reserve_index = fields
        .iter()
        .position(|(label, _, _)| label == "模式2兼容区")
        .unwrap();
    state.provision_mut().field_selected = reserve_index;
    assert!(!state.provision_toggle_selected_option());
}

#[test]
fn provision_prefers_scanned_onlyid_and_generates_candidate_only_when_missing() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    enter_provision(&mut state);
    assert_eq!(state.provision().form.label_id, "1402259934");

    let mut missing = device(64_000_000_000);
    missing.onlyid = None;
    state.replace_devices(vec![missing]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_begin_selected();
    assert!(edpcli::provision::OnlyId::parse(&state.provision().form.label_id).is_ok());
    assert!(!state.provision().form.label_id.is_empty());
}

#[test]
fn provision_uses_only_per_partition_quick_exact_inputs() {
    use edpcli::provision::CapacityInputMode;
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    assert!(!fields.iter().any(|(label, _, _)| label == "分配方式"));
    assert_eq!(
        state.provision().form.boot_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.encrypt_input_mode,
        CapacityInputMode::Quick
    );
    assert!(!fields
        .iter()
        .any(|(label, _, _)| label.contains("输入方式")));
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == "启动区容量 (sector)"));
    let giga_label = configured_quick_unit_label(edpcli::provision::QuickCapacityUnit::GiB);
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == &format!("交换区容量 ({giga_label})")));
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == &format!("保密区容量 ({giga_label})")));
}

#[test]
fn provision_text_field_cursor_edits_in_place() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    state.provision_mut().form.user = "ABCDE".into();
    state.provision_mut().field_selected = 1;
    state.provision_cursor_end();
    assert_eq!(state.provision_field_cursor(), 5);
    state.provision_move_cursor(-2);
    assert_eq!(state.provision_field_cursor(), 3);
    state.provision_push_char('X');
    assert_eq!(state.provision().form.user, "ABCXDE");
    assert_eq!(state.provision_field_cursor(), 4);
    state.provision_backspace();
    assert_eq!(state.provision().form.user, "ABCDE");
    assert_eq!(state.provision_field_cursor(), 3);
    state.provision_cursor_home();
    state.provision_push_char('Z');
    assert_eq!(state.provision().form.user, "ZABCDE");
    assert_eq!(state.provision_field_cursor(), 1);
}

#[test]
fn provision_capacity_unit_cycles_configured_units_without_geometry_change() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);

    state.provision_mut().form.share_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.share_quick_unit = QuickCapacityUnit::MiB;
    let sectors = 13_606_912u64;
    state.provision_mut().form.share_mib = configured_capacity_3(sectors, QuickCapacityUnit::MiB);
    let capacity_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("交换区容量"))
        .expect("share capacity field");
    state.provision_mut().field_selected = capacity_index;

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::GiB
    );
    let giga_value = configured_capacity_3(sectors, QuickCapacityUnit::GiB);
    let giga_label = configured_quick_unit_label(QuickCapacityUnit::GiB);
    assert_eq!(state.provision().form.share_mib, giga_value);
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(
            |(label, value, _)| label == &format!("交换区容量 ({giga_label})")
                && *value == giga_value
        ));
    let request = state.provision_request().expect("configured giga request");
    assert_eq!(request.share_mib, None);
    assert_eq!(request.share_sectors, Some(sectors));

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(state.provision().form.share_sectors, sectors.to_string());

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::MiB
    );
    assert_eq!(
        state.provision().form.share_mib,
        configured_capacity_3(sectors, QuickCapacityUnit::MiB)
    );
}

#[test]
fn provision_capacity_h_l_moves_previous_and_next_without_changing_geometry() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);

    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors = "13606912".into();
    state.provision_mut().field_selected = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("交换区容量"))
        .expect("share capacity field");

    assert!(state.provision_shift_selected_option(true));
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::GiB
    );
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(13_606_912)
    );

    assert!(state.provision_shift_selected_option(false));
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(state.provision().form.share_sectors, "13606912");

    assert!(state.provision_shift_selected_option(false));
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::MiB
    );
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(13_606_912)
    );

    assert!(state.provision_shift_selected_option(true));
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(state.provision().form.share_sectors, "13606912");
}

#[test]
fn editing_generated_giga_text_uses_user_value_even_if_display_text_is_identical() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    enter_provision(&mut state);
    authorize_plain_mode0_rebuild(&mut state);
    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors = "13606912".into();
    state.provision_mut().field_selected = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("交换区容量"))
        .unwrap();
    assert!(state.provision_toggle_selected_option());
    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::GiB
    );
    let generated = configured_capacity_3(13_606_912, QuickCapacityUnit::GiB);
    assert_eq!(state.provision().form.share_mib, generated);
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(13_606_912)
    );

    let last = generated.chars().last().unwrap();
    state.provision_cursor_end();
    state.provision_backspace();
    state.provision_push_char(last);
    assert_eq!(state.provision().form.share_mib, generated);
    let edited_sectors = configured_text_to_sectors(&generated, QuickCapacityUnit::GiB);
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(edited_sectors)
    );
    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_sectors,
        edited_sectors.to_string()
    );
}

#[test]
fn provision_exact_sector_capacity_cycles_through_configured_units_losslessly() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);

    state.provision_mut().form.boot_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.boot_sectors = "20417".into();
    let capacity_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("启动区容量"))
        .expect("boot capacity field");
    state.provision_mut().field_selected = capacity_index;

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.boot_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.boot_quick_unit,
        QuickCapacityUnit::MiB
    );
    let mega_value = configured_capacity_3(20_417, QuickCapacityUnit::MiB);
    let mega_label = configured_quick_unit_label(QuickCapacityUnit::MiB);
    assert_eq!(state.provision().form.boot_mib, mega_value);
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(
            |(label, value, _)| label == &format!("启动区容量 ({mega_label})")
                && *value == mega_value
        ));
    let request = state.provision_request().expect("configured mega request");
    assert_eq!(request.boot_mib, None);
    assert_eq!(request.boot_sectors, Some(20_417));

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.boot_quick_unit,
        QuickCapacityUnit::GiB
    );
    let giga_label = configured_quick_unit_label(QuickCapacityUnit::GiB);
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(|(label, _, _)| label == &format!("启动区容量 ({giga_label})")));

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.boot_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(state.provision().form.boot_sectors, "20417");
}

#[test]
fn provision_capacity_hints_match_each_partition() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    let hint_for = |label: &str| {
        let index = fields
            .iter()
            .position(|(field, _, _)| field.starts_with(label))
            .expect("partition capacity field");
        state.provision_field_hint(index).expect("partition hint")
    };

    let boot = hint_for("启动区容量");
    let share = hint_for("交换区容量");
    let encrypt = hint_for("保密区容量");
    assert_eq!(
        boot,
        format!(
            "Space 切换 {} / {} / sector · f 最大可用容量",
            configured_quick_unit_label(edpcli::provision::QuickCapacityUnit::MiB),
            configured_quick_unit_label(edpcli::provision::QuickCapacityUnit::GiB)
        )
    );
    assert_eq!(share, boot);
    assert_eq!(encrypt, boot);
}

#[test]
fn provision_input_policy_filters_invalid_characters_and_ranges() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let field_index = |state: &AppState, prefix: &str| {
        state
            .provision_visible_fields()
            .iter()
            .position(|(label, _, _)| label.starts_with(prefix))
            .expect("provision field")
    };

    state.provision_mut().form.label_id.clear();
    state.provision_mut().field_selected = field_index(&state, "标签标识");
    state.provision_cursor_home();
    state.provision_push_char('x');
    assert_eq!(state.provision().form.label_id, "");
    state.provision_push_char('-');
    state.provision_push_char('1');
    state.provision_push_char('-');
    assert_eq!(state.provision().form.label_id, "-1");

    state.provision_mut().form.share_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.share_mib.clear();
    state.provision_mut().field_selected = field_index(&state, "交换区容量");
    state.provision_cursor_home();
    for ch in ['1', '.', '2', '.', 'x'] {
        state.provision_push_char(ch);
    }
    assert_eq!(state.provision().form.share_mib, "1.2");

    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors.clear();
    state.provision_mut().field_selected = field_index(&state, "交换区容量");
    state.provision_cursor_home();
    state.provision_push_char('.');
    state.provision_push_char('4');
    assert_eq!(state.provision().form.share_sectors, "4");

    state.provision_mut().form.share_start_lba.clear();
    state.provision_mut().field_selected = field_index(&state, "交换区起点 LBA");
    state.provision_cursor_home();
    state.provision_push_char('x');
    state.provision_push_char('2');
    assert_eq!(state.provision().form.share_start_lba, "2");

    state.provision_mut().form.max_share_password_errors.clear();
    state.provision_mut().field_selected = field_index(&state, "交换区密码最大错误次数");
    state.provision_cursor_home();
    for ch in ['2', '5', '6'] {
        state.provision_push_char(ch);
    }
    assert_eq!(state.provision().form.max_share_password_errors, "25");
}

fn enter_plain_form(state: &mut AppState) {
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision_kind(state, 4), ProvisionKind::Plain);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
}

#[test]
fn plain_form_defaults_to_one_partition_at_lba2048_filling_the_disk() {
    let mut state = AppState::new();
    enter_plain_form(&mut state);

    let plan = state.provision_plain_plan().unwrap();
    let total_sectors = 64_000_000_000u64 / edpcli::application::support::SECTOR as u64;
    assert_eq!(plan.partitions.len(), 1);
    assert_eq!(plan.partitions[0].start_lba, 2048);
    assert_eq!(plan.partitions[0].sector_count, total_sectors - 2048);

    let fields = state.provision_visible_fields();
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0].0, "P1 起点 LBA");
    assert_eq!(fields[0].1, "2048");
    assert!(fields[1].0.starts_with("P1 容量"));
    assert_eq!(fields[2].0, "P1 文件系统");
    assert_eq!(fields[3].0, "P1 卷标");
}

#[test]
fn plain_add_gap_fill_and_delete_never_move_other_partitions() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    enter_plain_form(&mut state);

    {
        let p1 = &mut state.provision_mut().plain_form.partitions[0];
        p1.input_mode = CapacityInputMode::Exact;
        p1.sector_count = "10000".into();
    }
    assert!(state.provision_plain_add_partition());
    assert_eq!(state.provision().plain_form.partitions.len(), 2);
    assert_eq!(
        state.provision().plain_form.partitions[1].start_lba,
        "12048"
    );

    state.provision_mut().plain_form.partitions[1].start_lba = "20000".into();
    state.provision_mut().field_selected = 5; // P2 capacity
    assert!(state.provision_fill_selected_capacity());
    let p2_before = state.provision().plain_form.partitions[1].start_lba.clone();

    state.provision_mut().field_selected = 1; // P1 capacity
    assert!(state.provision_fill_selected_capacity());
    let plan = state.provision_plain_plan().unwrap();
    assert_eq!(plan.partitions[0].sector_count, 17_952);
    assert_eq!(plan.partitions[1].start_lba, 20_000);
    assert_eq!(
        state.provision().plain_form.partitions[1].start_lba,
        p2_before
    );

    state.provision_mut().field_selected = 4; // P2
    assert!(state.provision_plain_delete_selected_partition());
    assert_eq!(state.provision().plain_form.partitions.len(), 1);
    assert_eq!(state.provision().plain_form.partitions[0].start_lba, "2048");
    assert_eq!(
        state.provision().plain_form.partitions[0].sector_count,
        "17952"
    );
}

#[test]
fn plain_start_fill_finds_earliest_gap_without_moving_other_partitions() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    enter_plain_form(&mut state);
    {
        let p1 = &mut state.provision_mut().plain_form.partitions[0];
        p1.input_mode = CapacityInputMode::Exact;
        p1.sector_count = "10000".into();
    }
    assert!(state.provision_plain_add_partition());
    state.provision_mut().plain_form.partitions[1].start_lba = "30000".into();
    state.provision_mut().plain_form.partitions[1].input_mode = CapacityInputMode::Exact;
    state.provision_mut().plain_form.partitions[1].sector_count = "5000".into();
    let p2_start_before = state.provision().plain_form.partitions[1].start_lba.clone();
    state.provision_mut().plain_form.partitions[0].start_lba = "15000".into();
    state.provision_mut().field_selected = 0;

    assert!(state.provision_fill_selected_capacity());
    assert_eq!(state.provision().plain_form.partitions[0].start_lba, "2048");
    assert_eq!(
        state.provision().plain_form.partitions[1].start_lba,
        p2_start_before
    );
}

#[test]
fn plain_plan_rejects_overlap_and_fill_produces_valid_layout() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    enter_plain_form(&mut state);
    {
        let p1 = &mut state.provision_mut().plain_form.partitions[0];
        p1.input_mode = CapacityInputMode::Exact;
        p1.sector_count = "30000".into();
    }
    assert!(state.provision_plain_add_partition());
    state.provision_mut().plain_form.partitions[1].start_lba = "20000".into();
    assert!(state.provision_plain_plan().unwrap_err().contains("重叠"));

    state.provision_mut().plain_form.partitions[1].start_lba = "40000".into();
    state.provision_mut().field_selected = 5;
    assert!(state.provision_fill_selected_capacity());
    let plan = state.provision_plain_plan().unwrap();
    assert_eq!(plan.partitions.len(), 2);
    assert!(plan.partitions[0].end_lba().unwrap() < plan.partitions[1].start_lba);
}

#[test]
fn provision_fill_selected_capacity_uses_same_maximum_as_layout_and_text_f_is_literal() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    let before = state.provision_request().unwrap();
    let usable_end =
        edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            64_000_000_000 / 512,
            512,
        )
        .unwrap()
        .start_lba;
    let max_sectors = usable_end - before.encrypt_start_lba.unwrap();
    let expected_max = edpcli::application::support::fmt_capacity_sectors(max_sectors);
    assert!(state
        .provision_layout_editor_details()
        .iter()
        .any(|detail| { detail.text.contains(&format!("最大 {expected_max}")) }));
    assert!(state.provision_fill_selected_capacity());
    assert_eq!(
        state.provision_request().unwrap().encrypt_sectors,
        Some(max_sectors)
    );

    let user = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "用户名")
        .expect("user field");
    state.provision_mut().form.user.clear();
    state.provision_mut().field_selected = user;
    state.provision_cursor_home();
    assert!(!state.provision_fill_selected_capacity());
    state.provision_push_char('f');
    assert_eq!(state.provision().form.user, "f");
}

#[test]
fn provision_fill_selected_capacity_recovers_from_empty_capacity_input() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    state.provision_mut().form.encrypt_mib.clear();

    assert!(state.provision_fill_selected_capacity());
    let request = state
        .provision_request()
        .expect("fill should repair empty capacity");
    assert!(request.encrypt_sectors.is_some_and(|sectors| sectors > 0));
}

#[test]
fn provision_fill_start_finds_minimum_gap_without_mutating_other_form_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    let boot_start = fields
        .iter()
        .position(|(label, _, _)| label == "启动区起点 LBA")
        .expect("boot start");
    let share_start_before = state.provision().form.share_start_lba.clone();
    let encrypt_start_before = state.provision().form.encrypt_start_lba.clone();
    let boot_capacity_before = state.provision().form.boot_sectors.clone();
    state.provision_mut().form.boot_start_lba = "100".into();
    state.provision_mut().field_selected = boot_start;

    assert!(state.provision_fill_selected_capacity());
    assert_eq!(state.provision().form.boot_start_lba, "63");
    assert_eq!(state.provision().form.share_start_lba, share_start_before);
    assert_eq!(
        state.provision().form.encrypt_start_lba,
        encrypt_start_before
    );
    assert_eq!(state.provision().form.boot_sectors, boot_capacity_before);
    assert_eq!(
        state
            .provision()
            .message
            .as_ref()
            .map(|message| message.tone()),
        Some(edpcli::tui::ui::UiMessageTone::Success),
        "successful f auto-fill feedback must be a success message, never an error-red message"
    );

    let layout = state.provision_layout_model();
    let reserved = layout
        .segments
        .iter()
        .find(|segment| segment.start_lba == 13)
        .expect("EDP reserved header range must be present in provision capacity map");
    assert_eq!(
        reserved.kind,
        edpcli::tui::disk_layout::DiskRegionKind::Reserved
    );
    assert_eq!(reserved.label, "保留区域");
    assert_eq!(reserved.end_exclusive().unwrap(), 63);
    assert!(state
        .provision_layout_editor_details()
        .iter()
        .any(|detail| {
            detail.region_kind == Some(edpcli::tui::disk_layout::DiskRegionKind::Reserved)
                && detail.columns.as_ref().is_some_and(|columns| {
                    columns[0] == "保留区域" && columns[2] == "LBA 13–62" && columns[3] == "● 保留"
                })
        }));
}

#[test]
fn provision_fill_capacity_uses_current_start_and_next_fixed_start_only() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().form.share_start_lba = "20480".into();
    state.provision_mut().form.encrypt_start_lba = "50000".into();
    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors = "1000".into();
    let boot_start_before = state.provision().form.boot_start_lba.clone();
    let encrypt_start_before = state.provision().form.encrypt_start_lba.clone();
    let share_capacity = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "交换区容量 (sector)")
        .expect("share capacity");
    state.provision_mut().field_selected = share_capacity;

    assert!(state.provision_fill_selected_capacity());
    assert_eq!(state.provision().form.share_sectors, "29520");
    assert_eq!(state.provision().form.boot_start_lba, boot_start_before);
    assert_eq!(
        state.provision().form.encrypt_start_lba,
        encrypt_start_before
    );
}

#[test]
fn invalid_partition_draft_remains_visible_with_red_conflict_segment_and_is_blocked() {
    use edpcli::provision::CapacityInputMode;
    use edpcli::tui::disk_layout::{DiskLayoutDetailTone, DiskRegionKind};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    state.provision_mut().form.share_start_lba = "20480".into();
    state.provision_mut().form.encrypt_start_lba = "50000".into();
    state.provision_mut().form.share_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.share_sectors = "30000".into();

    let model = state.provision_layout_model();
    let conflict = model
        .segments
        .iter()
        .find(|segment| segment.kind == DiskRegionKind::Conflict)
        .expect("current invalid draft must remain renderable with a conflict interval");
    assert_eq!(conflict.start_lba, 50000);
    assert_eq!(conflict.sector_count, 480);

    let details = state.provision_layout_editor_details();
    assert!(details.iter().any(|detail| {
        detail.region_kind == Some(DiskRegionKind::Conflict)
            && detail.tone == DiskLayoutDetailTone::Danger
            && detail
                .columns
                .as_ref()
                .is_some_and(|columns| columns[3].contains("冲突"))
    }));
    assert!(details.iter().any(|detail| {
        detail.tone == DiskLayoutDetailTone::Danger && detail.text.contains("当前草稿布局无效")
    }));
    assert!(
        !details
            .iter()
            .any(|detail| detail.text.contains("目标布局尚未通过校验")),
        "invalid geometry must not replace the current draft map/list with a placeholder"
    );
    assert!(state.provision_request().is_err());
}

#[test]
fn provision_layout_editor_reports_total_space_and_selected_partition_limits() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    let details = state.provision_layout_editor_details();
    let model = state.provision_layout_model();
    assert_eq!(model.total_sectors, 64_000_000_000 / 512);
    assert!(
        details
            .iter()
            .any(|detail| detail.text.contains("可分区 LBA")),
        "{details:?}"
    );
    assert!(
        details.iter().any(|detail| detail.text.contains("剩余")),
        "{details:?}"
    );
    let bar = model.bar(40);
    assert_eq!(bar.len(), 40);
    assert!(bar.contains(&edpcli::tui::disk_layout::DiskRegionKind::Encrypt));
    assert!(bar.contains(&edpcli::tui::disk_layout::DiskRegionKind::Free));
    assert!(
        details
            .iter()
            .any(|detail| detail.text.contains("当前区域  保密区")),
        "{details:?}"
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.text.contains("当前容量") && detail.text.contains("最大")),
        "{details:?}"
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.text.starts_with("✓ 当前布局")),
        "{details:?}"
    );
}

#[test]
fn provision_layout_rows_are_sorted_by_start_lba_including_free_space() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(8_053_063_680)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    state.provision_mut().form.boot_input_mode = CapacityInputMode::Exact;
    state.provision_mut().form.boot_sectors = "20417".into();
    state.provision_mut().form.boot_start_lba = "63".into();
    state.provision_mut().form.share_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.share_quick_unit = QuickCapacityUnit::GiB;
    state.provision_mut().form.share_mib = "6".into();
    state.provision_mut().form.share_start_lba = "20480".into();
    state.provision_mut().form.encrypt_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.encrypt_quick_unit = QuickCapacityUnit::GiB;
    state.provision_mut().form.encrypt_mib = "1".into();
    state.provision_mut().form.encrypt_start_lba = "13627392".into();

    let details = state.provision_layout_editor_details();
    let rows = details
        .iter()
        .filter(|detail| detail.region_kind.is_some())
        .filter_map(|detail| detail.columns.as_ref())
        .collect::<Vec<_>>();
    let starts = rows
        .iter()
        .map(|columns| {
            columns[2]
                .strip_prefix("LBA ")
                .unwrap()
                .split('–')
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(starts.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
    let model = state.provision_layout_model();
    assert!(model
        .segments
        .iter()
        .any(|segment| segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Free));
}

#[test]
fn registered_mode0_to_mode1_preview_keeps_encrypt_anchor_and_blocks_overlap() {
    use edpcli::protocol::sectors::EdpfPartition;
    use edpcli::provision::{CapacityInputMode, DiskProvisionKind};
    let encrypt_start = 4_020_480u64;
    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
    crate::common::confirm_row_identity(&mut row);
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
            size_bytes: 4_000_000 * 512,
        },
        EdpfPartition {
            ptype: 4,
            active: 1,
            enc: 1,
            start_lba: encrypt_start,
            size_bytes: 2_097_153 * 512,
        },
    ]);
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(enter_provision_kind(&mut state, 1), ProvisionKind::Mode1);
    state.provision_mut().form.format_share = true;
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(
        state.provision().form.encrypt_input_mode,
        CapacityInputMode::Exact
    );
    let share_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("交换区容量"))
        .expect("share capacity");
    state.provision_mut().field_selected = share_index;
    let max_share = encrypt_start - 63;
    let details = state.provision_layout_editor_details();
    assert!(
        details.iter().any(|detail| detail.text.contains(&format!(
            "最大 {}",
            edpcli::application::support::fmt_capacity_sectors(max_share)
        ))),
        "{details:?}"
    );

    let smaller = encrypt_start - 63 - 4096;
    state.provision_mut().form.share_sectors = smaller.to_string();
    let model = state.provision_layout_model();
    assert!(model.segments.iter().any(|segment| {
        segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Free
            && segment.start_lba == encrypt_start - 4096
            && segment.sector_count == 4096
    }));
    assert!(model.segments.iter().any(|segment| {
        segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Encrypt
            && segment.start_lba == encrypt_start
    }));
    let request = state
        .provision_request()
        .expect("shrink must leave a legal gap");
    assert_eq!(request.encrypt_start_lba, Some(encrypt_start));

    state.provision_mut().form.share_sectors = (encrypt_start - 63 + 1).to_string();
    let details = state.provision_layout_editor_details();
    assert!(details.iter().any(|detail| {
        detail.tone == edpcli::tui::disk_layout::DiskLayoutDetailTone::Danger
            && detail.text.contains("overlap")
    }));
    assert!(state.provision_request().is_err());
}

#[test]
fn plain_mode0_preview_reflows_unanchored_share_after_boot_edit() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);
    authorize_plain_mode0_rebuild(&mut state);
    state.provision_mut().form.boot_sectors = "10000".into();
    state.provision_mut().form.label_id = "1402259934".into();
    state.provision_mut().form.user = "测试用户".into();
    state.provision_mut().form.dept = "输电运检中心".into();
    let request = state.provision_request().expect("plain mode0 request");
    assert_eq!(request.boot_start_lba, Some(63));
    assert_eq!(request.share_start_lba, Some(10_063));
    assert!(state
        .provision_layout_model()
        .segments
        .iter()
        .any(|segment| {
            segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Share
                && segment.start_lba == 10_063
        }));
    let boot_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("启动区容量"))
        .expect("boot capacity");
    state.provision_mut().field_selected = boot_index;
    state.provision_mut().form.boot_sectors = "20000".into();
    let enlarged = state
        .provision_request()
        .expect("boot edit must reflow unanchored partitions");
    assert_eq!(enlarged.share_start_lba, Some(20_063));
    assert!(state
        .provision_layout_model()
        .segments
        .iter()
        .any(|segment| {
            segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Share
                && segment.start_lba == 20_063
        }));
}

#[test]
fn mode0_defaults_share_to_remaining_space_once_without_linking_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(enter_provision(&mut state), ProvisionKind::Mode0);

    let total_sectors = 64_000_000_000u64 / 512;
    let lce =
        edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            512,
        )
        .unwrap();
    let usable_sectors = lce.start_lba - edpcli::provision::OFFICIAL_PARTITION_START_SECTOR;
    let encrypt_sectors = 1024 * 2048;
    let expected_share_mib = (usable_sectors - 20_417 - encrypt_sectors) / 2048;
    let expected_share_sectors = expected_share_mib * 2048;
    let format_giga_3 =
        |sectors: u64| configured_capacity_3(sectors, edpcli::provision::QuickCapacityUnit::GiB);

    assert_eq!(state.provision().form.boot_sectors, "20417");
    assert_eq!(
        state.provision().form.encrypt_mib,
        format_giga_3(encrypt_sectors)
    );
    assert_eq!(
        state.provision().form.share_mib,
        format_giga_3(expected_share_sectors)
    );
    let expected_remainder = usable_sectors - 20_417 - expected_share_sectors - encrypt_sectors;
    assert_eq!(
        state
            .provision_layout_model()
            .segments
            .iter()
            .filter(
                |segment| segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Free
                    && segment.start_lba < lce.start_lba
            )
            .map(|segment| segment.sector_count)
            .sum::<u64>(),
        expected_remainder
    );

    let original_share = state.provision().form.share_mib.clone();
    state.provision_mut().field_selected = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| {
            label
                == &format!(
                    "保密区容量 ({})",
                    configured_quick_unit_label(edpcli::provision::QuickCapacityUnit::GiB)
                )
        })
        .expect("encrypt field");
    state.provision_cursor_end();
    let current_len = state.provision().form.encrypt_mib.chars().count();
    for _ in 0..current_len {
        state.provision_backspace();
    }
    for ch in "0.500".chars() {
        state.provision_push_char(ch);
    }
    let edited_encrypt_sectors =
        configured_text_to_sectors("0.500", edpcli::provision::QuickCapacityUnit::GiB);
    assert_eq!(state.provision().form.encrypt_mib, "0.500");
    assert_eq!(state.provision().form.share_mib, original_share);
    assert_eq!(
        state
            .provision_layout_model()
            .segments
            .iter()
            .filter(
                |segment| segment.kind == edpcli::tui::disk_layout::DiskRegionKind::Free
                    && segment.start_lba < lce.start_lba
            )
            .map(|segment| segment.sector_count)
            .sum::<u64>(),
        expected_remainder + encrypt_sectors - edited_encrypt_sectors
    );

    state.provision_begin_selected();
    assert_eq!(state.provision().form.encrypt_mib, "0.500");
    assert_eq!(state.provision().form.share_mib, original_share);

    state.replace_devices(vec![device(32_000_000_000)]);
    state.provision_begin_selected();
    assert_eq!(
        state.provision().form.encrypt_mib,
        format_giga_3(encrypt_sectors)
    );
    assert_ne!(state.provision().form.share_mib, original_share);
}

#[test]
fn mode0_live_layout_reports_invalid_geometry_without_rebalancing_other_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    enter_provision(&mut state);
    let original_encrypt = state.provision().form.encrypt_mib.clone();
    state.provision_mut().form.share_mib = "999999999".into();

    let details = state.provision_layout_editor_details();
    assert!(details
        .iter()
        .any(|detail| { detail.tone == edpcli::tui::disk_layout::DiskLayoutDetailTone::Danger }));
    assert!(state.provision_request().is_err());
    assert_eq!(state.provision().form.encrypt_mib, original_encrypt);
}

#[test]
fn provision_force_change_password_checkbox_defaults_off_and_toggles() {
    let mut state = AppState::new();
    state.provision_mut().kind = ProvisionKind::Mode1;
    state.provision_mut().field_selected = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "初始化密码强制修改")
        .unwrap();

    assert!(!state.provision().form.force_change_password);
    assert!(state.provision_toggle_selected_option());
    assert!(state.provision().form.force_change_password);
    assert!(state.provision_toggle_selected_option());
    assert!(!state.provision().form.force_change_password);
}

#[test]
fn vim_vertical_navigation_is_bounded() {
    let mut state = AppState::new();
    state.set_item_count(5);

    assert_eq!(state.selected(), 0);
    assert_eq!(state.navigate(NavCommand::Up, 10), StateEffect::None);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::Down, 10);
    state.navigate(NavCommand::Down, 10);
    assert_eq!(state.selected(), 2);

    for _ in 0..10 {
        state.navigate(NavCommand::Down, 10);
    }
    assert_eq!(state.selected(), 4);
}

#[test]
fn vim_top_bottom_and_half_page_navigation_are_deterministic() {
    let mut state = AppState::new();
    state.set_item_count(100);

    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(state.selected(), 99);

    state.navigate(NavCommand::Top, 20);
    assert_eq!(state.selected(), 0);

    state.navigate(NavCommand::HalfPageDown, 20);
    assert_eq!(state.selected(), 10);

    state.navigate(NavCommand::HalfPageUp, 20);
    assert_eq!(state.selected(), 0);
}

#[test]
fn search_command_and_help_modes_return_to_normal_with_escape() {
    let mut state = AppState::new();

    state.navigate(NavCommand::Search, 10);
    assert_eq!(state.input_mode(), InputMode::Search);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);

    state.navigate(NavCommand::CommandPalette, 10);
    assert_eq!(state.input_mode(), InputMode::Command);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);

    state.navigate(NavCommand::Help, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);
    assert!(state.help_open());
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);
    assert!(!state.help_open());
}

#[test]
fn quit_is_deferred_during_critical_write_phase() {
    let mut state = AppState::new();

    assert_eq!(
        state.navigate(NavCommand::Quit, 10),
        StateEffect::ExitRequested
    );

    state.set_critical_operation(true);
    assert_eq!(
        state.navigate(NavCommand::Quit, 10),
        StateEffect::ExitDeferred
    );
    assert_eq!(state.navigate(NavCommand::Escape, 10), StateEffect::None);
    assert!(state.exit_pending());

    state.set_critical_operation(false);
    assert_eq!(state.take_deferred_exit(), StateEffect::ExitRequested);
    assert!(!state.exit_pending());
}

#[test]
fn empty_lists_never_underflow_selection() {
    let mut state = AppState::new();
    state.set_item_count(0);

    for cmd in [
        NavCommand::Down,
        NavCommand::Up,
        NavCommand::Top,
        NavCommand::Bottom,
        NavCommand::HalfPageDown,
        NavCommand::HalfPageUp,
    ] {
        state.navigate(cmd, 20);
        assert_eq!(state.selected(), 0);
    }
}

#[test]
fn advanced_inspect_lazy_sector_window_is_bounded_and_pageable() {
    use edpcli::application::inspect::{AdvancedInspectMode, AdvancedInspectWorkspace};
    use edpcli::application::inspect_tree::InspectNodeKind;
    use edpcli::inspect::InspectMeta;
    use edpcli::tui::state::AdvancedInspectSource;

    let context = crate::common::edp_inspect_context(10_000);

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(AdvancedInspectWorkspace {
        source: "disk6".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: Vec::new(),
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
        disk_layout: None,
        disk_layout_issue: None,
        backup_manifest: None,
    }));

    let rows = state.advanced_inspect_tree_rows();
    let partition_index = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.1"))
        .expect("partition region");
    state.advanced_inspect_move_tree(partition_index as isize);
    state.advanced_inspect_toggle_selected();

    let first_page = state.advanced_inspect_tree_rows();
    let first_page_sectors = first_page
        .iter()
        .filter(|row| {
            row.kind == InspectNodeKind::Sector
                && row.range.start_lba >= 2_048
                && row.range.start_lba < 2_248
        })
        .collect::<Vec<_>>();
    assert_eq!(first_page_sectors.len(), 64);
    assert_eq!(first_page_sectors.first().unwrap().range.start_lba, 2_048);
    assert_eq!(first_page_sectors.last().unwrap().range.start_lba, 2_111);
    assert!(first_page.iter().any(|row| row.label.starts_with("下一页")));
    assert!(
        first_page.len() < 100,
        "tree unexpectedly eager: {}",
        first_page.len()
    );

    let next_index = first_page
        .iter()
        .position(|row| row.label.starts_with("下一页"))
        .expect("next page row");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(next_index as isize - current as isize);
    state.advanced_inspect_toggle_selected();

    let second_page = state.advanced_inspect_tree_rows();
    let second_page_sectors = second_page
        .iter()
        .filter(|row| {
            row.kind == InspectNodeKind::Sector
                && row.range.start_lba >= 2_048
                && row.range.start_lba < 2_248
        })
        .collect::<Vec<_>>();
    assert_eq!(second_page_sectors.len(), 64);
    assert_eq!(second_page_sectors.first().unwrap().range.start_lba, 2_112);
    assert_eq!(second_page_sectors.last().unwrap().range.start_lba, 2_175);
    assert!(!second_page
        .iter()
        .any(|row| { row.kind == InspectNodeKind::Sector && row.range.start_lba == 2_048 }));
    assert!(second_page.iter().any(|row| row.label.contains("上一页")));
    assert!(
        second_page.len() < 101,
        "tree unexpectedly eager: {}",
        second_page.len()
    );
}

#[test]
fn advanced_sector_inspector_is_on_demand_bounded_and_fail_soft() {
    use edpcli::application::inspect::{
        AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    };
    use edpcli::application::inspect_tree::InspectNodeKind;
    use edpcli::inspect::InspectMeta;
    use edpcli::tui::state::{AdvancedInspectSource, SectorInspectMode};

    fn item(
        lba: u64,
        decoded: Option<Vec<u8>>,
        decode_error: Option<&str>,
        meta_text: Option<&str>,
    ) -> AdvancedInspectItem {
        AdvancedInspectItem {
            lba,
            regions: vec![format!("LBA{lba}")],
            raw: vec![0x5a; edpcli::application::support::SECTOR],
            raw_sha256: format!("raw-{lba}"),
            raw_nonzero: edpcli::application::support::SECTOR,
            decoded_sha256: decoded.as_ref().map(|_| format!("decoded-{lba}")),
            decode_ranges: if decoded.is_some() {
                vec![edpcli::inspect::DecodeRange::new(
                    0,
                    edpcli::application::support::SECTOR,
                )]
            } else {
                Vec::new()
            },
            decoded,
            method: Some(if decode_error.is_some() {
                "raw-only".into()
            } else {
                "test".into()
            }),
            decode_error: decode_error.map(str::to_string),
            parse_state: edpcli::inspect::InspectParseState::Parsed,
            diagnostics: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            meta_text: meta_text.map(str::to_string),
        }
    }

    let context = crate::common::edp_inspect_context(5_000);
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(9)));
    state.advanced_inspect_finish(Ok(AdvancedInspectWorkspace {
        source: "disk9".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: vec![item(0, None, None, Some("meta-lba0"))],
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
        disk_layout: None,
        disk_layout_issue: None,
        backup_manifest: None,
    }));

    let rows = state.advanced_inspect_tree_rows();
    let protocol = rows
        .iter()
        .position(|row| row.id.ends_with("/region.protocol"))
        .unwrap();
    state.advanced_inspect_move_tree(protocol as isize);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let sector0 = rows
        .iter()
        .position(|row| row.kind == InspectNodeKind::Sector && row.range.start_lba == 0)
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(sector0 as isize - current as isize);

    let request = state
        .advanced_inspect_open_selected_sector()
        .expect("Meta-only LBA0 must request Decode supplement");
    assert_eq!(request.1, 0);
    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 0);
    assert_eq!(sector.mode, SectorInspectMode::Mixed);
    assert!(sector.pending);

    state.advanced_inspect_sector_finish(
        0,
        Ok(item(
            0,
            Some(vec![0xa5; edpcli::application::support::SECTOR]),
            None,
            None,
        )),
    );
    let merged = state.advanced_inspect_sector_item().unwrap();
    assert_eq!(merged.meta_text.as_deref(), Some("meta-lba0"));
    assert_eq!(merged.decoded.as_ref().unwrap()[0], 0xa5);
    assert!(!state.advanced_inspect_sector().unwrap().pending);

    state.advanced_inspect_sector_move_cursor(10_000);
    assert_eq!(
        state.advanced_inspect_sector().unwrap().cursor,
        edpcli::application::support::SECTOR - 1
    );
    state.advanced_inspect_sector_move_cursor(-10_000);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 0);
    state.advanced_inspect_sector_set_mode(SectorInspectMode::Raw);
    assert_eq!(
        state.advanced_inspect_sector().unwrap().mode,
        SectorInspectMode::Raw
    );
    state.advanced_inspect_sector_set_mode(SectorInspectMode::Decode);
    assert_eq!(
        state.advanced_inspect_sector().unwrap().mode,
        SectorInspectMode::Decode
    );
    state.advanced_inspect_sector_set_mode(SectorInspectMode::Mixed);
    assert_eq!(
        state.advanced_inspect_sector().unwrap().mode,
        SectorInspectMode::Mixed
    );

    let request = state
        .advanced_inspect_shift_sector(1)
        .expect("uncached LBA1 must request on-demand read");
    assert_eq!(request.1, 1);
    state.advanced_inspect_sector_finish(1, Ok(item(1, None, Some("decoder unavailable"), None)));
    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 1);
    assert!(!sector.pending);
    assert_eq!(sector.error.as_deref(), Some("decoder unavailable"));
    assert_eq!(state.advanced_inspect_sector_item().unwrap().raw[0], 0x5a);
    assert!(state.advanced_inspect_shift_sector(-1).is_none());
    assert_eq!(state.advanced_inspect_sector().unwrap().lba, 0);

    for lba in 13..=18 {
        state.advanced_inspect_sector_finish(
            lba,
            Ok(item(
                lba,
                Some(vec![lba as u8; edpcli::application::support::SECTOR]),
                None,
                None,
            )),
        );
    }
    let workspace = state.advanced_inspect().unwrap().result.as_ref().unwrap();
    let on_demand = workspace
        .items
        .iter()
        .filter(|value| value.lba >= 13)
        .map(|value| value.lba)
        .collect::<Vec<_>>();
    assert_eq!(on_demand, vec![14, 15, 16, 17, 18]);
}

#[test]
fn provision_scheme_picker_moves_without_moving_device_selection() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(state.selected(), 0);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_scheme_picker_open());

    state.navigate(NavCommand::Down, 20);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected(), 0);
    assert_eq!(state.provision_scheme_selected(), 2);
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode2);
}

#[test]
fn every_table_kind_supports_shared_whole_column_reordering() {
    use edpcli::tui::table_layout::{layout_for, TableKind};

    for kind in [
        TableKind::Devices,
        TableKind::Backups,
        TableKind::InspectFields,
    ] {
        let mut state = AppState::new();
        let count = layout_for(kind).specs().len();
        let original = (0..count).collect::<Vec<_>>();
        assert_eq!(state.table_column_order(kind), original);

        assert!(
            state.move_table_column_for_viewport(kind, false, 160),
            "{kind:?}: h/l should move active column to visual position 1"
        );
        assert_eq!(state.table_column_order(kind), original);

        assert!(
            state.reorder_table_column_for_viewport(kind, true, 160),
            "{kind:?}: < should swap the whole active column left"
        );
        let mut expected = original.clone();
        expected.swap(0, 1);
        assert_eq!(state.table_column_order(kind), expected);
        assert_eq!(state.table_active_column(kind), 0);

        assert!(
            state.reorder_table_column_for_viewport(kind, false, 160),
            "{kind:?}: > should swap the whole active column right"
        );
        assert_eq!(state.table_column_order(kind), original);
        assert_eq!(state.table_active_column(kind), 1);
    }
}

#[test]
fn table_copy_follows_logical_column_after_runtime_reorder() {
    use edpcli::tui::table_layout::TableKind;

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);

    assert!(state.move_table_column_for_viewport(TableKind::Devices, false, 160));
    assert!(state.move_table_column_for_viewport(TableKind::Devices, false, 160));
    assert_eq!(state.table_active_column(TableKind::Devices), 2);
    assert_eq!(
        state
            .table_copy_payload(TableKind::Devices, false)
            .as_deref(),
        Some("输电运检中心")
    );

    assert!(state.reorder_table_column_for_viewport(TableKind::Devices, true, 160));
    assert_eq!(state.table_active_column(TableKind::Devices), 1);
    assert_eq!(state.table_logical_column(TableKind::Devices, 1), 2);
    assert_eq!(
        state
            .table_copy_payload(TableKind::Devices, false)
            .as_deref(),
        Some("输电运检中心"),
        "cell copy must remain bound to Department after whole-column reorder"
    );

    let row = state
        .table_copy_payload(TableKind::Devices, true)
        .expect("copy whole device row");
    let cells = row.split('\t').collect::<Vec<_>>();
    assert_eq!(cells[1], "输电运检中心");
    assert_eq!(cells.len(), 11);

    assert!(state.move_table_column_edge_for_viewport(TableKind::Devices, true, 160));
    let last = state.table_copy_payload(TableKind::Devices, false).unwrap();
    assert!(!last.is_empty());
    assert!(state.move_table_column_edge_for_viewport(TableKind::Devices, false, 160));
    let first = state.table_copy_payload(TableKind::Devices, false).unwrap();
    assert_ne!(first, last);
}
