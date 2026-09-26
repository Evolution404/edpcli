use edpcli::application::inspect::{
    AbsoluteByteRange, AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    InspectField, InspectFieldStatus, InspectFieldType,
};
use edpcli::backup_metadata::PartitionGeometry;
use edpcli::inspect::{FieldChild, FieldStyle, InspectMeta};
use edpcli::inspect_target::InspectDiskContext;
use edpcli::tui::{
    render,
    state::{AdvancedInspectSource, AppState, SectorInspectMode},
};
use ratatui::{backend::TestBackend, Terminal};

fn workspace(items: Vec<AdvancedInspectItem>) -> AdvancedInspectWorkspace {
    let context = InspectDiskContext::new(vec![0; edpcli::common::METADATA_IMAGE_LEN], None, 4_096);
    AdvancedInspectWorkspace {
        source: "test-disk".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items,
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    }
}

fn workspace_with_partition(items: Vec<AdvancedInspectItem>) -> AdvancedInspectWorkspace {
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
        partition_size: 300 * edpcli::common::SECTOR as u64,
        sector_count: 300,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    });
    AdvancedInspectWorkspace {
        source: "partitioned-test-disk".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items,
        export_dir: None,
        topology: edpcli::application::inspect_tree::build_inspect_topology(&context),
    }
}

fn item(lba: u64, decoded: bool) -> AdvancedInspectItem {
    let mut raw = (0..edpcli::common::SECTOR)
        .map(|index| index as u8)
        .collect::<Vec<_>>();
    raw[0] = 0x12;
    let mut decoded_bytes = raw.clone();
    decoded_bytes[0] = 0xA5;
    AdvancedInspectItem {
        lba,
        regions: vec!["test".into()],
        raw,
        raw_sha256: format!("raw-{lba}"),
        raw_nonzero: 510,
        decoded: decoded.then_some(decoded_bytes),
        decoded_sha256: decoded.then(|| format!("decoded-{lba}")),
        method: decoded.then(|| "test-decoder".into()),
        decode_error: (!decoded).then(|| "decoder unavailable".into()),
        parse_state: edpcli::inspect::InspectParseState::Parsed,
        diagnostics: Vec::new(),
        fields: if lba == 0 {
            vec![InspectField {
                key: edpcli::inspect::InspectFieldKey::Synthetic,
                range: AbsoluteByteRange {
                    start: 0,
                    end_exclusive: 2,
                },
                field_type: InspectFieldType::Identity,
                raw: vec![0x12, 0x01],
                decoded: vec![0xA5, 0x01],
                field_logical: None,
                transform: None,
                status: InspectFieldStatus::Known,
                label: "KnownField".into(),
                value: "typed-value".into(),
                style: FieldStyle::Identity,
                group: Some("test".into()),
                children: vec![FieldChild {
                    label: "bit-child".into(),
                    value: "1".into(),
                }],
            }]
        } else {
            Vec::new()
        },
        notes: Vec::new(),
        meta_text: None,
    }
}

fn select_protocol_lba0(state: &mut AppState) {
    let rows = state.advanced_inspect_tree_rows();
    let region = rows
        .iter()
        .position(|row| row.id.ends_with("/region.protocol"))
        .expect("protocol region");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(region as isize - current as isize);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let extent = rows
        .iter()
        .position(|row| row.id.ends_with("/region.protocol.extent"))
        .expect("protocol extent");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(extent as isize - current as isize);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let sector = rows
        .iter()
        .position(|row| row.id.ends_with("/sector.0"))
        .expect("LBA0 sector");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(sector as isize - current as isize);
}

