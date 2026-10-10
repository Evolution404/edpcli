use super::*;
use crate::provision::DiskProvisionKind;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_pin() -> MediaIdentityPin {
    MediaIdentityPin::new(
        super::super::media_identity::MediaIdentitySnapshot::default(),
        &[0; 13 * SECTOR],
    )
}

#[test]
fn media_pin_accepts_v3_raw_serial_and_rejects_changed_serial() {
    use super::super::media_identity::{serial_digest_evidence, SerialQuality};
    let image = vec![0; 13 * SECTOR];
    let mut observed = super::super::media_identity::MediaIdentitySnapshot::default();
    observed.hardware.serial = Some("HIKSEMI-TEST-001".into());
    observed.hardware.serial_quality = SerialQuality::Usable;
    let mut prepared = observed.clone();
    prepared.hardware.serial_sha256 =
        serial_digest_evidence(prepared.hardware.serial.as_deref()).sha256;
    let pin = MediaIdentityPin::new(prepared, &image);
    pin.verify(&observed, &image).unwrap();
    super::super::media_identity::MediaIdentityResumePin::from_pin(&pin)
        .verify(&observed, &image)
        .unwrap();
    observed.hardware.serial = Some("HIKSEMI-TEST-002".into());
    assert_eq!(
        pin.verify(&observed, &image),
        Err(super::super::media_identity::MediaIdentityPinConflict::SerialChangedOrLost)
    );
}

