use std::path::PathBuf;

use edpcli::application::BackupWorkspaceItem;
use edpcli::disk_scan::Row;
use edpcli::diskio::BackupIntegrityStatus;
use edpcli::tui::{
    render,
    state::{AppState, NavCommand},
};
use ratatui::{backend::TestBackend, Terminal};

fn device(disk: u32) -> Row {
    Row {
        disk,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some("disk&ven_test&prod_test".into()),
        identity_pin: None,
        onlyid: Some(format!("{disk}001")),
        dept: None,
        user: None,
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

fn backup(index: usize, name: &str) -> BackupWorkspaceItem {
    BackupWorkspaceItem {
        index,
        path: PathBuf::from(name),
        file_name: name.into(),
        display_time: "2026-09-19 06:00".into(),
        size_bytes: Some(64_000_000_000),
        vid: Some("1234".into()),
        pid: Some("5678".into()),
        device_id: Some("disk&ven_test&prod_test".into()),
        onlyid: Some("7001".into()),
        identity: None,
        user: None,
        dept: None,
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Plain,
        integrity_status: BackupIntegrityStatus::Verified,
        size_ok: true,
        content_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        ),
    }
}

#[test]
fn ch14_identity_projection_is_shared_and_unknown_kind_is_honest() {
    use edpcli::application::identity::WorkspaceIdentity;

    let device = device(6);
    let backup = backup(1, "identity.edpb");
    let device_cells = WorkspaceIdentity::from_device(&device).display_cells();
    let backup_cells = WorkspaceIdentity::from_backup(&backup).display_cells();
    assert_eq!(device_cells[0], backup_cells[0]);
    assert_eq!(device_cells[1], backup_cells[1]);
    assert_eq!(device_cells[2], backup_cells[2]);
    let mut unknown = backup;
    unknown.integrity_status = BackupIntegrityStatus::Invalid;
    unknown.size_ok = false;
    unknown.size_bytes = None;
    unknown.vid = None;
    unknown.pid = None;
    unknown.device_id = None;
    unknown.onlyid = None;
    let cells = WorkspaceIdentity::from_backup(&unknown).display_cells();
    assert_eq!(cells[0], "—");
    assert_eq!(cells[1], "—");
    assert_eq!(cells[2], "—");
    assert_eq!(cells[3], "—");
    assert_eq!(
        cells[6], "—",
        "invalid backup must not report Plain as known"
    );
}

#[test]
fn canonical_identity_projection_distinguishes_confirmed_possible_and_conflict() {
    use edpcli::application::identity::WorkspaceIdentity;
    use edpcli::application::media_identity::{
        serial_digest_evidence, MediaIdentityPin, MediaIdentitySnapshot,
    };

    let identity = |raw_serial: Option<&str>| {
        let mut snapshot = MediaIdentitySnapshot::default();
        let serial = serial_digest_evidence(raw_serial);
        snapshot.hardware.serial_sha256 = serial.sha256;
        snapshot.hardware.serial_quality = serial.quality;
        snapshot.hardware.vid = Some(0x1234);
        snapshot.hardware.pid = Some(0x5678);
        snapshot.hardware.total_sectors = Some(125_000_000);
        snapshot.hardware.logical_sector_size = Some(512);
        snapshot.hardware.vendor = Some("test".into());
        snapshot.hardware.product = Some("test".into());
        snapshot.hardware.revision = Some("1.0".into());
        snapshot
    };
    let mut target = device(6);
    target.identity_pin = Some(MediaIdentityPin::new(
        identity(Some("SERIAL-001")),
        &[0; 13 * 512],
    ));
    let mut saved = backup(1, "identity.edpb");
    saved.identity = Some(identity(Some("SERIAL-001")));
    assert!(
        WorkspaceIdentity::from_backup_against(&saved, Some(&target))
            .canonical_status()
            .contains("已确认")
    );
    saved.identity = Some(identity(Some("SERIAL-002")));
    assert!(
        WorkspaceIdentity::from_backup_against(&saved, Some(&target))
            .canonical_status()
            .contains("硬件冲突")
    );
    target.identity_pin = Some(MediaIdentityPin::new(identity(None), &[0; 13 * 512]));
    saved.identity = Some(identity(None));
    assert!(
        WorkspaceIdentity::from_backup_against(&saved, Some(&target))
            .canonical_status()
            .contains("可能相关")
    );
}

#[test]
fn ch14_identity_search_matches_both_workspaces() {
    for query in ["1234:5678", "test_test", "64.00gb", "7001"] {
        let mut devices = AppState::new();
        let mut row = device(6);
        row.onlyid = Some("7001".into());
        devices.replace_devices(vec![row]);
        devices.navigate(NavCommand::Search, 20);
        for ch in query.chars() {
            devices.push_input_char(ch);
        }
        assert_eq!(devices.item_count(), 1, "device query {query}");

        let mut backups = AppState::new();
        backups.replace_backups(vec![backup(1, "identity.edpb")]);
        backups.navigate(NavCommand::WorkspaceBackups, 20);
        backups.navigate(NavCommand::Search, 20);
        for ch in query.chars() {
            backups.push_input_char(ch);
        }
        assert_eq!(backups.item_count(), 1, "backup query {query}");
    }
}

#[test]
fn ch14_table_projection_is_stable_across_animation_and_scroll() {
    use edpcli::tui::table_layout::TableKind;

    let mut state = AppState::new();
    state.replace_devices(vec![device(6), device(7)]);
    let generation = state
        .table_view_data(TableKind::Devices)
        .unwrap()
        .generation;
    let row_count = state
        .table_view_data(TableKind::Devices)
        .unwrap()
        .rows
        .len();
    for _ in 0..10 {
        state.advance_animation();
        state.navigate(NavCommand::Down, 20);
        state.scroll_table(TableKind::Devices, false);
    }
    let view = state.table_view_data(TableKind::Devices).unwrap();
    assert_eq!(view.generation, generation);
    assert_eq!(view.rows.len(), row_count);
    state.replace_devices(vec![device(8)]);
    assert!(
        state
            .table_view_data(TableKind::Devices)
            .unwrap()
            .generation
            > generation
    );
}

#[test]
fn switching_to_backups_pins_the_selected_device() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(6), device(7)]);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(7));

    state.navigate(NavCommand::WorkspaceBackups, 20);
    assert_eq!(state.selected_device_disk(), Some(7));
}

