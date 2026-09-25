mod common;

use std::path::PathBuf;

use edpcli::application::inspect::load_backup_inspect;
use edpcli::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use edpcli::edpb::{self, CoreCapture};
use edpcli::tui::{
    render,
    state::{AppState, InspectMode, NavCommand},
};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

fn netac_edpb(tag: &str) -> Option<(common::TmpDir, PathBuf)> {
    let data = common::load_disk_image("netac")?;
    let tmp = common::TmpDir::new(tag);
    let path = tmp.0.join("netac.edpb");
    edpb::write_core_backup(
        &path,
        &CoreCapture {
            snapshot_id: format!("tui-inspect-{tag}"),
            created_epoch: 1_789_000_000,
            disk_number: Some(6),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some("1402259934".into()),
            total_sectors: Some(122_880_000),
            logical_sector_size: 512,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "encrypted".into(),
            lba0_12: &data,
        },
    )
    .unwrap();
    Some((tmp, path))
}

#[test]
fn backup_inspect_reuses_domain_analyzer_for_all_metadata_lbas() {
    let Some((_tmp, path)) = netac_edpb("inspect_workspace") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    assert_eq!(workspace.items.len(), METADATA_SECTOR_COUNT);
    for (lba, item) in workspace.items.iter().enumerate() {
        assert_eq!(item.lba, lba as u64);
        assert_eq!(item.raw.len(), 512);
        assert_eq!(item.decoded.as_ref().map(Vec::len), Some(512));
        assert!(item.method.is_some());
    }
}

#[test]
fn backup_inspect_rejects_legacy_bin_images() {
    let tmp = common::TmpDir::new("inspect_reject_7168");
    let path = tmp.0.join("legacy-lba0-13.bin");
    std::fs::write(&path, vec![0u8; METADATA_IMAGE_LEN + SECTOR]).unwrap();

    let err = load_backup_inspect(&path).expect_err("legacy .bin must be rejected");
    assert!(err.contains(".edpb"), "{err}");
}

#[test]
fn inspect_overlay_has_vim_lba_navigation_and_v_view_mode_cycle() {
    let mut state = AppState::new();
    state.open_inspect(METADATA_SECTOR_COUNT);

    assert_eq!(state.inspect_selected_lba(), Some(0));
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.inspect_selected_lba(), Some(1));
    state.navigate(NavCommand::Bottom, 20);
    assert_eq!(
        state.inspect_selected_lba(),
        Some((METADATA_SECTOR_COUNT - 1) as u32)
    );

    assert_eq!(state.inspect_mode(), Some(InspectMode::Fields));
    state.inspect_cycle_mode();
    assert_eq!(state.inspect_mode(), Some(InspectMode::DecodedHex));
    state.inspect_cycle_mode();
    assert_eq!(state.inspect_mode(), Some(InspectMode::RawHex));
    state.inspect_cycle_mode();
    assert_eq!(state.inspect_mode(), Some(InspectMode::Fields));
}

fn render_inspect(state: &AppState) -> (String, String) {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, state))
        .expect("draw inspect");
    let cells = terminal.backend().buffer().content();
    let text = cells.iter().map(|cell| cell.symbol()).collect::<String>();
    let accent = edpcli::tui::theme::current().palette().accent;
    let active_tab = cells
        .iter()
        .filter(|cell| {
            cell.style().fg == Some(accent)
                && cell.style().add_modifier.contains(Modifier::UNDERLINED)
        })
        .map(|cell| cell.symbol())
        .collect::<String>();
    (text, active_tab)
}

#[test]
fn inspect_renders_all_tabs_and_moves_highlight_with_mode() {
    let Some((_tmp, path)) = netac_edpb("inspect_tabs") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    let mut state = AppState::new();
    state.replace_inspect(workspace);

    let (text, highlighted) = render_inspect(&state);
    assert!(text.contains('字') && text.contains('段'), "{text}");
    assert!(text.contains("Decoded Hex"), "{text}");
    assert!(text.contains("Raw Hex"), "{text}");
    assert!(
        highlighted.contains('字') && highlighted.contains('段'),
        "{highlighted}"
    );

    state.inspect_cycle_mode();
    let (text, highlighted) = render_inspect(&state);
    assert!(
        text.contains('字')
            && text.contains('段')
            && text.contains("Decoded Hex")
            && text.contains("Raw Hex")
    );
    assert!(highlighted.contains("Decoded Hex"), "{highlighted}");

    state.inspect_cycle_mode();
    let (text, highlighted) = render_inspect(&state);
    assert!(
        text.contains('字')
            && text.contains('段')
            && text.contains("Decoded Hex")
            && text.contains("Raw Hex")
    );
    assert!(highlighted.contains("Raw Hex"), "{highlighted}");
}

#[test]
fn inspect_overlay_escape_returns_to_workspace_instead_of_exiting() {
    let mut state = AppState::new();
    state.open_inspect(METADATA_SECTOR_COUNT);
    let effect = state.navigate(NavCommand::Escape, 20);
    assert_eq!(effect, edpcli::tui::state::StateEffect::None);
    assert!(state.inspect_selected_lba().is_none());
}
