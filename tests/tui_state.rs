use edpcli::tui::state::{
    AppState, InputMode, NavCommand, ProvisionForm, ProvisionKind, ProvisionStage, StateEffect,
    Workspace,
};

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
    state.scroll_table(TableKind::Devices, false);
    state.scroll_table(TableKind::Devices, false);
    state.begin_provision_for_selected_device().unwrap();
    assert_eq!(state.navigation().depth(), 1);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
    assert_eq!(state.selected_device_disk(), Some(7));
    assert_eq!(state.table_scroll_offset(TableKind::Devices), 2);
    assert_eq!(state.navigation().depth(), 0);
}

fn device(size: u64) -> edpcli::disk_scan::Row {
    edpcli::disk_scan::Row {
        disk: 6,
        size,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
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

#[test]
fn registered_mode0_to_mode1_form_keeps_exact_encrypt_geometry() {
    use edpcli::provision::{CapacityInputMode, DiskProvisionKind};
    use edpcli::sectors::EdpfPartition;
    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.navigate(NavCommand::Down, 20);
    state.provision_begin_selected();
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
    use edpcli::provision::DiskProvisionKind;
    use edpcli::sectors::EdpfPartition;

    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
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
    assert!(state.provision_toggle_force_change_password());
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    assert_eq!(state.item_count(), 1);
    assert!(state.selected_device_disk().is_none());
    assert_eq!(state.provision_select_disk(), Some(6));
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    assert_eq!(state.item_count(), ProvisionKind::ALL.len());
}

#[test]
fn provision_requires_a_new_explicit_usb_selection_after_other_workspace_selection() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    assert_eq!(state.selected_device_disk(), Some(6));
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    assert!(state.selected_device_disk().is_none());
    state.provision_begin_selected();
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    assert_eq!(state.provision_select_disk(), Some(6));
    state.provision_begin_selected();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
}

#[test]
fn provision_escape_walks_back_one_level_without_exiting() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);

    assert_eq!(state.provision_select_disk(), Some(6));
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);

    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    state.provision_begin_selected();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.workspace(), Workspace::Devices);
}

#[test]
fn provision_flow_is_hidden_from_tab_cycle_and_explicit_reentry_preserves_state() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    state.provision_begin_selected();
    state.provision_mut().form.label = "保持当前制盘状态!SAFE6".into();
    assert_eq!(state.provision().stage, ProvisionStage::Form);

    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Backups);
    state.navigate(NavCommand::NextWorkspace, 20);
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
    assert_eq!(form.encrypt_mib, "1024");
    assert!(edpcli::provision::OnlyId::parse(&form.label_id).is_ok());
    assert!(!form.force_change_password);
    assert!(!form.format_boot && !form.format_share && !form.format_encrypt);
    assert_eq!(
        form.boot_fs,
        edpcli::provision::OfficialFilesystemFormat::Fat16
    );
    assert_eq!(
        form.share_fs,
        edpcli::provision::OfficialFilesystemFormat::ExFat
    );
    form.label = "自定义标签!SAFE6".into();
    form.label_id = "123456789".into();
    assert_eq!(form.label, "自定义标签!SAFE6");
    assert_eq!(form.label_id, "123456789");
}

#[test]
fn provision_key_probe_prefills_only_verified_default_domains() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();

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
    assert!(state.provision().form.encrypt_source_password.is_empty());
    assert_eq!(
        state.provision().form.encrypt_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
}

#[test]
fn provision_key_probe_never_overwrites_user_entered_source_password() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
    state.provision_mut().form.share_source_password = "ManualOldPass!".into();

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
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
    state.provision_mut().form.share_source_password = "0000aaaa".into();
    state.provision_mut().form.share_source_knowledge =
        edpcli::provision::SourcePasswordKnowledge::DefaultVerified;

    let index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.contains("交换区来源密码"))
        .unwrap();
    state.provision_mut().field_selected = index;
    state.provision_push_char('x');

    assert_eq!(
        state.provision().form.share_source_knowledge,
        edpcli::provision::SourcePasswordKnowledge::Unknown
    );
}