#[test]
fn host_lineage_record_is_immutable_and_stays_under_backup_dir() {
    let root = std::env::temp_dir().join(format!(
        "edpcli-lineage-{}-{}",
        std::process::id(),
        TEST_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let record = identity_lineage::IdentityTransitionRecord {
        schema: "edpcli.identity-lineage.v1".into(),
        transaction_id: "test-transaction-001".into(),
        created_epoch: 1_789_603_200,
        operation: "PlainToMode0".into(),
        before_identity: super::super::media_identity::MediaIdentitySnapshot::default(),
        after_identity: super::super::media_identity::MediaIdentitySnapshot::default(),
        mandatory_backup_path: root.join("source.edpb"),
        mandatory_backup_sha256: "a".repeat(64),
        provision_summary_digest: "b".repeat(64),
    };
    let path = identity_lineage::persist(&root, &record).unwrap();
    assert!(path.starts_with(root.join(".edpcli/identity-lineage/v1")));
    assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("json"));
    assert!(identity_lineage::persist(&root, &record).is_err());
    let saved: identity_lineage::IdentityTransitionRecord =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, record);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn mandatory_backup_must_match_prepared_canonical_pin() {
    let root = std::env::temp_dir().join(format!(
        "edpcli-backup-pin-{}-{}",
        std::process::id(),
        TEST_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("plain.edpb");
    let image = vec![0; 13 * SECTOR];
    crate::edpb::write_core_backup(
        &path,
        &crate::edpb::CoreCapture {
            snapshot_id: "pin-test".into(),
            created_epoch: 1_789_603_200,
            disk_number: Some(4),
            vid: "3535".into(),
            pid: "6300".into(),
            device_id: "disk&ven_aigo&prod_u335".into(),
            onlyid: None,
            total_sectors: Some(1_000_000),
            logical_sector_size: SECTOR as u32,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "plain".into(),
            lba0_12: &image,
        },
    )
    .unwrap();
    let verified = crate::edpb::verify_file(&path).unwrap();
    let identity = crate::edpb::canonical_media_identity(&verified.manifest).unwrap();
    let report = super::super::post_restore::MetadataBackupReport {
        path,
        partition_count: 0,
        edp_protocol_saved: true,
    };
    let pin = MediaIdentityPin::new(identity.clone(), &image);
    assert_eq!(
        verify_mandatory_backup_pin(&report, &pin, &image).unwrap(),
        verified.file_sha256
    );
    let mut conflicting = identity;
    conflicting.hardware.serial_quality = super::super::media_identity::SerialQuality::Usable;
    conflicting.hardware.serial_sha256 = Some("a".repeat(64));
    let pin = MediaIdentityPin::new(conflicting, &image);
    let error = verify_mandatory_backup_pin(&report, &pin, &image).unwrap_err();
    assert_eq!(error.code, EXIT_TARGET);
    assert!(error.msg.contains("SerialChangedOrLost"), "{}", error.msg);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn mandatory_plain_backup_verifies_mbr_without_edp_protocol_artifact() {
    use crate::edpb::{
        ArtifactCompleteness, ArtifactInput, CoreCapture, Extent, ManifestPartition,
        MetadataCapture, Region, RestorePolicy, SemanticStatus,
    };

    let root = std::env::temp_dir().join(format!(
        "edpcli-plain-backup-pin-{}-{}",
        std::process::id(),
        TEST_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("plain.edpb");
    let mut image = vec![0; 13 * SECTOR];
    image[510..512].copy_from_slice(&[0x55, 0xaa]);
    image[0] = 0x42;
    image[450] = 0x07;
    image[454..458].copy_from_slice(&2048u32.to_le_bytes());
    image[458..462].copy_from_slice(&100_000u32.to_le_bytes());
    let capture = MetadataCapture {
        core: CoreCapture {
            snapshot_id: "plain-pin-test".into(),
            created_epoch: 1_789_603_200,
            disk_number: Some(4),
            vid: "3535".into(),
            pid: "6300".into(),
            device_id: "disk&ven_aigo&prod_u335".into(),
            onlyid: None,
            total_sectors: Some(1_000_000),
            logical_sector_size: SECTOR as u32,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "plain".into(),
            lba0_12: &image,
        },
        partitions: vec![ManifestPartition {
            index: 1,
            role: None,
            partition_type: Some("mbr:07".into()),
            start_lba: 2048,
            sector_count: 100_000,
            filesystem_hint: None,
            volume_label_hint: None,
        }],
        regions: vec![Region {
            id: "region.plain.partition_table".into(),
            role: "plain_partition_table".into(),
            start_lba: None,
            sector_count: None,
            semantic_status: SemanticStatus::Identified,
        }],
        extents: vec![Extent {
            id: "extent.plain.partition_table.0".into(),
            region_id: "region.plain.partition_table".into(),
            start_lba: 0,
            sector_count: 1,
            purpose: "mbr".into(),
        }],
        artifacts: vec![ArtifactInput {
            id: "raw.plain.partition_table.0".into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec!["extent.plain.partition_table.0".into()],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            data: image[..SECTOR].to_vec(),
        }],
        notes: vec![],
    };
    crate::edpb::write_metadata_backup(&path, &capture).unwrap();
    let verified = crate::edpb::verify_file(&path).unwrap();
    let identity = crate::edpb::canonical_media_identity(&verified.manifest).unwrap();
    let report = super::super::post_restore::MetadataBackupReport {
        path,
        partition_count: 1,
        edp_protocol_saved: false,
    };
    let pin = MediaIdentityPin::new(identity, &image);
    assert_eq!(
        verify_mandatory_backup_pin(&report, &pin, &image).unwrap(),
        verified.file_sha256
    );
    let mut changed_image = image.clone();
    changed_image[0] ^= 1;
    let changed_pin = MediaIdentityPin::new(pin.snapshot.clone(), &changed_image);
    assert!(
        verify_mandatory_backup_pin(&report, &changed_pin, &changed_image)
            .unwrap_err()
            .msg
            .contains("快照不一致")
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn portable_test_temp_file(stem: &str, extension: &str) -> std::path::PathBuf {
    let sequence = TEST_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{stem}-{}-{sequence}.{extension}",
        std::process::id()
    ))
}

struct Lba3Dev([u8; SECTOR]);

impl SectorDev for Lba3Dev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        if lba != 3 {
            return Err(io::Error::other("unexpected read"));
        }
        Ok(self.0.to_vec())
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
        Err(io::Error::other("read-only test device"))
    }

    // Explicit test device synchronization contract.
    fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    // This test device is already writable.
    fn reopen_rdwr(&mut self, _: std::time::Duration) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn mandatory_backup_failure_prevents_provision_commit() {
    let commit_called = std::cell::Cell::new(false);
    let error = run_mandatory_backup_before_commit(
        || Err(err(EXIT_IO, "backup failed")),
        || {
            commit_called.set(true);
            Ok(ProvisionCommitOutcome::Plain { partition_count: 1 })
        },
    )
    .unwrap_err();

    assert_eq!(error.code, EXIT_IO);
    assert!(
        !commit_called.get(),
        "commit must not run when mandatory backup fails"
    );
}

#[test]
fn mandatory_backup_success_runs_commit_after_backup() {
    let order = std::cell::RefCell::new(Vec::new());
    let report = run_mandatory_backup_before_commit(
        || {
            order.borrow_mut().push("backup");
            Ok(super::super::post_restore::MetadataBackupReport {
                path: std::path::PathBuf::from("test.edpb"),
                partition_count: 0,
                edp_protocol_saved: false,
            })
        },
        || {
            order.borrow_mut().push("commit");
            Ok(ProvisionCommitOutcome::Plain { partition_count: 2 })
        },
    )
    .unwrap();

    assert_eq!(&*order.borrow(), &["backup", "commit"]);
    assert_eq!(
        report.commit,
        ProvisionCommitOutcome::Plain { partition_count: 2 }
    );
}

#[test]
fn typed_execution_status_is_shared_across_cli_and_tui() {
    let backup = super::super::post_restore::MetadataBackupReport {
        path: std::path::PathBuf::from("test.edpb"),
        partition_count: 0,
        edp_protocol_saved: false,
    };
    let mut outcome = ProvisionWriteOutcome {
        backup,
        commit: ProvisionCommitOutcome::Official(ProvisionCommitReport {
            provision_succeeded: true,
            formats: vec![PartitionFormatResult {
                role: PartitionRole::Share,
                result: Ok(()),
            }],
        }),
        warnings: Vec::new(),
    };
    assert_eq!(
        outcome.execution_status(),
        ProvisionExecutionStatus::Success
    );
    outcome
        .warnings
        .push(ProvisionWarning::AfterIdentityObservationFailed(
            "offline".into(),
        ));
    assert_eq!(
        outcome.execution_status(),
        ProvisionExecutionStatus::CompletedWithWarnings
    );
    if let ProvisionCommitOutcome::Official(report) = &mut outcome.commit {
        report.formats[0].result = Err("format failure".into());
    }
    assert_eq!(
        outcome.execution_status(),
        ProvisionExecutionStatus::PartialFormatFailure
    );
    assert_eq!(outcome.execution_status().exit_code(), EXIT_IO);
}

#[test]
fn editor_preserve_assessment_reports_typed_geometry_reason() {
    let source = crate::provision::ExistingPartition {
        role: PartitionRole::Share,
        partition_type: crate::protocol::edpf::EdpPartitionType::Share,
        start_lba: 20480,
        sector_count: 4096,
        physically_encrypted: true,
        filesystem: Some(FilesystemKind::ExFat),
    };
    let same = source.as_target();
    assert!(PreserveAssessment::for_partition(Some(&source), &same).candidate);
    let moved = crate::provision::TargetPartitionGeometry {
        start_lba: 20481,
        ..same
    };
    let assessment = PreserveAssessment::for_partition(Some(&source), &moved);
    assert_eq!(
        assessment.failure,
        Some(crate::provision::CompatibilityFailure::StartLba)
    );
    assert!(!assessment.candidate);
}

#[test]
fn mode2_quick_and_exact_encrypt_capacity_are_partition_scoped() {
    let quick = target_encrypt_capacity_override(Some(128), None)
        .unwrap()
        .unwrap();
    let exact = target_encrypt_capacity_override(None, Some(128 * 2048))
        .unwrap()
        .unwrap();
    assert_eq!(quick.sectors(), 128 * 2048);
    assert_eq!(exact.sectors(), 128 * 2048);
    assert_eq!(quick.sectors(), exact.sectors());
}

#[test]
fn preserve_requires_a_full_prewrite_source_metadata_snapshot() {
    assert!(validate_preserve_source_snapshot(false, None).is_ok());
    assert!(validate_preserve_source_snapshot(true, None).is_err());
    assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 12 * SECTOR])).is_err());
    assert!(validate_preserve_source_snapshot(true, Some(&vec![0; 13 * SECTOR])).is_ok());
}

#[test]
fn manufacturer_lba3_is_copied_verbatim_into_the_write_plan() {
    let metadata = ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap();
    let mut patch = BTreeMap::new();
    patch.insert(3, vec![0; SECTOR]);
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: None,
    };
    let mut prepared = PreparedNewProvision {
        disk: 4,
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        source_kind: DiskProvisionKind::Mode1,
        mode: OfficialPartitionMode::BootShareCombined,
        algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: 900,
        write_image: OfficialProvisionWriteImage {
            metadata,
            total_sectors: 1024,
            patch,
        },
        format_targets: Vec::new(),
        target_plan: None,
        source_metadata: None,
        before_pin: fixture_pin(),
        plan: OfficialProvisionPlan::new(
            OfficialPartitionMode::BootShareCombined,
            OfficialPartitionSizes::new(32, 64, 128),
            crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
                chs_bytes: 0,
                start_byte_offset: 0,
                start_lba: 900,
                size_bytes: 3072,
                size_sectors: 6,
            },
            wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
            wrap_file_key(b"0000aaaa", [0; 16], FileKeyWrapMode::Sm4),
        )
        .unwrap(),
        expected_onlyid: "1".into(),
        expected_serial_digest: None,
        expected_probe: probe,
        expected_lba3: None,
    };
    let expected = [0xa5; SECTOR];
    let mut dev = Lba3Dev(expected);
    capture_manufacturer_lba3(&mut dev, &mut prepared).unwrap();
    assert_eq!(prepared.write_image.patch.get(&3).unwrap(), &expected);
    assert_eq!(prepared.expected_lba3, Some(expected));
}

#[derive(Default)]
struct MemoryDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    fail_at: Option<u32>,
}