#[test]
fn selecting_lba12_shows_canonical_fields_before_enter() {
    let mut sector = item(12, true);
    sector.fields = vec![InspectField {
        key: edpcli::inspect::InspectFieldKey::Synthetic,
        range: AbsoluteByteRange {
            start: 12 * 512 + 16,
            end_exclusive: 12 * 512 + 20,
        },
        field_type: InspectFieldType::Identity,
        raw: vec![2, 0, 0, 0],
        decoded: vec![2, 0, 0, 0],
        field_logical: None,
        transform: None,
        status: InspectFieldStatus::Known,
        label: "PartionType".into(),
        value: "type2".into(),
        style: FieldStyle::Identity,
        group: Some("Partition entries".into()),
        children: vec![FieldChild {
            label: "Start LBA".into(),
            value: "20480".into(),
        }],
    }];
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![sector])));
    select_protocol_lba0(&mut state);
    let rows = state.advanced_inspect_tree_rows();
    let lba12 = rows
        .iter()
        .position(|row| row.id.ends_with("/sector.12"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(lba12 as isize - current as isize);
    assert!(state.advanced_inspect_sector().is_none());

    let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("PartionType"), "{text}");
    assert!(text.contains("type2"), "{text}");
    assert!(text.contains("Start LBA"), "{text}");
    assert!(text.contains("20480"), "{text}");
}

#[test]
fn ch14_single_sector_tree_rows_omit_redundant_closed_range() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(!text.contains("LBA0 [0..0]"), "{text}");
    assert!(!text.contains("LBA12 [12..12]"), "{text}");
    assert!(
        text.replace(' ', "").contains("EDP主协议区[0..12]"),
        "{text}"
    );
}

#[test]
fn selecting_known_partition_sector_requests_read_only_preview() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace_with_partition(Vec::new())));
    let rows = state.advanced_inspect_tree_rows();
    let region = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0"))
        .unwrap();
    state.advanced_inspect_move_tree(region as isize);
    state.advanced_inspect_toggle_selected();
    let rows = state.advanced_inspect_tree_rows();
    let extent = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0.extent"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(extent as isize - current as isize);
    state.advanced_inspect_toggle_selected();
    let rows = state.advanced_inspect_tree_rows();
    let sector = rows
        .iter()
        .position(|row| row.id.ends_with("/sector.2048"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(sector as isize - current as isize);
    assert_eq!(state.advanced_inspect_preview_request().unwrap().1, 2_048);
    state.advanced_inspect_mark_preview_pending(2_048);
    assert!(state.advanced_inspect_preview_request().is_none());
    assert!(state.advanced_inspect_sector().is_none());
}

#[test]
fn ch14_failed_passive_preview_requires_explicit_retry_then_recovers() {
    use edpcli::tui::state::PreviewLoadState;

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace_with_partition(Vec::new())));
    let rows = state.advanced_inspect_tree_rows();
    let partition = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0"))
        .unwrap();
    state.advanced_inspect_move_tree(partition as isize);
    state.advanced_inspect_toggle_selected();
    let rows = state.advanced_inspect_tree_rows();
    let extent = rows
        .iter()
        .position(|row| row.id.ends_with("/region.partition.0.extent"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(extent as isize - current as isize);
    state.advanced_inspect_toggle_selected();
    let rows = state.advanced_inspect_tree_rows();
    let sector = rows
        .iter()
        .position(|row| row.id.ends_with("/sector.2048"))
        .unwrap();
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(sector as isize - current as isize);

    assert_eq!(state.advanced_inspect_preview_request().unwrap().1, 2_048);
    state.advanced_inspect_mark_preview_pending(2_048);
    assert_eq!(
        state.advanced_inspect_preview_state(2_048),
        PreviewLoadState::Pending { attempts: 1 }
    );
    state.advanced_inspect_sector_finish(2_048, Err("transient read".into()));
    assert!(matches!(
        state.advanced_inspect_preview_state(2_048),
        PreviewLoadState::Failed { attempts: 1, .. }
    ));
    assert!(state.advanced_inspect_preview_request().is_none());
    let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(text.contains("读取失败"), "{text}");
    assert!(text.contains("按r显式重试"), "{text}");
    assert_eq!(
        state.advanced_inspect_retry_selected_preview().unwrap().1,
        2_048
    );
    assert_eq!(
        state.advanced_inspect_preview_state(2_048),
        PreviewLoadState::Pending { attempts: 2 }
    );
    state.advanced_inspect_sector_finish(2_048, Ok(item(2_048, true)));
    assert_eq!(
        state.advanced_inspect_preview_state(2_048),
        PreviewLoadState::Ready
    );
}

#[test]
fn inspect_subworkspace_cycle_preserves_sector_cursor_and_return_target() {
    use edpcli::tui::state::AdvancedInspectPanel;

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    let breadcrumb = state.advanced_inspect_breadcrumb().unwrap();
    assert!(breadcrumb.display().contains("LBA0"));
    assert_eq!(breadcrumb.escape_hint(), "Esc 返回：设备列表");
    assert!(state.advanced_inspect_open_selected_sector().is_none());
    state.advanced_inspect_sector_set_cursor(37);
    assert_eq!(
        state.advanced_inspect_breadcrumb().unwrap().escape_hint(),
        "Esc 返回：Inspect"
    );
    for expected in [
        AdvancedInspectPanel::DiskLayout,
        AdvancedInspectPanel::Tree,
        AdvancedInspectPanel::Overview,
    ] {
        state.advanced_inspect_shift_panel(false);
        assert_eq!(state.advanced_inspect().unwrap().panel, expected);
        assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 37);
    }
    state.advanced_inspect_shift_panel(true);
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::Tree
    );
    let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(text.contains("Esc返回：Inspect"), "{text}");
    assert!(text.contains("节点概览"), "{text}");
    assert!(state.advanced_inspect_close_sector());
    assert_eq!(
        state.advanced_inspect().unwrap().panel,
        AdvancedInspectPanel::DiskLayout
    );
    assert_eq!(
        state.advanced_inspect_breadcrumb().unwrap().escape_hint(),
        "Esc 返回：设备列表"
    );
}