#[test]
fn mode0_to_mode1_unknown_encrypt_disables_only_encrypt_target_password() {
    use edpcli::provision::{DiskProvisionKind, SourcePasswordKnowledge};
    use edpcli::sectors::EdpfPartition;

    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode1);
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
        .position(|(label, _, _)| label.contains("二合一区目标密码"))
        .unwrap();
    let encrypt_target = fields
        .iter()
        .position(|(label, _, _)| label == "保密区目标密码")
        .unwrap();

    assert_eq!(fields[share_target].1, "0000aaaa");
    assert!(fields[share_target].2);
    assert_eq!(fields[encrypt_target].1, "— PreserveOpaque 禁用");
    assert!(!fields[encrypt_target].2);

    state.provision_mut().field_selected = encrypt_target;
    assert!(!state.provision_selected_field_is_editable());
    state.provision_mut().field_selected = share_target;
    assert!(state.provision_selected_field_is_editable());
}

#[test]
fn source_password_verify_request_is_scoped_to_selected_domain() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
    state.provision_mut().form.encrypt_source_password = "EncryptOld1!".into();

    let index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.contains("保密区来源密码"))
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
        edpcli::provision::OfficialFilesystemFormat::Fat16
    );

    state.provision_mut().kind = ProvisionKind::Mode2;
    let fields = state.provision_visible_fields();
    assert!(fields
        .iter()
        .any(|(label, value, _)| label.contains("兼容保留区") && *value == "固定，不格式化"));
    assert!(!fields.iter().any(|(label, _, _)| label == "启动区格式化"));
    let reserve_index = fields
        .iter()
        .position(|(label, _, _)| label.contains("兼容保留区"))
        .unwrap();
    state.provision_mut().field_selected = reserve_index;
    assert!(!state.provision_toggle_selected_option());
}

#[test]
fn provision_prefers_scanned_onlyid_and_generates_candidate_only_when_missing() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == "交换区容量 (MiB)"));
    assert!(fields
        .iter()
        .any(|(label, _, _)| label == "保密区容量 (MiB)"));
}

#[test]
fn provision_text_field_cursor_edits_in_place() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
fn provision_capacity_unit_cycles_mib_gib_sector_without_geometry_change() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    state.provision_mut().form.share_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.share_quick_unit = QuickCapacityUnit::MiB;
    state.provision_mut().form.share_mib = "6644".into();
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
    assert_eq!(state.provision().form.share_mib, "6.488");
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(|(label, value, _)| label == "交换区容量 (GiB)" && *value == "6.488"));
    let request = state.provision_request().expect("GiB request");
    assert_eq!(request.share_mib, None);
    assert_eq!(request.share_sectors, Some(13_606_912));

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Exact
    );
    assert_eq!(state.provision().form.share_sectors, "13606912");

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_input_mode,
        CapacityInputMode::Quick
    );
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::MiB
    );
    assert_eq!(state.provision().form.share_mib, "6644.000");
}

#[test]
fn editing_generated_gib_text_uses_user_value_even_if_display_text_is_identical() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
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
    assert_eq!(state.provision().form.share_mib, "6.488");
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(13_606_912)
    );

    state.provision_cursor_end();
    state.provision_backspace();
    state.provision_push_char('8');
    assert_eq!(state.provision().form.share_mib, "6.488");
    assert_eq!(
        state.provision_request().unwrap().share_sectors,
        Some(13_606_322)
    );
    assert!(state.provision_toggle_selected_option());
    assert_eq!(state.provision().form.share_sectors, "13606322");
}