impl SectorDev for MemoryDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        if self.fail_at == Some(lba) {
            return Err(io::Error::other("injected format failure"));
        }
        self.sectors.insert(lba, data.to_vec());
        Ok(())
    }

    // Explicit test device synchronization contract.
    fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    // This test device is already writable.
    fn reopen_rdwr(&mut self, _: std::time::Duration) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn plain_extent_reader_recovers_exact_fat16_boot_evidence_from_live_media() {
    let total_sectors = 200_000u64;
    let plan = PlainProvisionPlan::new(
        total_sectors,
        vec![PlainPartitionSpec::new(
            63,
            20_417,
            FilesystemKind::Fat16,
            "BOOT",
        )],
    )
    .unwrap();
    let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
    let mut dev = MemoryDev::default();
    for (&lba, sector) in &write_plan.writes {
        dev.sectors.insert(lba, sector.bytes.to_vec());
    }

    let extents = read_plain_source_extents(&mut dev, total_sectors).unwrap();
    assert_eq!(
        extents,
        vec![crate::provision::PlainSourceExtent {
            start_lba: 63,
            sector_count: 20_417,
            filesystem: Some(FilesystemKind::Fat16),
        }]
    );
}

#[test]
fn plain_extent_reader_rejects_fat16_with_stale_hidden_sector_geometry() {
    let total_sectors = 200_000u64;
    let plan = PlainProvisionPlan::new(
        total_sectors,
        vec![PlainPartitionSpec::new(
            63,
            20_417,
            FilesystemKind::Fat16,
            "BOOT",
        )],
    )
    .unwrap();
    let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
    let mut dev = MemoryDev::default();
    for (&lba, sector) in &write_plan.writes {
        dev.sectors.insert(lba, sector.bytes.to_vec());
    }

    let boot = dev.sectors.get_mut(&63).expect("FAT16 boot sector");
    boot[28..32].copy_from_slice(&64u32.to_le_bytes());
    assert_eq!(
        crate::filesystem::detect_boot_sector(20_417, boot).unwrap(),
        Some(FilesystemKind::Fat16),
        "type detection alone still sees FAT16"
    );

    let extents = read_plain_source_extents(&mut dev, total_sectors).unwrap();
    assert_eq!(extents.len(), 1);
    assert_eq!(extents[0].start_lba, 63);
    assert_eq!(extents[0].sector_count, 20_417);
    assert_eq!(
        extents[0].filesystem, None,
        "stale FAT16 hidden-sector geometry must not qualify for Plain boot preservation"
    );
    assert!(
        !crate::provision::plain_extent_preserve_candidate(
            &extents,
            &crate::provision::TargetPartitionGeometry {
                role: PartitionRole::Boot,
                partition_type: crate::protocol::edpf::EdpPartitionType::Boot,
                start_lba: 63,
                sector_count: 20_417,
                physically_encrypted: false,
                filesystem: Some(FilesystemKind::Fat16),
            },
        ),
        "a filesystem signature without matching on-disk geometry must rebuild"
    );
}

#[test]
fn plain_prewrite_snapshot_rejects_stale_lba7_metadata() {
    let total_sectors = 100_000;
    let plan = PlainProvisionPlan::default_for_disk(total_sectors).unwrap();
    let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: None,
    };
    let mut source_metadata = vec![0u8; 13 * SECTOR];
    source_metadata[3 * SECTOR..4 * SECTOR].fill(0xa5);
    let prepared = PreparedPlainProvision {
        disk: 4,
        device_id: "disk&ven_aigo&prod_u335".into(),
        plan,
        write_plan,
        source_kind: crate::provision::DiskProvisionKind::Plain,
        source_lce_start_lba: None,
        source_metadata: source_metadata.clone(),
        before_pin: fixture_pin(),
        expected_probe: probe,
    };
    let mut dev = MemoryDev::default();
    for lba in 0..13u32 {
        dev.sectors.insert(
            lba,
            source_metadata[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec(),
        );
    }

    verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap();
    dev.sectors.get_mut(&7).unwrap()[0] ^= 1;
    let error = verify_reopened_snapshot(&mut dev, &prepared.source_metadata).unwrap_err();
    assert!(error.msg.contains("LBA7"));
}

fn format_test_plan(mode: OfficialPartitionMode, key: &[u8; 16]) -> OfficialProvisionPlan {
    OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
            chs_bytes: 0,
            start_byte_offset: 4_194_000 * SECTOR as u64,
            start_lba: 4_194_000,
            size_bytes: 3072,
            size_sectors: 6,
        },
        wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
        wrap_file_key(b"0000aaaa", *key, FileKeyWrapMode::Sm4),
    )
    .unwrap()
}

#[test]
fn combined_boot_share_uses_boot_label_while_reusing_share_format_toggle() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::BootShareCombined, &key);
    let choices = plan_format_targets_typed(
        &plan,
        &FormatOptions {
            share: true,
            boot_label: "启动区".into(),
            share_label: "交换区".into(),
            ..FormatOptions::default()
        },
        &[1, 2],
        &key,
    )
    .unwrap();

    let combined = choices
        .iter()
        .find(|choice| choice.target.role == PartitionRole::BootShareCombined)
        .unwrap();
    assert!(combined.selected);
    assert_eq!(combined.volume_label, "启动区");
}

#[test]
fn format_executor_uses_the_same_matrix_and_preserves_protocol_sectors() {
    let key = [0x42; 16];
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = format_test_plan(mode, &key);
        let serials = vec![0x1234_5678; plan.format_targets().unwrap().len()];
        let options = FormatOptions {
            boot: mode != OfficialPartitionMode::WholeDiskEncrypted
                && mode != OfficialPartitionMode::BootShareCombined,
            share: mode != OfficialPartitionMode::WholeDiskEncrypted,
            encrypt: mode != OfficialPartitionMode::IntranetExtranetDualPartition,
            ..FormatOptions::default()
        };
        let choices = plan_format_targets_typed(&plan, &options, &serials, &key).unwrap();
        let mut dev = MemoryDev::default();
        for lba in 0..13u32 {
            dev.sectors.insert(lba, vec![lba as u8; SECTOR]);
        }
        for choice in choices.iter().filter(|choice| choice.selected) {
            execute_partition_format(&mut dev, choice).unwrap();
            let raw = dev
                .read_sector(choice.target.geometry.start_sector as u32)
                .unwrap();
            if choice.target.physically_encrypted {
                assert_ne!(raw.get(3..11), Some(&b"EXFAT   "[..]));
                assert_ne!(raw.get(54..62), Some(&b"FAT16   "[..]));
            } else if choice.filesystem == Some(FilesystemKind::Fat16) {
                assert_eq!(raw.get(54..62), Some(&b"FAT16   "[..]));
            } else {
                assert_eq!(raw.get(3..11), Some(&b"EXFAT   "[..]));
            }
        }
        for lba in 0..13u32 {
            assert_eq!(dev.read_sector(lba).unwrap(), vec![lba as u8; SECTOR]);
        }
        if mode == OfficialPartitionMode::WholeDiskEncrypted {
            assert!(!dev.sectors.contains_key(&63));
        }
    }
}

/// Inject deferred corruption on the *third* read of one authored LBA:
/// snapshot=1, transaction synchronized verification=2, independent
/// post-transaction physical metadata verification=3.
struct CorruptThirdRead {
    backing: MemoryDev,
    victim: u32,
    reads: usize,
}