#[test]
fn detail_field_table_has_vertical_row_viewport_and_row_column_position() {
    use edpcli::tui::pane::PaneId;

    let mut entry = item(0, true);
    entry.fields = (0..30)
        .map(|index| InspectField {
            key: edpcli::inspect::InspectFieldKey::Synthetic,
            range: AbsoluteByteRange {
                start: index,
                end_exclusive: index + 1,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![index as u8],
            decoded: vec![index as u8],
            field_logical: None,
            transform: None,
            status: InspectFieldStatus::Known,
            label: format!("Field{index:02}"),
            value: format!("value-{index:02}"),
            style: FieldStyle::Identity,
            group: None,
            children: Vec::new(),
        })
        .collect();

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![entry])));
    select_protocol_lba0(&mut state);
    state.advanced_inspect_focus_pane(PaneId::InspectDetail);
    state
        .pane_viewport_mut(PaneId::InspectDetail)
        .scroll_y
        .offset = 20;

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "");

    assert!(text.contains("行21"), "{text}");
    assert!(text.contains("/30"), "{text}");
    assert!(text.contains("列"), "{text}");
    assert!(text.contains("Field20"), "{text}");
    assert!(!text.contains("Field00"), "{text}");
}

#[test]
fn escape_pops_sector_then_inspect_without_exiting_app() {
    use edpcli::tui::state::{NavCommand, StateEffect};
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    state.advanced_inspect_open_selected_sector();
    assert_eq!(state.navigation().depth(), 2);
    assert_eq!(state.navigate(NavCommand::Escape, 24), StateEffect::None);
    assert!(state.advanced_inspect_sector().is_none());
    assert_eq!(state.navigation().depth(), 1);
    assert_eq!(state.navigate(NavCommand::Escape, 24), StateEffect::None);
    assert!(state.advanced_inspect().is_none());
    assert_eq!(state.navigation().depth(), 0);
    assert_eq!(
        state.navigate(NavCommand::Quit, 24),
        StateEffect::ExitRequested
    );
}

#[test]
fn inspect_browser_renders_across_required_terminal_sizes() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    for (width, height) in [(40, 10), (60, 18), (80, 24), (120, 36), (240, 60)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render::draw(frame, &state))
            .unwrap_or_else(|error| panic!("{width}x{height}: {error}"));
        state.advanced_inspect_shift_panel(false);
        terminal
            .draw(|frame| render::draw(frame, &state))
            .unwrap_or_else(|error| panic!("{width}x{height}: {error}"));
        state.advanced_inspect_shift_panel(false);
        terminal
            .draw(|frame| render::draw(frame, &state))
            .unwrap_or_else(|error| panic!("{width}x{height}: {error}"));
        state.advanced_inspect_shift_panel(false);
    }
}

