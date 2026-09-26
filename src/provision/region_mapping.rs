//! Canonical source/target region compatibility for reprovision planning.

use super::{
    ExistingPartition, ExistingPartitionRecord, FileKeyWrapMode, KeyDomainRole,
    OfficialFilesystemFormat, PartitionRole, TargetPartitionGeometry,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extent {
    pub start_lba: u64,
    pub sector_count: u64,
}

impl Extent {
    pub fn end_exclusive(self) -> Result<u64, String> {
        self.start_lba
            .checked_add(self.sector_count)
            .ok_or_else(|| "region extent overflows".into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalCryptoProfile {
    Plain,
    SectorEncrypted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemProfile {
    Known(OfficialFilesystemFormat),
    Unknown,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionKeyProfile {
    pub domain: KeyDomainRole,
    pub wrap_mode: Option<FileKeyWrapMode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRegion {
    pub role: PartitionRole,
    pub partition_type: u32,
    pub extent: Extent,
    pub physical_crypto: PhysicalCryptoProfile,
    pub filesystem: FilesystemProfile,
    pub key_profile: Option<RegionKeyProfile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetRegion {
    pub role: PartitionRole,
    pub partition_type: u32,
    pub extent: Extent,
    pub physical_crypto: PhysicalCryptoProfile,
    pub filesystem: FilesystemProfile,
    pub key_profile: Option<RegionKeyProfile>,
}

impl SourceRegion {
    pub fn from_existing(partition: ExistingPartition, record: ExistingPartitionRecord) -> Self {
        let key_profile =
            KeyDomainRole::from_partition_role(partition.role).map(|domain| RegionKeyProfile {
                domain,
                wrap_mode: (record.lba12.need_encrypt != 0)
                    .then(|| FileKeyWrapMode::from_raw(record.lba12.encrypt_mode))
                    .flatten(),
            });
        Self {
            role: partition.role,
            partition_type: partition.partition_type.raw(),
            extent: Extent {
                start_lba: partition.start_lba,
                sector_count: partition.sector_count,
            },
            physical_crypto: if partition.physically_encrypted {
                PhysicalCryptoProfile::SectorEncrypted
            } else {
                PhysicalCryptoProfile::Plain
            },
            filesystem: partition
                .filesystem
                .map(FilesystemProfile::Known)
                .unwrap_or(FilesystemProfile::Unknown),
            key_profile,
        }
    }
}

impl TargetRegion {
    pub fn from_target(partition: TargetPartitionGeometry) -> Self {
        let key_profile =
            KeyDomainRole::from_partition_role(partition.role).map(|domain| RegionKeyProfile {
                domain,
                wrap_mode: Some(FileKeyWrapMode::Sm4),
            });
        Self {
            role: partition.role,
            partition_type: partition.partition_type.raw(),
            extent: Extent {
                start_lba: partition.start_lba,
                sector_count: partition.sector_count,
            },
            physical_crypto: if partition.physically_encrypted {
                PhysicalCryptoProfile::SectorEncrypted
            } else {
                PhysicalCryptoProfile::Plain
            },
            filesystem: partition
                .filesystem
                .map(FilesystemProfile::Known)
                .unwrap_or(FilesystemProfile::Unknown),
            key_profile,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityFailure {
    CompatibilityReserveIsCanonicalRebuild,
    SemanticRole,
    PartitionType,
    StartLba,
    SectorCount,
    PhysicalCrypto,
    Filesystem,
    KeyDomain,
    WrapMode,
}

pub fn opaque_preserve_compatibility(
    source: SourceRegion,
    target: TargetRegion,
) -> Result<(), CompatibilityFailure> {
    if source.role == PartitionRole::CompatibilityReserve
        || target.role == PartitionRole::CompatibilityReserve
    {
        return Err(CompatibilityFailure::CompatibilityReserveIsCanonicalRebuild);
    }
    if source.role != target.role {
        return Err(CompatibilityFailure::SemanticRole);
    }
    if source.partition_type != target.partition_type {
        return Err(CompatibilityFailure::PartitionType);
    }
    if source.extent.start_lba != target.extent.start_lba {
        return Err(CompatibilityFailure::StartLba);
    }
    if source.extent.sector_count != target.extent.sector_count {
        return Err(CompatibilityFailure::SectorCount);
    }
    if source.physical_crypto != target.physical_crypto {
        return Err(CompatibilityFailure::PhysicalCrypto);
    }
    if source.filesystem != FilesystemProfile::Unknown
        && target.filesystem != FilesystemProfile::Unknown
        && source.filesystem != target.filesystem
    {
        return Err(CompatibilityFailure::Filesystem);
    }
    match (source.key_profile, target.key_profile) {
        (None, None) => {}
        (Some(source), Some(target)) => {
            if source.domain != target.domain {
                return Err(CompatibilityFailure::KeyDomain);
            }
            if source.wrap_mode != target.wrap_mode {
                return Err(CompatibilityFailure::WrapMode);
            }
        }
        _ => return Err(CompatibilityFailure::KeyDomain),
    }
    Ok(())
}

pub fn preserve_compatibility(
    source: SourceRegion,
    target: TargetRegion,
) -> Result<(), CompatibilityFailure> {
    if source.role == PartitionRole::CompatibilityReserve
        || target.role == PartitionRole::CompatibilityReserve
    {
        return Err(CompatibilityFailure::CompatibilityReserveIsCanonicalRebuild);
    }
    if source.role != target.role {
        return Err(CompatibilityFailure::SemanticRole);
    }
    if source.partition_type != target.partition_type {
        return Err(CompatibilityFailure::PartitionType);
    }
    if source.extent.start_lba != target.extent.start_lba {
        return Err(CompatibilityFailure::StartLba);
    }
    if source.extent.sector_count != target.extent.sector_count {
        return Err(CompatibilityFailure::SectorCount);
    }
    if source.physical_crypto != target.physical_crypto {
        return Err(CompatibilityFailure::PhysicalCrypto);
    }
    if source.filesystem == FilesystemProfile::Unknown
        || target.filesystem == FilesystemProfile::Unknown
        || source.filesystem != target.filesystem
    {
        return Err(CompatibilityFailure::Filesystem);
    }
    match (source.key_profile, target.key_profile) {
        (None, None) => {}
        (Some(source), Some(target)) => {
            if source.domain != target.domain {
                return Err(CompatibilityFailure::KeyDomain);
            }
            if source.wrap_mode != target.wrap_mode {
                return Err(CompatibilityFailure::WrapMode);
            }
        }
        _ => return Err(CompatibilityFailure::KeyDomain),
    }
    Ok(())
}

pub const fn migration_candidate(source: PartitionRole, target: PartitionRole) -> bool {
    matches!(
        (source, target),
        (PartitionRole::Boot, PartitionRole::BootShareCombined)
            | (PartitionRole::Share, PartitionRole::BootShareCombined)
            | (PartitionRole::BootShareCombined, PartitionRole::Boot)
            | (PartitionRole::BootShareCombined, PartitionRole::Share)
            | (PartitionRole::Encrypt, PartitionRole::Share)
            | (PartitionRole::Share, PartitionRole::Encrypt)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionMappingKind {
    PreserveCandidate,
    Rebuild,
    Drop,
    MigrateUnsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionMapping {
    pub source_index: Option<usize>,
    pub target_index: Option<usize>,
    pub kind: RegionMappingKind,
    pub failure: Option<CompatibilityFailure>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegionMappingPlan {
    pub mappings: Vec<RegionMapping>,
}

pub struct RegionMappingPlanner;

impl RegionMappingPlanner {
    pub fn map(source: &[SourceRegion], target: &[TargetRegion]) -> RegionMappingPlan {
        let mut mappings = Vec::new();
        let mut used_source = vec![false; source.len()];

        for (target_index, target_region) in target.iter().copied().enumerate() {
            if let Some((source_index, source_region)) = source
                .iter()
                .copied()
                .enumerate()
                .find(|(_, source_region)| source_region.role == target_region.role)
            {
                used_source[source_index] = true;
                match preserve_compatibility(source_region, target_region) {
                    Ok(()) => mappings.push(RegionMapping {
                        source_index: Some(source_index),
                        target_index: Some(target_index),
                        kind: RegionMappingKind::PreserveCandidate,
                        failure: None,
                    }),
                    Err(failure) => mappings.push(RegionMapping {
                        source_index: Some(source_index),
                        target_index: Some(target_index),
                        kind: RegionMappingKind::Rebuild,
                        failure: Some(failure),
                    }),
                }
            } else {
                let migrations = source
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(_, source_region)| {
                        migration_candidate(source_region.role, target_region.role)
                    })
                    .map(|(source_index, _)| source_index)
                    .collect::<Vec<_>>();
                if migrations.is_empty() {
                    mappings.push(RegionMapping {
                        source_index: None,
                        target_index: Some(target_index),
                        kind: RegionMappingKind::Rebuild,
                        failure: None,
                    });
                } else {
                    for source_index in migrations {
                        used_source[source_index] = true;
                        mappings.push(RegionMapping {
                            source_index: Some(source_index),
                            target_index: Some(target_index),
                            kind: RegionMappingKind::MigrateUnsupported,
                            failure: Some(CompatibilityFailure::SemanticRole),
                        });
                    }
                }
            }
        }

        for (source_index, used) in used_source.into_iter().enumerate() {
            if !used {
                mappings.push(RegionMapping {
                    source_index: Some(source_index),
                    target_index: None,
                    kind: RegionMappingKind::Drop,
                    failure: None,
                });
            }
        }

        RegionMappingPlan { mappings }
    }
}