impl SectorDev for CorruptThirdRead {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        let mut result = self.backing.read_sector(lba)?;
        if lba == self.victim {
            self.reads += 1;
            if self.reads == 3 {
                result[SECTOR - 1] ^= 0x40;
            }
        }
        Ok(result)
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.backing.write_sector(lba, data)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.backing.sync()
    }
    fn reopen_rdwr(&mut self, wait: std::time::Duration) -> io::Result<()> {
        self.backing.reopen_rdwr(wait)
    }
}

#[test]
fn format_second_physical_readback_detects_deferred_plain_and_ciphertext_corruption() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
    let choices = plan_format_targets_typed(
        &plan,
        &FormatOptions {
            boot: true,
            share: true,
            encrypt: true,
            ..FormatOptions::default()
        },
        &[1, 2, 3],
        &key,
    )
    .unwrap();
    for choice in choices.iter().filter(|choice| choice.selected) {
        // Choose authored FAT/exFAT metadata beyond the boot sector to rule
        // out first-sector-only verification and to include encrypted modes.
        let (&relative_lba, _) = choice
            .prepared_image
            .as_ref()
            .unwrap()
            .image
            .sectors()
            .iter()
            .find(|(lba, _)| **lba != 0)
            .expect("format image includes nonboot metadata");
        let victim = (choice.target.geometry.start_sector + relative_lba) as u32;
        let mut dev = CorruptThirdRead {
            backing: MemoryDev::default(),
            victim,
            reads: 0,
        };
        let error = execute_partition_format(&mut dev, choice).unwrap_err();
        assert!(error.msg.contains("二次回读LBA"), "{}", error.msg);
        assert_eq!(dev.reads, 3);
        // The corruption was only in the read channel; prior transaction
        // succeeded. Format must nevertheless refuse to report success.
        assert!(dev.backing.sectors.contains_key(&victim));
    }
}

#[test]
fn format_failure_keeps_the_protocol_and_prior_successful_partition() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
    let choices = plan_format_targets_typed(
        &plan,
        &FormatOptions {
            boot: true,
            share: true,
            ..FormatOptions::default()
        },
        &[1, 2, 3],
        &key,
    )
    .unwrap();
    let mut dev = MemoryDev::default();
    for lba in 0..13u32 {
        dev.sectors.insert(lba, vec![0xa5; SECTOR]);
    }
    execute_partition_format(&mut dev, &choices[0]).unwrap();
    dev.fail_at = Some(choices[1].target.geometry.start_sector as u32);
    assert!(execute_partition_format(&mut dev, &choices[1]).is_err());
    assert_eq!(
        &dev.read_sector(choices[0].target.geometry.start_sector as u32)
            .unwrap()[54..62],
        b"FAT16   "
    );
    for lba in 0..13u32 {
        assert_eq!(dev.read_sector(lba).unwrap(), vec![0xa5; SECTOR]);
    }
}