#[test]
fn sector_decode_waits_for_inflight_preview_then_retries() {
    let mut metadata = item(0, true);
    metadata.decoded = None;
    metadata.decoded_sha256 = None;
    metadata.decode_error = None;
    metadata.meta_text = Some("canonical meta".into());
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![metadata])));
    select_protocol_lba0(&mut state);
    assert_eq!(state.advanced_inspect_open_selected_sector().unwrap().1, 0);
    state.advanced_inspect_mark_decode_pending(0, false);
    assert_eq!(state.advanced_inspect_decode_request().unwrap().1, 0);
    state.advanced_inspect_mark_decode_pending(0, true);
    assert!(state.advanced_inspect_decode_request().is_none());
}

#[test]
fn sector_inspector_loads_on_demand_navigates_bytes_and_bounds_cache() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(Vec::new())));
    select_protocol_lba0(&mut state);

    let request = state
        .advanced_inspect_open_selected_sector()
        .expect("LBA0 should need an on-demand read");
    assert_eq!(request.1, 0);
    assert!(state.advanced_inspect_sector().unwrap().pending);

    state.advanced_inspect_sector_finish(0, Ok(item(0, true)));
    assert!(!state.advanced_inspect_sector().unwrap().pending);
    assert_eq!(state.advanced_inspect_sector_item().unwrap().lba, 0);

    state.advanced_inspect_sector_move_cursor(17);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 17);
    state.advanced_inspect_sector_move_cursor(-99);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 0);
    state.advanced_inspect_sector_move_cursor(999);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 511);

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
    state.advanced_inspect_sector_toggle_field();
    assert!(state.advanced_inspect_sector().unwrap().field_expanded);

    state.advanced_inspect_sector_move_cursor(-478);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 33);
    let next = state
        .advanced_inspect_shift_sector(1)
        .expect("LBA1 should require read");
    assert_eq!(next.1, 1);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 33);
    state.advanced_inspect_sector_finish(1, Ok(item(1, true)));
    assert!(state.advanced_inspect_shift_sector(-1).is_none());
    assert_eq!(state.advanced_inspect_sector().unwrap().lba, 0);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 33);
    assert!(state.advanced_inspect_shift_sector(-1).is_none());

    for lba in 100..106 {
        state.advanced_inspect_sector_finish(lba, Ok(item(lba, false)));
    }
    let loaded = state.advanced_inspect().unwrap().result.as_ref().unwrap();
    let non_metadata = loaded
        .items
        .iter()
        .filter(|value| value.lba >= edpcli::common::METADATA_SECTOR_COUNT as u64)
        .map(|value| value.lba)
        .collect::<Vec<_>>();
    assert_eq!(non_metadata.len(), 5);
    assert!(!non_metadata.contains(&100));
    assert!(non_metadata.contains(&105));
}

#[test]
fn sector_inspector_renders_32x16_offsets_ascii_typed_and_unknown_views() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    assert!(state.advanced_inspect_open_selected_sector().is_none());
    state.advanced_inspect_sector_toggle_field();

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    let compact = text.replace(' ', "");
    assert!(compact.contains("SectorInspector"), "{text}");
    assert!(compact.contains("+0x000"), "{text}");
    assert!(compact.contains("+0x1F0"), "{text}");
    assert!(compact.contains("KnownField"), "{text}");
    assert!(compact.contains("typed-value"), "{text}");
    assert!(compact.contains("b7="), "{text}");
    assert!(compact.contains("bit-child"), "{text}");

    state.advanced_inspect_sector_move_cursor(10);
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    let compact = text.replace(' ', "");
    assert!(compact.contains("Unknownbyte"), "{text}");

    for (width, height) in [(40, 10), (80, 24), (120, 36)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    }
}

