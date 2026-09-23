//! Pure provisioning domain model.
//!
//! Nothing in this module may open a device, execute a command or elevate privileges.
//! Hardware discovery belongs to the application/platform layers; builders consume only
//! immutable, already-resolved inputs from this domain.

mod generate;
mod layout;
mod profile;
mod spec;
mod validate;

pub use generate::{generate_image, ProvisionEntropy};
pub use layout::{
    build_official_partition_layout, OfficialPartitionGeometry, OfficialPartitionMode,
    OfficialPartitionSizes, OFFICIAL_PARTITION_START_SECTOR,
    WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};
pub use profile::ProvisionProfile;
pub use spec::{OnlyId, ProvisionMetadata, ProvisionSpec, TargetIdentity};
pub use validate::{ProvisionValidation, ProvisionValidator};

use crate::common::METADATA_IMAGE_LEN;

pub const PROVISION_IMAGE_LEN: usize = METADATA_IMAGE_LEN;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionImage([u8; PROVISION_IMAGE_LEN]);

impl ProvisionImage {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        let actual = bytes.len();
        let data: [u8; PROVISION_IMAGE_LEN] = bytes.try_into().map_err(|_| {
            format!("provision image length must be {PROVISION_IMAGE_LEN} bytes, got {actual}")
        })?;
        Ok(Self(data))
    }

    pub fn as_bytes(&self) -> &[u8; PROVISION_IMAGE_LEN] {
        &self.0
    }

    pub fn into_bytes(self) -> [u8; PROVISION_IMAGE_LEN] {
        self.0
    }
}