#[test]
fn portable_test_temp_file_name_uses_safe_ascii_components() {
    let path = portable_test_temp_file("edpcli-provision-export", "img");
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("portable test temp path must have a UTF-8 file name");
    assert!(name
        .bytes()
        .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') }));
    assert!(!name.contains(':'));
}

#[test]
fn plain_export_uses_native_transaction_preserves_lba3_and_rejects_device_target() {
    use std::io::{Read, Seek, SeekFrom};
    let total_sectors = 100_000;
    let plan = PlainProvisionPlan::default_for_disk(total_sectors).unwrap();
    let write_plan = build_plain_provision_write_plan(&plan, None, &[0x1234_5678]).unwrap();
    let mut source_metadata = vec![0u8; 13 * SECTOR];
    source_metadata[3 * SECTOR..4 * SECTOR].fill(0xa5);
    let prepared = PreparedPlainProvision {
        disk: 4,
        device_id: "disk&ven_aigo&prod_u335".into(),
        plan,
        write_plan,
        source_kind: DiskProvisionKind::Plain,
        source_lce_start_lba: None,
        source_metadata,
        before_pin: fixture_pin(),
        expected_probe: crate::platform::HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: crate::platform::NativeTransport::Uas,
            windows_pnp_instance_id: None,
            inquiry: None,
        },
    };
    assert!(export_sparse_plain_provision_image(Path::new("/dev/disk99"), &prepared).is_err());
    let path = portable_test_temp_file("edpcli-plain-native-export", "img");
    let _ = std::fs::remove_file(&path);
    export_sparse_plain_provision_image(&path, &prepared).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let mut mbr = [0u8; SECTOR];
    file.read_exact(&mut mbr).unwrap();
    assert_eq!(mbr.as_slice(), prepared.write_plan.mbr);
    file.seek(SeekFrom::Start(3 * SECTOR as u64)).unwrap();
    let mut lba3 = [0u8; SECTOR];
    file.read_exact(&mut lba3).unwrap();
    assert_eq!(lba3, [0xa5; SECTOR]);
    file.seek(SeekFrom::Start(7 * SECTOR as u64)).unwrap();
    let mut lba7 = [0u8; SECTOR];
    file.read_exact(&mut lba7).unwrap();
    assert_eq!(lba7, [0; SECTOR]);
    file.seek(SeekFrom::Start(2048 * SECTOR as u64)).unwrap();
    let mut boot = [0u8; SECTOR];
    file.read_exact(&mut boot).unwrap();
    assert_eq!(&boot[3..11], b"EXFAT   ");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn sparse_export_includes_selected_format_images() {
    let key = [0x42; 16];
    let plan = format_test_plan(OfficialPartitionMode::DefaultThreePartition, &key);
    let choices = plan_format_targets_typed(
        &plan,
        &FormatOptions {
            boot: true,
            ..FormatOptions::default()
        },
        &[0x1234_5678, 2, 3],
        &key,
    )
    .unwrap();
    let mut patch = BTreeMap::new();
    patch.insert(0, vec![0x5a; SECTOR]);
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: None,
    };
    let prepared = PreparedNewProvision {
        disk: 4,
        device_id: "disk&ven_aigo&prod_u335".into(),
        source_kind: DiskProvisionKind::Mode0,
        mode: plan.mode,
        algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: plan.lba7_compatibility_extent.start_lba,
        write_image: OfficialProvisionWriteImage {
            metadata: ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap(),
            total_sectors: 1_000_000,
            patch,
        },
        format_targets: choices,
        target_plan: None,
        source_metadata: None,
        before_pin: fixture_pin(),
        plan,
        expected_onlyid: "1".into(),
        expected_serial_digest: None,
        expected_probe: probe,
        expected_lba3: None,
    };
    let path = portable_test_temp_file("edpcli-provision-export", "img");
    let _ = std::fs::remove_file(&path);
    export_sparse_provision_image(&path, &prepared).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    let mut mbr = [0u8; SECTOR];
    file.read_exact(&mut mbr).unwrap();
    assert_eq!(mbr, [0x5a; SECTOR]);
    file.seek(SeekFrom::Start(63 * SECTOR as u64)).unwrap();
    let mut boot = [0u8; SECTOR];
    file.read_exact(&mut boot).unwrap();
    assert_eq!(&boot[54..62], b"FAT16   ");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_4kn_new_edp_all_modes_algorithms_export_and_verify_virtual_disk() {
    use crate::partition_transform::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };
    use crate::protocol::crypto::{a6b0_full, crc32_bare};
    use crate::protocol::image::NativeProtocolImage;
    use std::io::{Read, Seek, SeekFrom};

    let total = 262_144u64;
    let lce_lba = 250_000u64;
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total).unwrap();
    let did = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "TEST USER",
        "TEST DEPT",
        "EDP 4Kn VIRTUAL",
    )
    .unwrap();
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap();
    let entropy = ProvisionEntropy::new([0x5a; 252]);
    let key = [0x42u8; 16];
    let lce_geometry = crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
        chs_bytes: lce_lba * 4096 + 0xe0000,
        start_byte_offset: lce_lba * 4096,
        start_lba: lce_lba,
        size_bytes: 4096,
        size_sectors: 1,
    };

    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        for algorithm in [
            FileKeyWrapMode::Sm4,
            FileKeyWrapMode::A7f0,
            FileKeyWrapMode::Aes128Ecb,
        ] {
            let options = FormatOptions {
                boot: matches!(
                    mode,
                    OfficialPartitionMode::DefaultThreePartition
                        | OfficialPartitionMode::IntranetExtranetDualPartition
                ),
                share: mode != OfficialPartitionMode::WholeDiskEncrypted,
                encrypt: mode != OfficialPartitionMode::IntranetExtranetDualPartition,
                ..FormatOptions::default()
            };
            let plan = OfficialProvisionPlan::new(
                mode,
                OfficialPartitionSizes::new(32, 64, 128),
                lce_geometry,
                wrap_legacy_lba7_file_key(b"0000aaaa", [0u8; 8]),
                wrap_file_key(b"0000aaaa", key, algorithm),
            )
            .unwrap()
            .with_filesystems(options.filesystems());
            let targets = plan.format_targets_native(4096).unwrap();
            let serials = vec![0x1234_5678; targets.len()];
            let file_keys = vec![key; targets.len()];
            let candidate = native_image::plan_native_edp_4kn_image(
                &spec, &entropy, &plan, &options, &serials, &file_keys,
            )
            .unwrap();
            assert_eq!(candidate.total_sectors, total);
            assert_eq!(candidate.sector_bytes, 4096);
            assert_eq!(candidate.writes.last().unwrap().relative_lba, 0);
            assert!(candidate
                .writes
                .iter()
                .all(|write| write.data.len() == 4096));
            let native_bytes = (0..13)
                .flat_map(|lba| {
                    candidate
                        .writes
                        .iter()
                        .find(|write| write.relative_lba == lba)
                        .unwrap()
                        .data
                        .clone()
                })
                .collect::<Vec<_>>();
            let native = NativeProtocolImage::from_native_bytes(4096, native_bytes).unwrap();
            let parsed = crate::provision::parse_existing_provision_native(&native, &did, total)
                .unwrap()
                .unwrap();
            assert_eq!(parsed.profile.source_mode, mode);
            assert_eq!(parsed.records.len(), targets.len());

            // Official 4Kn LBA11 binds its PDKB decryption key to native
            // byte capacity, not the legacy total_lbas * 512 shortcut.
            let lba11 = native.block(11).unwrap();
            let mut key_input = lba11[..256].to_vec();
            key_input.extend_from_slice(spec.target().vid_hex().as_bytes());
            key_input.extend_from_slice(spec.target().pid_hex().as_bytes());
            key_input.extend_from_slice(&(total * 4096).to_le_bytes());
            let lba11_key = crc32_bare(&key_input).to_le_bytes();
            let decrypted_pdkb = a6b0_full(&lba11[256..512], &lba11_key, 0);
            assert_eq!(&decrypted_pdkb[..4], b"PDKB");
            assert_eq!(
                &decrypted_pdkb[4..4 + did.len()],
                did.as_bytes(),
                "LBA11 identity must use true 4Kn capacity"
            );
            assert!(
                lba11[512..].iter().all(|b| *b == 0),
                "new virtual native protocol must not reproduce leaked tail"
            );
            let compat = candidate
                .writes
                .iter()
                .find(|write| write.relative_lba == lce_lba)
                .unwrap();
            let lce_plain = a6b0_full(&compat.data, &[0u8; 8], lce_lba * 4096);
            assert_eq!(&lce_plain[..3072], crate::provision::lce_plaintext());
            assert_eq!(&lce_plain[3072..], &[0u8; 1024]);
            for (index, target) in targets.iter().enumerate() {
                let (selected, _) = options.choice(target.role);
                if !selected {
                    continue;
                }
                let lba = target.geometry.start_sector;
                let sector = candidate
                    .writes
                    .iter()
                    .find(|write| write.relative_lba == lba)
                    .expect("selected native filesystem must have a boot block");
                let plaintext = if target.physically_encrypted {
                    assert_eq!(crc32_bare(&key), plan.lba12_key_material.file_key_crc);
                    transform_native_sector_offline(
                        NativePartitionDataCipher::from_encrypt_mode(algorithm.raw()).unwrap(),
                        NativeCipherDirection::Decrypt,
                        &sector.data,
                        &file_keys[index],
                        lba,
                        4096,
                    )
                    .unwrap()
                } else {
                    sector.data.clone()
                };
                assert_eq!(
                    crate::filesystem::detect_native_boot_sector(
                        &plaintext,
                        target.geometry.sector_count(),
                        4096
                    )
                    .unwrap(),
                    target.filesystem,
                );
            }
            if targets.iter().any(|target| target.physically_encrypted) {
                let mut wrong_keys = file_keys.clone();
                wrong_keys[targets.iter().position(|t| t.physically_encrypted).unwrap()][0] ^= 1;
                assert!(native_image::plan_native_edp_4kn_image(
                    &spec,
                    &entropy,
                    &plan,
                    &options,
                    &serials,
                    &wrong_keys
                )
                .is_err());
            }
            // Full sparse disk execution and independent readback for every
            // mode, including the 4Kn candidate LCE, MBR and first FS block.
            if algorithm == FileKeyWrapMode::Sm4 {
                let path = portable_test_temp_file("edpcli-full-4kn-mode", "img");
                let _ = std::fs::remove_file(&path);
                assert!(native_image::export_native_edp_4kn_image(
                    std::path::Path::new("/dev/disk99"),
                    &spec,
                    &entropy,
                    &plan,
                    &options,
                    &serials,
                    &file_keys,
                )
                .is_err());
                native_image::export_native_edp_4kn_image(
                    &path, &spec, &entropy, &plan, &options, &serials, &file_keys,
                )
                .unwrap();
                assert_eq!(std::fs::metadata(&path).unwrap().len(), total * 4096);
                // Post-close/reopen verification must cover EVERY authored block,
                // not only the first FS sector, protocol header and LCE.
                native_image::verify_native_virtual_image(&path, &candidate).unwrap();
                let mut disk = std::fs::File::open(&path).unwrap();
                let mut observed = vec![0u8; 4096];
                disk.read_exact(&mut observed).unwrap();
                assert_eq!(observed, native.block(0).unwrap());
                disk.seek(SeekFrom::Start(lce_lba * 4096)).unwrap();
                disk.read_exact(&mut observed).unwrap();
                assert_eq!(observed, compat.data);
                let formatted = targets.iter().find(|t| options.choice(t.role).0).unwrap();
                disk.seek(SeekFrom::Start(formatted.geometry.start_sector * 4096))
                    .unwrap();
                disk.read_exact(&mut observed).unwrap();
                assert_eq!(
                    observed,
                    candidate
                        .writes
                        .iter()
                        .find(|write| { write.relative_lba == formatted.geometry.start_sector })
                        .unwrap()
                        .data
                );
                // Check the LCE opaque 1024B ciphertext suffix through a
                // separate reopened file path. This does not confer OEM trust.
                use std::io::Write as _;
                drop(disk);
                let mut tamper = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                tamper.seek(SeekFrom::Start(lce_lba * 4096 + 4095)).unwrap();
                tamper.write_all(&[compat.data[4095] ^ 0x40]).unwrap();
                tamper.sync_all().unwrap();
                drop(tamper);
                assert!(native_image::verify_native_virtual_image(&path, &candidate).is_err());
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
}

#[test]
fn native_4kn_mode1_combined_fat32_and_encrypted_exfat_format() {
    use crate::partition_transform::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };
    let total = 600_000u64;
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("991148").unwrap(),
        "USER",
        "DEPT",
        "MIXED 4Kn",
    )
    .unwrap();
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap();
    let key = [0x42u8; 16];
    let lce_lba = 550_000u64;
    let options = FormatOptions {
        share: true,
        encrypt: true,
        share_fs: FilesystemKind::Fat32,
        encrypt_fs: FilesystemKind::ExFat,
        ..FormatOptions::default()
    };
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(32, 1536, 128),
        crate::protocol::lba7_compat::Lba7CompatibilityExtentLayout {
            chs_bytes: lce_lba * 4096 + 0xe0000,
            start_byte_offset: lce_lba * 4096,
            start_lba: lce_lba,
            size_bytes: 4096,
            size_sectors: 1,
        },
        wrap_legacy_lba7_file_key(b"0000aaaa", [0u8; 8]),
        wrap_file_key(b"0000aaaa", key, FileKeyWrapMode::Aes128Ecb),
    )
    .unwrap()
    .with_filesystems(options.filesystems());
    let targets = plan.format_targets_native(4096).unwrap();
    let built = native_image::plan_native_edp_4kn_image(
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &plan,
        &options,
        &[1, 2],
        &[key, key],
    )
    .unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].filesystem, Some(FilesystemKind::Fat32));
    let fat = built
        .writes
        .iter()
        .find(|w| w.relative_lba == targets[0].geometry.start_sector)
        .unwrap();
    assert_eq!(
        crate::filesystem::detect_native_boot_sector(
            &fat.data,
            targets[0].geometry.sector_count(),
            4096
        )
        .unwrap(),
        Some(FilesystemKind::Fat32)
    );
    let encrypted = built
        .writes
        .iter()
        .find(|w| w.relative_lba == targets[1].geometry.start_sector)
        .unwrap();
    let decrypted = transform_native_sector_offline(
        NativePartitionDataCipher::AesCrossEcb,
        NativeCipherDirection::Decrypt,
        &encrypted.data,
        &key,
        targets[1].geometry.start_sector,
        4096,
    )
    .unwrap();
    assert_eq!(
        crate::filesystem::detect_native_boot_sector(
            &decrypted,
            targets[1].geometry.sector_count(),
            4096
        )
        .unwrap(),
        Some(FilesystemKind::ExFat)
    );
    let mut inconsistent_fs = options.clone();
    inconsistent_fs.share_fs = FilesystemKind::ExFat;
    assert!(native_image::plan_native_edp_4kn_image(
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &plan,
        &inconsistent_fs,
        &[1, 2],
        &[key, key],
    )
    .is_err());
    let mut unsupported_role = options.clone();
    unsupported_role.boot = true;
    assert!(native_image::plan_native_edp_4kn_image(
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &plan,
        &unsupported_role,
        &[1, 2],
        &[key, key],
    )
    .is_err());
}

