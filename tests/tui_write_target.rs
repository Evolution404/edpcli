use std::path::PathBuf;

use edpcli::application::BackupWorkspaceItem;
use edpcli::disk_scan::Row;
use edpcli::diskio::Md5Status;
use edpcli::tui::state::{AppState, NavCommand};

fn device(disk: u32) -> Row {
    Row {
        disk,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        device_id: Some("disk&ven_test&prod_test".into()),
        onlyid: Some(format!("{disk}001")),
        dept: None,
        user: None,
        n_baks: 0,
        denied: false,
        probe_error: None,
        is_nopwd: false,
        partitions: None,
    }
}

fn backup(index: usize, name: &str) -> BackupWorkspaceItem {
    BackupWorkspaceItem {
        index,
        path: PathBuf::from(name),
        file_name: name.into(),
        display_time: "2026-09-19 06:00".into(),
        onlyid: Some("7001".into()),
        user: None,
        dept: None,
        is_nopwd: false,
        md5_status: Md5Status::Ok,
        size_ok: true,
        content_md5: Some("0123456789abcdef0123456789abcdef".into()),
    }
}

#[test]
fn switching_to_backups_pins_the_selected_device() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(6), device(7)]);
    state.navigate(NavCommand::Down, 20);
    assert_eq!(state.selected_device_disk(), Some(7));

    state.navigate(NavCommand::Right, 20);
    assert_eq!(state.selected_device_disk(), Some(7));
}

#[test]
fn selected_backup_path_is_stable_for_restore_intent() {
    let mut state = AppState::new();
    state.replace_devices(vec![device(7)]);
    state.replace_backups(vec![backup(1, "one.bin"), backup(2, "two.bin")]);
    state.navigate(NavCommand::Right, 20);
    state.navigate(NavCommand::Down, 20);

    assert_eq!(state.selected_device_disk(), Some(7));
    assert_eq!(state.selected_backup_path(), Some(PathBuf::from("two.bin")));
}
