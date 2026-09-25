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
fn scan_task_source_uses_typed_slots_for_background_workers() {
    let source = include_str!("../src/tui/task.rs");
    assert!(source.contains("struct TaskSlot<P>"));
    assert!(source.contains("device_slot: TaskSlot<PathBuf>"));
    assert!(source.contains("backup_slot: TaskSlot<PathBuf>"));
    assert!(source.contains("verify_slot: TaskSlot<(PathBuf, PathBuf)>"));
    assert!(!source.contains("device_single_flight:"));
    assert!(!source.contains("backup_single_flight:"));
}