#[test]
fn native_4kn_plain_exports_and_reopens_valid_native_exfat() {
    use std::io::{Read, Seek, SeekFrom};
    let partitions = vec![PlainPartitionRequest {
        start_lba: 2048,
        size: PlainPartitionSize::Fill,
        filesystem: FilesystemKind::ExFat,
        volume_label: "NATIVE-PLN".into(),
    }];
    let expected = native_image::plan_native_plain_image(64_000, 4096, &partitions).unwrap();
    let path = portable_test_temp_file("edpcli-4kn-plain", "img");
    let _ = std::fs::remove_file(&path);
    native_image::export_native_plain_image(&path, &expected).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let mut block = vec![0u8; 4096];
    file.read_exact(&mut block).unwrap();
    assert_eq!(&block[510..512], &[0x55, 0xaa]);
    assert_eq!(
        u32::from_le_bytes(block[454..458].try_into().unwrap()),
        2048
    );
    file.seek(SeekFrom::Start(2048 * 4096)).unwrap();
    file.read_exact(&mut block).unwrap();
    assert_eq!(
        crate::filesystem::detect_native_boot_sector(&block, 64_000 - 2048, 4096).unwrap(),
        Some(FilesystemKind::ExFat),
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn all_four_edp_modes_export_produced_512b_virtual_disks_with_independent_readback() {
    use crate::partition_transform::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };
    use std::io::{Read, Seek, SeekFrom};

    // Sparse temporary ordinary files only; no device discovery or physical I/O.
    let total = 4_300_000u64;
    let key = [0x42; 16];
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total).unwrap();
    let device_id = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "TESTUSER",
        "TESTDEPT",
        "EDP VIRTUAL",
    )
    .unwrap();
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap();
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = format_test_plan(mode, &key);
        let image = build_official_provision_protocol_image(
            &spec,
            &ProvisionEntropy::new([0x5a; 252]),
            &plan,
        )
        .unwrap();
        let targets = plan.format_targets().unwrap();
        let options = FormatOptions {
            boot: mode == OfficialPartitionMode::DefaultThreePartition
                || mode == OfficialPartitionMode::IntranetExtranetDualPartition,
            share: mode != OfficialPartitionMode::WholeDiskEncrypted,
            encrypt: mode != OfficialPartitionMode::IntranetExtranetDualPartition,
            ..FormatOptions::default()
        };
        let serials = vec![0x1234_5678; targets.len()];
        let choices = plan_format_targets_typed(&plan, &options, &serials, &key).unwrap();
        // Exercise the pure offline authoring entrypoint independently of
        // hardware-bound PreparedNewProvision construction.
        let offline = native_image::plan_native_edp_512_image(
            &spec,
            &ProvisionEntropy::new([0x5a; 252]),
            &plan,
            &options,
            &serials,
            &vec![key; serials.len()],
        )
        .unwrap();
        assert_eq!(offline.total_sectors, total);
        assert_eq!(offline.sector_bytes, SECTOR as u32);
        assert_eq!(offline.writes.last().unwrap().relative_lba, 0);
        let offline_path = portable_test_temp_file("edpcli-edp-independent-offline", "img");
        let _ = std::fs::remove_file(&offline_path);
        native_image::export_native_edp_512_image(
            &offline_path,
            &spec,
            &ProvisionEntropy::new([0x5a; 252]),
            &plan,
            &options,
            &serials,
            &vec![key; serials.len()],
        )
        .unwrap();
        let prepared = PreparedNewProvision {
            disk: 4,
            device_id: device_id.clone(),
            source_kind: DiskProvisionKind::Mode0,
            mode,
            algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
            force_change_password: false,
            pass_info_policy: PassInfoPolicy::default(),
            lce_start_lba: plan.lba7_compatibility_extent.start_lba,
            write_image: image,
            format_targets: choices,
            target_plan: None,
            source_metadata: None,
            before_pin: fixture_pin(),
            plan,
            expected_onlyid: "1402259934".into(),
            expected_serial_digest: None,
            expected_probe: probe.clone(),
            expected_lba3: None,
        };
        let path = portable_test_temp_file("edpcli-edp-native-virtual", "img");
        let _ = std::fs::remove_file(&path);
        assert!(export_sparse_provision_image(Path::new("/dev/disk99"), &prepared).is_err());
        export_sparse_provision_image(&path, &prepared).unwrap();
        let mut file = std::fs::File::open(&path).unwrap();
        let mut native_protocol = vec![0u8; 13 * SECTOR];
        file.read_exact(&mut native_protocol).unwrap();
        assert_eq!(
            native_protocol,
            prepared.write_image.metadata.as_bytes(),
            "{mode:?}"
        );
        let mut independent = std::fs::File::open(&offline_path).unwrap();
        let mut independent_protocol = vec![0u8; 13 * SECTOR];
        independent.read_exact(&mut independent_protocol).unwrap();
        assert_eq!(
            independent_protocol, native_protocol,
            "{mode:?} independent producer"
        );
        std::fs::remove_file(&offline_path).unwrap();
        let decoded = crate::provision::parse_existing_provision(
            &ProvisionImage::from_bytes(native_protocol).unwrap(),
            &device_id,
            total,
        )
        .unwrap()
        .expect("generated 512B EDPF must independently parse");
        assert_eq!(decoded.profile.source_mode, mode);
        for (&lba, expected) in &prepared.write_image.patch {
            file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))
                .unwrap();
            let mut actual = [0u8; SECTOR];
            file.read_exact(&mut actual).unwrap();
            assert_eq!(actual.as_slice(), expected, "{mode:?} protocol LBA{lba}");
        }
        for choice in prepared
            .format_targets
            .iter()
            .filter(|choice| choice.selected)
        {
            let lba = choice.target.geometry.start_sector;
            file.seek(SeekFrom::Start(lba * SECTOR as u64)).unwrap();
            let mut raw = [0u8; SECTOR];
            file.read_exact(&mut raw).unwrap();
            let expected_raw = choice.prepared_image.as_ref().unwrap().image.sectors()[&0];
            let expected_plain = choice.verification_image.as_ref().unwrap().sectors()[&0];
            assert_eq!(raw, expected_raw, "{mode:?} formatted LBA{lba}");
            if choice.target.physically_encrypted {
                assert_ne!(raw, expected_plain);
                let decoded = transform_native_sector_offline(
                    NativePartitionDataCipher::Sm4Ecb,
                    NativeCipherDirection::Decrypt,
                    &raw,
                    &key,
                    lba,
                    SECTOR as u32,
                )
                .unwrap();
                assert_eq!(decoded, expected_plain, "{mode:?} decrypted LBA{lba}");
            } else {
                assert_eq!(raw, expected_plain, "{mode:?} unencrypted LBA{lba}");
            }
        }
        std::fs::remove_file(&path).unwrap();
    }
}

