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

#[test]
#[ignore = "Q0 red contract: enable after Q6 ordered progress transport"]
fn ch14_q0_progress_transport_preserves_event_batches() {
    let source = include_str!("../src/tui/task.rs");
    assert!(!source.contains("write_progress: Option<"));
    assert!(!source.contains("provision_progress: Option<"));
    assert!(source.contains("write_progress: Vec<"));
    assert!(source.contains("provision_progress: Vec<"));
}

#[test]
fn ch14_q0_failed_passive_preview_has_explicit_retry_state() {
    let source = include_str!("../src/tui/inspect/state.rs");
    assert!(!source.contains("preview_attempted: std::collections::BTreeSet"));
    assert!(source.contains("PreviewLoadState"));
}

#[test]
fn ch14_q0_device_and_backup_tables_use_shared_column_schema() {
    let source = include_str!("../src/tui/table_layout.rs");
    assert!(source.contains("TableColumnSpec"));
    assert!(source.contains("ColumnId"));
}

#[test]
#[ignore = "Q0 red contract: enable after Q4 typed layout and review presentation"]
fn ch14_q0_renderers_do_not_infer_business_state_from_display_text() {
    let layout = include_str!("../src/tui/disk_layout.rs");
    let provision = include_str!("../src/tui/provision/render.rs");
    assert!(!layout.contains("node.label.starts_with"));
    assert!(!layout.contains("node.label.ends_with"));
    assert!(!provision.contains("line.starts_with('✓')"));
    assert!(!provision.contains("line.starts_with('⚠')"));
    assert!(!provision.contains("contains(\"格式化：✗\")"));
}

#[test]
fn ch14_q0_inspect_fields_have_stable_keys_and_typed_transform() {
    let inspect = include_str!("../src/application/inspect.rs");
    assert!(inspect.contains("InspectFieldKey"));
    assert!(inspect.contains("FieldTransform"));
    assert!(inspect.contains("field_logical"));
}
