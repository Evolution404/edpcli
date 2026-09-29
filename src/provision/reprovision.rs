//! Sector-based, device-independent defaults for a target provisioning mode.

use crate::protocol::edpf::EdpPartitionType;
use crate::{
    common::SECTOR,
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    protocol::edpf::{EdpfEntry64, EdpfEntry96, PassInfo},
};

use super::{
    FilesystemKind, MigrationTransform, OfficialPartitionMode, PartitionRole, PassInfoPolicy,
    RegionMappingKind, RegionMappingPlanner, SourceRegion, TargetRegion,
    DEFAULT_MODE0_BOOT_SECTORS, OFFICIAL_PARTITION_START_SECTOR,
    WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};

mod disposition;
mod geometry;
mod model;
mod parsing;
mod plan;
mod prefill;

pub use disposition::*;
pub use geometry::*;
pub use model::*;
pub use parsing::*;
pub use plan::*;
pub use prefill::*;