#[test]
fn provision_exact_sector_capacity_cycles_through_decimal_mib_and_gib_losslessly() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
    assert_eq!(state.provision().form.boot_mib, "9.969");
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(|(label, value, _)| label == "启动区容量 (MiB)" && *value == "9.969"));
    let request = state.provision_request().expect("decimal MiB request");
    assert_eq!(request.boot_mib, None);
    assert_eq!(request.boot_sectors, Some(20_417));

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.boot_quick_unit,
        QuickCapacityUnit::GiB
    );
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(|(label, _, _)| label == "启动区容量 (GiB)"));

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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
    assert_eq!(boot, "Space 切换 MiB / GiB / sector · f 填满");
    assert_eq!(share, boot);
    assert_eq!(encrypt, boot);
}

#[test]
fn provision_input_policy_filters_invalid_characters_and_ranges() {
    use edpcli::provision::CapacityInputMode;

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    for _ in 0..4 {
        state.navigate(NavCommand::Down, 20);
    }
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Plain);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
}

#[test]
fn plain_form_defaults_to_one_partition_at_lba2048_filling_the_disk() {
    let mut state = AppState::new();
    enter_plain_form(&mut state);

    let plan = state.provision_plain_plan().unwrap();
    let total_sectors = 64_000_000_000u64 / edpcli::common::SECTOR as u64;
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    let max_sectors = state
        .provision_layout_editor_lines()
        .into_iter()
        .find_map(|line| {
            let marker = "最大可设 ";
            line.strip_prefix(marker)
                .and_then(|rest| rest.rsplit_once('('))
                .and_then(|(_, tail)| tail.strip_suffix(" sector)"))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .expect("maximum sector count");
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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
fn provision_form_sections_are_compact_and_user_facing() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    let mut sections = Vec::new();
    for index in 0..fields.len() {
        if let Some(section) = state.provision_field_section(index) {
            if sections.last().copied() != Some(section) {
                sections.push(section);
            }
        }
        if let Some(hint) = state.provision_field_hint(index) {
            for internal in ["canonical", "PassInfo", "XOR", "Preserve", "Rebuild"] {
                assert!(
                    !hint.contains(internal),
                    "internal term leaked in hint: {hint}"
                );
            }
        }
    }
    assert_eq!(
        sections,
        vec![
            "身份信息",
            "密码域",
            "分区布局",
            "格式化（可选）",
            "密码策略",
        ]
    );
}

#[test]
fn provision_layout_editor_reports_total_space_and_selected_partition_limits() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    let lines = state.provision_layout_editor_lines();

    assert!(lines.iter().any(|line| line.contains("整盘")), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("可分区 LBA")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("未分配")),
        "{lines:?}"
    );
    let bar = state.provision_layout_bar(40);
    assert_eq!(bar.len(), 40);
    assert!(bar.contains(&edpcli::tui::disk_layout::DiskRegionKind::Encrypt));
    assert!(bar.contains(&edpcli::tui::disk_layout::DiskRegionKind::Free));
    assert!(lines.iter().any(|line| line == "当前: 保密区"), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("最大可设")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("还能增加")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("限制: 可分区末端")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("✓ 当前布局")),
        "{lines:?}"
    );
}

#[test]
fn provision_compact_rows_keep_partition_capacity_and_start_together() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    let rows = state.provision_compact_field_rows();
    let share_row = rows
        .iter()
        .find(|(_, indexes)| {
            indexes
                .iter()
                .any(|index| fields[*index].0.starts_with("交换区容量"))
        })
        .expect("share row");
    let labels = share_row
        .1
        .iter()
        .map(|index| fields[*index].0.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 2);
    assert!(labels[0].starts_with("交换区容量"));
    assert_eq!(labels[1], "交换区起点 LBA");
}

