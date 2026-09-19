use edpcli::tui::task::SingleFlightGate;

#[test]
fn same_scan_kind_is_single_flight_until_completion() {
    let mut gate = SingleFlightGate::new();

    assert!(gate.try_start());
    assert!(!gate.try_start());
    assert!(gate.is_running());

    gate.finish();
    assert!(!gate.is_running());
    assert!(gate.try_start());
}

#[test]
fn scan_task_source_uses_single_flight_for_device_and_backup_workers() {
    let source = include_str!("../src/tui/task.rs");
    assert!(source.contains("device_single_flight"));
    assert!(source.contains("backup_single_flight"));
    assert!(source.contains("try_start()"));
    assert!(source.contains(".finish()"));
}
