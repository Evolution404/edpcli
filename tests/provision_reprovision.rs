use edpcli::{
    application::filesystem::FilesystemKind,
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
    protocol::crypto::{a6b0_full, a7f0_full, crc32_bare, xor_rolling},
    protocol::edpf::EdpPartitionType,
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        apply_target_geometry_overrides, generate_official_image, parse_existing_provision,
        parse_existing_provision_native, prefill_for_target_mode, wrap_file_key,
        wrap_legacy_lba7_file_key, CapacityInput, CapacityInputMode, CapacitySource,
        DiskProvisionKind, ExistingFileKeyError, ExistingPartition, ExistingProvisionProfile,
        FileKeyWrapMode, KeyDomainRole, KeyDomainSecretPair, KeyDomainSecrets,
        OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan, OnlyId,
        PartitionRole, PassInfoPolicy, PassthroughBasis, PasswordDisposition, PlainSourceExtent,
        ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec, ProvisionTarget,
        QuickCapacityUnit, RegionDisposition, SourcePasswordKnowledge, TargetGeometryOverrides,
        TargetIdentity, TargetProvisionPlan, OFFICIAL_PARTITION_START_SECTOR,
    },
};

const SECTOR_SIZE: u64 = 512;
const MIB_SECTORS: u64 = 2048;

// Geometry-only scenarios have no source key record. Use the same canonical
// compatibility engine as production with both key profiles explicitly absent.
fn geometry_preserve_candidate(
    source: Option<&ExistingPartition>,
    target: &edpcli::provision::TargetPartitionGeometry,
) -> bool {
    use edpcli::provision::{
        preserve_compatibility, Extent, FilesystemProfile, PhysicalCryptoProfile, SourceRegion,
        TargetRegion,
    };
    let Some(source) = source else { return false };
    let source = SourceRegion {
        role: source.role,
        partition_type: source.partition_type.raw(),
        extent: Extent {
            start_lba: source.start_lba,
            sector_count: source.sector_count,
        },
        physical_crypto: if source.physically_encrypted {
            PhysicalCryptoProfile::SectorEncrypted
        } else {
            PhysicalCryptoProfile::Plain
        },
        filesystem: source
            .filesystem
            .map(FilesystemProfile::Known)
            .unwrap_or(FilesystemProfile::Unknown),
        key_profile: None,
    };
    let mut target = TargetRegion::from_target(*target);
    target.key_profile = None;
    preserve_compatibility(source, target).is_ok()
}

fn domain_secrets(source: Option<&[u8]>, target: &[u8]) -> KeyDomainSecrets {
    KeyDomainSecrets::new(
        KeyDomainSecretPair::new(source, Some(target)),
        KeyDomainSecretPair::new(source, Some(target)),
    )
}

#[test]
fn provision_target_keeps_plain_outside_the_official_mode_domain() {
    assert_eq!(ProvisionTarget::Plain.official_mode(), None);
    assert_eq!(ProvisionTarget::Plain.mode_number(), None);
    assert_eq!(ProvisionTarget::Plain.full_name(), "普通盘");
    assert_eq!(
        ProvisionTarget::OFFICIAL.map(|target| target.mode_number().expect("official mode number")),
        [0, 1, 2, 3]
    );
    for mode in 0..=3 {
        let target = ProvisionTarget::from_mode_number(mode).expect("official target");
        assert_eq!(target.mode_number(), Some(mode));
        assert!(target.official_mode().is_some());
    }
    assert_eq!(ProvisionTarget::from_mode_number(4), None);
}

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
            PartitionRole::Boot => FilesystemKind::Fat16,
            PartitionRole::CompatibilityReserve => FilesystemKind::ExFat,
            _ => FilesystemKind::ExFat,
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
    assert!(geometry_preserve_candidate(Some(&source), &same));

    let mut moved = same;
    moved.start_lba += 1;
    assert!(!geometry_preserve_candidate(Some(&source), &moved));

    let mut resized = same;
    resized.sector_count += 1;
    assert!(!geometry_preserve_candidate(Some(&source), &resized));

    let mut different_role = same;
    different_role.role = PartitionRole::Share;
    assert!(!geometry_preserve_candidate(Some(&source), &different_role));
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
                assert!(
                    geometry_preserve_candidate(Some(old), target),
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
            assert!(!geometry_preserve_candidate(
                Some(old_encrypt),
                target_encrypt
            ));
        }
    }
}

