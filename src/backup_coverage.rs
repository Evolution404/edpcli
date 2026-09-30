//! Read-side coverage projection from a verified EDPB manifest.

use crate::edpb::{Artifact, ArtifactCompleteness, Extent, Manifest, Region};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCoverageRegion {
    pub id: String,
    pub role: String,
    pub total_sectors: Option<u64>,
    pub captured_sectors: u64,
    pub artifact_count: usize,
    pub completeness: ArtifactCompleteness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCoverage {
    pub regions: Vec<BackupCoverageRegion>,
    pub extent_count: usize,
    pub artifact_count: usize,
}

impl BackupCoverage {
    pub fn from_manifest(manifest: &Manifest) -> Self {
        Self::from_parts(&manifest.regions, &manifest.extents, &manifest.artifacts)
    }

    pub fn from_parts(regions: &[Region], extents: &[Extent], artifacts: &[Artifact]) -> Self {
        let regions = regions
            .iter()
            .map(|region| {
                let related = extents
                    .iter()
                    .filter(|extent| extent.region_id == region.id)
                    .collect::<Vec<_>>();
                let contributing = artifacts
                    .iter()
                    .filter(|artifact| {
                        artifact.completeness != ArtifactCompleteness::NotCaptured
                            && artifact
                                .source_extent_ids
                                .iter()
                                .any(|id| related.iter().any(|extent| extent.id == *id))
                    })
                    .collect::<Vec<_>>();
                let mut ranges = related
                    .iter()
                    .filter(|extent| {
                        contributing
                            .iter()
                            .any(|artifact| artifact.source_extent_ids.contains(&extent.id))
                    })
                    .filter_map(|extent| {
                        extent
                            .start_lba
                            .checked_add(extent.sector_count)
                            .map(|end| (extent.start_lba, end))
                    })
                    .collect::<Vec<_>>();
                ranges.sort_unstable();
                let mut covered = 0u64;
                let mut end = 0u64;
                for (start, next_end) in ranges {
                    covered = covered.saturating_add(next_end.saturating_sub(start.max(end)));
                    end = end.max(next_end);
                }
                let completeness = if contributing.is_empty() {
                    ArtifactCompleteness::NotCaptured
                } else if contributing
                    .iter()
                    .all(|artifact| artifact.completeness == ArtifactCompleteness::Complete)
                    && region.sector_count.is_some_and(|total| covered >= total)
                {
                    ArtifactCompleteness::Complete
                } else {
                    ArtifactCompleteness::Partial
                };
                BackupCoverageRegion {
                    id: region.id.clone(),
                    role: region.role.clone(),
                    total_sectors: region.sector_count,
                    captured_sectors: covered,
                    artifact_count: contributing.len(),
                    completeness,
                }
            })
            .collect();
        Self {
            regions,
            extent_count: extents.len(),
            artifact_count: artifacts.len(),
        }
    }
}
