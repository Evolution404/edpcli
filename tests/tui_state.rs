use edpcli::tui::state::{
    AppState, InputMode, NavCommand, ProvisionForm, ProvisionKind, ProvisionSizeMode, StateEffect,
};

fn device(size: u64) -> edpcli::disk_scan::Row {
    edpcli::disk_scan::Row {
        disk: 6,
        size,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some("disk&ven_test&prod_test".into()),
        onlyid: Some("1402259934".into()),
        dept: Some("输电运检中心".into()),
        user: Some("测试用户".into()),
        n_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        partitions: None,
    }
}

#[test]
fn provision_label_defaults_to_jiangsu_safe6_and_remains_editable() {
    let mut form = ProvisionForm::default();
    assert_eq!(form.label, "江苏电力!SAFE6");
    assert_eq!(form.password, "0000aaaa");
    assert_eq!(form.volume_label, "启动区");
    assert_eq!(form.size_mode, ProvisionSizeMode::Manual);
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
fn provision_format_controls_follow_current_mode_targets() {
    let mut state = AppState::new();
    state.provision_mut().kind = ProvisionKind::Mode1;
    let fields = state.provision_visible_fields();
    assert!(fields
        .iter()
        .any(|(label, _, _)| label.contains("启动/交换区 type2 明文")));
    assert!(fields
        .iter()
        .any(|(label, _, _)| label.contains("保密区 type4 加密")));
    assert!(!fields
        .iter()
        .any(|(label, _, _)| label.contains("启动区 type1")));
    let share_index = fields
        .iter()
        .position(|(label, _, _)| label.contains("启动/交换区 type2"))
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
        .any(|(label, value, _)| label.contains("兼容保留区") && *value == "— 不可格式化"));
    assert!(!fields
        .iter()
        .any(|(label, _, _)| label.contains("启动区 type1")));
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
fn provision_manual_ranges_follow_remaining_capacity_and_ratio_mode_fills_usable_space() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let boot_hint = state
        .provision_field_hint(1)
        .expect("boot sector range hint");
    assert!(boot_hint.contains("20417"));

    let total_sectors = 64_000_000_000u64 / 512;
    let lce =
        edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            512,
        )
        .unwrap();
    let usable_sectors = lce.start_lba - edpcli::provision::OFFICIAL_PARTITION_START_SECTOR;
    state.provision_mut().form.share_mib = "1024".into();
    let encrypt_hint = state.provision_field_hint(3).expect("encrypt range hint");
    let expected_max = (usable_sectors - 20_417 - 1024 * 2048) / 2048;
    assert!(encrypt_hint.contains(&format!("可填 1..{expected_max} MiB")));
    assert!(encrypt_hint.contains("剩余"));

    state.provision_mut().field_selected = 0;
    assert!(state.provision_toggle_selected_option());
    assert_eq!(state.provision().form.size_mode, ProvisionSizeMode::Ratio);
    state.provision_mut().form.label_id = "1402259934".into();
    state.provision_mut().form.user = "测试用户".into();
    state.provision_mut().form.dept = "输电运检中心".into();
    let request = state.provision_request().expect("ratio request");
    assert_eq!(request.boot_mib, None);
    assert_eq!(request.boot_sectors, Some(20_417));
    assert_eq!(
        request.share_mib.unwrap() + request.encrypt_mib.unwrap(),
        (usable_sectors - 20_417) / 2048
    );
}

#[test]
fn mode0_defaults_share_to_remaining_space_once_without_linking_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
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
    assert_eq!(state.provision().form.encrypt_mib, "1024");
    assert_eq!(
        state.provision().form.share_mib,
        expected_share_mib.to_string()
    );
    let expected_remainder = usable_sectors - 20_417 - expected_share_mib * 2048 - 1024 * 2048;
    let share_hint = state.provision_field_hint(2).expect("share capacity hint");
    assert!(share_hint.contains(&format!("剩余 {expected_remainder} 扇区")));

    let original_share = state.provision().form.share_mib.clone();
    state.provision_mut().field_selected = 3;
    for _ in 0..4 {
        state.provision_backspace();
    }
    for ch in "512".chars() {
        state.provision_push_char(ch);
    }
    assert_eq!(state.provision().form.encrypt_mib, "512");
    assert_eq!(state.provision().form.share_mib, original_share);
    assert!(state
        .provision_field_hint(3)
        .expect("encrypt capacity hint")
        .contains("剩余"));

    state.provision_begin_selected();
    assert_eq!(state.provision().form.encrypt_mib, "512");
    assert_eq!(state.provision().form.share_mib, original_share);

    state.replace_devices(vec![device(32_000_000_000)]);
    state.provision_begin_selected();
    assert_eq!(state.provision().form.encrypt_mib, "1024");
    assert_ne!(state.provision().form.share_mib, original_share);
}

#[test]
fn mode0_capacity_hint_reports_overflow_without_rebalancing_other_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_begin_selected();
    let original_encrypt = state.provision().form.encrypt_mib.clone();
    state.provision_mut().form.share_mib = "999999999".into();

    let hint = state.provision_field_hint(2).expect("overflow hint");
    assert!(hint.contains("超出"));
    assert_eq!(state.provision().form.encrypt_mib, original_encrypt);
}

#[test]
fn provision_force_change_password_checkbox_defaults_off_and_toggles() {
    let mut state = AppState::new();
    state.provision_mut().kind = ProvisionKind::Mode1;
    state.provision_mut().field_selected = state.provision_field_count() - 1;

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
    assert_eq!(
        state.navigate(NavCommand::Escape, 10),
        StateEffect::ExitDeferred
    );
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
