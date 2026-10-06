#[test]
fn write_safety_primitives_live_only_in_application_service() {
    let service = [
        include_str!("../src/application/write.rs"),
        include_str!("../src/application/write/backup.rs"),
        include_str!("../src/application/write/restore.rs"),
    ]
    .concat();
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
        "system::prepare_write",
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
        "system::prepare_write",
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
fn provision_tui_has_one_mandatory_backup_path_and_no_optional_prebackup_worker() {
    let state = include_str!("../src/tui/provision/state.rs");
    let task = include_str!("../src/tui/provision/task.rs");
    assert!(!state.contains("BackupPrompt"));
    assert!(!state.contains("BackupSaving"));
    assert!(!task.contains("request_provision_backup"));
    assert!(task.contains("request_provision_write"));
    assert!(task.contains("commit_provision_with_backup_on_disk"));
}

#[test]
fn chapter_18_b0_metadata_restore_and_prebackup_wording_are_locked() {
    let cli_ui = include_str!("../src/ui.rs");
    let tui_restore = include_str!("../src/tui/render.rs");
    let progress = include_str!("../src/application/progress.rs");

    for source in [cli_ui, tui_restore, progress] {
        assert!(
            !source.contains("请拔出重插"),
            "metadata restore must not imply that the filesystem is usable after restore"
        );
        assert!(
            source.contains("元数据恢复"),
            "restore completion must explicitly use metadata-restore semantics"
        );
    }
    assert!(
        progress.contains("Self::MandatoryBackup => \"制盘前元数据备份\""),
        "provision progress must describe the mandatory backup as metadata-only"
    );
}

#[test]
fn chapter_18_b0_mandatory_backup_precedes_lock_and_reopen() {
    let provision = include_str!("../src/application/provision.rs");
    let commit = include_str!("../src/application/provision/commit.rs");
    let chain = provision
        .split("pub fn commit_provision_with_backup_on_disk_with_progress(")
        .nth(1)
        .expect("mandatory backup application chain");
    let backup = chain
        .find("backup_create_on_disk(")
        .expect("mandatory metadata backup");
    let enter_commit = chain
        .find("commit_provision_on_disk_with_progress(")
        .expect("provision commit");
    assert!(
        backup < enter_commit,
        "backup must finish before commit begins"
    );
    assert!(
        commit.contains("progress(Phase::Identity, Step::LockAndReopen, None);"),
        "commit must retain the typed LockAndReopen boundary"
    );
}

#[test]
fn chapter_18_b0_disk5_fixture_freezes_metadata_only_restore_root_cause() {
    const SECTOR: usize = 512;
    const ORIGINAL_START: u32 = 2_048;
    const EDP_LEFTOVER_START: usize = 63;

    let mut backup_lba0_12 = vec![0u8; 13 * SECTOR];
    let entry = &mut backup_lba0_12[446..462];
    entry[4] = 0x07;
    entry[8..12].copy_from_slice(&ORIGINAL_START.to_le_bytes());
    entry[12..16].copy_from_slice(&100_000u32.to_le_bytes());
    backup_lba0_12[510..512].copy_from_slice(&[0x55, 0xaa]);

    let mut after_edp = vec![0u8; (ORIGINAL_START as usize + 16) * SECTOR];
    after_edp[EDP_LEFTOVER_START * SECTOR + 3..EDP_LEFTOVER_START * SECTOR + 11]
        .copy_from_slice(b"EXFAT   ");
    after_edp[..13 * SECTOR].copy_from_slice(&backup_lba0_12);

    let restored_start =
        u32::from_le_bytes(after_edp[454..458].try_into().expect("MBR start-LBA field"));
    assert_eq!(restored_start, ORIGINAL_START);
    assert_eq!(
        &after_edp[EDP_LEFTOVER_START * SECTOR + 3..EDP_LEFTOVER_START * SECTOR + 11],
        b"EXFAT   ",
        "intervening EDP filesystem evidence remains at LBA63"
    );
    assert_ne!(
        &after_edp[ORIGINAL_START as usize * SECTOR + 3..ORIGINAL_START as usize * SECTOR + 11],
        b"EXFAT   ",
        "restoring partition metadata does not restore the LBA2048 filesystem"
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
    let service = [
        include_str!("../src/application/write.rs"),
        include_str!("../src/application/write/backup.rs"),
        include_str!("../src/application/write/restore.rs"),
    ]
    .concat();
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

#[test]
fn plain_identity_recheck_accepts_zero_lba4_and_rejects_explicit_edp_onlyid() {
    use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use edpcli::ports::CmdRunner;
    use std::time::Duration;

    struct NativeRunner;
    impl CmdRunner for NativeRunner {
        fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
            Err(io::Error::other("native-only test runner"))
        }

        fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
            Some(HardwareProbe {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Bot,
                windows_pnp_instance_id: None,
                inquiry: Some(InquiryInfo {
                    vendor: "aigo".into(),
                    product: "U335".into(),
                    revision: "1100".into(),
                }),
            })
        }
    }

    struct PlainDev {
        lba4: Vec<u8>,
    }
    impl SectorDev for PlainDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            match lba {
                4 => Ok(self.lba4.clone()),
                7 => Ok(vec![0; 512]),
                _ => Err(io::Error::other("unexpected LBA")),
            }
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            unreachable!("identity verification is read-only")
        }
    }

    let runner = NativeRunner;
    let expected = "disk&ven_aigo&prod_u335&rev_1100";
    let mut plain = PlainDev { lba4: vec![0; 512] };
    verify_expected_identity(&runner, 4, None, Some(expected), &mut plain)
        .expect("zero-LBA4 Plain media should bind to exact hardware-derived device_id");

    let mut damaged = PlainDev { lba4: vec![0; 512] };
    damaged.lba4[..9].copy_from_slice(b"$$$123$$$");
    let error = verify_expected_identity(&runner, 4, None, Some(expected), &mut damaged)
        .expect_err("explicit EDP onlyid must block Plain hardware fallback");
    assert!(
        error.msg.contains("onlyid") || error.msg.contains("LBA4"),
        "{}",
        error.msg
    );
}

