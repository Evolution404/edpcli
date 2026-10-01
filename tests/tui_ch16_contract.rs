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
        decode_ranges: Vec::new(),
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
        hardware_model: None,
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
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    state
}

#[test]
fn ch16_provision_form_is_two_pane_without_protection_or_shortcut_footer() {
    let state = provision_state();
    let text = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    for value in ["制盘配置", "生成计划", "计划确认", "执行", "完成"] {
        assert!(text.contains(value), "missing {value}");
    }
    assert!(text.contains("参数"), "{text}");
    assert!(text.contains("目标与磁盘布局"), "{text}");
    assert!(!text.contains("固定目标"), "{text}");
    assert!(!text.contains("写盘保护"), "{text}");
    assert!(
        !text.contains("↑/↓字段"),
        "form must not render a shortcut footer"
    );
}

#[test]
fn provision_lba8_advanced_identity_expands_edits_and_collapses_with_o_contract() {
    let mut state = provision_state();
    let base_count = state.provision_visible_fields().len();
    assert!(!state.provision().advanced_identity_open);
    let fields = state.provision_visible_fields();
    assert!(fields
        .iter()
        .any(|(label, value, _)| label == "高级设置" && value.contains("o 展开")));
    assert!(!fields.iter().any(|(label, _, _)| label == "GLab"));

    assert!(
        !state.provision_toggle_advanced_identity(),
        "o must only toggle while the Advanced Settings row is focused"
    );
    let advanced_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "高级设置")
        .expect("advanced settings row");
    state.provision_mut().field_selected = advanced_index;
    assert!(state.provision_toggle_advanced_identity());
    assert!(state.provision().advanced_identity_open);
    let fields = state.provision_visible_fields();
    assert_eq!(fields.len(), base_count + 14);
    assert_eq!(fields[state.provision().field_selected].0, "高级设置");
    assert!(fields[state.provision().field_selected]
        .1
        .contains("o 收起"));
    assert!(fields
        .iter()
        .any(|(label, value, _)| label == "Autonum" && *value == "YD000001"));

    let screen = rendered_lines(&state, 160, 60).join("\n").replace(' ', "");
    assert!(screen.contains("高级设置"), "{screen}");
    assert!(screen.contains("o收起"), "{screen}");
    assert!(!screen.contains("LBA8高级身份"), "{screen}");
    assert!(screen.contains("GLab"), "{screen}");
    let lines = rendered_lines(&state, 160, 60)
        .iter()
        .map(|line| line.replace(' ', ""))
        .collect::<Vec<_>>();
    let advanced_y = lines
        .iter()
        .position(|line| line.contains("高级设置"))
        .expect("advanced settings row must render");
    let glab_y = lines
        .iter()
        .position(|line| line.contains("GLab"))
        .expect("first advanced identity row must render");
    let indus_y = lines
        .iter()
        .position(|line| line.contains("Indus"))
        .expect("second advanced identity row must render");
    assert_eq!(
        glab_y,
        advanced_y + 1,
        "expanded advanced identity must start immediately below 高级设置 without a blank line"
    );
    assert_eq!(
        indus_y,
        glab_y + 1,
        "advanced identity rows must be contiguous without wrapped blank lines"
    );

    let glab_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "GLab")
        .expect("GLab row");
    state.provision_mut().field_selected = glab_index;
    assert!(state.provision_selected_field_is_editable());
    let before = state.provision().form.lba8_identity.glab.clone();
    state.provision_cursor_end();
    state.provision_push_char('|');
    assert_eq!(state.provision().form.lba8_identity.glab, before);
    assert!(state
        .provision()
        .message
        .as_deref()
        .is_some_and(|message| message.contains("LBA8")));
    state.provision_push_char('X');
    assert_eq!(
        state.provision().form.lba8_identity.glab,
        format!("{before}X")
    );
    assert!(state.provision().message.is_none());

    let advanced_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "高级设置")
        .expect("advanced settings row");
    state.provision_mut().field_selected = advanced_index;
    assert!(state.provision_toggle_advanced_identity());
    assert!(!state.provision().advanced_identity_open);
    assert_eq!(state.provision_visible_fields().len(), base_count);
    assert_eq!(
        state.provision_visible_fields()[state.provision().field_selected].0,
        "高级设置"
    );
    assert!(
        state.provision_visible_fields()[state.provision().field_selected]
            .1
            .contains("o 展开")
    );
}