#[test]
fn field_to_hex_link_preserves_cross_sector_range_and_yank_register() {
    let mut first = item(0, true);
    let cross = InspectField {
        key: edpcli::inspect::InspectFieldKey::Synthetic,
        range: AbsoluteByteRange {
            start: 0x1f0,
            end_exclusive: edpcli::common::SECTOR as u64 + 0x30,
        },
        field_type: InspectFieldType::Identity,
        raw: (0..64).map(|value| value as u8).collect(),
        decoded: (0..64).map(|value| (value as u8) ^ 0x5a).collect(),
        field_logical: None,
        transform: None,
        status: InspectFieldStatus::Preserved,
        label: "CrossField".into(),
        value: "cross-value".into(),
        style: FieldStyle::Identity,
        group: Some("cross".into()),
        children: vec![FieldChild {
            label: "cross-child".into(),
            value: "kept".into(),
        }],
    };
    first.fields.push(cross.clone());

    let second = item(1, true);
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![first, second])));
    select_protocol_lba0(&mut state);
    state.advanced_inspect_toggle_selected();

    let rows = state.advanced_inspect_tree_rows();
    let field_index = rows
        .iter()
        .position(|row| {
            row.kind == edpcli::application::inspect_tree::InspectNodeKind::Field
                && row.label == "CrossField"
        })
        .expect("cross-sector field must be visible in the tree");
    let current = state.advanced_inspect().unwrap().tree_selected;
    state.advanced_inspect_move_tree(field_index as isize - current as isize);

    let selected = state
        .advanced_inspect_selected_field()
        .expect("tree Field must resolve to the canonical InspectField");
    assert_eq!(selected.range, cross.range);
    assert!(state.advanced_inspect_open_selected_field().is_none());
    assert_eq!(state.navigation().depth(), 2);
    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 0);
    assert_eq!(sector.cursor, 0x1f0);
    assert_eq!(
        state.advanced_inspect_sector_active_field().unwrap().range,
        cross.range
    );
    assert_eq!(
        state.advanced_inspect_sector_yank(false).as_deref(),
        Some("CrossField = cross-value")
    );
    let raw_yank = state
        .advanced_inspect_sector_yank(true)
        .expect("raw field yank");
    assert_eq!(raw_yank.split_whitespace().count(), 64);
    assert_eq!(
        state.advanced_inspect_yank_register(),
        Some(raw_yank.as_str())
    );

    assert!(state.advanced_inspect_shift_sector(1).is_none());
    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 1);
    assert_eq!(sector.cursor, 0);
    assert_eq!(
        state.advanced_inspect_sector_active_field().unwrap().range,
        cross.range
    );

    state.advanced_inspect_sector_move_cursor(0x40);
    assert!(state.advanced_inspect_sector_active_field().is_none());
    assert!(state.advanced_inspect_close_sector());
    assert_eq!(state.navigation().depth(), 1);
}

#[test]
fn field_statuses_remain_distinct_and_unknown_byte_stays_unclassified() {
    let mut entry = item(0, true);
    entry.fields = [
        (0x10, InspectFieldStatus::Known, "Known"),
        (0x20, InspectFieldStatus::Unknown, "UnknownField"),
        (0x30, InspectFieldStatus::Reserved, "Reserved"),
        (0x40, InspectFieldStatus::Preserved, "Preserved"),
    ]
    .into_iter()
    .map(|(start, status, label)| InspectField {
        key: edpcli::inspect::InspectFieldKey::Synthetic,
        range: AbsoluteByteRange {
            start,
            end_exclusive: start + 2,
        },
        field_type: InspectFieldType::Flag,
        raw: vec![start as u8, 0],
        decoded: vec![start as u8, 0],
        field_logical: None,
        transform: None,
        status,
        label: label.into(),
        value: format!("value-{start:02X}"),
        style: FieldStyle::Flag,
        group: None,
        children: Vec::new(),
    })
    .collect();

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![entry])));
    select_protocol_lba0(&mut state);
    assert!(state.advanced_inspect_open_selected_sector().is_none());

    for (offset, expected) in [
        (0x10, InspectFieldStatus::Known),
        (0x20, InspectFieldStatus::Unknown),
        (0x30, InspectFieldStatus::Reserved),
        (0x40, InspectFieldStatus::Preserved),
    ] {
        let current = state.advanced_inspect_sector().unwrap().cursor;
        state.advanced_inspect_sector_move_cursor(offset as isize - current as isize);
        assert_eq!(
            state.advanced_inspect_sector_active_field().unwrap().status,
            expected
        );
    }

    let current = state.advanced_inspect_sector().unwrap().cursor;
    state.advanced_inspect_sector_move_cursor(0x50 - current as isize);
    assert!(state.advanced_inspect_sector_active_field().is_none());

    let backend = TestBackend::new(160, 50);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.replace(' ', "").contains("Unknownbyte"), "{text}");
}

