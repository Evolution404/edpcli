use std::collections::BTreeMap;

use crate::partition_transform::PartitionTransform;

use super::{default_registry, FilesystemGeometry, FilesystemKind, FormatRequest};

const SECTOR_SIZE: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseFilesystemImage {
    volume_sectors: u64,
    sectors: BTreeMap<u64, [u8; SECTOR_SIZE]>,
}

impl SparseFilesystemImage {
    pub fn from_writes(
        volume_sectors: u64,
        writes: impl IntoIterator<Item = (u64, [u8; SECTOR_SIZE])>,
    ) -> Self {
        Self {
            volume_sectors,
            sectors: writes.into_iter().collect(),
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
            sectors: self
                .sectors
                .iter()
                .map(|(&lba, sector)| (lba, transform.transform_sector(lba, sector)))
                .collect(),
        }
    }
}

pub fn validate_volume_label(filesystem: FilesystemKind, label: &str) -> Result<(), String> {
    let registry = default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        format!(
            "filesystem {} has no registered driver",
            filesystem.config_token()
        )
    })?;
    let request = FormatRequest {
        filesystem,
        volume_label: (!label.is_empty()).then(|| label.to_string()),
        volume_serial: None,
    };
    driver
        .validate_format_request(&request)
        .map_err(|error| error.to_string())
}

pub fn build_empty_filesystem(
    filesystem: FilesystemKind,
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: Option<&str>,
) -> Result<SparseFilesystemImage, String> {
    let registry = default_registry();
    let driver = registry.driver(filesystem).ok_or_else(|| {
        format!(
            "filesystem {} has no registered driver",
            filesystem.config_token()
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
    let plan = driver
        .build_format_plan(geometry, &request)
        .map_err(|error| error.to_string())?;
    Ok(SparseFilesystemImage::from_writes(
        volume_sectors,
        plan.writes
            .into_iter()
            .map(|write| (write.relative_lba, write.data)),
    ))
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