#[test]
fn provision_verified_source_status_is_success_and_normal_values_have_no_input_fill() {
    use edpcli::provision::SourcePasswordKnowledge;

    let mut state = provision_state();
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: edpcli::provision::DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));
    state.provision_mut().field_selected = 1;
    state.provision_mut().form.label_id = "ZTESTONLY".into();

    let (width, height) = (160, 45);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let lines = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let compact = lines
        .iter()
        .map(|line| line.replace(' ', ""))
        .collect::<Vec<_>>();
    assert!(
        !compact.iter().any(|line| line.contains("来源状态")),
        "source status must be merged into the password rows"
    );
    let success_fg = edpcli::tui::theme::current().success().fg.unwrap();
    let exchange_row = compact
        .iter()
        .find(|line| line.contains("交换区") && line.contains("原密码") && line.contains("新密码"))
        .expect("exchange password row");
    let separator = exchange_row.find('│').expect("password column separator");
    let verified = exchange_row.find('✓').expect("source verification status");
    let target = exchange_row.find("新密码").expect("target password column");
    assert!(
        verified < separator && separator < target,
        "password row must be 原密码 + 状态 │ 新密码: {exchange_row}"
    );
    let exchange_y = compact
        .iter()
        .position(|line| line == exchange_row)
        .expect("exchange password row index") as u16;
    let separator_x = (0..70)
        .find(|x| buffer[(*x, exchange_y)].symbol() == "│")
        .expect("password column separator x");
    assert!(
        separator_x < 34,
        "password row must use compact content-driven column sizing instead of a 50/50 split: x={separator_x}, row={}",
        lines[exchange_y as usize]
    );
    for domain in ["交换区", "保密区"] {
        let y = compact
            .iter()
            .position(|line| {
                line.contains(domain) && line.contains("原密码") && line.contains("新密码")
            })
            .unwrap_or_else(|| panic!("missing verified status for {domain}"))
            as u16;
        let x = (0..width)
            .find(|x| buffer[(*x, y)].symbol() == "✓")
            .unwrap_or_else(|| panic!("verified mark missing on row: {}", lines[y as usize]));
        assert_eq!(buffer[(x, y)].fg, success_fg, "{domain}");
    }

    let value = state.provision().form.label_id.clone();
    let needle = value.chars().next().expect("label id").to_string();
    let y = compact
        .iter()
        .position(|line| line.contains("标签标识") && line.contains(&value))
        .expect("normal identity row") as u16;
    let x = (0..80)
        .find(|x| buffer[(*x, y)].symbol() == needle)
        .expect("normal field value");
    let label_x = (0..x)
        .find(|label_x| buffer[(*label_x, y)].symbol() == "标")
        .expect("identity label");
    assert_eq!(
        buffer[(x, y)].bg,
        buffer[(label_x, y)].bg,
        "normal value must share the pane background instead of painting a separate input strip"
    );
    assert_eq!(
        buffer[(x, y)].fg,
        edpcli::tui::theme::current().secondary_text().fg.unwrap(),
        "normal value should use lightweight body text styling"
    );
}

#[test]
fn provision_form_capacity_indicator_tracks_boot_share_encrypt_without_vertical_jitter() {
    let mut state = provision_state();
    let field_index = |state: &AppState, prefix: &str| {
        state
            .provision_visible_fields()
            .iter()
            .position(|(label, _, _)| label.starts_with(prefix))
            .unwrap_or_else(|| panic!("missing field prefix {prefix}"))
    };

    let identity = field_index(&state, "标签标识");
    state.provision_mut().field_selected = identity;
    let identity_lines = rendered_lines(&state, 160, 45);
    assert!(
        !identity_lines.iter().any(|line| line.contains('▲')),
        "identity fields must hide the capacity-map indicator"
    );
    let identity_header_y = identity_lines
        .iter()
        .position(|line| {
            let compact = line.replace(' ', "");
            compact.contains("区域") && compact.contains("LBA范围")
        })
        .expect("layout table header");

    let mut arrow_x = Vec::new();
    for (prefix, region) in [
        ("启动区容量", "启动区"),
        ("交换区容量", "交换区"),
        ("保密区容量", "保密区"),
    ] {
        let index = field_index(&state, prefix);
        state.provision_mut().field_selected = index;
        let lines = rendered_lines(&state, 160, 45);
        let (y, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains('▲'))
            .unwrap_or_else(|| panic!("missing capacity indicator for {prefix}"));
        let x = line
            .chars()
            .position(|symbol| symbol == '▲')
            .expect("capacity indicator x");
        arrow_x.push(x);
        let header_y = lines
            .iter()
            .position(|line| {
                let compact = line.replace(' ', "");
                compact.contains("区域") && compact.contains("LBA范围")
            })
            .expect("layout table header");
        assert_eq!(
            header_y, identity_header_y,
            "indicator row must keep the layout table at a stable y position"
        );
        assert!(
            y < header_y,
            "capacity indicator must stay directly below the map and above the region table"
        );
        let linked_row = lines
            .iter()
            .find(|line| {
                let right = line.chars().skip(70).collect::<String>();
                let compact = right.replace(' ', "");
                compact.contains(region) && compact.contains("LBA")
            })
            .unwrap_or_else(|| panic!("missing linked region row for {region}"));
        assert!(
            linked_row.chars().skip(70).any(|symbol| symbol == '▌'),
            "left field focus must visibly activate the matching right-side region row: {linked_row}"
        );
    }
    assert!(
        arrow_x[0] < arrow_x[1] && arrow_x[1] < arrow_x[2],
        "indicator must move with the real map segments: {arrow_x:?}"
    );
}

