use edpcli::tui::state::{
    AppState, InputMode, NavCommand, ProvisionForm, ProvisionKind, ProvisionStage, StateEffect,
    Workspace,
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
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
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
    state.provision_skip_backup();
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
    state.provision_skip_backup();
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
fn provision_has_four_physical_modes_plus_offline_and_prompts_for_backup_first() {
    assert_eq!(
        ProvisionKind::ALL,
        [
            ProvisionKind::Mode0,
            ProvisionKind::Mode1,
            ProvisionKind::Mode2,
            ProvisionKind::Mode3,
            ProvisionKind::Offline,
        ]
    );

    let mut row = device(64_000_000_000);
    row.n_baks = 2;
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    assert_eq!(state.item_count(), 1);
    assert!(state.selected_device_disk().is_none());
    assert_eq!(state.provision_select_disk(), Some(6));
    assert_eq!(state.provision().stage, ProvisionStage::BackupPrompt);
    assert!(state.provision_backup_summary().contains("已保存 2 份"));

    state.provision_skip_backup();
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    assert_eq!(state.item_count(), ProvisionKind::ALL.len());
}

#[test]
fn provision_backup_prompt_only_enters_menu_after_save_finishes() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    state.provision_select_disk();
    assert!(state.provision_backup_summary().contains("没有保存记录"));

    state.provision_begin_backup_save();
    assert_eq!(state.provision().stage, ProvisionStage::BackupSaving);
    state.provision_finish_backup_save(Ok(()));
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
    state.provision_skip_backup();
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
    assert_eq!(state.provision().stage, ProvisionStage::BackupPrompt);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.provision().stage, ProvisionStage::SelectDisk);
    assert!(state.selected_device_disk().is_none());

    assert_eq!(state.provision_select_disk(), Some(6));
    state.provision_skip_backup();
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.provision().stage, ProvisionStage::BackupPrompt);
    assert_eq!(state.selected_device_disk(), Some(6));

    state.provision_skip_backup();
    state.provision_begin_selected();
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.navigate(NavCommand::Escape, 20), StateEffect::None);
    assert_eq!(state.provision().stage, ProvisionStage::Menu);
}

#[test]
fn provision_tab_roundtrip_preserves_current_flow_state() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    assert_eq!(state.provision_select_disk(), Some(6));
    state.provision_skip_backup();
    state.provision_begin_selected();
    state.provision_mut().form.label = "保持当前制盘状态!SAFE6".into();
    assert_eq!(state.provision().stage, ProvisionStage::Form);

    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::PreviousWorkspace, 20);

    assert_eq!(state.workspace(), Workspace::Provision);
    assert_eq!(state.provision().stage, ProvisionStage::Form);
    assert_eq!(state.selected_device_disk(), Some(6));
    assert_eq!(state.provision().form.label, "保持当前制盘状态!SAFE6");

    state.navigate(NavCommand::Right, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
    state.navigate(NavCommand::Left, 20);
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
    assert_eq!(form.password, "0000aaaa");
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
    state.provision_skip_backup();
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
    state.provision_skip_backup();
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
    assert!(fields.iter().any(|(label, _, _)| label == "启动区输入方式"));
    assert!(fields.iter().any(|(label, _, _)| label == "交换区输入方式"));
    assert!(fields.iter().any(|(label, _, _)| label == "保密区输入方式"));
}

#[test]
fn provision_capacity_unit_cycles_mib_gib_sector_without_geometry_change() {
    use edpcli::provision::{CapacityInputMode, QuickCapacityUnit};

    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    state.provision_mut().form.share_input_mode = CapacityInputMode::Quick;
    state.provision_mut().form.share_quick_unit = QuickCapacityUnit::MiB;
    state.provision_mut().form.share_mib = "6644".into();
    let mode_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "交换区输入方式")
        .expect("share input mode field");
    state.provision_mut().field_selected = mode_index;

    assert!(state.provision_toggle_selected_option());
    assert_eq!(
        state.provision().form.share_quick_unit,
        QuickCapacityUnit::GiB
    );
    assert_eq!(state.provision().form.share_mib, "6.48828125");
    assert!(state
        .provision_visible_fields()
        .iter()
        .any(|(label, value, _)| label == "交换区容量 (GiB)" && *value == "6.48828125"));
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
    assert_eq!(state.provision().form.share_mib, "6644");
}

#[test]
fn provision_capacity_hints_match_each_partition() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let fields = state.provision_visible_fields();
    let hint_for = |label: &str| {
        let index = fields
            .iter()
            .position(|(field, _, _)| field.starts_with(label))
            .expect("partition capacity field");
        state.provision_field_hint(index).expect("partition hint")
    };

    let boot = hint_for("启动区");
    let share = hint_for("交换区");
    let encrypt = hint_for("保密区");
    assert!(boot.starts_with("启动区"), "{boot}");
    assert!(share.starts_with("交换区"), "{share}");
    assert!(encrypt.starts_with("保密区"), "{encrypt}");
    assert_ne!(boot, share);
    assert_ne!(share, encrypt);
}

#[test]
fn provision_form_sections_are_compact_and_user_facing() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
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
        vec!["身份信息", "分区布局", "格式化（可选）", "密码策略"]
    );
}

#[test]
fn provision_layout_editor_reports_total_space_and_selected_partition_limits() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);

    let encrypt = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label.starts_with("保密区容量"))
        .expect("encrypt capacity");
    state.provision_mut().field_selected = encrypt;
    let lines = state.provision_layout_editor_lines(40);

    assert!(lines.iter().any(|line| line.contains("整盘")), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("可分区 LBA")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("未分配")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("比例 [")),
        "{lines:?}"
    );
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
fn provision_compact_rows_keep_partition_capacity_mode_and_start_together() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
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
    assert_eq!(labels[1], "交换区输入方式");
    let share_start_row = rows
        .iter()
        .find(|(_, indexes)| {
            indexes
                .iter()
                .any(|index| fields[*index].0 == "交换区起点 LBA")
        })
        .expect("share start row");
    assert_eq!(share_start_row.1.len(), 1);
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
    state.provision_skip_backup();
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
    let constraints = state.provision_layout_editor_lines(40);
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
    state.provision_skip_backup();
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
    let constraints = state.provision_layout_editor_lines(40);
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
    state.provision_skip_backup();
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
    for _ in 0..4 {
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
    assert_eq!(state.provision().form.encrypt_mib, "1024");
    assert_ne!(state.provision().form.share_mib, original_share);
}

#[test]
fn mode0_live_layout_reports_invalid_geometry_without_rebalancing_other_fields() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(64_000_000_000)]);
    state.navigate(NavCommand::WorkspaceProvision, 20);
    state.provision_select_disk();
    state.provision_skip_backup();
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
