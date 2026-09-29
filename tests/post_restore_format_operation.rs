//! Production post-restore format authorization with a sparse virtual sector device.
#![cfg(target_os = "macos")]

use crate::common::{self, load_disk_image, netac_runner};
use edpcli::application::media_identity::MediaIdentityPin;
use edpcli::application::post_restore::{
    assess_partitions_readonly, format_partition_on_disk, MetadataRestoreOutcome,
    MetadataRestoreReport, PartitionFormatRequest, PostRestorePartitionState,
};
use edpcli::application::Prompter;
use edpcli::common::SECTOR;
use edpcli::diskio::SectorDev;
use edpcli::edpb::ManifestPartition;
use edpcli::filesystem::FilesystemKind;
use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

const TOTAL: u64 = 122_880_000;

struct SparseFormatDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    writes: Vec<u32>,
    syncs: usize,
    swap_on_reopen: bool,
    fail_write: bool,
    fail_write_once_after: Option<usize>,
}

impl SparseFormatDev {
    fn new() -> Self {
        let mut mbr = vec![0u8; SECTOR];
        for (slot, start) in [(0, 2_048u32), (1, 150_000u32)] {
            let base = 446 + slot * 16;
            mbr[base + 4] = 0x07;
            mbr[base + 8..base + 12].copy_from_slice(&start.to_le_bytes());
            mbr[base + 12..base + 16].copy_from_slice(&100_000u32.to_le_bytes());
        }
        mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
        Self {
            sectors: BTreeMap::from([(0, mbr)]),
            writes: Vec::new(),
            syncs: 0,
            swap_on_reopen: false,
            fail_write: false,
            fail_write_once_after: None,
        }
    }
}

impl SectorDev for SparseFormatDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0u8; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, bytes: &[u8]) -> io::Result<()> {
        if self.fail_write {
            return Err(io::Error::other("injected format failure"));
        }
        if self
            .fail_write_once_after
            .is_some_and(|after| self.writes.len() >= after)
        {
            self.fail_write_once_after = None;
            return Err(io::Error::other("injected one-shot write failure"));
        }
        self.writes.push(lba);
        self.sectors.insert(lba, bytes.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.syncs += 1;
        Ok(())
    }

    fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
        if self.swap_on_reopen {
            self.sectors.get_mut(&0).unwrap()[454..458].copy_from_slice(&4_096u32.to_le_bytes());
        }
        Ok(())
    }
}

struct Confirm(bool);

impl Prompter for Confirm {
    fn prompt_line(&mut self, _message: &str) -> String {
        String::new()
    }

    fn confirm_yes(&mut self, _message: &str) -> bool {
        self.0
    }
}

fn partitions() -> Vec<ManifestPartition> {
    [(1, 2_048), (2, 150_000)]
        .into_iter()
        .map(|(index, start_lba)| ManifestPartition {
            index,
            role: Some("mbr_primary".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba,
            sector_count: 100_000,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        })
        .collect()
}

fn fixture(dev: &mut SparseFormatDev) -> (common::FakeRunner, MetadataRestoreOutcome) {
    let runner = netac_runner(6);
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    let parts = partitions();
    let assessment = assess_partitions_readonly(dev, "plain", "", TOTAL, &parts).unwrap();
    let outcome = MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.plain.partition_table.0".into()],
        },
        assessment,
        partitions: parts,
        device_state: "plain".into(),
        device_id: String::new(),
        total_sectors: TOTAL,
        format_target_pin: Some(
            edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin),
        ),
    };
    (runner, outcome)
}

fn request() -> PartitionFormatRequest {
    PartitionFormatRequest {
        partition_index: 1,
        filesystem: FilesystemKind::ExFat,
    }
}

fn run(
    runner: &common::FakeRunner,
    dev: &mut SparseFormatDev,
    confirm: bool,
    outcome: &MetadataRestoreOutcome,
) -> edpcli::application::post_restore::PostRestoreFormatResult {
    format_partition_on_disk(
        runner,
        6,
        dev,
        &mut Confirm(confirm),
        outcome.format_target_pin.as_ref().unwrap(),
        outcome,
        &request(),
        "恢复卷",
        0x1234_5678,
    )
}

