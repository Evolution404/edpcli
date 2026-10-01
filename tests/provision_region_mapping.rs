use edpcli::{
    protocol::edpf::EdpPartitionType,
    provision::{
        CompatibilityFailure, Extent, FileKeyWrapMode, FilesystemProfile, KeyDomainRole,
        PartitionRole, PhysicalCryptoProfile, RegionKeyProfile, RegionMappingKind,
        RegionMappingPlanner, SourceRegion, TargetRegion,
    },
};

fn source(
    role: PartitionRole,
    partition_type: EdpPartitionType,
    start_lba: u64,
    sector_count: u64,
    physical_crypto: PhysicalCryptoProfile,
    key_domain: Option<KeyDomainRole>,
) -> SourceRegion {
    SourceRegion {
        role,
        partition_type: partition_type.raw(),
        extent: Extent {
            start_lba,
            sector_count,
        },
        physical_crypto,
        filesystem: FilesystemProfile::Known(edpcli::filesystem::FilesystemKind::ExFat),
        key_profile: key_domain.map(|domain| RegionKeyProfile {
            domain,
            wrap_mode: Some(FileKeyWrapMode::Sm4),
        }),
    }
}

fn target(
    role: PartitionRole,
    partition_type: EdpPartitionType,
    start_lba: u64,
    sector_count: u64,
    physical_crypto: PhysicalCryptoProfile,
    key_domain: Option<KeyDomainRole>,
) -> TargetRegion {
    TargetRegion {
        role,
        partition_type: partition_type.raw(),
        extent: Extent {
            start_lba,
            sector_count,
        },
        physical_crypto,
        filesystem: FilesystemProfile::Known(edpcli::filesystem::FilesystemKind::ExFat),
        key_profile: key_domain.map(|domain| RegionKeyProfile {
            domain,
            wrap_mode: Some(FileKeyWrapMode::Sm4),
        }),
    }
}

#[test]
fn exact_region_is_a_preserve_candidate_before_password_policy() {
    let source = source(
        PartitionRole::Encrypt,
        EdpPartitionType::Encrypt,
        8_000_000,
        2_000_000,
        PhysicalCryptoProfile::SectorEncrypted,
        Some(KeyDomainRole::Encrypt),
    );
    let target = target(
        PartitionRole::Encrypt,
        EdpPartitionType::Encrypt,
        8_000_000,
        2_000_000,
        PhysicalCryptoProfile::SectorEncrypted,
        Some(KeyDomainRole::Encrypt),
    );
    let plan = RegionMappingPlanner::map(&[source], &[target]);
    assert_eq!(plan.mappings.len(), 1);
    assert_eq!(plan.mappings[0].kind, RegionMappingKind::PreserveCandidate);
    assert_eq!(plan.mappings[0].failure, None);
}

#[test]
fn mode0_to_mode1_rebuilds_combined_instead_of_migrating_files() {
    let source = [
        source(
            PartitionRole::Boot,
            EdpPartitionType::Boot,
            63,
            20_417,
            PhysicalCryptoProfile::Plain,
            None,
        ),
        source(
            PartitionRole::Share,
            EdpPartitionType::Share,
            20_480,
            5_979_520,
            PhysicalCryptoProfile::SectorEncrypted,
            Some(KeyDomainRole::Share),
        ),
        source(
            PartitionRole::Encrypt,
            EdpPartitionType::Encrypt,
            6_000_000,
            2_000_000,
            PhysicalCryptoProfile::SectorEncrypted,
            Some(KeyDomainRole::Encrypt),
        ),
    ];
    let target = [
        target(
            PartitionRole::BootShareCombined,
            EdpPartitionType::Share,
            63,
            5_999_937,
            PhysicalCryptoProfile::Plain,
            Some(KeyDomainRole::Share),
        ),
        target(
            PartitionRole::Encrypt,
            EdpPartitionType::Encrypt,
            6_000_000,
            2_000_000,
            PhysicalCryptoProfile::SectorEncrypted,
            Some(KeyDomainRole::Encrypt),
        ),
    ];

    let plan = RegionMappingPlanner::map(&source, &target);
    let combined = plan
        .mappings
        .iter()
        .find(|mapping| mapping.target_index == Some(0))
        .unwrap();
    assert_eq!(combined.kind, RegionMappingKind::Rebuild);
    assert_eq!(combined.source_index, None);
    assert_eq!(combined.failure, Some(CompatibilityFailure::SemanticRole));
    assert_eq!(
        plan.mappings
            .iter()
            .find(|mapping| mapping.target_index == Some(1))
            .unwrap()
            .kind,
        RegionMappingKind::PreserveCandidate
    );
    assert_eq!(
        plan.mappings
            .iter()
            .filter(|mapping| mapping.kind == RegionMappingKind::Drop)
            .map(|mapping| mapping.source_index)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1)]
    );
}

#[test]
fn mode2_encrypt_to_mode3_share_requires_rebuild_and_drops_old_region() {
    let source = [source(
        PartitionRole::Encrypt,
        EdpPartitionType::Encrypt,
        126,
        9_000_000,
        PhysicalCryptoProfile::SectorEncrypted,
        Some(KeyDomainRole::Encrypt),
    )];
    let target = [target(
        PartitionRole::Share,
        EdpPartitionType::Share,
        20_480,
        8_000_000,
        PhysicalCryptoProfile::SectorEncrypted,
        Some(KeyDomainRole::Share),
    )];
    let plan = RegionMappingPlanner::map(&source, &target);
    let target_mapping = plan
        .mappings
        .iter()
        .find(|mapping| mapping.target_index == Some(0))
        .unwrap();
    assert_eq!(target_mapping.kind, RegionMappingKind::Rebuild);
    assert_eq!(target_mapping.source_index, None);
    assert_eq!(
        target_mapping.failure,
        Some(CompatibilityFailure::SemanticRole)
    );
    assert!(plan
        .mappings
        .iter()
        .any(|mapping| mapping.kind == RegionMappingKind::Drop && mapping.source_index == Some(0)));
    assert!(!plan
        .mappings
        .iter()
        .any(|mapping| mapping.kind == RegionMappingKind::PreserveCandidate));
}

#[test]
fn compatibility_reserve_is_always_canonical_rebuild() {
    let source = source(
        PartitionRole::CompatibilityReserve,
        EdpPartitionType::Boot,
        63,
        63,
        PhysicalCryptoProfile::Plain,
        None,
    );
    let target = target(
        PartitionRole::CompatibilityReserve,
        EdpPartitionType::Boot,
        63,
        63,
        PhysicalCryptoProfile::Plain,
        None,
    );
    let plan = RegionMappingPlanner::map(&[source], &[target]);
    assert_eq!(plan.mappings[0].kind, RegionMappingKind::Rebuild);
    assert_eq!(
        plan.mappings[0].failure,
        Some(CompatibilityFailure::CompatibilityReserveIsCanonicalRebuild)
    );
}
