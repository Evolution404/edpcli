//! Typed K6 file-level migration planning.
//!
//! This module is pure: it validates already-parsed inventories and produces a
//! deterministic manifest. Device I/O and payload staging stay in the
//! application layer.

use std::collections::BTreeMap;
use std::fmt;

use crate::filesystem_analysis::{FileEntry, FilePayloadLocator};

use super::{
    migration_transform, MigrationSource, MigrationTransform, SourceRegion, TargetPartitionGeometry,
};

pub const MAX_MIGRATION_ENTRIES: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationBudgets {
    pub staging_available_bytes: u64,
    pub target_available_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct MigrationInventory {
    pub source_index: usize,
    pub region: SourceRegion,
    pub entries: Vec<FileEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationManifestEntry {
    pub source_index: usize,
    pub source_region: SourceRegion,
    pub transform: MigrationTransform,
    pub path: String,
    pub is_directory: bool,
    pub logical_size: u64,
    pub attributes: u32,
    pub mtime: Option<String>,
    pub ctime: Option<String>,
    pub payload_locator: Option<FilePayloadLocator>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationManifest {
    pub entries: Vec<MigrationManifestEntry>,
    pub total_logical_bytes: u64,
    pub staging_required_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationPreflightError {
    MissingInventory {
        source_index: usize,
    },
    SourceInventoryMismatch {
        source_index: usize,
    },
    TransformMismatch {
        source_index: usize,
        transform: MigrationTransform,
    },
    InvalidPath {
        path: String,
    },
    TargetPathConflict {
        path: String,
    },
    MissingPayloadLocator {
        path: String,
    },
    LocatorLogicalSizeMismatch {
        path: String,
    },
    PayloadOutsideSource {
        path: String,
    },
    EntryBudgetExceeded {
        entries: usize,
        limit: usize,
    },
    StagingBudgetExceeded {
        required: u64,
        available: u64,
    },
    TargetCapacityExceeded {
        required: u64,
        available: u64,
    },
    ArithmeticOverflow,
}

impl fmt::Display for MigrationPreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInventory { source_index } => {
                write!(formatter, "migration source {source_index} has no parsed inventory")
            }
            Self::SourceInventoryMismatch { source_index } => write!(
                formatter,
                "migration source {source_index} inventory does not describe the planned source region"
            ),
            Self::TransformMismatch {
                source_index,
                transform,
            } => write!(
                formatter,
                "migration source {source_index} transform {transform:?} does not match source/target roles"
            ),
            Self::InvalidPath { path } => write!(formatter, "invalid migration path {path:?}"),
            Self::TargetPathConflict { path } => {
                write!(formatter, "multiple migration sources map to target path {path:?}")
            }
            Self::MissingPayloadLocator { path } => {
                write!(formatter, "migration file {path:?} has no payload locator")
            }
            Self::LocatorLogicalSizeMismatch { path } => write!(
                formatter,
                "migration file {path:?} payload locator logical size does not match inventory"
            ),
            Self::PayloadOutsideSource { path } => write!(
                formatter,
                "migration file {path:?} payload locator lies outside its source partition"
            ),
            Self::EntryBudgetExceeded { entries, limit } => write!(
                formatter,
                "migration entry count {entries} exceeds supported limit {limit}"
            ),
            Self::StagingBudgetExceeded {
                required,
                available,
            } => write!(
                formatter,
                "migration staging requires {required} bytes but only {available} are available"
            ),
            Self::TargetCapacityExceeded {
                required,
                available,
            } => write!(
                formatter,
                "migration payload requires {required} bytes but target budget is {available}"
            ),
            Self::ArithmeticOverflow => write!(formatter, "migration size arithmetic overflow"),
        }
    }
}

impl std::error::Error for MigrationPreflightError {}

fn canonical_path(path: &str, is_directory: bool) -> Result<String, MigrationPreflightError> {
    if !path.starts_with('/') {
        return Err(MigrationPreflightError::InvalidPath {
            path: path.to_string(),
        });
    }
    if path == "/" {
        return Ok("/".into());
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty()
        || trimmed.len() > 4096
        || trimmed
            .split('/')
            .skip(1)
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(MigrationPreflightError::InvalidPath {
            path: path.to_string(),
        });
    }
    Ok(if is_directory {
        format!("{trimmed}/")
    } else {
        trimmed.to_string()
    })
}

fn validate_locator(
    path: &str,
    logical_size: u64,
    locator: &FilePayloadLocator,
    region: SourceRegion,
) -> Result<(), MigrationPreflightError> {
    if locator.logical_size != logical_size {
        return Err(MigrationPreflightError::LocatorLogicalSizeMismatch {
            path: path.to_string(),
        });
    }
    let mut capacity_sectors = 0u64;
    for extent in &locator.extents {
        if extent.sector_count == 0 {
            return Err(MigrationPreflightError::PayloadOutsideSource {
                path: path.to_string(),
            });
        }
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
        if end > region.extent.sector_count {
            return Err(MigrationPreflightError::PayloadOutsideSource {
                path: path.to_string(),
            });
        }
        capacity_sectors = capacity_sectors
            .checked_add(extent.sector_count)
            .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
    }
    let capacity_bytes = capacity_sectors
        .checked_mul(512)
        .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
    if logical_size > capacity_bytes {
        return Err(MigrationPreflightError::PayloadOutsideSource {
            path: path.to_string(),
        });
    }
    Ok(())
}

pub fn build_migration_manifest(
    sources: &[MigrationSource],
    target: &TargetPartitionGeometry,
    inventories: &[MigrationInventory],
    budgets: MigrationBudgets,
) -> Result<MigrationManifest, MigrationPreflightError> {
    let mut inventory_by_source = BTreeMap::new();
    for inventory in inventories {
        inventory_by_source.insert(inventory.source_index, inventory);
    }

    let mut entries = Vec::new();
    let mut occupied_paths = BTreeMap::<String, String>::new();
    let mut total_logical_bytes = 0u64;
    let mut file_count = 0u64;
    let mut directory_count = 0u64;

    for source in sources {
        if migration_transform(source.region.role, target.role) != Some(source.transform) {
            return Err(MigrationPreflightError::TransformMismatch {
                source_index: source.source_index,
                transform: source.transform,
            });
        }
        let inventory = inventory_by_source.get(&source.source_index).ok_or(
            MigrationPreflightError::MissingInventory {
                source_index: source.source_index,
            },
        )?;
        if inventory.region != source.region {
            return Err(MigrationPreflightError::SourceInventoryMismatch {
                source_index: source.source_index,
            });
        }

        for entry in &inventory.entries {
            let path = canonical_path(&entry.path, entry.is_directory)?;
            if path == "/" {
                continue;
            }
            if entries.len() >= MAX_MIGRATION_ENTRIES {
                return Err(MigrationPreflightError::EntryBudgetExceeded {
                    entries: entries.len() + 1,
                    limit: MAX_MIGRATION_ENTRIES,
                });
            }
            let collision_key = path.trim_end_matches('/').to_lowercase();
            if let Some(previous) = occupied_paths.insert(collision_key, path.clone()) {
                return Err(MigrationPreflightError::TargetPathConflict {
                    path: if previous == path {
                        path
                    } else {
                        format!("{previous} / {path}")
                    },
                });
            }

            let payload_locator = if entry.is_directory {
                directory_count = directory_count
                    .checked_add(1)
                    .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
                None
            } else {
                let locator = entry.payload_locator.clone().ok_or_else(|| {
                    MigrationPreflightError::MissingPayloadLocator { path: path.clone() }
                })?;
                validate_locator(&path, entry.logical_size, &locator, source.region)?;
                total_logical_bytes = total_logical_bytes
                    .checked_add(entry.logical_size)
                    .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
                file_count = file_count
                    .checked_add(1)
                    .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
                Some(locator)
            };

            entries.push(MigrationManifestEntry {
                source_index: source.source_index,
                source_region: source.region,
                transform: source.transform,
                path,
                is_directory: entry.is_directory,
                logical_size: entry.logical_size,
                attributes: entry.attributes,
                mtime: entry.mtime.clone(),
                ctime: entry.ctime.clone(),
                payload_locator,
            });
        }
    }

    let target_geometry_bytes = target
        .sector_count
        .checked_mul(512)
        .ok_or(MigrationPreflightError::ArithmeticOverflow)?;
    let target_available = budgets.target_available_bytes.min(target_geometry_bytes);
    if total_logical_bytes > budgets.staging_available_bytes {
        return Err(MigrationPreflightError::StagingBudgetExceeded {
            required: total_logical_bytes,
            available: budgets.staging_available_bytes,
        });
    }
    if total_logical_bytes > target_available {
        return Err(MigrationPreflightError::TargetCapacityExceeded {
            required: total_logical_bytes,
            available: target_available,
        });
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(MigrationManifest {
        entries,
        total_logical_bytes,
        staging_required_bytes: total_logical_bytes,
        file_count,
        directory_count,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStagedEntry {
    pub source_index: usize,
    pub transform: MigrationTransform,
    pub path: String,
    pub is_directory: bool,
    pub data: Vec<u8>,
    pub attributes: u32,
    pub mtime: Option<String>,
    pub ctime: Option<String>,
}

pub fn finalize_staged_entry(
    manifest: &MigrationManifestEntry,
    data: Vec<u8>,
) -> Result<MigrationStagedEntry, String> {
    if manifest.is_directory {
        if !data.is_empty() {
            return Err(format!(
                "migration directory {:?} unexpectedly has payload bytes",
                manifest.path
            ));
        }
    } else if data.len() as u64 != manifest.logical_size {
        return Err(format!(
            "migration payload size mismatch for {:?}: {} != {}",
            manifest.path,
            data.len(),
            manifest.logical_size
        ));
    }
    Ok(MigrationStagedEntry {
        source_index: manifest.source_index,
        transform: manifest.transform,
        path: manifest.path.clone(),
        is_directory: manifest.is_directory,
        data,
        attributes: manifest.attributes,
        mtime: manifest.mtime.clone(),
        ctime: manifest.ctime.clone(),
    })
}