#[test]
fn unconfirmed_format_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    assert!(run(&runner, &mut dev, false, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn non_needs_format_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.assessment.partitions[0].state = PostRestorePartitionState::Usable;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn physical_identity_mismatch_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.format_target_pin.as_mut().unwrap().vid = Some(0xffff);
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn geometry_mismatch_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.partitions[0].sector_count += 1;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn reopen_media_swap_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    dev.swap_on_reopen = true;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn formats_only_selected_partition_and_reassesses_usable() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    let result = run(&runner, &mut dev, true, &outcome);
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert!(!dev.writes.is_empty());
    assert!(dev.writes.iter().all(|lba| (2_048..102_048).contains(lba)));
    assert!(dev.syncs >= 1);
    let after =
        assess_partitions_readonly(&mut dev, "plain", "", TOTAL, &outcome.partitions).unwrap();
    assert_eq!(after.partitions[0].state, PostRestorePartitionState::Usable);
    assert_eq!(
        after.partitions[1].state,
        PostRestorePartitionState::NeedsFormat
    );
}

#[test]
fn format_failure_preserves_verified_metadata_restore_result() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    dev.fail_write_once_after = Some(2);
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(outcome.report.metadata_restored);
    assert!(outcome.report.readback_verified);
    assert!(!dev.writes.is_empty());
    assert!(dev.writes.iter().all(|lba| (2_048..102_048).contains(lba)));
    for lba in &dev.writes {
        assert_eq!(dev.sectors.get(lba).unwrap(), &vec![0u8; SECTOR]);
    }
}

#[test]
fn encrypted_edp_partition_cannot_enter_plaintext_format() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let protocol = image[..13 * SECTOR].to_vec();
    let provision = edpcli::provision::ProvisionImage::from_bytes(protocol.clone()).unwrap();
    let parsed = edpcli::provision::parse_existing_provision(
        &provision,
        "disk&ven_netac&prod_onlydisk",
        TOTAL,
    )
    .unwrap()
    .unwrap();
    let Some((index, partition)) = parsed
        .profile
        .partitions
        .iter()
        .enumerate()
        .find(|(index, _)| parsed.records[*index].lba12.need_encrypt != 0)
    else {
        eprintln!("跳过: 夹具没有加密分区");
        return;
    };
    let manifest = ManifestPartition {
        index: index as u32 + 1,
        role: Some("encrypt".into()),
        partition_type: Some("edp:4".into()),
        start_lba: partition.start_lba,
        sector_count: partition.sector_count,
        filesystem_hint: Some("exfat".into()),
        volume_label_hint: None,
    };
    let mut dev = SparseFormatDev::new();
    for lba in 0..13 {
        dev.sectors.insert(
            lba as u32,
            protocol[lba * SECTOR..(lba + 1) * SECTOR].to_vec(),
        );
    }
    let runner = netac_runner(6);
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, &mut dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    let assessment = edpcli::application::post_restore::PostRestoreAssessment {
        partitions: vec![edpcli::application::post_restore::PostRestorePartition {
            index: manifest.index,
            role: manifest.role.clone(),
            start_lba: manifest.start_lba,
            sector_count: manifest.sector_count,
            filesystem_hint: manifest.filesystem_hint.clone(),
            detected_filesystem: None,
            requires_original_key: true,
            state: PostRestorePartitionState::NeedsFormat,
            detail: "fixture".into(),
        }],
        issues: Vec::new(),
    };
    let outcome = MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        },
        assessment,
        partitions: vec![manifest],
        device_state: "edp".into(),
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        total_sectors: TOTAL,
        format_target_pin: Some(
            edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin),
        ),
    };
    let result = format_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &PartitionFormatRequest {
            partition_index: index as u32 + 1,
            filesystem: FilesystemKind::ExFat,
        },
        "恢复卷",
        0x1234_5678,
    );
    assert!(result.result.is_err());
    assert!(dev.writes.is_empty());
}

