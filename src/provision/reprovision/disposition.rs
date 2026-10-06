use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartitionAction {
    PreserveExact,
    Rebuild,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionDisposition {
    PreserveOpaque,
    PreserveVerified,
    RewrapVerified,
    Rebuild,
    Drop,
}

impl RegionDisposition {
    pub const fn preserves_extent(self) -> bool {
        matches!(
            self,
            Self::PreserveOpaque | Self::PreserveVerified | Self::RewrapVerified
        )
    }
}

/// Geometry alone is necessary but not sufficient for actual preservation. The
/// writer must additionally require verified per-partition key material.
pub fn decide_partition_action(
    source: Option<&ExistingPartition>,
    target: &TargetPartitionGeometry,
) -> PartitionAction {
    match source {
        Some(source)
            if source.role == target.role
                && source.partition_type == target.partition_type
                && source.start_lba == target.start_lba
                && source.sector_count == target.sector_count
                && source.physically_encrypted == target.physically_encrypted
                && source.filesystem.is_some()
                && source.filesystem == target.filesystem
                && source.role != PartitionRole::CompatibilityReserve =>
        {
            PartitionAction::PreserveExact
        }
        _ => PartitionAction::Rebuild,
    }
}
