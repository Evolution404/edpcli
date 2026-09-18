use edpcli::tui::task::GenerationGate;

#[test]
fn stale_background_results_are_rejected_by_generation() {
    let mut gate = GenerationGate::new();
    let first = gate.begin();
    let second = gate.begin();

    assert!(second > first);
    assert!(!gate.is_current(first));
    assert!(gate.is_current(second));
}

#[test]
fn redraw_path_never_performs_device_or_backup_scans() {
    let render = include_str!("../src/tui/render.rs");
    let event = include_str!("../src/tui/event.rs");
    let state = include_str!("../src/tui/state.rs");

    for source in [render, event, state] {
        for forbidden in [
            "scan_device_dashboard",
            "scan_backup_dir",
            "BackupSelector::load",
            "std::fs::read_dir",
            "FileDev::open",
        ] {
            assert!(
                !source.contains(forbidden),
                "redraw/event/state path must stay I/O-free: found {forbidden}"
            );
        }
    }
}

#[test]
fn task_source_runs_device_scan_on_a_worker_thread() {
    let task = include_str!("../src/tui/task.rs");
    assert!(
        task.contains("std::thread::spawn"),
        "device scan must run away from the redraw thread"
    );
    assert!(
        task.contains("scan_device_dashboard"),
        "worker must reuse the shared application service"
    );
}
