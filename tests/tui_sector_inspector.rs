use edpcli::application::inspect::{
    AbsoluteByteRange, AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
    InspectField, InspectFieldStatus, InspectFieldType,
};
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
        fields: if lba == 0 {
            vec![InspectField {
                range: AbsoluteByteRange {
                    start: 0,
                    end_exclusive: 2,
                },
                field_type: InspectFieldType::Identity,
                raw: vec![0x12, 0x01],
                decoded: vec![0xA5, 0x01],
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
