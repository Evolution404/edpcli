#[test]
fn write_safety_primitives_live_only_in_application_service() {
    let service = include_str!("../src/application/write.rs");
    for required in [
        "guard_usb_disk",
        "create_backup",
        "prepare_write",
        "reopen_rdwr",
        "verify_reopened_snapshot",
        "atomic_write_sectors",
    ] {
        assert!(
            service.contains(required),
            "shared write service must own safety step: {required}"
        );
    }

    let cli = include_str!("../src/cli.rs");
    for forbidden in ["sysinfo::prepare_write", ".reopen_rdwr(", "diskio::atomic_write_sectors"] {
        assert!(
            !cli.contains(forbidden),
            "CLI must consume the shared write service, found {forbidden}"
        );
    }

    let tui = include_str!("../src/tui/mod.rs");
    for forbidden in ["sysinfo::prepare_write", ".reopen_rdwr(", "diskio::atomic_write_sectors"] {
        assert!(
            !tui.contains(forbidden),
            "TUI must consume the shared write service, found {forbidden}"
        );
    }
}

#[test]
fn critical_exit_contract_covers_ctrl_c_through_quit_intent() {
    let event = include_str!("../src/tui/event.rs");
    let state = include_str!("../src/tui/state.rs");
    assert!(event.contains("KeyModifiers::CONTROL"));
    assert!(event.contains("NavCommand::Quit"));
    assert!(state.contains("ExitDeferred"));
    assert!(state.contains("critical_operation"));
}