#[test]
fn plain_source_exact_extent_matrix_is_strict_and_boot_only() {
    const MODES: [OfficialPartitionMode; 4] = [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ];
    let usable_end = 12_000_000;

    for mode in MODES {
        let targets = prefill_for_target_mode(None, mode, usable_end, SECTOR_SIZE)
            .unwrap()
            .target_partitions(SECTOR_SIZE)
            .unwrap();
        for target in targets {
            let exact = PlainSourceExtent {
                start_lba: target.start_lba,
                sector_count: target.sector_count,
                filesystem: target.filesystem,
            };
            let should_preserve = target.role == PartitionRole::Boot;
            assert_eq!(
                edpcli::provision::plain_extent_preserve_candidate(&[exact], &target),
                should_preserve,
                "{mode:?} {:?} exact extent",
                target.role
            );

            let wrong_filesystem = PlainSourceExtent {
                filesystem: Some(if target.filesystem == Some(FilesystemKind::Fat16) {
                    FilesystemKind::ExFat
                } else {
                    FilesystemKind::Fat16
                }),
                ..exact
            };
            assert!(
                !edpcli::provision::plain_extent_preserve_candidate(&[wrong_filesystem], &target),
                "{mode:?} {:?} must reject an exact extent with the wrong filesystem",
                target.role
            );
            let unknown_filesystem = PlainSourceExtent {
                filesystem: None,
                ..exact
            };
            assert!(
                !edpcli::provision::plain_extent_preserve_candidate(&[unknown_filesystem], &target),
                "{mode:?} {:?} must fail closed when the Plain filesystem is unknown",
                target.role
            );

            for mismatch in [
                PlainSourceExtent {
                    start_lba: target.start_lba.saturating_sub(1),
                    sector_count: target.sector_count,
                    filesystem: target.filesystem,
                },
                PlainSourceExtent {
                    start_lba: target.start_lba + 1,
                    sector_count: target.sector_count,
                    filesystem: target.filesystem,
                },
                PlainSourceExtent {
                    start_lba: target.start_lba,
                    sector_count: target.sector_count.saturating_sub(1),
                    filesystem: target.filesystem,
                },
                PlainSourceExtent {
                    start_lba: target.start_lba,
                    sector_count: target.sector_count + 1,
                    filesystem: target.filesystem,
                },
            ] {
                assert!(
                    !edpcli::provision::plain_extent_preserve_candidate(&[mismatch], &target),
                    "{mode:?} {:?} must reject off-by-one physical extents",
                    target.role
                );
            }
        }
    }
}

