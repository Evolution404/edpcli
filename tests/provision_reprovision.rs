use edpcli::{
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
    protocol::edpf::EdpPartitionType,
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        decide_partition_action, generate_official_image, parse_existing_provision,
        prefill_for_target_mode, wrap_file_key, wrap_legacy_lba7_file_key, CapacityInput,
        CapacityInputMode, CapacitySource, ExistingPartition, ExistingProvisionProfile,
        FileKeyWrapMode, OfficialFilesystemFormat, OfficialPartitionMode, OfficialPartitionSizes,
        OfficialProvisionPlan, OnlyId, PartitionAction, PartitionRole, ProvisionEntropy,
        ProvisionMetadata, ProvisionProfile, ProvisionSpec, QuickCapacityUnit, TargetIdentity,
        OFFICIAL_PARTITION_START_SECTOR,
    },
};

const SECTOR_SIZE: u64 = 512;
const MIB_SECTORS: u64 = 2048;

fn part(
    role: PartitionRole,
    partition_type: EdpPartitionType,
    start_lba: u64,
    sector_count: u64,
    encrypted: bool,
) -> ExistingPartition {
    ExistingPartition {
        role,
        partition_type,
        start_lba,
        sector_count,
        physically_encrypted: encrypted,
        filesystem: Some(match role {
            PartitionRole::Boot => OfficialFilesystemFormat::Fat16,
            PartitionRole::CompatibilityReserve => OfficialFilesystemFormat::ExFat,
            _ => OfficialFilesystemFormat::ExFat,
        }),
    }
}

#[test]
fn capacity_input_keeps_sector_canonical_for_quick_and_exact_values() {
    let quick =
        CapacityInput::from_quick(1024, QuickCapacityUnit::MiB, CapacitySource::SystemDefault)
            .unwrap();
    assert_eq!(quick.mode(), CapacityInputMode::Quick);
    assert_eq!(quick.sectors(), 1024 * MIB_SECTORS);

    let exact = CapacityInput::from_exact(2_097_000, CapacitySource::ExistingPartition).unwrap();
    assert_eq!(exact.mode(), CapacityInputMode::Exact);
    assert_eq!(exact.sectors(), 2_097_000);
    assert_eq!(exact.whole_mib(), None);

    let switched = quick.to_exact();
    assert_eq!(switched.mode(), CapacityInputMode::Exact);
    assert_eq!(switched.sectors(), quick.sectors());
}

#[test]
fn preserve_exact_requires_identical_semantics_geometry_and_physical_state() {
    let source = part(
        PartitionRole::Encrypt,
        EdpPartitionType::Encrypt,
        6_291_457,
        2_097_000,
        true,
    );
    let same = source.as_target();
    assert_eq!(
        decide_partition_action(Some(&source), &same),
        PartitionAction::PreserveExact
    );

    let mut moved = same;
    moved.start_lba += 1;
    assert_eq!(
        decide_partition_action(Some(&source), &moved),
        PartitionAction::Rebuild
    );

    let mut resized = same;
    resized.sector_count += 1;
    assert_eq!(
        decide_partition_action(Some(&source), &resized),
        PartitionAction::Rebuild
    );

    let mut different_role = same;
    different_role.role = PartitionRole::Share;
    assert_eq!(
        decide_partition_action(Some(&source), &different_role),
        PartitionAction::Rebuild
    );
}

#[test]
fn plain_mode0_prefill_uses_official_boot_one_gib_encrypt_and_remaining_share() {
    let usable_end = OFFICIAL_PARTITION_START_SECTOR + 40_000_000;
    let prefill = prefill_for_target_mode(
        None,
        OfficialPartitionMode::DefaultThreePartition,
        usable_end,
        SECTOR_SIZE,
    )
    .unwrap();

    assert_eq!(prefill.boot.as_ref().unwrap().sectors(), 20_417);
    assert_eq!(
        prefill.encrypt.as_ref().unwrap().sectors(),
        1024 * MIB_SECTORS
    );
    let share = prefill.share.as_ref().unwrap();
    assert_eq!(share.mode(), CapacityInputMode::Quick);
    assert_eq!(share.sectors() % MIB_SECTORS, 0);
    assert!(share.sectors() > 0);
}