#[test]
fn provision_default_source_password_is_visible_while_verifying_and_separator_never_moves() {
    use edpcli::provision::SourcePasswordKnowledge;

    let mut state = provision_state();
    assert_eq!(state.provision().form.share_source_password, "0000aaaa");
    assert_eq!(state.provision().form.encrypt_source_password, "0000aaaa");

    let render_separator = |state: &AppState| {
        let (width, height) = (160, 45);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, state)).unwrap();
        let buffer = terminal.backend().buffer();
        let row_y = (0..height)
            .find(|y| {
                let line = (0..width)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .replace(' ', "");
                line.contains("交换区") && line.contains("原密码") && line.contains("新密码")
            })
            .expect("exchange password row");
        let row = (0..width)
            .map(|x| buffer[(x, row_y)].symbol())
            .collect::<String>();
        let separator = (0..width)
            .find(|x| buffer[(*x, row_y)].symbol() == "│")
            .expect("password separator");
        (separator, row)
    };

    let (before, initial_row) = render_separator(&state);
    assert!(
        ["◐", "◓", "◑", "◒"]
            .iter()
            .any(|glyph| initial_row.contains(glyph)),
        "initial source-password candidate must show the shared verification spinner: {initial_row}"
    );

    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: edpcli::provision::DiskProvisionKind::Mode0,
        share: Some(SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));
    let (after, verified_row) = render_separator(&state);
    assert!(verified_row.contains('✓'), "{verified_row}");
    assert_eq!(
        before, after,
        "spinner → verified transition must never move the password-column separator"
    );
}

#[test]
fn provision_source_password_edit_auto_verify_contract_is_revision_safe() {
    use edpcli::provision::{KeyDomainRole, SourcePasswordKnowledge};

    let mut state = provision_state();
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

    state.provision_mut().field_selected = source_index;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_push_char('A');
    assert!(
        state.provision_end_insert(),
        "dirty source password must request automatic verification on Insert exit"
    );
    let (domain, _, revision_a) = state
        .provision_source_password_verify_request()
        .expect("verification request")
        .expect("source password request");
    assert_eq!(domain, KeyDomainRole::Share);

    let spinner_a = rendered_lines(&state, 160, 45)
        .into_iter()
        .find(|line| {
            let compact = line.replace(' ', "");
            compact.contains("交换区") && compact.contains("原密码")
        })
        .expect("share password row");
    assert!(
        ["◐", "◓", "◑", "◒"]
            .iter()
            .any(|glyph| spinner_a.contains(glyph)),
        "verifying state must use the shared spinner: {spinner_a}"
    );
    state.advance_animation();
    state.advance_animation();
    let spinner_b = rendered_lines(&state, 160, 45)
        .into_iter()
        .find(|line| {
            let compact = line.replace(' ', "");
            compact.contains("交换区") && compact.contains("原密码")
        })
        .expect("share password row after animation");
    assert_ne!(spinner_a, spinner_b, "spinner frame must visibly advance");

    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_push_char('B');
    assert!(state.provision_end_insert());
    let (_, _, revision_b) = state
        .provision_source_password_verify_request()
        .expect("second verification request")
        .expect("second source password request");
    assert!(revision_b > revision_a);

    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision_a,
        Ok(SourcePasswordKnowledge::UserVerified),
    );
    assert_eq!(
        state.provision().form.share_source_knowledge,
        SourcePasswordKnowledge::Unknown,
        "stale verification result must not overwrite the newer password"
    );
    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision_b,
        Ok(SourcePasswordKnowledge::UserVerified),
    );
    assert_eq!(
        state.provision().form.share_source_knowledge,
        SourcePasswordKnowledge::UserVerified
    );

    state.provision_mut().field_selected = target_index;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_push_char('C');
    assert!(
        !state.provision_end_insert(),
        "editing the new password must not verify the source password"
    );
}