#[test]
fn format_hardware_gate_rejects_changed_serial_probe_capacity_and_device_id() {
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x3535),
        pid: Some(0x6300),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "aigo".into(),
            product: "U335".into(),
            revision: "PMAP".into(),
        }),
    };
    let total = 16_777_216;
    let device_id = TargetIdentity::from_probe(&probe, total)
        .unwrap()
        .device_id()
        .to_string();
    let check =
        |fresh: &crate::platform::HardwareProbe, capacity, serial: Option<&str>, id: &str| {
            let digest = super::super::media_identity::serial_digest_evidence(Some("SERIAL-1"));
            verify_format_hardware(
                &probe,
                total,
                id,
                digest.sha256.as_deref(),
                fresh,
                capacity,
                serial,
            )
        };
    assert!(check(&probe, total, Some("SERIAL-1"), &device_id).is_ok());
    assert!(check(&probe, total, Some("SERIAL-2"), &device_id).is_err());
    assert!(check(&probe, total, None, &device_id).is_err());
    assert!(check(&probe, total + 1, Some("SERIAL-1"), &device_id).is_err());
    assert!(check(&probe, total, Some("SERIAL-1"), "disk&ven_other&prod_other").is_err());
    let mut changed = probe.clone();
    changed.vid = Some(0x0951);
    assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
    let mut changed = probe.clone();
    changed.inquiry.as_mut().unwrap().revision = "DIFF".into();
    assert!(check(&changed, total, Some("SERIAL-1"), &device_id).is_err());
}

fn protocol_readback_fixture() -> (PreparedNewProvision, MemoryDev) {
    let total = 16_777_216u64;
    let probe = crate::platform::HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: crate::platform::NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(crate::platform::InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total).unwrap();
    let device_id = target.device_id().to_string();
    let spec = ProvisionSpec::new(
        target,
        ProvisionMetadata::new(
            OnlyId::parse("1402259934").unwrap(),
            "USER06",
            "江苏省电力有限公司",
            "江苏电力!SAFE6",
        )
        .unwrap(),
        ProvisionProfile::canonical_v1(),
    )
    .unwrap();
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(32, 64, 128),
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total, 512).unwrap(),
        wrap_legacy_lba7_file_key(b"0000aaaa", [0x7d; 8]),
        wrap_file_key(b"0000aaaa", [0x42; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    let write_image =
        build_official_provision_protocol_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan)
            .unwrap();
    let dev = MemoryDev {
        sectors: write_image.patch.clone(),
        fail_at: None,
    };
    let prepared = PreparedNewProvision {
        disk: 4,
        device_id,
        source_kind: DiskProvisionKind::Mode0,
        mode: plan.mode,
        algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
        force_change_password: false,
        pass_info_policy: PassInfoPolicy::default(),
        lce_start_lba: plan.lba7_compatibility_extent.start_lba,
        write_image,
        format_targets: vec![],
        target_plan: None,
        source_metadata: None,
        before_pin: fixture_pin(),
        plan,
        expected_onlyid: "1402259934".into(),
        expected_serial_digest: None,
        expected_probe: probe,
        expected_lba3: Some([0; SECTOR]),
    };
    (prepared, dev)
}

#[test]
fn protocol_readback_gate_rejects_changed_onlyid_and_layout() {
    let (prepared, mut dev) = protocol_readback_fixture();
    verify_protocol_readback(&mut dev, &prepared).unwrap();
    dev.sectors.get_mut(&4).unwrap()[4] ^= 1;
    assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
    dev.sectors
        .insert(4, prepared.write_image.patch[&4].clone());
    dev.sectors.get_mut(&12).unwrap()[0] ^= 1;
    assert!(verify_protocol_readback(&mut dev, &prepared).is_err());
}