#[test]
fn jump_lba_repositions_lazy_extent_without_eager_materialization() {
    use edpcli::application::inspect_tree::InspectNodeKind;

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace_with_partition(Vec::new())));

    state.advanced_inspect_jump_lba(2_177).unwrap();

    let rows = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert_eq!(rows[selected].kind, InspectNodeKind::Sector);
    assert_eq!(rows[selected].range.start_lba, 2_177);

    let visible_partition_sectors = rows
        .iter()
        .filter(|row| {
            row.kind == InspectNodeKind::Sector && (2_048..2_348).contains(&row.range.start_lba)
        })
        .map(|row| row.range.start_lba)
        .collect::<Vec<_>>();
    assert_eq!(visible_partition_sectors.len(), 64);
    assert_eq!(visible_partition_sectors.first().copied(), Some(2_176));
    assert_eq!(visible_partition_sectors.last().copied(), Some(2_239));
    assert!(!visible_partition_sectors.contains(&2_048));
    assert!(
        rows.len() < 100,
        "jump unexpectedly materialized too much topology: {} rows",
        rows.len()
    );
    assert!(
        state
            .advanced_inspect()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .items
            .is_empty(),
        "tree jump must not read sectors"
    );
}

#[test]
fn jump_prompt_accepts_hex_lba_and_exact_absolute_byte_offset() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace_with_partition(Vec::new())));

    state.advanced_inspect_begin_jump();
    for ch in "0x881".chars() {
        state.advanced_inspect_prompt_push(ch);
    }
    assert!(state.advanced_inspect_submit_prompt().unwrap().is_none());

    let rows = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert_eq!(rows[selected].range.start_lba, 2_177);

    let absolute = 2_177 * edpcli::common::SECTOR as u64 + 123;
    state.advanced_inspect_begin_jump();
    state.advanced_inspect_toggle_jump_unit();
    for ch in format!("0x{absolute:X}").chars() {
        state.advanced_inspect_prompt_push(ch);
    }
    let request = state
        .advanced_inspect_submit_prompt()
        .unwrap()
        .expect("uncached byte-offset jump must request the target sector");
    assert_eq!(request.1, 2_177);

    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 2_177);
    assert_eq!(sector.cursor, 123);
    assert!(sector.pending);
}

#[test]
fn jump_rejects_invalid_overflow_and_out_of_range_without_clamping() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace_with_partition(Vec::new())));
    let initial = state.advanced_inspect().unwrap().tree_selected;

    for input in ["0xGG", "18446744073709551616", "10000"] {
        state.advanced_inspect_begin_jump();
        for ch in input.chars() {
            state.advanced_inspect_prompt_push(ch);
        }
        assert!(state.advanced_inspect_submit_prompt().is_err(), "{input}");
        assert_eq!(state.advanced_inspect().unwrap().tree_selected, initial);
        assert!(state.advanced_inspect_prompt().is_some());
        state.advanced_inspect_cancel_prompt();
    }

    state.advanced_inspect_begin_jump();
    state.advanced_inspect_toggle_jump_unit();
    let total_bytes = 10_000 * edpcli::common::SECTOR as u64;
    for ch in total_bytes.to_string().chars() {
        state.advanced_inspect_prompt_push(ch);
    }
    assert!(state.advanced_inspect_submit_prompt().is_err());
    assert_eq!(state.advanced_inspect().unwrap().tree_selected, initial);
    assert!(state.advanced_inspect_sector().is_none());
}

#[test]
fn structured_search_expands_field_path_and_keeps_field_to_hex_flow() {
    use edpcli::application::inspect_tree::InspectNodeKind;

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));

    state.advanced_inspect_search("KnownField").unwrap();
    let rows = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert_eq!(rows[selected].kind, InspectNodeKind::Field);
    assert_eq!(rows[selected].label, "KnownField");

    state.advanced_inspect_search("typed-value").unwrap();
    let field = state
        .advanced_inspect_selected_field()
        .expect("typed-value search must locate the canonical field");
    assert_eq!(field.label, "KnownField");
    assert_eq!(field.value, "typed-value");

    assert!(state.advanced_inspect_open_selected_field().is_none());
    let sector = state.advanced_inspect_sector().unwrap();
    assert_eq!(sector.lba, 0);
    assert_eq!(sector.cursor, 0);
    assert_eq!(
        sector
            .pinned_field
            .as_ref()
            .map(|field| field.label.as_str()),
        Some("KnownField")
    );
}

