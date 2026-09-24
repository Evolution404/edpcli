//! Pure provisioning domain model.
//!
//! Nothing in this module may open a device, execute a command or elevate privileges.
//! Hardware discovery belongs to the application/platform layers; builders consume only
//! immutable, already-resolved inputs from this domain.

mod filesystem;
mod generate;
mod keys;
mod layout;
mod lce;
mod profile;
mod reprovision;
mod spec;
mod validate;
mod write_plan;

pub use filesystem::{
    build_empty_exfat, build_empty_fat16, build_official_exfat_partition,
    build_official_exfat_partitions, build_official_partition_filesystem, encrypt_sparse_mode2,
    OfficialFilesystemFormat, PartitionFilesystemImage, SparseFilesystemImage,
};
pub use generate::{generate_image, generate_official_image, ProvisionEntropy};
pub use keys::{
    wrap_file_key, wrap_legacy_lba7_file_key, FileKeyWrapMode, LegacyLba7KeyMaterial,
    ProvisionKeyMaterial,
};
pub use layout::{
    build_official_partition_layout, official_format_targets,
    official_format_targets_with_filesystems, official_mbr_partition_type,
    physical_partition_encryption, visible_mbr_partition_type, OfficialPartitionFilesystems,
    OfficialPartitionGeometry, OfficialPartitionMode, OfficialPartitionSizes,
    OfficialProvisionPlan, PartitionFormatTarget, PartitionRole, DEFAULT_MODE0_BOOT_SECTORS,
    OFFICIAL_PARTITION_START_SECTOR, WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};
pub use lce::{build_lce_ciphertext, lce_plaintext};
pub use profile::{PassInfoPolicy, ProvisionProfile, DEFAULT_SAFE6_LABEL};
pub use reprovision::{
    apply_target_geometry_overrides, decide_partition_action, force_change_password_from_sectors,
    parse_existing_provision, pass_info_policy_from_sectors, prefill_for_target_mode,
    validate_target_geometry, CapacityInput, CapacityInputMode, CapacitySource, DiskProvisionKind,
    ExistingPartition, ExistingPartitionRecord, ExistingProvisionProfile, ParsedExistingProvision,
    PartitionAction, ProvisionPrefill, ProvisionTarget, QuickCapacityUnit, TargetGeometryOverrides,
    TargetPartitionGeometry, TargetPartitionPlan, TargetProvisionPlan,
};
pub use spec::{OnlyId, ProvisionMetadata, ProvisionSpec, TargetIdentity};
pub use validate::{
    OfficialProvisionValidation, OfficialProvisionValidator, ProvisionValidation,
    ProvisionValidator,
};
pub use write_plan::{
    build_official_provision_protocol_image, build_official_provision_write_image,
    OfficialProvisionWriteImage,
};

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