fn encrypted_mode0_fixture() -> (
    common::FakeRunner,
    SparseFormatDev,
    MetadataRestoreOutcome,
    Vec<u8>,
) {
    use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
    use edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry;
    use edpcli::provision::{
        generate_official_image, parse_existing_provision, wrap_file_key,
        wrap_legacy_lba7_file_key, FileKeyWrapMode, OfficialPartitionMode, OfficialPartitionSizes,
        OfficialProvisionPlan, OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile,
        ProvisionSpec, TargetIdentity,
    };

    const TEST_TOTAL: u64 = 16_777_216;
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, TEST_TOTAL).unwrap();
    let device_id = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "USER06",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap();
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(b"0000aaaa", [1; 8]),
        wrap_file_key(b"0000aaaa", [2; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        1,
        wrap_legacy_lba7_file_key(b"SharePass1!", [0x31; 8]),
        wrap_file_key(b"SharePass1!", [0x41; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        2,
        wrap_legacy_lba7_file_key(b"EncryptPass1!", [0x51; 8]),
        wrap_file_key(b"EncryptPass1!", [0x61; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    let image = generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan).unwrap();
    let protocol = image.as_bytes().to_vec();
    let parsed = parse_existing_provision(&image, &device_id, TEST_TOTAL)
        .unwrap()
        .unwrap();
    let partitions = parsed
        .profile
        .partitions
        .iter()
        .enumerate()
        .map(|(index, partition)| ManifestPartition {
            index: index as u32 + 1,
            role: Some(format!("{:?}", partition.role)),
            partition_type: Some(format!("edp:{:?}", partition.partition_type)),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        })
        .collect::<Vec<_>>();
    let mut dev = SparseFormatDev::new();
    for lba in 0..13 {
        dev.sectors.insert(
            lba as u32,
            protocol[lba * SECTOR..(lba + 1) * SECTOR].to_vec(),
        );
    }
    let mut runner = netac_runner(6);
    runner.canned.insert(
        "diskutil info -plist disk6".into(),
        common::diskutil_info_plist((TEST_TOTAL * SECTOR as u64) as i64),
    );
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, &mut dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    let assessment =
        assess_partitions_readonly(&mut dev, "edp", &device_id, TEST_TOTAL, &partitions).unwrap();
    let outcome = MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        },
        assessment,
        partitions,
        device_state: "edp".into(),
        device_id,
        total_sectors: TEST_TOTAL,
        format_target_pin: Some(
            edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin),
        ),
    };
    (runner, dev, outcome, protocol)
}

#[test]
fn original_password_format_preserves_key_records_and_verifies_encrypted_boot() {
    use edpcli::application::post_restore::{
        assess_partitions_with_password_readonly, format_encrypted_partition_on_disk,
        EncryptedPostRestoreError,
    };
    use edpcli::provision::ExistingFileKeyError;

    let (runner, mut dev, outcome, protocol) = encrypted_mode0_fixture();
    let target = &outcome.partitions[2];
    let request = PartitionFormatRequest {
        partition_index: target.index,
        filesystem: FilesystemKind::ExFat,
    };
    assert_eq!(
        outcome.assessment.partitions[2].state,
        PostRestorePartitionState::PasswordRequired
    );
    let missing = format_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &request,
        None,
        "保密区",
        0x1234_5678,
    );
    assert_eq!(
        missing.result,
        Err(EncryptedPostRestoreError::FileKey(
            ExistingFileKeyError::PasswordRequired
        ))
    );
    let wrong = format_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &request,
        Some(b"wrong"),
        "保密区",
        0x1234_5678,
    );
    assert_eq!(
        wrong.result,
        Err(EncryptedPostRestoreError::FileKey(
            ExistingFileKeyError::PasswordMismatch
        ))
    );
    assert!(dev.writes.is_empty());
    let cancelled = format_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(false),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &request,
        Some(b"EncryptPass1!"),
        "保密区",
        0x1234_5678,
    );
    assert!(matches!(
        cancelled.result,
        Err(EncryptedPostRestoreError::Operation(_))
    ));
    assert!(dev.writes.is_empty());
    let success = format_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &request,
        Some(b"EncryptPass1!"),
        "保密区",
        0x1234_5678,
    );
    assert!(success.result.is_ok(), "{:?}", success.result);
    assert!(!dev.writes.is_empty());
    assert!(dev.writes.iter().all(|lba| {
        (target.start_lba..target.start_lba + target.sector_count).contains(&u64::from(*lba))
    }));
    for lba in 0..13 {
        assert_eq!(
            dev.sectors.get(&(lba as u32)).unwrap(),
            &protocol[lba * SECTOR..(lba + 1) * SECTOR]
        );
    }
    let after = assess_partitions_with_password_readonly(
        &mut dev,
        "edp",
        &outcome.device_id,
        outcome.total_sectors,
        &outcome.partitions,
        target.index,
        b"EncryptPass1!",
    )
    .unwrap();
    assert_eq!(after.partitions[2].state, PostRestorePartitionState::Usable);
}