#[test]
fn provision_layout_rows_are_sorted_by_start_lba_including_free_space() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(8_053_063_680)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

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

    let rows = state
        .provision_layout_editor_lines()
        .into_iter()
        .filter(|line| {
            line.starts_with("启动区")
                || line.starts_with("交换区")
                || line.starts_with("保密区")
                || line.starts_with("空闲")
        })
        .collect::<Vec<_>>();
    let starts = rows
        .iter()
        .map(|line| {
            let value = line
                .split("LBA ")
                .nth(1)
                .expect("layout row LBA")
                .split('–')
                .next()
                .expect("layout row start");
            value.parse::<u64>().expect("numeric start LBA")
        })
        .collect::<Vec<_>>();
    assert!(starts.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
}

#[test]
fn registered_mode0_to_mode1_preview_keeps_encrypt_anchor_and_blocks_overlap() {
    use edpcli::provision::{CapacityInputMode, DiskProvisionKind};
    use edpcli::sectors::EdpfPartition;
    let encrypt_start = 4_020_480u64;
    let mut row = device(64_000_000_000);
    row.provision_kind = DiskProvisionKind::Mode0;
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
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode1);
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
    let constraints = state.provision_layout_editor_lines();
    assert!(
        constraints.iter().any(|line| {
            line.contains(&format!("限制: 后续保密区固定起点 LBA {encrypt_start}"))
        }),
        "{constraints:?}"
    );

    let smaller = encrypt_start - 63 - 4096;
    state.provision_mut().form.share_sectors = smaller.to_string();
    let preview = state.provision_geometry_preview_lines();
    assert!(preview
        .iter()
        .any(|line| line.contains("空隙  4096 sector")));
    assert!(preview
        .iter()
        .any(|line| line.contains(&format!("LBA {encrypt_start}–"))));
    let request = state
        .provision_request()
        .expect("shrink must leave a legal gap");
    assert_eq!(request.encrypt_start_lba, Some(encrypt_start));

    state.provision_mut().form.share_sectors = (encrypt_start - 63 + 1).to_string();
    let preview = state.provision_geometry_preview_lines();
    assert!(preview.iter().any(|line| line.contains("overlap")));
    assert!(state.provision_request().is_err());
}

#[test]
fn plain_mode0_preview_reflows_unanchored_share_after_boot_edit() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
    state.provision_mut().form.boot_sectors = "10000".into();
    state.provision_mut().form.label_id = "1402259934".into();
    state.provision_mut().form.user = "测试用户".into();
    state.provision_mut().form.dept = "输电运检中心".into();
    let request = state.provision_request().expect("plain mode0 request");
    assert_eq!(request.boot_start_lba, Some(63));
    assert_eq!(request.share_start_lba, Some(10_063));
    assert!(state
        .provision_geometry_preview_lines()
        .iter()
        .any(|line| line.contains("LBA 10063–")));
    let boot_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("启动区容量"))
        .expect("boot capacity");
    state.provision_mut().field_selected = boot_index;
    let constraints = state.provision_layout_editor_lines();
    assert!(
        constraints
            .iter()
            .any(|line| { line.contains("后续未锚定分区可自动后移") }),
        "{constraints:?}"
    );
}

#[test]
fn mode0_defaults_share_to_remaining_space_once_without_linking_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let total_sectors = 64_000_000_000u64 / 512;
    let lce =
        edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            512,
        )
        .unwrap();
    let usable_sectors = lce.start_lba - edpcli::provision::OFFICIAL_PARTITION_START_SECTOR;
    let expected_share_mib = (usable_sectors - 20_417 - 1024 * 2048) / 2048;

    assert_eq!(state.provision().form.boot_sectors, "20417");
    assert_eq!(state.provision().form.encrypt_mib, "1024.000");
    assert_eq!(
        state.provision().form.share_mib,
        format!("{expected_share_mib}.000")
    );
    let expected_remainder = usable_sectors - 20_417 - expected_share_mib * 2048 - 1024 * 2048;
    assert!(state
        .provision_geometry_preview_lines()
        .iter()
        .any(|line| line == &format!("未分配  {expected_remainder} sector")));

    let original_share = state.provision().form.share_mib.clone();
    state.provision_mut().field_selected = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "保密区容量 (MiB)")
        .expect("encrypt field");
    state.provision_cursor_end();
    let current_len = state.provision().form.encrypt_mib.chars().count();
    for _ in 0..current_len {
        state.provision_backspace();
    }
    for ch in "512".chars() {
        state.provision_push_char(ch);
    }
    assert_eq!(state.provision().form.encrypt_mib, "512");
    assert_eq!(state.provision().form.share_mib, original_share);
    assert!(state
        .provision_geometry_preview_lines()
        .iter()
        .any(|line| line == &format!("未分配  {} sector", expected_remainder + 512 * 2048)));

    state.provision_begin_selected();
    assert_eq!(state.provision().form.encrypt_mib, "512");
    assert_eq!(state.provision().form.share_mib, original_share);

    state.replace_devices(vec![device(32_000_000_000)]);
    state.provision_begin_selected();
    assert_eq!(state.provision().form.encrypt_mib, "1024.000");
    assert_ne!(state.provision().form.share_mib, original_share);
}