#[test]
fn provision_source_password_failure_uses_red_cross_and_keeps_opaque_passthrough() {
    use edpcli::provision::KeyDomainRole;
    use edpcli::sectors::EdpfPartition;

    let mut row = device();
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
    crate::common::confirm_row_identity(&mut row);
    let mut state = AppState::new();
    state.replace_devices(vec![row]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    state.provision_begin_selected();
    state.provision_enter_form_workspace();
    state.provision_finish_key_probe(Ok(edpcli::application::provision::ProvisionKeyProbe {
        source_kind: edpcli::provision::DiskProvisionKind::Mode0,
        share: Some(edpcli::provision::SourcePasswordKnowledge::DefaultVerified),
        share_opaque_profile: true,
        encrypt: Some(edpcli::provision::SourcePasswordKnowledge::DefaultVerified),
        encrypt_opaque_profile: true,
    }));
    let source_index = state
        .provision_visible_fields()
        .iter()
        .position(|(label, _, _)| label == "原密码")
        .expect("share source password");
    state.provision_mut().field_selected = source_index;
    assert!(state.provision_begin_insert());
    state.provision_cursor_end();
    state.provision_push_char('X');
    assert!(state.provision_end_insert());
    let (_, _, revision) = state
        .provision_source_password_verify_request()
        .expect("verification request")
        .expect("source password request");
    state.provision_finish_source_password_verify(
        KeyDomainRole::Share,
        revision,
        Err("来源密码验证失败".into()),
    );
    assert!(
        !state.provision().form.format_share,
        "failed source-password verification must never auto-enable destructive formatting"
    );
    let request = state.provision_request().expect(
        "failed verification with unchanged target password should still allow opaque passthrough",
    );
    assert!(!request.format.share);
    assert_eq!(
        request
            .key_domains
            .target_password(edpcli::provision::PartitionRole::Share),
        None,
        "opaque passthrough must not reinterpret the default target candidate as a password-change request"
    );

    let (width, height) = (160, 45);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let row_y = (0..height)
        .find(|y| {
            let line = (0..width)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>();
            line.replace(' ', "").contains("交换区") && line.contains('✗')
        })
        .expect("failed share password row");
    let cross_x = (0..width)
        .find(|x| buffer[(*x, row_y)].symbol() == "✗")
        .expect("failed verification cross");
    assert_eq!(
        buffer[(cross_x, row_y)].fg,
        edpcli::tui::theme::current().danger().fg.unwrap()
    );
    let failed_row = (0..width)
        .map(|x| buffer[(x, row_y)].symbol())
        .collect::<String>()
        .replace(' ', "");
    let separator = failed_row.find('│').expect("password column separator");
    let failed = failed_row.find('✗').expect("failed source status");
    let target = failed_row.find("新密码").expect("target password column");
    assert!(
        failed < separator && separator < target,
        "failure status must stay beside 原密码 and must not replace/disable 新密码: {failed_row}"
    );
    let right_passthrough = (0..height).any(|y| {
        let right = (70..width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .replace(' ', "");
        right.contains("交换区") && right.contains("透传")
    });
    assert!(
        right_passthrough,
        "failed source-password verification with unchanged target password must keep an exact-compatible domain on opaque passthrough"
    );

    let provision_controller = include_str!("../src/tui/controller/provision.rs");
    assert!(!provision_controller.contains("ViewOrVerify"));
    assert!(!provision_controller.contains("ProvisionSourcePasswordVerify"));
}

#[test]
fn provision_breadcrumb_tracks_page_surface_not_overlay_stage() {
    use edpcli::tui::state::ProvisionStage;

    let mut state = provision_state();
    let form = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    assert!(form.contains("设备/disk6>制盘/mode0>制盘配置"), "{form}");
    assert!(form.contains("Esc返回：设备列表"), "{form}");

    state.provision_mut().stage = ProvisionStage::Planning;
    let planning = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    assert!(
        planning.contains("设备/disk6>制盘/mode0>制盘配置"),
        "{planning}"
    );
    assert!(planning.contains("2生成计划"), "{planning}");

    state.provision_mut().stage = ProvisionStage::Review;
    state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_review();
    let review = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    assert!(
        review.contains("设备/disk6>制盘/mode0>计划确认"),
        "{review}"
    );
    assert!(review.contains("Esc返回：制盘配置"), "{review}");

    state.provision_mut().stage = ProvisionStage::Confirm;
    let confirm = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    assert!(
        confirm.contains("设备/disk6>制盘/mode0>计划确认"),
        "{confirm}"
    );
    assert!(confirm.contains("Esc返回：制盘配置"), "{confirm}");
}

#[test]
fn provision_planning_modal_is_centered_on_the_full_terminal_viewport() {
    use edpcli::tui::state::ProvisionStage;
    use ratatui::layout::Rect;

    for (width, height) in [(160, 45), (120, 36), (60, 18)] {
        let mut state = provision_state();
        state.provision_mut().stage = ProvisionStage::Planning;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();

        let expected = edpcli::tui::ui::centered_modal_rect(Rect::new(0, 0, width, height), 36, 7);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(expected.x, expected.y)].symbol(),
            "┌",
            "{width}x{height}: planning modal must use the full viewport center"
        );
        assert_eq!(
            buffer[(
                expected.x + expected.width - 1,
                expected.y + expected.height - 1,
            )]
                .symbol(),
            "┘",
            "{width}x{height}: planning modal bottom-right corner mismatch"
        );
        let text = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replace(' ', "");
        assert!(text.contains("正在生成制盘计划"), "{text}");
        assert!(
            !text.contains("正在只读检查目标并生成精确制盘计划"),
            "{text}"
        );
        assert!(!text.contains("只读规划"), "{text}");
        assert!(
            ["◐", "◓", "◑", "◒"]
                .iter()
                .any(|glyph| text.contains(glyph)),
            "planning modal must render the shared spinner: {text}"
        );
    }
}