#[test]
fn selected_backup_path_is_stable_for_restore_intent() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(7)]);
    state.replace_backups(vec![backup(1, "one.bin"), backup(2, "two.bin")]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.navigate(NavCommand::Down, 20);

    assert_eq!(state.selected_device_disk(), Some(7));
    assert_eq!(state.selected_backup_path(), Some(PathBuf::from("two.bin")));
}

#[test]
fn refresh_preserves_targets_by_stable_identity_after_reordering() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(6), device(7)]);
    state.navigate(NavCommand::Down, 20);
    state.replace_devices(vec![device(7), device(6)]);
    assert_eq!(state.selected_device_disk(), Some(7));

    state.replace_backups(vec![backup(1, "one.bin"), backup(2, "two.bin")]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.navigate(NavCommand::Down, 20);
    state.replace_backups(vec![backup(2, "two.bin"), backup(1, "one.bin")]);
    assert_eq!(state.selected_backup_path(), Some(PathBuf::from("two.bin")));
}

#[test]
fn filtered_backup_selection_maps_to_the_real_backup_for_actions() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(7)]);
    state.replace_backups(vec![backup(1, "one.bin"), backup(2, "two.bin")]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.navigate(NavCommand::Search, 20);
    for ch in "two".chars() {
        state.push_input_char(ch);
    }

    assert_eq!(state.item_count(), 1);
    assert_eq!(state.visible_backup_indices(), vec![1]);
    assert_eq!(state.selected_backup_path(), Some(PathBuf::from("two.bin")));
    let (path, _) = state
        .selected_backup_delete_target()
        .expect("filtered backup delete target");
    assert_eq!(path, PathBuf::from("two.bin"));
}

#[test]
fn filtered_backup_workspace_renders_only_matching_rows() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(7)]);
    let mut one = backup(1, "one.bin");
    one.user = Some("Alice".into());
    let mut two = backup(2, "two.bin");
    two.user = Some("Bob".into());
    state.replace_backups(vec![one, two]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state.navigate(NavCommand::Search, 20);
    for ch in "bob".chars() {
        state.push_input_char(ch);
    }

    let backend = TestBackend::new(160, 34);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, &state))
        .expect("draw filtered backups");
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Bob"), "{text}");
    assert!(!text.contains("Alice"), "{text}");
    assert!(
        text.contains("▌"),
        "focused backup row must render ▌: {text}"
    );
    assert!(text.contains("1/2"), "{text}");
}

#[test]
fn backup_multi_selection_is_path_pinned_and_reconciles_after_refresh() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(7)]);
    state.replace_backups(vec![
        backup(1, "one.bin"),
        backup(2, "two.bin"),
        backup(3, "three.bin"),
    ]);
    state.navigate(NavCommand::WorkspaceBackups, 20);

    state.toggle_selected_backup();
    state.navigate(NavCommand::Down, 20);
    state.toggle_selected_backup();
    assert_eq!(state.backup_selection_count(), 2);
    assert!(state.backup_is_selected(&PathBuf::from("one.bin")));
    assert!(state.backup_is_selected(&PathBuf::from("two.bin")));

    let targets = state
        .begin_backup_batch_delete()
        .expect("two pinned batch targets");
    assert_eq!(targets.len(), 2);
    let backend = TestBackend::new(120, 28);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, &state))
        .expect("draw batch delete planning");
    state.close_backup_batch_delete();

    state.replace_backups(vec![backup(2, "two.bin"), backup(3, "three.bin")]);
    assert_eq!(state.backup_selection_count(), 1);
    assert!(!state.backup_is_selected(&PathBuf::from("one.bin")));
    assert!(state.backup_is_selected(&PathBuf::from("two.bin")));

    let backend = TestBackend::new(160, 34);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render::draw(frame, &state))
        .expect("draw selected backup rows");
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert_eq!(state.backup_selection_count(), 1);
    assert!(text.contains('✓'), "{text}");
}
#[test]
fn switching_to_backups_pins_the_real_device_selected_through_a_filter() {
    let mut state = AppState::new();
    let mut first = device(6);
    first.user = Some("Alice".into());
    let mut second = device(7);
    second.user = Some("Bob".into());
    state.replace_devices(vec![first, second]);

    state.navigate(NavCommand::Search, 20);
    for ch in "bob".chars() {
        state.push_input_char(ch);
    }
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.selected_device_disk(), Some(7));

    state.navigate(NavCommand::WorkspaceBackups, 20);
    assert_eq!(state.selected_device_disk(), Some(7));
}