#[test]
fn edp_plaintext_partition_uses_the_same_authorized_format_path() {
    let (runner, mut dev, outcome, protocol) = encrypted_mode0_fixture();
    let target = &outcome.partitions[0];
    assert!(!outcome.assessment.partitions[0].requires_original_key);
    assert_eq!(
        outcome.assessment.partitions[0].state,
        PostRestorePartitionState::NeedsFormat
    );
    let result = format_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &PartitionFormatRequest {
            partition_index: 1,
            filesystem: FilesystemKind::Fat16,
        },
        "启动区",
        0x1234_5678,
    );
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert!(dev.writes.iter().all(|lba| {
        (target.start_lba..target.start_lba + target.sector_count).contains(&u64::from(*lba))
    }));
    for lba in 0..13 {
        assert_eq!(
            dev.sectors.get(&(lba as u32)).unwrap(),
            &protocol[lba * SECTOR..(lba + 1) * SECTOR]
        );
    }
}

#[test]
fn corrupted_file_key_crc_is_typed_and_has_zero_writes() {
    use edpcli::application::post_restore::{
        format_encrypted_partition_on_disk, EncryptedPostRestoreError,
    };
    use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare};
    use edpcli::provision::ExistingFileKeyError;

    let (runner, mut dev, mut outcome, _) = encrypted_mode0_fixture();
    let crc = crc32_bare(outcome.device_id.as_bytes()).to_le_bytes();
    let mut plain = a6b0_full(dev.sectors.get(&12).unwrap(), &crc, 0);
    let offset = 2 * 0x60 + 0x34;
    plain[offset] ^= 1;
    dev.sectors.insert(12, a7f0_full(&plain, &crc, 0).to_vec());
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, &mut dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    outcome.format_target_pin =
        Some(edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin));
    let request = PartitionFormatRequest {
        partition_index: 3,
        filesystem: FilesystemKind::ExFat,
    };
    let result = format_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &request,
        Some(b"EncryptPass1!"),
        "保密区",
        0x1234_5678,
    );
    assert_eq!(
        result.result,
        Err(EncryptedPostRestoreError::FileKey(
            ExistingFileKeyError::FileKeyCrcMismatch
        ))
    );
    assert!(dev.writes.is_empty());
}

fn reinitialize_request() -> edpcli::application::post_restore::EncryptedPartitionReinitializeRequest
{
    edpcli::application::post_restore::EncryptedPartitionReinitializeRequest::new(
        3,
        b"FreshPass1!",
        b"FreshPass1!",
    )
    .unwrap()
}

fn run_reinitialize(
    runner: &common::FakeRunner,
    dev: &mut SparseFormatDev,
    confirm: bool,
    outcome: &MetadataRestoreOutcome,
) -> edpcli::application::post_restore::EncryptedPartitionReinitializeResult {
    edpcli::application::post_restore::reinitialize_encrypted_partition_on_disk(
        runner,
        6,
        dev,
        &mut Confirm(confirm),
        outcome.format_target_pin.as_ref().unwrap(),
        outcome,
        &reinitialize_request(),
        FilesystemKind::ExFat,
        "保密区",
        0x1234_5678,
    )
}