#[test]
fn provision_write_confirmation_is_centered_on_the_full_terminal_viewport() {
    use edpcli::tui::state::ProvisionStage;
    use ratatui::layout::Rect;

    for (width, height) in [(160, 45), (120, 36), (60, 18)] {
        let mut state = provision_state();
        state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_review();
        state.provision_mut().stage = ProvisionStage::Confirm;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();

        // This fixture intentionally has no prepared plan, so the fail-closed
        // confirmation renders two detail lines: target identity + YES semantics.
        let expected = edpcli::tui::ui::centered_modal_rect(Rect::new(0, 0, width, height), 82, 12);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(expected.x, expected.y)].symbol(),
            "┌",
            "{width}x{height}: write confirmation must use the full viewport center"
        );
        assert_eq!(
            buffer[(
                expected.x + expected.width - 1,
                expected.y + expected.height - 1,
            )]
                .symbol(),
            "┘",
            "{width}x{height}: write confirmation bottom-right corner mismatch"
        );
    }
}

#[test]
fn shared_action_confirmation_is_centered_on_the_full_terminal_viewport() {
    use edpcli::tui::ui::{
        render_action_confirmation_modal, ActionConfirmationSpec, ConfirmationTone,
    };
    use ratatui::{layout::Rect, text::Line};

    for (width, height) in [(160, 45), (120, 36), (60, 18)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                render_action_confirmation_modal(
                    frame,
                    ActionConfirmationSpec {
                        title: "确认",
                        headline: "执行操作？",
                        details: vec![Line::from("详情")],
                        tone: ConfirmationTone::Warning,
                    },
                )
            })
            .unwrap();
        let expected = edpcli::tui::ui::centered_modal_rect(Rect::new(0, 0, width, height), 76, 9);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(expected.x, expected.y)].symbol(), "┌");
        assert_eq!(
            buffer[(
                expected.x + expected.width - 1,
                expected.y + expected.height - 1
            )]
                .symbol(),
            "┘"
        );
    }
}