#[test]
fn edp_identity_recheck_accepts_short_and_revision_candidates_for_same_hardware() {
    use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use edpcli::ports::CmdRunner;
    use edpcli::protocol::crypto::{crc32_bare, xor_rolling};
    use std::time::Duration;

    struct NativeRunner;
    impl CmdRunner for NativeRunner {
        fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
            Err(io::Error::other("native-only test runner"))
        }

        fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
            Some(HardwareProbe {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Bot,
                windows_pnp_instance_id: None,
                inquiry: Some(InquiryInfo {
                    vendor: "aigo".into(),
                    product: "U335".into(),
                    revision: "1100".into(),
                }),
            })
        }
    }

    struct EdpDev(Vec<u8>);
    impl SectorDev for EdpDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            match lba {
                7 => Ok(self.0.clone()),
                _ => Err(io::Error::other("unexpected LBA")),
            }
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            unreachable!("identity verification is read-only")
        }
    }

    let actual = "disk&ven_aigo&prod_u335&rev_1100";
    let expected = "disk&ven_aigo&prod_u335";
    let crc = crc32_bare(actual.as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let mut decoded = vec![0u8; 512];
    decoded[..4].copy_from_slice(b"EDPF");
    let raw = xor_rolling(&decoded, k0);

    verify_expected_identity(&NativeRunner, 4, None, Some(expected), &mut EdpDev(raw)).expect(
        "short/long device_id variants from the same current hardware must not look like a swap",
    );
}

#[cfg(target_os = "macos")]
#[test]
fn plain_identity_recheck_accepts_whole_disk_ntfs_with_nonzero_lba4_code() {
    use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use edpcli::ports::CmdRunner;
    use std::time::Duration;

    struct MacPlainRunner;
    impl CmdRunner for MacPlainRunner {
        fn check_output(&self, cmd: &[&str], _timeout: Duration) -> io::Result<String> {
            if cmd == ["diskutil", "info", "-plist", "disk4"] {
                return Ok(
                    "<plist version=\"1.0\"><dict><key>IOKitSize</key><integer>15502147584</integer><key>Size</key><integer>15502147584</integer><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>15502143488</integer></dict></plist>"
                        .into(),
                );
            }
            Err(io::Error::other("unexpected command"))
        }

        fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
            Some(HardwareProbe {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Bot,
                windows_pnp_instance_id: None,
                inquiry: Some(InquiryInfo {
                    vendor: "aigo".into(),
                    product: "U335".into(),
                    revision: "1100".into(),
                }),
            })
        }
    }

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

    let total = 30_277_632u64;
    let mut image = vec![0u8; 13 * 512];
    {
        let boot = &mut image[..512];
        boot[..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[11..13].copy_from_slice(&512u16.to_le_bytes());
        boot[13] = 8;
        boot[21] = 0xf8;
        boot[40..48].copy_from_slice(&(total - 1).to_le_bytes());
        boot[48..56].copy_from_slice(&4u64.to_le_bytes());
        boot[56..64].copy_from_slice(&8u64.to_le_bytes());
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    }
    image[4 * 512..4 * 512 + 16].copy_from_slice(&[
        0x66, 0x61, 0x90, 0x1f, 0x07, 0xc3, 0x06, 0x1e, 0x66, 0x60, 0x66, 0xb8, 1, 0, 0, 0,
    ]);

    let runner = MacPlainRunner;
    let expected = "disk&ven_aigo&prod_u335&rev_1100";
    verify_expected_identity(&runner, 4, None, Some(expected), &mut ImageDev(image))
        .expect("strict whole-disk NTFS Plain evidence should allow nonzero LBA4 boot code");
}

use crate::common;

use std::io;

use edpcli::application::write::verify_expected_identity;
use edpcli::ports::SectorDev;