#[test]
fn reinitialize_requires_new_password_twice_and_a_distinct_password() {
    use edpcli::application::post_restore::EncryptedPartitionReinitializeRequest;
    assert!(EncryptedPartitionReinitializeRequest::new(3, b"", b"").is_err());
    assert!(EncryptedPartitionReinitializeRequest::new(3, b"one", b"two").is_err());
    let (runner, mut dev, outcome, _) = encrypted_mode0_fixture();
    let same =
        EncryptedPartitionReinitializeRequest::new(3, b"EncryptPass1!", b"EncryptPass1!").unwrap();
    let result = edpcli::application::post_restore::reinitialize_encrypted_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &same,
        FilesystemKind::ExFat,
        "保密区",
        0x1234_5678,
    );
    assert!(result.result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn reinitialize_safety_checks_prevent_all_writes() {
    let (runner, mut dev, outcome, _) = encrypted_mode0_fixture();
    assert!(run_reinitialize(&runner, &mut dev, false, &outcome)
        .result
        .is_err());
    assert!(dev.writes.is_empty());

    let (runner, mut dev, mut outcome, _) = encrypted_mode0_fixture();
    outcome.format_target_pin.as_mut().unwrap().vid = Some(0xffff);
    assert!(run_reinitialize(&runner, &mut dev, true, &outcome)
        .result
        .is_err());
    assert!(dev.writes.is_empty());

    let (runner, mut dev, mut outcome, _) = encrypted_mode0_fixture();
    outcome.partitions[2].sector_count += 1;
    assert!(run_reinitialize(&runner, &mut dev, true, &outcome)
        .result
        .is_err());
    assert!(dev.writes.is_empty());

    let (runner, mut dev, outcome, _) = encrypted_mode0_fixture();
    dev.swap_on_reopen = true;
    assert!(run_reinitialize(&runner, &mut dev, true, &outcome)
        .result
        .is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn reinitialize_replaces_only_selected_key_domain_and_filesystem() {
    use edpcli::application::post_restore::assess_partitions_with_password_readonly;
    use edpcli::provision::{parse_existing_provision, ProvisionImage};

    let (runner, mut dev, outcome, protocol) = encrypted_mode0_fixture();
    let before = parse_existing_provision(
        &ProvisionImage::from_bytes(protocol.clone()).unwrap(),
        &outcome.device_id,
        outcome.total_sectors,
    )
    .unwrap()
    .unwrap();
    let target = &outcome.partitions[2];
    let result = run_reinitialize(&runner, &mut dev, true, &outcome);
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert!(outcome.report.metadata_restored && outcome.report.readback_verified);
    assert!(dev.writes.iter().all(|lba| {
        *lba == 7
            || *lba == 12
            || (target.start_lba..target.start_lba + target.sector_count).contains(&u64::from(*lba))
    }));
    for lba in 0..13usize {
        if lba != 7 && lba != 12 {
            assert_eq!(
                dev.sectors.get(&(lba as u32)).unwrap(),
                &protocol[lba * SECTOR..(lba + 1) * SECTOR]
            );
        }
    }
    let mut updated_protocol = protocol;
    for lba in [7usize, 12] {
        updated_protocol[lba * SECTOR..(lba + 1) * SECTOR]
            .copy_from_slice(dev.sectors.get(&(lba as u32)).unwrap());
    }
    let after = parse_existing_provision(
        &ProvisionImage::from_bytes(updated_protocol).unwrap(),
        &outcome.device_id,
        outcome.total_sectors,
    )
    .unwrap()
    .unwrap();
    assert_eq!(before.profile, after.profile);
    assert_eq!(before.records[0..2], after.records[0..2]);
    assert!(after.records[2]
        .verified_file_key(Some(b"EncryptPass1!"))
        .is_err());
    assert!(after.records[2]
        .verified_file_key(Some(b"FreshPass1!"))
        .is_ok());
    let assessed = assess_partitions_with_password_readonly(
        &mut dev,
        "edp",
        &outcome.device_id,
        outcome.total_sectors,
        &outcome.partitions,
        3,
        b"FreshPass1!",
    )
    .unwrap();
    assert_eq!(
        assessed.partitions[2].state,
        PostRestorePartitionState::Usable
    );
}

#[test]
fn reinitialize_failure_does_not_change_metadata_restore_report() {
    let (runner, mut dev, outcome, protocol) = encrypted_mode0_fixture();
    dev.fail_write_once_after = Some(2);
    assert!(run_reinitialize(&runner, &mut dev, true, &outcome)
        .result
        .is_err());
    assert!(outcome.report.metadata_restored && outcome.report.readback_verified);
    for lba in 0..13usize {
        assert_eq!(
            dev.sectors.get(&(lba as u32)).unwrap(),
            &protocol[lba * SECTOR..(lba + 1) * SECTOR]
        );
    }
    assert_eq!(
        dev.sectors
            .get(&(outcome.partitions[2].start_lba as u32))
            .unwrap(),
        &vec![0u8; SECTOR]
    );
}