#[test]
fn mode0_to_mode1_prefill_preserves_exact_encrypt_geometry_even_when_not_whole_mib() {
    let encrypt_start = 6_291_457;
    let encrypt_sectors = 2_097_000;
    let source = ExistingProvisionProfile {
        source_mode: OfficialPartitionMode::DefaultThreePartition,
        partitions: vec![
            part(
                PartitionRole::Boot,
                EdpPartitionType::Boot,
                63,
                20_417,
                false,
            ),
            part(
                PartitionRole::Share,
                EdpPartitionType::Share,
                20_480,
                encrypt_start - 20_480,
                true,
            ),
            part(
                PartitionRole::Encrypt,
                EdpPartitionType::Encrypt,
                encrypt_start,
                encrypt_sectors,
                true,
            ),
        ],
    };

    let prefill = prefill_for_target_mode(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        encrypt_start + encrypt_sectors + 100_000,
        SECTOR_SIZE,
    )
    .unwrap();

    let combined = prefill.share.as_ref().unwrap();
    let encrypt = prefill.encrypt.as_ref().unwrap();
    assert_eq!(combined.mode(), CapacityInputMode::Exact);
    assert_eq!(
        combined.sectors(),
        encrypt_start - OFFICIAL_PARTITION_START_SECTOR
    );
    assert_eq!(encrypt.mode(), CapacityInputMode::Exact);
    assert_eq!(encrypt.sectors(), encrypt_sectors);
    assert_eq!(encrypt.whole_mib(), None);

    let targets = prefill.target_partitions(SECTOR_SIZE).unwrap();
    let target_encrypt = targets
        .iter()
        .find(|target| target.role == PartitionRole::Encrypt)
        .unwrap();
    let source_encrypt = source.partition(PartitionRole::Encrypt).unwrap();
    assert_eq!(target_encrypt.start_lba, source_encrypt.start_lba);
    assert_eq!(target_encrypt.sector_count, source_encrypt.sector_count);
    assert_eq!(
        decide_partition_action(Some(source_encrypt), target_encrypt),
        PartitionAction::PreserveExact
    );
}

#[test]
fn mode1_to_mode0_prefill_keeps_encrypt_start_by_deriving_exact_share() {
    let encrypt_start = 8_000_123;
    let encrypt_sectors = 1_500_001;
    let source = ExistingProvisionProfile {
        source_mode: OfficialPartitionMode::BootShareCombined,
        partitions: vec![
            part(
                PartitionRole::BootShareCombined,
                EdpPartitionType::Share,
                63,
                encrypt_start - 63,
                false,
            ),
            part(
                PartitionRole::Encrypt,
                EdpPartitionType::Encrypt,
                encrypt_start,
                encrypt_sectors,
                true,
            ),
        ],
    };

    let prefill = prefill_for_target_mode(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        encrypt_start + encrypt_sectors + 100_000,
        SECTOR_SIZE,
    )
    .unwrap();

    assert_eq!(prefill.boot.as_ref().unwrap().sectors(), 20_417);
    assert_eq!(
        prefill.share.as_ref().unwrap().sectors(),
        encrypt_start - 63 - 20_417
    );
    assert_eq!(prefill.encrypt.as_ref().unwrap().sectors(), encrypt_sectors);

    let targets = prefill.target_partitions(SECTOR_SIZE).unwrap();
    let target_encrypt = targets
        .iter()
        .find(|target| target.role == PartitionRole::Encrypt)
        .unwrap();
    assert_eq!(target_encrypt.start_lba, encrypt_start);
}