#[test]
fn ch16_provision_running_separates_progress_phase_step_log_and_safety() {
    use edpcli::application::progress::{
        OverallProgress, Phase, ProgressEvent, Step, TransactionActivityPhase, Unit, WorkProgress,
    };
    use edpcli::tui::state::ProvisionStage;
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Running;
    state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_running();
    let now = std::time::Instant::now();
    let mut run = edpcli::application::progress::OperationRunState::new(
        edpcli::application::progress::OperationKind::Provision,
        "disk6",
    );
    run.started_at = now;
    run.last_activity_at = now;
    state.provision_mut().run = Some(run);
    let event = ProgressEvent::new(Phase::Transaction, Step::ProtocolReadback, 2, 7)
        .with_overall(OverallProgress::from_basis_points(7_000))
        .with_work(WorkProgress {
            current: 75,
            total: 100,
            unit: Unit::Sectors,
            activity: Some(TransactionActivityPhase::FormatWrite),
        });
    state.provision_push_progress(event);
    let lines = rendered_lines(&state, 160, 45);
    let text = lines.join("\n").replace(' ', "");
    for value in [
        "总体进度",
        "70%",
        "当前任务",
        "阶段",
        "2/7",
        "事务写入",
        "当前步骤",
        "协议读回校验",
        "工作进度",
        "75/100sector·75%",
        "动态描述",
        "正在写入文件系统结构",
        "最近活动",
        "运行记录",
        "安全提示",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.replace(' ', "").contains("安全提示"))
            .count(),
        1,
        "Operation Running must expose exactly one safety footer"
    );
    assert!(!text.contains("扇区活动"), "{text}");
    assert!(!text.contains("安全写盘事务执行中"), "{text}");
    assert!(!text.contains("就绪"), "{text}");
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
fn operation_log_table_aligns_milestones_and_animates_current_snapshot() {
    use edpcli::application::progress::{
        LogPolicy, OperationKind, OperationRunState, Phase, ProgressEvent, Severity, Step,
        TransactionActivityPhase, Unit, WorkProgress,
    };
    use edpcli::provision::PartitionRole;
    use edpcli::tui::state::ProvisionStage;
    use std::time::Duration;

    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Running;
    state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_running();
    let now = std::time::Instant::now();
    let mut run = OperationRunState::new(OperationKind::Provision, "disk6");
    run.started_at = now;

    for (offset, phase, step, current, total, severity) in [
        (
            0,
            Phase::Backup,
            Step::MandatoryBackup,
            1,
            8,
            Severity::Info,
        ),
        (
            1,
            Phase::Identity,
            Step::BackupVerification,
            2,
            8,
            Severity::Info,
        ),
        (
            2,
            Phase::Transaction,
            Step::ProtocolWrite,
            3,
            8,
            Severity::Warning,
        ),
        (
            3,
            Phase::Readback,
            Step::ProtocolReadback,
            4,
            8,
            Severity::Error,
        ),
    ] {
        let mut event = ProgressEvent::new(phase, step, current, total);
        event.severity = severity;
        event.emitted_at = now + Duration::from_secs(offset);
        event.log_policy = LogPolicy::Append;
        run.push(event);
    }

    let mut current = ProgressEvent::new(
        Phase::Format,
        Step::PartitionFormat(PartitionRole::Boot),
        5,
        8,
    )
    .with_work(WorkProgress {
        current: 128,
        total: 512,
        unit: Unit::Sectors,
        activity: Some(TransactionActivityPhase::FormatWrite),
    });
    current.log_policy = LogPolicy::SnapshotOnly;
    current.emitted_at = now + Duration::from_secs(4);
    run.push(current);
    assert_eq!(
        run.log.len(),
        4,
        "current snapshot must not become a history row"
    );
    state.provision_mut().run = Some(run);

    let (width, height) = (160, 45);
    let render_frame = |state: &edpcli::tui::state::AppState| {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, state)).unwrap();
        let buffer = terminal.backend().buffer();
        let lines = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        (terminal, lines)
    };

    let (terminal, lines) = render_frame(&state);
    let buffer = terminal.backend().buffer();
    let compact_lines = lines
        .iter()
        .map(|line| line.replace(' ', ""))
        .collect::<Vec<_>>();
    let header_y = compact_lines
        .iter()
        .position(|line| line.contains("时间") && line.contains("状态") && line.contains("阶段"))
        .expect("operation log header") as u16;
    let row_for = |needle: &str| {
        compact_lines
            .iter()
            .enumerate()
            .skip(header_y as usize + 1)
            .find_map(|(y, line)| line.contains(needle).then_some(y as u16))
            .unwrap_or_else(|| panic!("missing log row {needle}"))
    };
    let rows = [
        row_for("制盘前元数据备份"),
        row_for("备份身份校验"),
        row_for("协议事务写盘"),
        row_for("协议读回校验"),
        row_for("启动区格式化与读回"),
    ];
    let symbols = ["✓", "✓", "!", "✗", "◐"];
    let mut status_x = None;
    for (row, symbol) in rows.into_iter().zip(symbols) {
        let x = (0..width)
            .find(|x| buffer[(*x, row)].symbol() == symbol)
            .unwrap_or_else(|| panic!("missing status {symbol} on row {row}"));
        if let Some(expected) = status_x {
            assert_eq!(x, expected, "status column must stay aligned");
        } else {
            status_x = Some(x);
        }
    }

    let x = status_x.unwrap();
    let theme = edpcli::tui::theme::current();
    assert_eq!(buffer[(x, rows[0])].fg, theme.success().fg.unwrap());
    assert_eq!(buffer[(x, rows[1])].fg, theme.success().fg.unwrap());
    assert_eq!(buffer[(x, rows[2])].fg, theme.warning().fg.unwrap());
    assert_eq!(buffer[(x, rows[3])].fg, theme.danger().fg.unwrap());
    assert_eq!(buffer[(x, rows[4])].fg, theme.accent().fg.unwrap());

    let header_status_x = (0..width)
        .find(|x| buffer[(*x, header_y)].symbol() == "状")
        .expect("status header");
    assert_eq!(
        x, header_status_x,
        "header and status cells must share one column"
    );

    state.advance_animation();
    state.advance_animation();
    let (terminal, lines) = render_frame(&state);
    let buffer = terminal.backend().buffer();
    let spinner_row = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| {
            line.replace(' ', "")
                .contains("启动区格式化与读回")
                .then_some(y as u16)
        })
        .expect("animated current row");
    assert_eq!(buffer[(x, spinner_row)].symbol(), "◓");
}