#[test]
fn plain_to_official_plan_matrix_initializes_key_domains_and_only_preserves_exact_boot() {
    const MODES: [OfficialPartitionMode; 4] = [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ];
    let usable_end = 12_000_000;

    for mode in MODES {
        let targets = prefill_for_target_mode(None, mode, usable_end, SECTOR_SIZE)
            .unwrap()
            .target_partitions(SECTOR_SIZE)
            .unwrap();
        let exact_boot = targets
            .iter()
            .find(|target| target.role == PartitionRole::Boot)
            .map(|target| PlainSourceExtent {
                start_lba: target.start_lba,
                sector_count: target.sector_count,
                filesystem: target.filesystem,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let plan = TargetProvisionPlan::build_with_plain_extents(
            None,
            &exact_boot,
            mode,
            &targets,
            usable_end,
            &KeyDomainSecrets::default_targets(),
        )
        .unwrap();

        for part in &plan.partitions {
            if part.geometry.role == PartitionRole::Boot && !exact_boot.is_empty() {
                assert!(part.disposition.preserves_extent(), "{mode:?}");
                assert_eq!(
                    part.disposition,
                    RegionDisposition::PreserveVerified,
                    "{mode:?}"
                );
                assert_eq!(
                    part.preserved_record, None,
                    "plain boot preserve has no EDP key record"
                );
                assert_eq!(part.password_disposition, None);
                assert_eq!(part.target_password_policy, None);
            } else if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                assert!(
                    !part.disposition.preserves_extent(),
                    "{mode:?} {:?}",
                    part.geometry.role
                );
                assert_eq!(part.disposition, RegionDisposition::Rebuild);
                assert_eq!(
                    part.password_disposition,
                    Some(PasswordDisposition::Blocked)
                );
                assert_eq!(
                    part.target_password_policy,
                    Some(edpcli::provision::TargetPasswordPolicy::InitializeNew)
                );
            } else {
                assert!(
                    !part.disposition.preserves_extent(),
                    "{mode:?} {:?}",
                    part.geometry.role
                );
            }
        }

        let mut authorized = plan.clone();
        let roles = authorized
            .partitions
            .iter()
            .filter(|part| part.geometry.role != PartitionRole::CompatibilityReserve)
            .map(|part| part.geometry.role)
            .collect::<Vec<_>>();
        for role in roles {
            authorized.force_rebuild_for_format(role);
        }
        for part in &authorized.partitions {
            if KeyDomainRole::from_partition_role(part.geometry.role).is_some() {
                assert_eq!(
                    part.password_disposition,
                    Some(PasswordDisposition::Rebuild)
                );
                assert_eq!(
                    part.target_password_policy,
                    Some(edpcli::provision::TargetPasswordPolicy::InitializeNew)
                );
            }
            if part.geometry.role != PartitionRole::CompatibilityReserve {
                assert_eq!(part.disposition, RegionDisposition::Rebuild);
            }
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
    assert!(geometry_preserve_candidate(
        Some(source_encrypt),
        target_encrypt
    ));
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

fn generated_mode0_with_domain_passwords(
    share_password: &[u8],
    encrypt_password: &[u8],
) -> (edpcli::provision::ProvisionImage, String) {
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
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(b"0000aaaa", [0x01; 8]),
        wrap_file_key(b"0000aaaa", [0x02; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        1,
        wrap_legacy_lba7_file_key(share_password, [0x31; 8]),
        wrap_file_key(share_password, [0x41; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        2,
        wrap_legacy_lba7_file_key(encrypt_password, [0x51; 8]),
        wrap_file_key(encrypt_password, [0x61; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    (
        generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan).unwrap(),
        did,
    )
}

#[test]
fn source_password_probe_is_independent_per_key_domain() {
    let (image, did) = generated_mode0_with_domain_passwords(b"SharePass1!", b"EncryptPass1!");
    let parsed = parse_existing_provision(&image, &did, 16_777_216)
        .unwrap()
        .unwrap();

    assert_eq!(
        parsed.source_password_knowledge(KeyDomainRole::Share, Some(b"SharePass1!")),
        SourcePasswordKnowledge::UserVerified
    );
    assert_eq!(
        parsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(b"SharePass1!")),
        SourcePasswordKnowledge::Unknown
    );
    assert_eq!(
        parsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(b"EncryptPass1!")),
        SourcePasswordKnowledge::UserVerified
    );
}

#[test]
fn existing_partition_file_key_is_typed_and_checks_mode_before_password() {
    let (image, did) = generated_mode0_with_domain_passwords(b"SharePass1!", b"EncryptPass1!");
    let parsed = parse_existing_provision(&image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    let record = parsed.records[2];
    assert_eq!(
        record.verified_file_key(None),
        Err(ExistingFileKeyError::PasswordRequired)
    );
    assert_eq!(
        record.verified_file_key(Some(b"wrong")),
        Err(ExistingFileKeyError::PasswordMismatch)
    );
    assert_eq!(
        record.verified_file_key(Some(b"EncryptPass1!")),
        Ok([0x61; 16])
    );
    for mode in [FileKeyWrapMode::A7f0, FileKeyWrapMode::Aes128Ecb] {
        let mut variant = record;
        let material = wrap_file_key(b"EncryptPass1!", [0x61; 16], mode);
        variant.lba12.encrypt_mode = mode.raw();
        variant.lba12.user_key_crc = material.user_key_crc;
        variant.lba12.file_key_crc = material.file_key_crc;
        variant.lba12.encrypted_file_key = material.wrapped_file_key;
        assert_eq!(
            variant.verified_file_key(Some(b"EncryptPass1!")),
            Ok([0x61; 16])
        );
    }
    let mut unsupported = record;
    unsupported.lba12.encrypt_mode = 99;
    assert_eq!(
        unsupported.verified_file_key(None),
        Err(ExistingFileKeyError::UnsupportedEncryptMode)
    );
    let mut damaged = record;
    damaged.lba12.file_key_crc ^= 1;
    assert_eq!(
        damaged.verified_file_key(Some(b"EncryptPass1!")),
        Err(ExistingFileKeyError::FileKeyCrcMismatch)
    );
}

#[test]
fn default_password_probe_is_per_domain_and_enables_verified_preserve() {
    let (image, did) = generated_mode0_with_domain_passwords(b"SharePass1!", b"0000aaaa");
    let mut source = parse_existing_provision(&image, &did, 16_777_216)
        .unwrap()
        .unwrap();

    assert_eq!(
        source.source_password_knowledge(KeyDomainRole::Share, None),
        SourcePasswordKnowledge::Unknown
    );
    assert_eq!(
        source.source_password_knowledge(KeyDomainRole::Encrypt, None),
        SourcePasswordKnowledge::DefaultVerified
    );

    source
        .confirm_filesystem(PartitionRole::Encrypt, FilesystemKind::ExFat)
        .unwrap();
    let prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &KeyDomainSecrets::default_targets(),
    )
    .unwrap();
    let encrypt = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::Encrypt)
        .unwrap();
    assert!(encrypt.disposition.preserves_extent());
}

fn generated_source_with_force_change(
    mode: OfficialPartitionMode,
    force_change_password: bool,
) -> (ProvisionSpec, edpcli::provision::ProvisionImage, String) {
    generated_source_with_policy(
        mode,
        PassInfoPolicy {
            force_change_password,
            ..PassInfoPolicy::default()
        },
    )
}

fn generated_source_with_policy(
    mode: OfficialPartitionMode,
    pass_info_policy: PassInfoPolicy,
) -> (ProvisionSpec, edpcli::provision::ProvisionImage, String) {
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
    let target = TargetIdentity::from_probe(&probe, 16_777_216).unwrap();
    let did = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "USER06",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    let spec = ProvisionSpec::new(
        target,
        metadata,
        ProvisionProfile::canonical_v1().with_pass_info_policy(pass_info_policy),
    )
    .unwrap();
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

fn generated_source(
    mode: OfficialPartitionMode,
) -> (ProvisionSpec, edpcli::provision::ProvisionImage, String) {
    generated_source_with_force_change(mode, false)
}

#[test]
fn target_plan_matrix_matches_geometry_for_all_sixteen_edp_transitions() {
    const MODES: [OfficialPartitionMode; 4] = [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ];
    let usable_end = 16_000_000;

    for source_mode in MODES {
        let (_, image, did) = generated_source(source_mode);
        let mut source = parse_existing_provision(&image, &did, 16_777_216)
            .unwrap()
            .unwrap();
        let roles = source
            .profile
            .partitions
            .iter()
            .map(|part| part.role)
            .collect::<Vec<_>>();
        for role in roles {
            if role != PartitionRole::CompatibilityReserve {
                source
                    .confirm_filesystem(
                        role,
                        if role == PartitionRole::Boot {
                            FilesystemKind::Fat16
                        } else {
                            FilesystemKind::ExFat
                        },
                    )
                    .unwrap();
            }
        }

        for target_mode in MODES {
            let targets = prefill_for_target_mode(
                Some(&source.profile),
                target_mode,
                usable_end,
                SECTOR_SIZE,
            )
            .unwrap()
            .target_partitions(SECTOR_SIZE)
            .unwrap();
            let plan = TargetProvisionPlan::build(
                Some(&source),
                target_mode,
                &targets,
                usable_end,
                &domain_secrets(Some(b"ProofPass1!"), b"ProofPass1!"),
            )
            .unwrap();

            for (target, planned) in targets.iter().zip(&plan.partitions) {
                let expected =
                    geometry_preserve_candidate(source.profile.partition(target.role), target);
                assert_eq!(
                    planned.disposition.preserves_extent(),
                    expected,
                    "{source_mode:?} -> {target_mode:?} {:?}",
                    target.role
                );
                if !expected {
                    assert_eq!(planned.disposition, RegionDisposition::Rebuild);
                } else {
                    assert!(
                        planned.disposition.preserves_extent(),
                        "{source_mode:?} -> {target_mode:?} {:?} must preserve the exact extent",
                        target.role
                    );
                }
                match planned.password_disposition {
                    Some(PasswordDisposition::Passthrough(PassthroughBasis::Verified)) => {
                        assert_eq!(
                            planned.target_password_policy,
                            Some(edpcli::provision::TargetPasswordPolicy::ReuseVerified),
                            "{source_mode:?} -> {target_mode:?} {:?} verified passthrough policy",
                            target.role
                        );
                    }
                    Some(PasswordDisposition::Rewrap) => assert_eq!(
                        planned.target_password_policy,
                        Some(edpcli::provision::TargetPasswordPolicy::ReplaceVerified)
                    ),
                    Some(PasswordDisposition::Rebuild) => assert_eq!(
                        planned.target_password_policy,
                        Some(edpcli::provision::TargetPasswordPolicy::InitializeNew)
                    ),
                    Some(PasswordDisposition::Passthrough(PassthroughBasis::OpaqueCompatible)) => {
                        assert_eq!(
                            planned.target_password_policy,
                            Some(edpcli::provision::TargetPasswordPolicy::PreserveOpaque)
                        );
                    }
                    Some(PasswordDisposition::Blocked) | None => {}
                }
            }
        }
    }
}

#[test]
fn existing_profile_inherits_force_change_only_from_consistent_passinfo() {
    let (_, false_image, did) =
        generated_source_with_force_change(OfficialPartitionMode::DefaultThreePartition, false);
    let parsed_false = parse_existing_provision(&false_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(parsed_false.force_change_password, Some(false));

    let (_, true_image, did) =
        generated_source_with_force_change(OfficialPartitionMode::DefaultThreePartition, true);
    let parsed_true = parse_existing_provision(&true_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(parsed_true.force_change_password, Some(true));
    assert_eq!(
        parsed_true.pass_info_policy,
        Some(PassInfoPolicy {
            force_change_password: true,
            ..PassInfoPolicy::default()
        })
    );

    let custom_policy = PassInfoPolicy {
        force_change_password: true,
        cancel_password_complexity_check: true,
        max_share_password_errors: 7,
        max_encrypt_password_errors: 9,
    };
    let (_, custom_image, custom_did) =
        generated_source_with_policy(OfficialPartitionMode::DefaultThreePartition, custom_policy);
    let parsed_custom = parse_existing_provision(&custom_image, &custom_did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(parsed_custom.pass_info_policy, Some(custom_policy));

    let mut inconsistent = true_image.as_bytes().to_vec();
    let crc = crc32_bare(did.as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let mut plain7 = xor_rolling(&inconsistent[7 * 512..8 * 512], k0);
    plain7[0xc2] = 0;
    let wire7 = xor_rolling(&plain7, k0);
    inconsistent[7 * 512..8 * 512].copy_from_slice(&wire7);
    let inconsistent = edpcli::provision::ProvisionImage::from_bytes(inconsistent).unwrap();
    let parsed_inconsistent = parse_existing_provision(&inconsistent, &did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(parsed_inconsistent.force_change_password, None);
    assert_eq!(parsed_inconsistent.pass_info_policy, None);

    let mut inconsistent_complexity = custom_image.as_bytes().to_vec();
    let crc = crc32_bare(custom_did.as_bytes());
    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let mut plain7 = xor_rolling(&inconsistent_complexity[7 * 512..8 * 512], k0);
    plain7[0xca] = 0;
    let wire7 = xor_rolling(&plain7, k0);
    inconsistent_complexity[7 * 512..8 * 512].copy_from_slice(&wire7);
    let inconsistent_complexity =
        edpcli::provision::ProvisionImage::from_bytes(inconsistent_complexity).unwrap();
    let parsed = parse_existing_provision(&inconsistent_complexity, &custom_did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.pass_info_policy, None);
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
            Some(DiskProvisionKind::from_mode(mode))
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
            None
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
fn incompatible_target_role_requires_rebuild_and_explicit_format_authorization() {
    let (_, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let source = parse_existing_provision(&source_image, &did, 16_777_216)
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
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &KeyDomainSecrets::default_targets(),
    )
    .unwrap();

    let combined = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::BootShareCombined)
        .expect("mode1 combined target");
    assert_eq!(combined.disposition, RegionDisposition::Rebuild);
    assert!(!combined.disposition.preserves_extent());
    assert_eq!(combined.preserved_record, None);
    assert_eq!(
        combined.password_disposition,
        Some(PasswordDisposition::Blocked)
    );
    assert_eq!(
        combined.target_password_policy,
        Some(edpcli::provision::TargetPasswordPolicy::InitializeNew)
    );

    let mut plan = plan;
    assert!(plan.force_rebuild_for_format(PartitionRole::BootShareCombined));
    let combined = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::BootShareCombined)
        .unwrap();
    assert_eq!(combined.disposition, RegionDisposition::Rebuild);
    assert!(!combined.disposition.preserves_extent());
    assert_eq!(
        combined.password_disposition,
        Some(PasswordDisposition::Rebuild)
    );
    assert!(combined.reason.contains("重新格式化"));
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
        &domain_secrets(Some(b"ProofPass1!"), b"ProofPass1!"),
    )
    .unwrap();
    assert!(!unknown_fs.partitions[1].disposition.preserves_extent());

    source
        .confirm_filesystem(PartitionRole::Encrypt, FilesystemKind::ExFat)
        .unwrap();
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &domain_secrets(Some(b"ProofPass1!"), b"ProofPass1!"),
    )
    .unwrap();
    assert!(!plan.partitions[0].disposition.preserves_extent());
    assert!(plan.partitions[1].disposition.preserves_extent());
    assert_eq!(
        plan.partitions[1].password_disposition,
        Some(PasswordDisposition::Passthrough(PassthroughBasis::Verified))
    );
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
        &domain_secrets(Some(b"incorrect"), b"ProofPass1!"),
    )
    .unwrap();
    assert!(wrong_password.partitions[1].disposition.preserves_extent());
    assert_eq!(
        wrong_password.partitions[1].disposition,
        RegionDisposition::PreserveOpaque
    );
}

#[test]
fn unknown_password_can_opaque_preserve_without_decrypting_filesystem() {
    let (_, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let source = parse_existing_provision(&source_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(
        source
            .profile
            .partition(PartitionRole::Encrypt)
            .unwrap()
            .filesystem,
        None
    );

    let prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &KeyDomainSecrets::default(),
    )
    .unwrap();
    let encrypt = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::Encrypt)
        .unwrap();

    assert_eq!(encrypt.disposition, RegionDisposition::PreserveOpaque);
    assert_eq!(
        encrypt.password_disposition,
        Some(PasswordDisposition::Passthrough(
            PassthroughBasis::OpaqueCompatible
        ))
    );
    assert_eq!(
        encrypt.source_password_knowledge,
        Some(SourcePasswordKnowledge::Unknown)
    );
    assert_eq!(
        encrypt.target_password_policy,
        Some(edpcli::provision::TargetPasswordPolicy::PreserveOpaque)
    );
    assert_eq!(
        encrypt.preserved_record,
        source.record(PartitionRole::Encrypt).copied()
    );
}

#[test]
fn exact_encrypted_extent_with_unknown_password_stays_a_preserve_candidate() {
    let (_, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let mut source = parse_existing_provision(&source_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    source
        .confirm_filesystem(PartitionRole::Encrypt, FilesystemKind::ExFat)
        .unwrap();

    let prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();

    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &domain_secrets(None, b"ProofPass1!"),
    )
    .unwrap();

    let encrypt = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::Encrypt)
        .expect("mode1 encrypt target");

    assert!(encrypt.disposition.preserves_extent());
    assert_eq!(encrypt.disposition, RegionDisposition::PreserveOpaque);
    assert_eq!(
        encrypt.password_disposition,
        Some(PasswordDisposition::Blocked),
        "unknown source plus an explicit target password must require explicit rebuild authorization"
    );
    assert_eq!(
        encrypt.preserved_record,
        source.record(PartitionRole::Encrypt).copied(),
        "opaque preserve must retain the exact source key record without unwrap"
    );
}

#[test]
fn verified_source_with_different_target_password_plans_rewrap_without_rebuild() {
    let (_, source_image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let mut source = parse_existing_provision(&source_image, &did, 16_777_216)
        .unwrap()
        .unwrap();
    source
        .confirm_filesystem(PartitionRole::Encrypt, FilesystemKind::ExFat)
        .unwrap();
    let prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        16_000_000,
        512,
    )
    .unwrap();
    let targets = prefill.target_partitions(512).unwrap();
    let plan = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::BootShareCombined,
        &targets,
        16_000_000,
        &domain_secrets(Some(b"ProofPass1!"), b"NewEncryptPass2!"),
    )
    .unwrap();
    let encrypt = plan
        .partitions
        .iter()
        .find(|part| part.geometry.role == PartitionRole::Encrypt)
        .unwrap();
    assert!(encrypt.disposition.preserves_extent());
    assert_eq!(encrypt.disposition, RegionDisposition::RewrapVerified);
    assert_eq!(
        encrypt.password_disposition,
        Some(PasswordDisposition::Rewrap)
    );
    assert_eq!(
        encrypt.preserved_record,
        source.record(PartitionRole::Encrypt).copied()
    );
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
    assert!(geometry_preserve_candidate(
        source.partition(PartitionRole::Encrypt),
        &targets[1]
    ));
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
                        assert!(geometry_preserve_candidate(
                            source.as_ref().unwrap().partition(target.role),
                            target
                        ));
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
    assert!(geometry_preserve_candidate(
        source.partition(PartitionRole::Boot),
        &targets[0]
    ));
    assert_eq!(targets[1].sector_count, 8_000_000 - 20_480 - 2_097_152);
    assert!(!geometry_preserve_candidate(
        source.partition(PartitionRole::Share),
        &targets[1]
    ));
}

/// Synthesized transform of the verified *512B* official generator, not a
/// claim that any official Windows 4Kn producer generated these bytes.
#[test]
fn native_4kn_edpf_source_password_verification_matrix_is_fail_closed() {
    use edpcli::protocol::image::NativeProtocolImage;

    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let (_, image, did) = generated_source(mode);
        let crc = crc32_bare(did.as_bytes());
        let mut projection = image.as_bytes().to_vec();
        let original_projection = projection.clone();
        let start7 = 7 * 512;
        let start12 = 12 * 512;
        let mut plain7 = xor_rolling(
            &projection[start7..start7 + 512],
            (crc & 0xffff) ^ (crc >> 16),
        );
        let mut plain12 = a6b0_full(&projection[start12..start12 + 512], &crc.to_le_bytes(), 0);
        let count = u32::from_le_bytes(plain12[8..12].try_into().unwrap()) as usize;
        assert!(matches!(count, 2 | 3));
        for index in 0..count {
            let a = index * 0x40;
            let b = index * 0x60;
            // The protocol's physical-sector-width field changes from 512
            // to 4096. The record layout itself stays exactly 512B.
            plain7[a + 0x20..a + 0x28].copy_from_slice(&4096u64.to_le_bytes());
            plain12[b + 0x20..b + 0x28].copy_from_slice(&4096u64.to_le_bytes());
            let old = u64::from_le_bytes(plain12[b + 0x28..b + 0x30].try_into().unwrap());
            let native_count = (old / 4096).max(1);
            let new_bytes = native_count * 4096;
            plain12[b + 0x28..b + 0x30].copy_from_slice(&new_bytes.to_le_bytes());
            if index == 0 {
                plain7[a + 0x28..a + 0x30].copy_from_slice(&new_bytes.to_le_bytes());
            }
        }
        // The official producer retains LBA7 entry0's visible geometry,
        // while every later LBA7 entry points to the same native LCE block.
        // This is a synthetic 4Kn fixture, not a claimed manufacturer golden.
        let source_lce_start = (0..count)
            .map(|index| {
                let base = index * 0x60;
                let start =
                    u64::from_le_bytes(plain12[base + 0x18..base + 0x20].try_into().unwrap());
                let byte_len =
                    u64::from_le_bytes(plain12[base + 0x28..base + 0x30].try_into().unwrap());
                start + byte_len / 4096
            })
            .max()
            .unwrap()
            + 1;
        assert!(source_lce_start + 1 < 16_777_216);
        for index in 1..count {
            let base = index * 0x40;
            plain7[base + 0x18..base + 0x20].copy_from_slice(&source_lce_start.to_le_bytes());
            plain7[base + 0x28..base + 0x30].copy_from_slice(&4096u64.to_le_bytes());
        }
        let first_native_bytes = u64::from_le_bytes(plain12[0x28..0x30].try_into().unwrap());
        projection[458..462].copy_from_slice(&((first_native_bytes / 4096) as u32).to_le_bytes());
        projection[start7..start7 + 512]
            .copy_from_slice(&xor_rolling(&plain7, (crc & 0xffff) ^ (crc >> 16)));
        projection[start12..start12 + 512].copy_from_slice(&a7f0_full(
            &plain12,
            &crc.to_le_bytes(),
            0,
        ));

        // Create native-source complete blocks with nonzero opaque tails.
        let mut native = NativeProtocolImage::from_protocol_zero_tailed(&projection, 4096).unwrap();
        let mut raw = native.native_bytes().to_vec();
        for lba in 0..13 {
            raw[lba * 4096 + 512..(lba + 1) * 4096].fill(0xa0 + lba as u8);
        }
        native = NativeProtocolImage::from_native_bytes(4096, raw.clone()).unwrap();
        let source = parse_existing_provision_native(&native, &did, 16_777_216)
            .unwrap()
            .unwrap();
        assert_eq!(source.profile.source_mode, mode);
        assert_eq!(source.total_sectors, 16_777_216);
        // P25: bind all native EDPF partitions to independently confirmed
        // geometry BEFORE staging a purely offline protocol/LCE replay.
        // Synthetic 4Kn adapted from legacy official generator; not a new
        // manufacturer's native writer golden.
        let target_parts = source
            .profile
            .partitions
            .iter()
            .map(|part| {
                let mut geometry = part.as_target();
                geometry.filesystem = if part.role == PartitionRole::CompatibilityReserve {
                    None
                } else {
                    Some(match part.role {
                        PartitionRole::Boot => FilesystemKind::Fat16,
                        PartitionRole::BootShareCombined => FilesystemKind::ExFat,
                        PartitionRole::Share | PartitionRole::Encrypt => FilesystemKind::ExFat,
                        PartitionRole::CompatibilityReserve => unreachable!(),
                    })
                };
                geometry
            })
            .collect::<Vec<_>>();
        let lce_start = target_parts
            .iter()
            .map(|part| part.start_lba + part.sector_count)
            .max()
            .unwrap()
            + 1;
        assert_eq!(lce_start, source_lce_start);
        assert!(
            lce_start + 1 < 16_777_216,
            "synthetic source must have a free LCE block"
        );
        let native_layout = edpcli::provision::NativeEdpLayoutPlan::from_confirmed_geometry(
            mode,
            16_777_216,
            4096,
            &target_parts,
            lce_start,
            1,
        )
        .unwrap();
        let source_lce = [vec![0x69; 4096]];
        let staged = native_layout
            .verified_source_replay_native_blocks(&native, &did, &source_lce)
            .unwrap();
        assert_eq!(staged.len(), 14);
        assert_eq!(staged.last().unwrap().relative_lba, 0);
        assert_eq!(staged.last().unwrap().data, raw[..4096]);
        assert_eq!(staged[12].relative_lba, lce_start);
        assert_eq!(staged[12].data, source_lce[0]);
        for native_lba in 1..13usize {
            assert_eq!(staged[native_lba - 1].relative_lba, native_lba as u64);
            assert_eq!(
                staged[native_lba - 1].data,
                raw[native_lba * 4096..(native_lba + 1) * 4096]
            );
        }
        assert!(!native_layout.may_write());
        // Independently reconstruct all 13 native blocks from the staged
        // virtual plan. This is consumer validation, never a device write.
        let mut reconstructed = vec![0u8; 13 * 4096];
        for block in &staged {
            if block.relative_lba < 13 {
                let offset = block.relative_lba as usize * 4096;
                reconstructed[offset..offset + 4096].copy_from_slice(&block.data);
            }
        }
        assert_eq!(reconstructed, raw, "including all opaque native tails");
        let replayed_native =
            edpcli::protocol::image::NativeProtocolImage::from_native_bytes(4096, reconstructed)
                .unwrap();
        let replayed = parse_existing_provision_native(&replayed_native, &did, 16_777_216)
            .unwrap()
            .unwrap();
        assert_eq!(replayed.profile, source.profile);
        assert_eq!(replayed.records, source.records);
        assert_eq!(
            replayed_native.protocol_projection(),
            native.protocol_projection()
        );
        assert!(native_layout
            .verified_source_replay_native_blocks(&native, "wrong_device_id", &source_lce,)
            .is_err());
        assert!(native_layout
            .verified_source_replay_native_blocks(&native, "", &source_lce,)
            .is_err());
        assert!(native_layout
            .verified_source_replay_native_blocks(&native, &did, &[vec![0; 512]],)
            .is_err());

        // A stale second partition is invisible to an MBR-only preflight.
        // The strict EDPF+all-partitions entry must reject it.
        let mut corrupted_plain12 = plain12.clone();
        let second_size = 0x60 + 0x28;
        let current_bytes = u64::from_le_bytes(
            corrupted_plain12[second_size..second_size + 8]
                .try_into()
                .unwrap(),
        );
        assert!(current_bytes >= 2 * 4096);
        corrupted_plain12[second_size..second_size + 8]
            .copy_from_slice(&(current_bytes - 4096).to_le_bytes());
        let mut stale_native = native.native_bytes().to_vec();
        stale_native[12 * 4096..12 * 4096 + 512].copy_from_slice(&a7f0_full(
            &corrupted_plain12,
            &crc.to_le_bytes(),
            0,
        ));
        let stale_native =
            edpcli::protocol::image::NativeProtocolImage::from_native_bytes(4096, stale_native)
                .unwrap();
        assert!(
            native_layout
                .source_replay_native_blocks(&stale_native, &source_lce)
                .is_ok(),
            "legacy MBR-only preflight cannot see stale LBA12 secondary extent"
        );
        assert!(
            native_layout
                .verified_source_replay_native_blocks(&stale_native, &did, &source_lce,)
                .is_err(),
            "strict source replay must reject an unchanged MBR with a stale secondary extent"
        );

        // LBA7 pointers are authoritative for LCE. Altering one pointer
        // or its byte length must fail strict source replay despite an
        // unchanged MBR and unchanged LBA12 partition geometry.
        for index in 1..count {
            for corrupt_size in [false, true] {
                let mut invalid_plain7 = plain7.clone();
                let base = index * 0x40;
                if corrupt_size {
                    invalid_plain7[base + 0x28..base + 0x30]
                        .copy_from_slice(&8192u64.to_le_bytes());
                } else {
                    invalid_plain7[base + 0x18..base + 0x20]
                        .copy_from_slice(&(lce_start + 1).to_le_bytes());
                }
                let mut invalid_native = raw.clone();
                invalid_native[7 * 4096..7 * 4096 + 512]
                    .copy_from_slice(&xor_rolling(&invalid_plain7, (crc & 0xffff) ^ (crc >> 16)));
                let invalid_native =
                    NativeProtocolImage::from_native_bytes(4096, invalid_native).unwrap();
                assert!(
                    parse_existing_provision_native(&invalid_native, &did, 16_777_216)
                        .unwrap()
                        .is_some()
                );
                assert!(
                    native_layout
                        .source_replay_native_blocks(&invalid_native, &source_lce)
                        .is_ok(),
                    "old MBR-only replay cannot check LBA7 LCE pointers"
                );
                assert!(
                    native_layout
                        .verified_source_replay_native_blocks(&invalid_native, &did, &source_lce)
                        .is_err(),
                    "strict replay must reject incorrect LBA7 LCE pointer/length"
                );
            }
        }
        for record in &source.records {
            assert_eq!(record.lba7.sector_size, 4096);
            assert_eq!(record.lba12.sector_size, 4096);
        }
        for domain in [KeyDomainRole::Share, KeyDomainRole::Encrypt] {
            let Some(record) = source.record_for_domain(domain) else {
                continue;
            };
            assert_eq!(
                record.verified_sm4_file_key(b"ProofPass1!").unwrap(),
                [0x14; 16],
                "{mode:?} {domain:?} genuine fixture key"
            );
            assert!(record.verified_sm4_file_key(b"IncorrectPass!").is_err());
            assert!(record.verified_sm4_file_key(b"").is_err());
        }
        assert_eq!(native.native_bytes(), raw); // read-only never mutates
        assert_ne!(
            native.protocol_projection().as_slice(),
            original_projection.as_slice()
        );

        let mut corrupt_mbr = raw.clone();
        corrupt_mbr[458..462].copy_from_slice(&1u32.to_le_bytes());
        let corrupt_mbr = NativeProtocolImage::from_native_bytes(4096, corrupt_mbr).unwrap();
        assert!(parse_existing_provision_native(&corrupt_mbr, &did, 16_777_216).is_err());

        let wrong_id = parse_existing_provision_native(&native, "bad_device_id", 16_777_216);
        assert!(
            !matches!(wrong_id, Ok(Some(_))),
            "wrong source identity may not verify"
        );
        assert!(parse_existing_provision_native(&native, &did, 70).is_err());
        let wrong_length =
            NativeProtocolImage::from_native_bytes(4096, raw[..raw.len() - 1].to_vec());
        assert!(wrong_length.is_err());
        let mut corrupted = projection.clone();
        corrupted[start12 + 4] ^= 1;
        let corrupted = NativeProtocolImage::from_protocol_zero_tailed(&corrupted, 4096).unwrap();
        assert!(!matches!(
            parse_existing_provision_native(&corrupted, &did, 16_777_216),
            Ok(Some(_))
        ));
        // A legacy 512B image must continue to be parsed under its own
        // original 512B geometry (the new branch must not mutate the writer).
        let original_source = parse_existing_provision(&image, &did, 16_777_216)
            .unwrap()
            .unwrap();
        assert_eq!(original_source.profile.source_mode, mode);
        let legacy_native =
            NativeProtocolImage::from_native_bytes(512, original_projection).unwrap();
        assert_eq!(
            parse_existing_provision_native(&legacy_native, &did, 16_777_216)
                .unwrap()
                .unwrap()
                .profile,
            original_source.profile
        );
    }
}

/// The source profile was observed read-only on physical U391:
/// 4096B logical sectors, mode0, encrypt_mode=3 on both encrypted entries.
/// The material here is SYNTHETIC and deliberately contains no real keys.
#[test]
fn native_4kn_aes3_edpf_source_record_matches_real_u391_wrap_profile() {
    use edpcli::protocol::image::NativeProtocolImage;
    let (_, image, did) = generated_source(OfficialPartitionMode::DefaultThreePartition);
    let crc = crc32_bare(did.as_bytes());
    let mut projection = image.as_bytes().to_vec();
    let mut decrypted7 = xor_rolling(&projection[7 * 512..8 * 512], (crc & 0xffff) ^ (crc >> 16));
    let mut decrypted12 = a6b0_full(&projection[12 * 512..13 * 512], &crc.to_le_bytes(), 0);
    assert_eq!(
        u32::from_le_bytes(decrypted12[8..12].try_into().unwrap()),
        3
    );
    for index in 0..3 {
        let off7 = 0x40 * index;
        let off12 = 0x60 * index;
        decrypted7[off7 + 0x20..off7 + 0x28].copy_from_slice(&4096u64.to_le_bytes());
        decrypted12[off12 + 0x20..off12 + 0x28].copy_from_slice(&4096u64.to_le_bytes());
        let old_size =
            u64::from_le_bytes(decrypted12[off12 + 0x28..off12 + 0x30].try_into().unwrap());
        let native_bytes = (old_size / 4096).max(1) * 4096;
        decrypted12[off12 + 0x28..off12 + 0x30].copy_from_slice(&native_bytes.to_le_bytes());
        if index == 0 {
            decrypted7[off7 + 0x28..off7 + 0x30].copy_from_slice(&native_bytes.to_le_bytes());
            projection[458..462].copy_from_slice(&((native_bytes / 4096) as u32).to_le_bytes());
        } else {
            let key = wrap_file_key(
                b"NativeAesFixturePass",
                [0x27; 16],
                FileKeyWrapMode::Aes128Ecb,
            );
            decrypted12[off12 + 0x30..off12 + 0x34]
                .copy_from_slice(&key.user_key_crc.to_le_bytes());
            decrypted12[off12 + 0x34..off12 + 0x38]
                .copy_from_slice(&key.file_key_crc.to_le_bytes());
            decrypted12[off12 + 0x38..off12 + 0x48].copy_from_slice(&key.wrapped_file_key);
            decrypted12[off12 + 0x58] = 3;
        }
    }
    projection[7 * 512..8 * 512]
        .copy_from_slice(&xor_rolling(&decrypted7, (crc & 0xffff) ^ (crc >> 16)));
    projection[12 * 512..13 * 512].copy_from_slice(&edpcli::protocol::crypto::a7f0_full(
        &decrypted12,
        &crc.to_le_bytes(),
        0,
    ));
    let native = NativeProtocolImage::from_protocol_zero_tailed(&projection, 4096).unwrap();
    let parsed = parse_existing_provision_native(&native, &did, 16_777_216)
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.profile.source_mode,
        OfficialPartitionMode::DefaultThreePartition
    );
    for domain in [KeyDomainRole::Share, KeyDomainRole::Encrypt] {
        let record = parsed.record_for_domain(domain).unwrap();
        assert_eq!(record.lba12.encrypt_mode, FileKeyWrapMode::Aes128Ecb.raw());
        assert_eq!(record.lba12.sector_size, 4096);
        assert_eq!(
            record.verified_file_key(Some(b"NativeAesFixturePass")),
            Ok([0x27; 16])
        );
        assert!(record.verified_file_key(Some(b"wrong password")).is_err());
        assert!(record
            .verified_sm4_file_key(b"NativeAesFixturePass")
            .is_err());
    }
    assert_eq!(
        native.protocol_projection().as_slice(),
        projection.as_slice()
    );
    let mut corrupted = projection.clone();
    corrupted[458..462].copy_from_slice(&1u32.to_le_bytes());
    let corrupted = NativeProtocolImage::from_protocol_zero_tailed(&corrupted, 4096).unwrap();
    assert!(parse_existing_provision_native(&corrupted, &did, 16_777_216).is_err());
}