#[test]
fn same_mode_prefill_uses_exact_source_partition_sizes() {
    let source = ExistingProvisionProfile {
        source_mode: OfficialPartitionMode::IntranetExtranetDualPartition,
        partitions: vec![
            part(
                PartitionRole::Boot,
                EdpPartitionType::Boot,
                63,
                1_048_577,
                false,
            ),
            part(
                PartitionRole::Share,
                EdpPartitionType::Share,
                1_048_640,
                4_000_003,
                true,
            ),
        ],
    };

    let prefill = prefill_for_target_mode(
        Some(&source),
        OfficialPartitionMode::IntranetExtranetDualPartition,
        10_000_000,
        SECTOR_SIZE,
    )
    .unwrap();

    assert_eq!(
        prefill.boot.as_ref().unwrap().mode(),
        CapacityInputMode::Exact
    );
    assert_eq!(prefill.boot.as_ref().unwrap().sectors(), 1_048_577);
    assert_eq!(prefill.share.as_ref().unwrap().sectors(), 4_000_003);
}

fn generated_source(mode: OfficialPartitionMode) -> (edpcli::provision::ProvisionImage, String) {
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, 16_777_216).unwrap();
    let did = target.device_id().to_string();
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
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        ),
        wrap_file_key(b"ProofPass1!", [0x14; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    (
        generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan).unwrap(),
        did,
    )
}

#[test]
fn existing_profile_decodes_all_four_modes_and_keeps_partition_owned_key_fields() {
    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let (image, did) = generated_source(mode);
        let parsed = parse_existing_provision(&image, &did, 16_777_216)
            .unwrap()
            .unwrap();
        assert_eq!(parsed.profile.source_mode, mode);
        assert_eq!(parsed.records.len(), mode.partition_types().len());
        for (part, record) in parsed.profile.partitions.iter().zip(&parsed.records) {
            assert_eq!(part.start_lba, record.lba12.start_sector);
            assert_eq!(part.sector_count, record.lba12.partition_size / 512);
            if part.physically_encrypted {
                assert_ne!(record.lba12.file_key_crc, 0);
                assert_ne!(record.lba12.encrypted_file_key, [0; 16]);
            }
            assert!(
                part.filesystem.is_none(),
                "metadata alone cannot prove filesystem compatibility"
            );
        }
        assert!(parse_existing_provision(&image, "wrong-device", 16_777_216)
            .unwrap()
            .is_none());
    }
}

#[test]
fn shrinking_combined_keeps_encrypt_anchor_and_gap_but_overlap_fails_closed() {
    let source = ExistingProvisionProfile {
        source_mode: OfficialPartitionMode::DefaultThreePartition,
        partitions: vec![
            part(
                PartitionRole::Boot,
                EdpPartitionType::Boot,
                63,
                20_417,
                false,
            ),
            part(
                PartitionRole::Share,
                EdpPartitionType::Share,
                20_480,
                6_000_000,
                true,
            ),
            part(
                PartitionRole::Encrypt,
                EdpPartitionType::Encrypt,
                6_020_480,
                2_097_000,
                true,
            ),
        ],
    };
    let mut prefill = prefill_for_target_mode(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        9_000_000,
        512,
    )
    .unwrap();
    let anchored = prefill.encrypt_start_lba.unwrap();
    prefill.share =
        Some(CapacityInput::from_exact(anchored - 63 - 10, CapacitySource::UserEdited).unwrap());
    let targets = prefill.target_partitions(512).unwrap();
    assert_eq!(targets[1].start_lba, anchored);
    assert_eq!(
        edpcli::provision::validate_target_geometry(&targets, prefill.usable_end_lba).unwrap(),
        10 + (9_000_000 - anchored - 2_097_000)
    );
    assert_eq!(
        decide_partition_action(source.partition(PartitionRole::Encrypt), &targets[1]),
        PartitionAction::PreserveExact
    );
    prefill.share =
        Some(CapacityInput::from_exact(anchored - 63 + 1, CapacitySource::UserEdited).unwrap());
    assert!(prefill
        .target_partitions(512)
        .unwrap_err()
        .contains("overlap"));
    assert_eq!(prefill.encrypt_start_lba, Some(anchored));
}