#[test]
fn lce_readback_gate_reparses_pointer_and_decrypts_all_six_sectors() {
    let (prepared, mut dev) = protocol_readback_fixture();
    verify_lce_readback(&mut dev, &prepared).unwrap();

    let tampered_lba = u32::try_from(prepared.lce_start_lba + 3).unwrap();
    dev.sectors.get_mut(&tampered_lba).unwrap()[17] ^= 1;
    let error = verify_lce_readback(&mut dev, &prepared).unwrap_err();
    assert!(error.msg.contains("gold plaintext"));

    dev.sectors.insert(
        tampered_lba,
        prepared.write_image.patch[&tampered_lba].clone(),
    );
    let mut wrong_geometry = prepared.clone();
    wrong_geometry.lce_start_lba += 1;
    let error = verify_lce_readback(&mut dev, &wrong_geometry).unwrap_err();
    assert!(error.msg.contains("LCE 几何"));
}

#[test]
fn lce_readback_gate_rejects_corrupted_final_lba7_pointer_table() {
    let (prepared, mut dev) = protocol_readback_fixture();
    dev.sectors.get_mut(&7).unwrap()[0] ^= 1;
    let error = verify_lce_readback(&mut dev, &prepared).unwrap_err();
    assert!(error.msg.contains("最终 LBA7"));
}

#[test]
fn writable_filesystem_validation_only_applies_to_selected_format_actions() {
    let key = [0x42; 16];
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        for readonly_fs in [FilesystemKind::Ntfs, FilesystemKind::Fat12] {
            let plan = format_test_plan(mode, &key).with_filesystems(
                crate::provision::OfficialPartitionFilesystems::all(readonly_fs),
            );
            let targets = plan.format_targets().unwrap();
            let serials = vec![1; targets.len()];
            let choices =
                plan_format_targets_typed(&plan, &FormatOptions::default(), &serials, &key)
                    .expect("unselected regions need no writable filesystem writer");
            assert!(choices
                .iter()
                .all(|choice| !choice.selected && choice.prepared_image.is_none()));
            for target in targets.iter().filter(|target| target.format_capable) {
                let mut options = FormatOptions::default();
                match target.role {
                    PartitionRole::Boot => options.boot = true,
                    PartitionRole::Share | PartitionRole::BootShareCombined => options.share = true,
                    PartitionRole::Encrypt => options.encrypt = true,
                    PartitionRole::CompatibilityReserve => unreachable!(),
                }
                assert!(
                    plan_format_targets_typed(&plan, &options, &serials, &key).is_err(),
                    "{mode:?}: {:?} must reject formatting {readonly_fs:?}",
                    target.role
                );
            }
        }
    }
    let plan = format_test_plan(OfficialPartitionMode::BootShareCombined, &key).with_filesystems(
        crate::provision::OfficialPartitionFilesystems {
            boot: FilesystemKind::Ntfs,
            share: FilesystemKind::Ntfs,
            encrypt: FilesystemKind::ExFat,
        },
    );
    let choices = plan_format_targets_typed(
        &plan,
        &FormatOptions {
            encrypt: true,
            ..FormatOptions::default()
        },
        &[1, 2],
        &key,
    )
    .expect("preserving NTFS must not prevent formatting a separate exFAT region");
    assert!(!choices[0].selected && choices[0].prepared_image.is_none());
    assert!(choices[1].selected && choices[1].prepared_image.is_some());
}

#[test]
fn non_sms4_application_prepare_is_rejected_before_any_device_access() {
    // Use a fake disk number that must never be probed. Rejection is an
    // application API invariant, not merely a TUI form constraint.
    let mut request = OfficialProvisionRequest {
        target: ProvisionTarget::Official(OfficialPartitionMode::DefaultThreePartition),
        algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
        boot_start_lba: None,
        share_start_lba: None,
        encrypt_start_lba: None,
        boot_mib: None,
        boot_sectors: None,
        share_mib: None,
        share_sectors: None,
        encrypt_mib: None,
        encrypt_sectors: None,
        label_id: String::new(),
        user: String::new(),
        dept: String::new(),
        label: String::new(),
        lba8_identity: crate::provision::Lba8Identity::default(),
        key_domains: KeyDomainSecrets::default(),
        volume_label: String::new(),
        format: FormatOptions::default(),
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
    };
    for (algorithm, crypt, mode) in [
        (crate::provision::OfficialLabelAlgorithm::Aes, 1, 1),
        (crate::provision::OfficialLabelAlgorithm::AesCross, 2, 3),
    ] {
        request.algorithm = algorithm;
        let mut dev = MemoryDev::default();
        let failure = prepare_target_provision(
            &crate::platform::system::SysRunner,
            u32::MAX,
            &request,
            &mut dev,
        )
        .unwrap_err();
        assert_eq!(failure.code, EXIT_TARGET);
        assert!(
            failure.msg.contains(&format!("crypt={crypt}")),
            "{}",
            failure.msg
        );
        assert!(
            failure.msg.contains(&format!("EncryptMode={mode}")),
            "{}",
            failure.msg
        );
        assert!(dev.sectors.is_empty(), "rejected request must not write");
    }
}

#[test]
fn non_sms4_prepared_plan_cannot_reach_device_commit_even_when_mutated() {
    let (mut prepared, mut dev) = protocol_readback_fixture();
    let before = dev.sectors.clone();
    for algorithm in [
        crate::provision::OfficialLabelAlgorithm::Aes,
        crate::provision::OfficialLabelAlgorithm::AesCross,
    ] {
        prepared.algorithm = algorithm;
        let failure = super::commit::commit_new_provision_with_progress(
            &crate::platform::system::SysRunner,
            &mut dev,
            &prepared,
            &mut |_, _, _| panic!("rejected plan must not announce device IO"),
        )
        .unwrap_err();
        assert_eq!(failure.code, EXIT_TARGET);
        assert!(failure.msg.contains("仅认证SMS4写入"));
        assert_eq!(dev.sectors, before, "no writes can be performed");
    }
}

#[test]
fn application_size_errors_use_the_current_partition_name() {
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
    ] {
        let mut request = OfficialProvisionRequest {
            target: ProvisionTarget::Official(mode),
            algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
            boot_start_lba: None,
            share_start_lba: None,
            encrypt_start_lba: None,
            boot_mib: None,
            boot_sectors: None,
            share_mib: Some(1),
            share_sectors: Some(1),
            encrypt_mib: None,
            encrypt_sectors: None,
            label_id: "1".into(),
            user: String::new(),
            dept: String::new(),
            label: String::new(),
            lba8_identity: crate::provision::Lba8Identity::default(),
            key_domains: KeyDomainSecrets::default(),
            volume_label: "启动区".into(),
            format: FormatOptions::default(),
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
        };
        let region = if mode == OfficialPartitionMode::BootShareCombined {
            "二合一区"
        } else {
            "交换区"
        };
        assert!(sizes(&request, mode).unwrap_err().msg.contains(region));
        request.share_mib = None;
        request.share_sectors = Some(0);
        assert!(sizes(&request, mode).unwrap_err().msg.contains(region));
    }
}
