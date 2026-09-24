use edpcli::{
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
    protocol::edpf::EdpPartitionType,
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        apply_target_geometry_overrides, decide_partition_action, generate_official_image,
        parse_existing_provision, prefill_for_target_mode, wrap_file_key,
        wrap_legacy_lba7_file_key, CapacityInput, CapacityInputMode, CapacitySource,
        DiskProvisionKind, ExistingPartition, ExistingProvisionProfile, FileKeyWrapMode,
        OfficialFilesystemFormat, OfficialPartitionMode, OfficialPartitionSizes,
        OfficialProvisionPlan, OnlyId, PartitionAction, PartitionRole, ProvisionEntropy,
        ProvisionMetadata, ProvisionProfile, ProvisionSpec, QuickCapacityUnit,
        TargetGeometryOverrides, TargetIdentity, TargetProvisionPlan,
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

fn matrix_source(mode: OfficialPartitionMode, usable_end: u64) -> ExistingProvisionProfile {
    let encrypt_start = 6_020_480;
    let encrypt_sectors = 2_097_153;
    let partitions = match mode {
        OfficialPartitionMode::DefaultThreePartition => vec![
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
        OfficialPartitionMode::BootShareCombined => vec![
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
        OfficialPartitionMode::WholeDiskEncrypted => vec![
            ExistingPartition {
                role: PartitionRole::CompatibilityReserve,
                partition_type: EdpPartitionType::Boot,
                start_lba: 63,
                sector_count: 63,
                physically_encrypted: false,
                filesystem: None,
            },
            part(
                PartitionRole::Encrypt,
                EdpPartitionType::Encrypt,
                126,
                usable_end - 126,
                true,
            ),
        ],
        OfficialPartitionMode::IntranetExtranetDualPartition => vec![
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
        ],
    };
    ExistingProvisionProfile {
        source_mode: mode,
        partitions,
    }
}

fn target_roles(mode: OfficialPartitionMode) -> &'static [PartitionRole] {
    match mode {
        OfficialPartitionMode::DefaultThreePartition => &[
            PartitionRole::Boot,
            PartitionRole::Share,
            PartitionRole::Encrypt,
        ],
        OfficialPartitionMode::BootShareCombined => {
            &[PartitionRole::BootShareCombined, PartitionRole::Encrypt]
        }
        OfficialPartitionMode::WholeDiskEncrypted => {
            &[PartitionRole::CompatibilityReserve, PartitionRole::Encrypt]
        }
        OfficialPartitionMode::IntranetExtranetDualPartition => {
            &[PartitionRole::Boot, PartitionRole::Share]
        }
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
fn prefill_matrix_covers_plain_and_all_four_by_four_transitions() {
    const M0: OfficialPartitionMode = OfficialPartitionMode::DefaultThreePartition;
    const M1: OfficialPartitionMode = OfficialPartitionMode::BootShareCombined;
    const M2: OfficialPartitionMode = OfficialPartitionMode::WholeDiskEncrypted;
    const M3: OfficialPartitionMode = OfficialPartitionMode::IntranetExtranetDualPartition;

    #[derive(Clone, Copy)]
    struct Case {
        name: &'static str,
        source: Option<OfficialPartitionMode>,
        target: OfficialPartitionMode,
        preserve: &'static [PartitionRole],
    }

    const NONE: &[PartitionRole] = &[];
    const BOOT_SHARE: &[PartitionRole] = &[PartitionRole::Boot, PartitionRole::Share];
    const ENCRYPT: &[PartitionRole] = &[PartitionRole::Encrypt];
    const M0_ALL: &[PartitionRole] = &[
        PartitionRole::Boot,
        PartitionRole::Share,
        PartitionRole::Encrypt,
    ];
    const M1_ALL: &[PartitionRole] = &[PartitionRole::BootShareCombined, PartitionRole::Encrypt];

    let cases = [
        Case {
            name: "plain→0",
            source: None,
            target: M0,
            preserve: NONE,
        },
        Case {
            name: "plain→1",
            source: None,
            target: M1,
            preserve: NONE,
        },
        Case {
            name: "plain→2",
            source: None,
            target: M2,
            preserve: NONE,
        },
        Case {
            name: "plain→3",
            source: None,
            target: M3,
            preserve: NONE,
        },
        Case {
            name: "0→0",
            source: Some(M0),
            target: M0,
            preserve: M0_ALL,
        },
        Case {
            name: "0→1",
            source: Some(M0),
            target: M1,
            preserve: ENCRYPT,
        },
        Case {
            name: "0→2",
            source: Some(M0),
            target: M2,
            preserve: ENCRYPT,
        },
        Case {
            name: "0→3",
            source: Some(M0),
            target: M3,
            preserve: BOOT_SHARE,
        },
        Case {
            name: "1→0",
            source: Some(M1),
            target: M0,
            preserve: ENCRYPT,
        },
        Case {
            name: "1→1",
            source: Some(M1),
            target: M1,
            preserve: M1_ALL,
        },
        Case {
            name: "1→2",
            source: Some(M1),
            target: M2,
            preserve: ENCRYPT,
        },
        Case {
            name: "1→3",
            source: Some(M1),
            target: M3,
            preserve: NONE,
        },
        Case {
            name: "2→0",
            source: Some(M2),
            target: M0,
            preserve: NONE,
        },
        Case {
            name: "2→1",
            source: Some(M2),
            target: M1,
            preserve: ENCRYPT,
        },
        Case {
            name: "2→2",
            source: Some(M2),
            target: M2,
            preserve: ENCRYPT,
        },
        Case {
            name: "2→3",
            source: Some(M2),
            target: M3,
            preserve: NONE,
        },
        Case {
            name: "3→0",
            source: Some(M3),
            target: M0,
            preserve: BOOT_SHARE,
        },
        Case {
            name: "3→1",
            source: Some(M3),
            target: M1,
            preserve: NONE,
        },
        Case {
            name: "3→2",
            source: Some(M3),
            target: M2,
            preserve: NONE,
        },
        Case {
            name: "3→3",
            source: Some(M3),
            target: M3,
            preserve: BOOT_SHARE,
        },
    ];
    assert_eq!(cases.len(), 20);

    let usable_end = 12_000_000;
    for case in cases {
        let source = case.source.map(|mode| matrix_source(mode, usable_end));
        let prefill =
            prefill_for_target_mode(source.as_ref(), case.target, usable_end, SECTOR_SIZE)
                .unwrap_or_else(|error| panic!("{} prefill failed: {error}", case.name));
        let targets = prefill
            .target_partitions(SECTOR_SIZE)
            .unwrap_or_else(|error| panic!("{} target geometry failed: {error}", case.name));
        let roles: Vec<_> = targets.iter().map(|target| target.role).collect();
        assert_eq!(roles, target_roles(case.target), "{} roles", case.name);

        if let Some(source) = source.as_ref() {
            for &role in case.preserve {
                let old = source
                    .partition(role)
                    .unwrap_or_else(|| panic!("{} missing source {role:?}", case.name));
                let target = targets
                    .iter()
                    .find(|target| target.role == role)
                    .unwrap_or_else(|| panic!("{} missing target {role:?}", case.name));
                assert_eq!(
                    target.start_lba, old.start_lba,
                    "{} {role:?} start",
                    case.name
                );
                assert_eq!(
                    target.sector_count, old.sector_count,
                    "{} {role:?} size",
                    case.name
                );
                assert_eq!(
                    decide_partition_action(Some(old), target),
                    PartitionAction::PreserveExact,
                    "{} {role:?} preserve",
                    case.name
                );
            }
        }

        if case.name == "2→0" {
            let old_encrypt = source
                .as_ref()
                .unwrap()
                .partition(PartitionRole::Encrypt)
                .unwrap();
            let target_encrypt = targets
                .iter()
                .find(|target| target.role == PartitionRole::Encrypt)
                .unwrap();
            assert_ne!(target_encrypt.start_lba, old_encrypt.start_lba);
            assert_eq!(
                decide_partition_action(Some(old_encrypt), target_encrypt),
                PartitionAction::Rebuild
            );
        }
    }
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
fn geometry_overrides_keep_registered_encrypt_anchored_and_reject_overlap() {
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

    let shrunk = apply_target_geometry_overrides(
        prefill.clone(),
        Some(&source),
        TargetGeometryOverrides {
            share: Some(
                CapacityInput::from_exact(
                    encrypt_start - OFFICIAL_PARTITION_START_SECTOR - 4096,
                    CapacitySource::UserEdited,
                )
                .unwrap(),
            ),
            ..TargetGeometryOverrides::default()
        },
    )
    .unwrap();
    assert_eq!(shrunk.encrypt_start_lba, Some(encrypt_start));
    assert_eq!(
        shrunk.target_partitions(SECTOR_SIZE).unwrap()[0]
            .end_lba()
            .unwrap(),
        encrypt_start - 4096
    );

    let overlap = apply_target_geometry_overrides(
        prefill,
        Some(&source),
        TargetGeometryOverrides {
            share: Some(
                CapacityInput::from_exact(
                    encrypt_start - OFFICIAL_PARTITION_START_SECTOR + 1,
                    CapacitySource::UserEdited,
                )
                .unwrap(),
            ),
            ..TargetGeometryOverrides::default()
        },
    )
    .unwrap_err();
    assert!(overlap.contains("overlap"));
}

#[test]
fn geometry_overrides_reflow_only_unanchored_plain_partitions() {
    let usable_end = OFFICIAL_PARTITION_START_SECTOR + 40_000_000;
    let prefill = prefill_for_target_mode(
        None,
        OfficialPartitionMode::DefaultThreePartition,
        usable_end,
        SECTOR_SIZE,
    )
    .unwrap();
    let boot = CapacityInput::from_exact(10_000, CapacitySource::UserEdited).unwrap();
    let edited = apply_target_geometry_overrides(
        prefill,
        None,
        TargetGeometryOverrides {
            boot: Some(boot),
            ..TargetGeometryOverrides::default()
        },
    )
    .unwrap();
    assert_eq!(
        edited.share_start_lba,
        Some(OFFICIAL_PARTITION_START_SECTOR + 10_000)
    );
    let share_end = edited.share_start_lba.unwrap() + edited.share.unwrap().sectors();
    assert_eq!(edited.encrypt_start_lba, Some(share_end));
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

fn generated_source(
    mode: OfficialPartitionMode,
) -> (ProvisionSpec, edpcli::provision::ProvisionImage, String) {
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
        spec.clone(),
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
        let (_, image, did) = generated_source(mode);
        assert_eq!(
            DiskProvisionKind::from_metadata(image.as_bytes(), &did),
            DiskProvisionKind::from_mode(mode)
        );
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
        assert_eq!(
            DiskProvisionKind::from_metadata(image.as_bytes(), "wrong-device"),
            DiskProvisionKind::Plain
        );
    }
}

#[test]
fn moving_type4_from_slot_two_to_one_reencodes_headers_and_reuses_only_its_key_material() {
    let (spec, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let parsed = parse_existing_provision(&source_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    let old_type4 = *parsed.record(PartitionRole::Encrypt).unwrap();
    let prefill = prefill_for_target_mode(
        Some(&parsed.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
    let target_plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionSizes::new(1, 1, 1),
        compat,
        wrap_legacy_lba7_file_key(b"other", [0x11; 8]),
        wrap_file_key(b"other", [0x22; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_target_geometry(&targets, 512)
    .unwrap()
    .with_partition_key_material(
        1,
        old_type4.lba7_key_material(),
        old_type4.lba12_key_material().unwrap(),
    )
    .unwrap();
    let target_image =
        generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &target_plan).unwrap();
    let crc = crc32_bare(did.as_bytes());
    let old12 = a6b0_full(
        &source_image.as_bytes()[12 * 512..13 * 512],
        &crc.to_le_bytes(),
        0,
    );
    let new12 = a6b0_full(
        &target_image.as_bytes()[12 * 512..13 * 512],
        &crc.to_le_bytes(),
        0,
    );
    assert_eq!(
        &new12[0x60 + 0x30..0x60 + 0x59],
        &old12[2 * 0x60 + 0x30..2 * 0x60 + 0x59]
    );
    assert_eq!(
        u32::from_le_bytes(new12[0x60 + 8..0x60 + 12].try_into().unwrap()),
        2
    );
    assert_eq!(
        u64::from_le_bytes(new12[0x60 + 0x18..0x60 + 0x20].try_into().unwrap()),
        old_type4.lba12.start_sector
    );
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let old7 = xor_rolling(&source_image.as_bytes()[7 * 512..8 * 512], k0);
    let new7 = xor_rolling(&target_image.as_bytes()[7 * 512..8 * 512], k0);
    assert_eq!(
        &new7[0x40 + 0x30..0x40 + 0x40],
        &old7[2 * 0x40 + 0x30..2 * 0x40 + 0x40]
    );
    assert_eq!(
        u32::from_le_bytes(new7[0x40 + 8..0x40 + 12].try_into().unwrap()),
        2
    );
}

#[test]
fn target_plan_preserves_only_verified_matching_data() {
    let (_, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let mut source = parse_existing_provision(&source_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    let prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    let unknown_fs = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        b"ProofPass1!",
    )
    .unwrap();
    assert_eq!(unknown_fs.partitions[1].action, PartitionAction::Rebuild);

    source
        .confirm_filesystem(PartitionRole::Encrypt, OfficialFilesystemFormat::ExFat)
        .unwrap();
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        b"ProofPass1!",
    )
    .unwrap();
    assert_eq!(plan.partitions[0].action, PartitionAction::Rebuild);
    assert_eq!(plan.partitions[1].action, PartitionAction::PreserveExact);
    assert_eq!(
        plan.preserved_extents().collect::<Vec<_>>(),
        vec![(targets[1].start_lba, targets[1].sector_count)]
    );
    assert_eq!(
        plan.partitions[1]
            .preserved_record
            .unwrap()
            .lba12
            .file_key_crc,
        source
            .record(PartitionRole::Encrypt)
            .unwrap()
            .lba12
            .file_key_crc
    );

    let wrong_password = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        b"incorrect",
    )
    .unwrap();
    assert_eq!(
        wrong_password.partitions[1].action,
        PartitionAction::Rebuild
    );
    assert!(wrong_password.partitions[1].reason.contains("FileKey"));
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

#[test]
fn plain_and_four_registered_sources_prefill_every_target_mode() {
    let boot = part(
        PartitionRole::Boot,
        EdpPartitionType::Boot,
        63,
        20_417,
        false,
    );
    let share = part(
        PartitionRole::Share,
        EdpPartitionType::Share,
        20_480,
        5_000_000,
        true,
    );
    let combined = part(
        PartitionRole::BootShareCombined,
        EdpPartitionType::Share,
        63,
        5_020_417,
        false,
    );
    let reserve = part(
        PartitionRole::CompatibilityReserve,
        EdpPartitionType::Boot,
        63,
        63,
        false,
    );
    let encrypt = part(
        PartitionRole::Encrypt,
        EdpPartitionType::Encrypt,
        5_020_480,
        2_097_000,
        true,
    );
    let sources = [
        None,
        Some(ExistingProvisionProfile {
            source_mode: OfficialPartitionMode::DefaultThreePartition,
            partitions: vec![boot, share, encrypt],
        }),
        Some(ExistingProvisionProfile {
            source_mode: OfficialPartitionMode::BootShareCombined,
            partitions: vec![combined, encrypt],
        }),
        Some(ExistingProvisionProfile {
            source_mode: OfficialPartitionMode::WholeDiskEncrypted,
            partitions: vec![reserve, encrypt],
        }),
        Some(ExistingProvisionProfile {
            source_mode: OfficialPartitionMode::IntranetExtranetDualPartition,
            partitions: vec![boot, share],
        }),
    ];
    let targets = [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ];
    let mut cases = 0;
    for source in &sources {
        for mode in targets {
            let prefill = prefill_for_target_mode(source.as_ref(), mode, 20_000_000, 512).unwrap();
            let partitions = prefill.target_partitions(512).unwrap();
            assert_eq!(partitions.len(), mode.partition_types().len());
            if source
                .as_ref()
                .is_some_and(|source| source.source_mode == mode)
            {
                for target in &partitions {
                    if target.role != PartitionRole::CompatibilityReserve {
                        assert_eq!(
                            decide_partition_action(
                                source.as_ref().unwrap().partition(target.role),
                                target
                            ),
                            PartitionAction::PreserveExact
                        );
                    }
                }
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 20);
}

#[test]
fn mode3_to_mode0_shrinks_share_only_when_new_encrypt_does_not_fit_in_gap() {
    let source = ExistingProvisionProfile {
        source_mode: OfficialPartitionMode::IntranetExtranetDualPartition,
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
                7_000_000,
                true,
            ),
        ],
    };
    let prefill = prefill_for_target_mode(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        8_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    assert_eq!(targets[0].sector_count, 20_417);
    assert_eq!(
        decide_partition_action(source.partition(PartitionRole::Boot), &targets[0]),
        PartitionAction::PreserveExact
    );
    assert_eq!(targets[1].sector_count, 8_000_000 - 20_480 - 2_097_152);
    assert_eq!(
        decide_partition_action(source.partition(PartitionRole::Share), &targets[1]),
        PartitionAction::Rebuild
    );
}