#[test]
fn jump_and_search_prompts_render_at_small_medium_and_wide_sizes() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));

    for begin_search in [false, true] {
        if begin_search {
            state.advanced_inspect_begin_search();
            for ch in "KnownField".chars() {
                state.advanced_inspect_prompt_push(ch);
            }
        } else {
            state.advanced_inspect_begin_jump();
            for ch in "0x20".chars() {
                state.advanced_inspect_prompt_push(ch);
            }
        }

        for (width, height) in [(40, 10), (80, 24), (120, 36)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render::draw(frame, &state)).unwrap();
        }
        state.advanced_inspect_cancel_prompt();
    }
}

#[test]
fn structured_search_next_and_previous_cycle_all_cached_matches() {
    let mut first = item(0, true);
    let mut second = item(1, true);
    second.fields = vec![InspectField {
        key: edpcli::inspect::InspectFieldKey::Synthetic,
        range: AbsoluteByteRange {
            start: edpcli::common::SECTOR as u64,
            end_exclusive: edpcli::common::SECTOR as u64 + 2,
        },
        field_type: InspectFieldType::Identity,
        raw: vec![0x12, 0x01],
        decoded: vec![0xA5, 0x01],
        field_logical: None,
        transform: None,
        status: InspectFieldStatus::Known,
        label: "KnownField".into(),
        value: "second-match".into(),
        style: FieldStyle::Identity,
        group: Some("test".into()),
        children: Vec::new(),
    }];
    first.fields[0].value = "first-match".into();

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![first, second])));

    state.advanced_inspect_search("KnownField").unwrap();
    assert_eq!(
        state.advanced_inspect_selected_field().unwrap().value,
        "first-match"
    );
    state.advanced_inspect_search_next(false).unwrap();
    assert_eq!(
        state.advanced_inspect_selected_field().unwrap().value,
        "second-match"
    );
    state.advanced_inspect_search_next(false).unwrap();
    assert_eq!(
        state.advanced_inspect_selected_field().unwrap().value,
        "first-match"
    );
    state.advanced_inspect_search_next(true).unwrap();
    assert_eq!(
        state.advanced_inspect_selected_field().unwrap().value,
        "second-match"
    );
}

#[test]
fn sector_inspector_supports_row_sector_and_half_page_vim_navigation() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    assert!(state.advanced_inspect_open_selected_sector().is_none());

    state.advanced_inspect_sector_set_cursor(37);
    state.advanced_inspect_sector_row_start();
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 32);
    state.advanced_inspect_sector_row_end();
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 47);
    state.advanced_inspect_sector_top();
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 0);
    state.advanced_inspect_sector_half_page(false);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 128);
    state.advanced_inspect_sector_half_page(true);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 0);
    let lba = state.advanced_inspect_sector().unwrap().lba;
    state.advanced_inspect_sector_page(false);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 256);
    assert_eq!(state.advanced_inspect_sector().unwrap().lba, lba);
    state.advanced_inspect_sector_page(true);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 0);
    assert_eq!(state.advanced_inspect_sector().unwrap().lba, lba);
    state.advanced_inspect_sector_bottom();
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 511);

    state.advanced_inspect_sector_toggle_field();
    assert!(state.advanced_inspect_sector().unwrap().field_expanded);
    state.advanced_inspect_begin_search();
    assert!(state.advanced_inspect_sector().is_none());
    assert!(state.advanced_inspect_prompt().is_some());
}

#[test]
fn narrow_sector_inspector_keeps_selected_byte_visible_without_mutating_cursor() {
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(6)));
    state.advanced_inspect_finish(Ok(workspace(vec![item(0, true)])));
    select_protocol_lba0(&mut state);
    assert!(state.advanced_inspect_open_selected_sector().is_none());

    state.advanced_inspect_sector_move_cursor(511);
    assert_eq!(state.advanced_inspect_sector().unwrap().cursor, 511);

    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    assert!(
        text.contains("+0x1F0"),
        "narrow Sector Inspector must keep the selected byte row visible: {text}"
    );
    assert_eq!(
        state.advanced_inspect_sector().unwrap().cursor,
        511,
        "responsive rendering must not rewrite byte selection"
    );
}
