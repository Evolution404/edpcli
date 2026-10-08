use std::{collections::BTreeMap, sync::Arc};

use crate::partition_transform::PartitionTransform;

use super::{
    default_registry, FilesystemError, FilesystemErrorKind, FilesystemGeometry, FilesystemKind,
    FormatRequest,
};

const SECTOR_SIZE: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseFilesystemImage {
    volume_sectors: u64,
    // Immutable format metadata is shared across display/planning snapshots.
    // A physical transform always builds a separate map to prevent aliasing.
    sectors: Arc<BTreeMap<u64, [u8; SECTOR_SIZE]>>,
}

impl SparseFilesystemImage {
    pub fn from_writes(
        volume_sectors: u64,
        writes: impl IntoIterator<Item = (u64, [u8; SECTOR_SIZE])>,
    ) -> Self {
        Self {
            volume_sectors,
            sectors: Arc::new(writes.into_iter().collect()),
        }
    }

    pub fn volume_sectors(&self) -> u64 {
        self.volume_sectors
    }

    pub fn sectors(&self) -> &BTreeMap<u64, [u8; SECTOR_SIZE]> {
        &self.sectors
    }

    pub fn sector_or_zero(&self, relative_lba: u64) -> Option<[u8; SECTOR_SIZE]> {
        if relative_lba >= self.volume_sectors {
            return None;
        }
        Some(
            self.sectors
                .get(&relative_lba)
                .copied()
                .unwrap_or([0; SECTOR_SIZE]),
        )
    }

    pub fn metadata_bytes(&self) -> usize {
        self.sectors.len() * SECTOR_SIZE
    }

    pub fn transformed<T: PartitionTransform>(&self, transform: &T) -> Self {
        Self {
            volume_sectors: self.volume_sectors,
            sectors: Arc::new(
                self.sectors
                    .iter()
                    .map(|(&lba, sector)| (lba, transform.transform_sector(lba, sector)))
                    .collect(),
            ),
        }
    }
}

pub fn validate_volume_label_typed(
    filesystem: FilesystemKind,
    label: &str,
) -> Result<(), FilesystemError> {
    let registry = default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        FilesystemError::for_filesystem(
            filesystem,
            FilesystemErrorKind::Unsupported,
            format!(
                "filesystem {} has no registered driver",
                filesystem.config_token()
            ),
        )
    })?;
    let request = FormatRequest {
        filesystem,
        volume_label: (!label.is_empty()).then(|| label.to_string()),
        volume_serial: None,
    };
    driver.validate_format_request(&request)
}

pub fn validate_volume_label(filesystem: FilesystemKind, label: &str) -> Result<(), String> {
    validate_volume_label_typed(filesystem, label).map_err(|error| error.to_string())
}

pub fn build_empty_filesystem_typed(
    filesystem: FilesystemKind,
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: Option<&str>,
) -> Result<SparseFilesystemImage, FilesystemError> {
    let registry = default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        FilesystemError::for_filesystem(
            filesystem,
            FilesystemErrorKind::Unsupported,
            format!(
                "filesystem {} has no registered driver",
                filesystem.config_token()
            ),
        )
    })?;
    let request = FormatRequest {
        filesystem,
        volume_label: volume_label
            .filter(|label| !label.is_empty())
            .map(str::to_string),
        volume_serial: Some(volume_serial),
    };
    let geometry = FilesystemGeometry::new(partition_offset, volume_sectors, SECTOR_SIZE as u32);
    let plan = driver.build_format_plan(geometry, &request)?;
    Ok(SparseFilesystemImage::from_writes(
        volume_sectors,
        plan.writes
            .into_iter()
            .map(|write| (write.relative_lba, write.data)),
    ))
}

pub fn build_empty_filesystem(
    filesystem: FilesystemKind,
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: Option<&str>,
) -> Result<SparseFilesystemImage, String> {
    build_empty_filesystem_typed(
        filesystem,
        partition_offset,
        volume_sectors,
        volume_serial,
        volume_label,
    )
    .map_err(|error| error.to_string())
}

pub fn build_empty_fat16(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
) -> Result<SparseFilesystemImage, String> {
    build_empty_filesystem(
        FilesystemKind::Fat16,
        partition_offset,
        volume_sectors,
        volume_serial,
        Some(volume_label),
    )
}

pub fn build_empty_fat32(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
) -> Result<SparseFilesystemImage, String> {
    build_empty_filesystem(
        FilesystemKind::Fat32,
        partition_offset,
        volume_sectors,
        volume_serial,
        Some(volume_label),
    )
}

pub fn build_empty_exfat(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
) -> Result<SparseFilesystemImage, String> {
    build_empty_filesystem(
        FilesystemKind::ExFat,
        partition_offset,
        volume_sectors,
        volume_serial,
        Some(volume_label),
    )
}

#[cfg(test)]
mod shared_image_tests {
    use super::*;
    use crate::partition_transform::IdentityTransform;

    #[test]
    fn clone_shares_immutable_sparse_metadata_but_transform_is_independent() {
        let original = SparseFilesystemImage::from_writes(
            100,
            [(0, [0x12; SECTOR_SIZE]), (42, [0x34; SECTOR_SIZE])],
        );
        let cloned = original.clone();
        assert!(Arc::ptr_eq(&original.sectors, &cloned.sectors));
        assert_eq!(cloned, original);
        assert_eq!(original.sector_or_zero(7), Some([0; SECTOR_SIZE]));
        let transformed = cloned.transformed(&IdentityTransform);
        assert_eq!(transformed, original);
        assert!(!Arc::ptr_eq(&original.sectors, &transformed.sectors));
        assert_eq!(Arc::strong_count(&original.sectors), 2);
        assert_eq!(original.metadata_bytes(), 2 * SECTOR_SIZE);
    }

    struct Alter;
    impl PartitionTransform for Alter {
        fn transform_sector(&self, _lba: u64, source: &[u8; SECTOR_SIZE]) -> [u8; SECTOR_SIZE] {
            let mut value = *source;
            value[0] ^= 0xff;
            value
        }
    }
    #[test]
    fn encrypted_physical_payload_must_not_modify_plain_verification_image() {
        let plain = SparseFilesystemImage::from_writes(20, [(3, [0x11; SECTOR_SIZE])]);
        let verification = plain.clone();
        let physical = plain.transformed(&Alter);
        assert_eq!(plain.sector_or_zero(3).unwrap()[0], 0x11);
        assert_eq!(verification.sector_or_zero(3).unwrap()[0], 0x11);
        assert_eq!(physical.sector_or_zero(3).unwrap()[0], 0xee);
        assert!(!Arc::ptr_eq(&physical.sectors, &plain.sectors));
    }
}