#[test]
fn operation_log_started_event_spins_until_a_later_milestone_arrives() {
    use edpcli::application::progress::{
        OperationKind, OperationRunState, Phase, ProgressEvent, Step,
    };
    use edpcli::tui::state::ProvisionStage;

    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Running;
    state.provision_mut().pane_focus = edpcli::tui::pane::PaneFocus::provision_running();
    let mut run = OperationRunState::new(OperationKind::Provision, "disk6");
    run.push(ProgressEvent::started(
        OperationKind::Provision,
        Phase::Backup,
        Step::MandatoryBackup,
        "正在创建制盘前元数据备份",
    ));
    state.provision_mut().run = Some(run);

    let screen = rendered_lines(&state, 120, 36)
        .join(
            "
",
        )
        .replace(' ', "");
    assert!(screen.contains("◐"), "{screen}");
    assert!(screen.contains("制盘前元数据备份"), "{screen}");

    let mut next = ProgressEvent::new(Phase::Identity, Step::BackupVerification, 2, 8);
    next.detail = Some("备份身份校验完成".into());
    state.provision_push_progress(next);
    let screen = rendered_lines(&state, 120, 36)
        .join(
            "
",
        )
        .replace(' ', "");
    assert!(screen.contains("✓"), "{screen}");
    assert!(screen.contains("制盘前元数据备份"), "{screen}");
}

#[test]
fn ch16_provision_result_uses_workbench_hero_and_panes() {
    use edpcli::application::provision::ProvisionExecutionStatus as Status;
    use edpcli::tui::state::ProvisionStage;
    let mut state = provision_state();
    state.provision_mut().stage = ProvisionStage::Result;
    for (status, label) in [
        (Status::Success, "✓制盘成功"),
        (Status::CompletedWithWarnings, "⚠制盘完成·存在警告"),
        (Status::PartialFormatFailure, "⚠制盘完成·部分格式化失败"),
        (Status::FatalFailure, "✗制盘失败"),
    ] {
        state.provision_mut().result_status = Some(status);
        let text = rendered_lines(&state, 140, 40).join("\n").replace(' ', "");
        assert!(text.contains(label), "missing {label}");
        for required in ["制盘结果", "分区结果", "全盘布局", "验收与执行"] {
            assert!(text.contains(required), "missing {required}");
        }
        assert!(text.contains("Esc/Enter返回设备列表"));
        assert!(!text.contains("最近进度事件"));
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
    let device = navigation.find("设备").expect("missing 设备 tab");
    let backups = navigation.find("备份").expect("missing 备份 tab");
    assert!(device < backups);
    assert!(!navigation.contains("检查"));
    assert!(!navigation.contains("制盘"));
    assert!(!navigation.contains("Inspect"));
}

#[test]
fn ch16_inspect_is_nested_and_top_level_switching_cannot_leave_it() {
    use edpcli::tui::state::{NavCommand, Workspace};

    let mut state = lba8_state();
    assert_eq!(
        Workspace::TOP_LEVEL,
        [Workspace::Devices, Workspace::Backups]
    );
    assert_eq!(
        Workspace::ALL,
        [
            Workspace::Devices,
            Workspace::Inspect,
            Workspace::Backups,
            Workspace::Provision
        ]
    );
    assert_eq!(state.workspace(), Workspace::Inspect);
    let lines = rendered_lines(&state, 120, 36).join("\n").replace(' ', "");
    assert!(lines.contains("检查") || lines.contains("Inspect"));
    state.navigate(NavCommand::NextWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Inspect);
    assert!(state.advanced_inspect().is_some());
    state.navigate(NavCommand::PreviousWorkspace, 20);
    assert_eq!(state.workspace(), Workspace::Inspect);
    assert!(state.advanced_inspect().is_some());
    state.navigate(NavCommand::Escape, 20);
    assert_eq!(state.workspace(), Workspace::Devices);
}

#[test]
fn ch16_devices_and_backups_have_independent_pane_focus_and_viewports() {
    use edpcli::tui::pane::PaneId;

    let mut state = AppState::new();
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
    state.focus_devices_pane(PaneId::DevicesTree);
    state.pane_viewport_mut(PaneId::DevicesTree).scroll_y.offset = 7;
    state.focus_backups_pane(PaneId::BackupCoverage);
    state.pane_viewport_mut(PaneId::BackupCoverage).scroll_x = 3;
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesTree);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupCoverage);
    assert_eq!(state.pane_viewport(PaneId::DevicesTree).scroll_y.offset, 7);
    assert_eq!(state.pane_viewport(PaneId::BackupCoverage).scroll_x, 3);
}

