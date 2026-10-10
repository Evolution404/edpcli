//! Only a separately verified disposable macOS Disk Image may enter this
//! native EDPB recovery rehearsal. The v4 evidence-only restore contract is
//! unchanged; the entire real-device restore interface remains disabled.
#![cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]

use edpcli::application::evidence::native_restore_preview::{
    materialize_native_restore_evidence_for_hil, plan_native_restore_readonly,
};
use edpcli::application::filesystem::{NativeFilesystemWrite, NativeVirtualDiskPlan};
use edpcli::application::provision::native_commit::commit_native_plan_on_disk;
use edpcli::diskio::{
    inspect_native_journal, NativeBlockDevice, NativeJournalState, NativeRawBlockDevice,
};
use edpcli::platform::{self, system, ObservedDeviceGeometry};
use edpcli::provision::TargetIdentity;

struct NeverWrite;
impl edpcli::application::Prompter for NeverWrite {
    fn prompt_line(&mut self, _: &str) -> String {
        String::new()
    }
    fn prompt_secret(&mut self, _: &str) -> edpcli::provision::SecretBytes {
        edpcli::provision::SecretBytes::from_owned(Vec::new())
    }
    fn confirm_yes(&mut self, _: &str) -> bool {
        false
    }
}

#[test]
#[ignore = "explicit disposable Disk Image HIL; never run against physical media"]
fn native_edpb_evidence_virtual_wal_restore_and_fresh_readback() {
    let raw = std::env::var("EDPCLI_CRYPTO_HIL_RAW").expect("test-owned raw Disk Image");
    assert!(
        raw.starts_with("/dev/rdisk")
            && raw["/dev/rdisk".len()..]
                .chars()
                .all(|ch| ch.is_ascii_digit())
    );
    let sector: u32 = std::env::var("EDPCLI_CRYPTO_HIL_SECTOR")
        .unwrap()
        .parse()
        .unwrap();
    assert!(matches!(sector, 1024 | 2048 | 4096));
    let capacity: u64 = std::env::var("EDPCLI_CRYPTO_HIL_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(capacity, 536_870_912);
    let disk = platform::parse_disk_selector(&raw).unwrap();
    platform::set_include_virtual(true);
    // OS classification is independently verified by diskutil on *each* run.
    // The HIL invocation cannot upgrade real USB to Disk Image.
    assert!(platform::confirmed_virtual_disk_image(
        &system::SysRunner,
        disk
    ));
    let geometry = ObservedDeviceGeometry {
        capacity_bytes: capacity,
        logical_sector_bytes: Some(sector),
        physical_sector_bytes: None,
    }
    .native_read_geometry()
    .unwrap();
    assert_eq!(
        system::device_geometry(&system::SysRunner, disk)
            .unwrap()
            .native_read_geometry()
            .unwrap(),
        geometry
    );
    let probe = system::native_provision_probe(&system::SysRunner, disk).unwrap();
    let identity = TargetIdentity::from_probe(&probe, geometry.native_sector_count)
        .unwrap()
        .device_id()
        .to_owned();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = std::env::temp_dir().join(format!(
        "edpcli-edpb-native-wal-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&scratch).unwrap();

    let report = edpcli::application::write::backup_create_on_disk(
        &system::SysRunner,
        disk,
        scratch.clone(),
        &mut NeverWrite,
        None,
        Some(&identity),
    )
    .expect("native OS block evidence backup");
    let preview = plan_native_restore_readonly(&report.path, &identity, geometry).unwrap();
    let restore = materialize_native_restore_evidence_for_hil(&report.path, &identity, geometry)
        .expect("verified raw native evidence only");
    assert_eq!(
        restore.writes.len(),
        preview.proposed_lbas_in_write_order.len()
    );
    assert_eq!(restore.writes.last().unwrap().relative_lba, 0);
    let lce_before = restore
        .writes
        .iter()
        .filter(|block| {
            block.relative_lba >= preview.native_lce_start
                && block.relative_lba < preview.native_lce_start + preview.native_lce_blocks
        })
        .map(|block| block.data.clone())
        .collect::<Vec<_>>();
    assert_eq!(lce_before.len(), preview.native_lce_blocks as usize);
    let unchanged_lba0 = restore.writes.last().unwrap().data.clone();
    let original_lba8 = restore
        .writes
        .iter()
        .find(|b| b.relative_lba == 8)
        .unwrap()
        .data
        .clone();
    let mut changed_lba8 = original_lba8.clone();
    *changed_lba8.last_mut().unwrap() ^= 0x5a;
    let mutation = NativeVirtualDiskPlan {
        sector_bytes: sector,
        total_sectors: geometry.native_sector_count,
        writes: vec![
            NativeFilesystemWrite {
                relative_lba: 8,
                data: changed_lba8.clone(),
            },
            NativeFilesystemWrite {
                relative_lba: 0,
                data: unchanged_lba0.clone(),
            },
        ],
    };
    let mutate_wal = scratch.join("mutate.wal");
    commit_native_plan_on_disk(&system::SysRunner, disk, &mutation, &mutate_wal)
        .expect("test-only mutation through exact native TargetSession + WAL");
    assert_eq!(
        inspect_native_journal(&mutate_wal).unwrap().state,
        NativeJournalState::Committed
    );
    {
        let mut dev = NativeRawBlockDevice::open_readonly(&raw, geometry).unwrap();
        assert_eq!(dev.read_block_fresh(8).unwrap(), changed_lba8);
        assert_eq!(dev.read_block_fresh(0).unwrap(), unchanged_lba0);
    }
    let restore_wal = scratch.join("restore.wal");
    commit_native_plan_on_disk(&system::SysRunner, disk, &restore, &restore_wal)
        .expect("test-only verified v4 evidence replay through SAME native WAL");
    assert_eq!(
        inspect_native_journal(&restore_wal).unwrap().state,
        NativeJournalState::Committed
    );
    // Full native-block readback with a separately opened descriptor, including
    // complete LCE native tails and the byte modified in the earlier transaction.
    let mut fresh = NativeRawBlockDevice::open_readonly(&raw, geometry).unwrap();
    for block in &restore.writes {
        assert_eq!(
            fresh.read_block_fresh(block.relative_lba).unwrap(),
            block.data,
            "native OS readback LBA{}",
            block.relative_lba
        );
    }
    assert_eq!(fresh.read_block_fresh(8).unwrap(), original_lba8);
    assert_eq!(fresh.read_block_fresh(0).unwrap(), unchanged_lba0);
    drop(fresh);
    std::fs::remove_file(&report.path).unwrap();
    std::fs::remove_file(&mutate_wal).unwrap();
    std::fs::remove_file(&restore_wal).unwrap();
    std::fs::remove_dir(&scratch).unwrap();
    println!("[A03] PASS {sector}B native EDPB actual OS WAL damage/restore/readback; v4 evidence-only permission unchanged");
}
