#[test]
fn write_safety_primitives_live_only_in_application_service() {
    let service = include_str!("../src/application/write.rs");
    for required in [
        "guard_usb_disk",
        "create_metadata_backup",
        "prepare_write",
        "reopen_rdwr",
        "verify_reopened_snapshot",
        "execute_write_transaction",
    ] {
        assert!(
            service.contains(required),
            "shared write service must own safety step: {required}"
        );
    }

    let cli = include_str!("../src/cli.rs");
    for forbidden in [
        "sysinfo::prepare_write",
        ".reopen_rdwr(",
        "diskio::atomic_write_sectors",
    ] {
        assert!(
            !cli.contains(forbidden),
            "CLI must consume the shared write service, found {forbidden}"
        );
    }
    assert!(
        !cli.contains("diskio::execute_write_transaction"),
        "CLI must not bypass the shared application write service"
    );

    let tui = include_str!("../src/tui/mod.rs");
    for forbidden in [
        "sysinfo::prepare_write",
        ".reopen_rdwr(",
        "diskio::atomic_write_sectors",
    ] {
        assert!(
            !tui.contains(forbidden),
            "TUI must consume the shared write service, found {forbidden}"
        );
    }
    assert!(
        !tui.contains("diskio::execute_write_transaction"),
        "TUI must not bypass the shared application write service"
    );
}

#[test]
fn critical_exit_contract_covers_ctrl_c_through_quit_intent() {
    let keymap = include_str!("../src/tui/keymap.rs");
    let state = include_str!("../src/tui/state.rs");
    assert!(keymap.contains("KeyModifiers::CONTROL"));
    assert!(keymap.contains("Some(TuiAction::Quit)"));
    assert!(state.contains("ExitDeferred"));
    assert!(state.contains("critical_operation"));
}

#[test]
fn shared_write_service_does_not_print_directly_into_tui_terminal() {
    let service = include_str!("../src/application/write.rs");
    assert!(!service.contains("println!("));
    assert!(!service.contains("print!("));
    assert!(service.contains(".write_event("));

    let transaction = include_str!("../src/diskio/transaction.rs");
    let atomic = transaction
        .split("pub fn atomic_write_sectors")
        .nth(1)
        .expect("atomic write source");
    assert!(!atomic.contains("eprintln!(\"!! 写入失败"));
}

#[test]
fn selected_device_identity_is_rechecked_before_the_operation_starts() {
    struct ImageDev(Vec<u8>);
    impl SectorDev for ImageDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            let start = lba as usize * 512;
            Ok(self.0[start..start + 512].to_vec())
        }
        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            unreachable!("identity verification is read-only")
        }
    }

    let Some(image) = common::load_disk_image("netac") else {
        return;
    };
    let runner = common::FakeRunner {
        canned: Default::default(),
    };
    let mut dev = ImageDev(image);
    let error = verify_expected_identity(&runner, 6, Some("different-device"), None, &mut dev)
        .expect_err("changed onlyid must fail closed");
    assert!(error.msg.contains("选择/确认期间发生变化"), "{}", error.msg);
}
use crate::common;

use std::io;

use edpcli::application::write::verify_expected_identity;
use edpcli::diskio::SectorDev;