#[test]
fn ch16_devices_wide_is_list_plus_tree_plus_detail_without_redundant_current_device_banner() {
    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    let text = rendered_lines(&state, 160, 45).join("\n").replace(' ', "");
    for value in [
        "设备列表",
        "设备信息",
        "容量布局",
        "身份与协议",
        "状态与备份",
        "无法建立可靠容量布局",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    assert!(!text.contains("当前设备·disk6"));
    assert!(!text.contains("EDPCORE·LIVE"));
}

#[test]
fn ch16_devices_compact_enter_opens_tree_then_detail_and_escape_walks_back() {
    use edpcli::tui::{pane::PaneId, state::NavCommand};

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    assert_eq!(state.activate_device_for_viewport(40).unwrap(), None);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesTree);
    let text = rendered_lines(&state, 40, 10).join("\n").replace(' ', "");
    for value in ["设备信息", "身份与协议", "容量布局"] {
        assert!(text.contains(value), "missing {value} at 40x10");
    }
    assert!(!text.contains("当前设备·disk6"));
    state.device_info_focus_detail();
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesDetail);
    state.navigate(NavCommand::Escape, 7);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesTree);
    state.navigate(NavCommand::Escape, 7);
    assert_eq!(state.devices_focused_pane(), PaneId::DevicesList);
}

#[test]
fn ch16_device_detail_pane_remains_reachable_at_standard_width() {
    use edpcli::tui::{pane::PaneId, state::NavCommand};

    let mut state = AppState::new();
    state.replace_devices(vec![device()]);
    state.focus_devices_pane(PaneId::DevicesDetail);
    let default_text = rendered_lines(&state, 100, 30).join("\n").replace(' ', "");
    assert!(default_text.contains("容量布局"));
    assert!(default_text.contains("无法建立可靠容量布局"));

    state.focus_devices_pane(PaneId::DevicesTree);
    state.navigate(NavCommand::Bottom, 10);
    state.device_info_move_tree(-1);
    state.focus_devices_pane(PaneId::DevicesDetail);
    let identity_text = rendered_lines(&state, 100, 30).join("\n").replace(' ', "");
    assert!(identity_text.contains("身份与协议"));
    assert!(identity_text.contains("device_id"));
    assert!(!identity_text.contains("总设备1"));
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
    assert!(text.contains("对象摘要"));
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
        "字段/证据",
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
            text.contains("结构树") || text.contains("LBA8"),
            "missing workspace at {width}x{height}"
        );
    }
}

#[test]
fn ch16_legacy_inspect_view_shortcuts_are_removed() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use edpcli::tui::{keymap::KeyMapper, state::InputMode};

    let mut mapper = KeyMapper::new();
    for digit in ['1', '2', '3', '4'] {
        assert_eq!(
            mapper.map(
                InputMode::Normal,
                KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE)
            ),
            None,
            "Inspect browser no longer exposes page-level numeric view tabs"
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
        card, data_table, notice_banner, panel, status_badge, BadgeTone, BannerTone,
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
                Paragraph::new("动态状态").block(panel("状态", false)),
                ratatui::layout::Rect::new(0, 5, 40, 4),
            );
        })
        .unwrap();
    let text = (0..10)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .map(|position| terminal.backend().buffer()[position].symbol().to_owned())
        .collect::<String>()
        .replace(' ', "");
    for value in ["设备", "正常", "disk4", "扫描完成", "动态状态"] {
        assert!(text.contains(value), "missing {value}");
    }
}
