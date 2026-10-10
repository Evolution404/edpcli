//! Sector-based, device-independent defaults for a target provisioning mode.

use crate::filesystem::FilesystemKind;
use crate::protocol::edpf::EdpPartitionType;
use crate::{
    common::SECTOR,
    protocol::crypto::{a6b0_full, crc32_bare, xor_rolling},
    protocol::edpf::{EdpfEntry64, EdpfEntry96, PassInfo},
};

use super::{
    official_boot_sectors_from_end_mib, OfficialPartitionMode, PartitionRole, PassInfoPolicy,
    DEFAULT_OEM_BOOT_END_MIB, OFFICIAL_PARTITION_START_SECTOR,
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