#[test]
fn mode0_live_layout_reports_invalid_geometry_without_rebalancing_other_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_begin_selected();
    let original_encrypt = state.provision().form.encrypt_mib.clone();
    state.provision_mut().form.share_mib = "999999999".into();

    let preview = state.provision_geometry_preview_lines();
    assert!(preview.iter().any(|line| line.starts_with("布局无效:")));
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
    assert!(state.provision_toggle_force_change_password());
    assert!(state.provision().form.force_change_password);
    assert!(state.provision_toggle_force_change_password());
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
    assert_eq!(state.input_mode(), InputMode::Help);
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.input_mode(), InputMode::Normal);
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
    use edpcli::backup_metadata::PartitionGeometry;
    use edpcli::inspect::InspectMeta;
    use edpcli::inspect_target::InspectDiskContext;
    use edpcli::tui::state::AdvancedInspectSource;

    let mut context =
        InspectDiskContext::new(vec![0; edpcli::common::METADATA_IMAGE_LEN], None, 10_000);
    context.partitions.push(PartitionGeometry {
        index: 0,
        partition_type: 2,
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: 2_048,
        sector_size: edpcli::common::SECTOR as u64,
        partition_size: 200 * edpcli::common::SECTOR as u64,
        sector_count: 200,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    });

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(AdvancedInspectWorkspace {
        source: "disk6".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: Vec::new(),
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    }));

    let rows = state.advanced_inspect_tree_rows();
    let partition_index = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0"))
        .expect("partition region");
    state.advanced_inspect_move_tree(partition_index as isize);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let extent_index = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0.extent"))
        .expect("partition extent");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(extent_index as isize - current as isize);
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
    use edpcli::inspect_target::InspectDiskContext;
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
            raw: vec![0x5a; edpcli::common::SECTOR],
            raw_sha256: format!("raw-{lba}"),
            raw_nonzero: edpcli::common::SECTOR,
            decoded_sha256: decoded.as_ref().map(|_| format!("decoded-{lba}")),
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

    let context = InspectDiskContext::new(vec![0; edpcli::common::METADATA_IMAGE_LEN], None, 5_000);
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(9)));
    state.advanced_inspect_finish(Ok(AdvancedInspectWorkspace {
        source: "disk9".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: vec![item(0, None, None, Some("meta-lba0"))],
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    }));

    let rows = state.advanced_inspect_tree_rows();
    let protocol = rows
        .iter()
        .position(|row| row.id.ends_with("/region.protocol"))
        .unwrap();
    state.advanced_inspect_move_tree(protocol as isize);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let extent = rows
        .iter()
        .position(|row| row.id.ends_with("/region.protocol.extent"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(extent as isize - current as isize);
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
            Some(vec![0xa5; edpcli::common::SECTOR]),
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
        edpcli::common::SECTOR - 1
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
                Some(vec![lba as u8; edpcli::common::SECTOR]),
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
