use crate::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
use crate::edpb::{ArtifactCompleteness, Manifest, RestoreContract, RestorePolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupRestoreRegionKind {
    CompleteBytes,
    StructureOnly,
    OutOfScope,
    PartialOrInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRestoreRegionStatus {
    pub label: String,
    pub kind: BackupRestoreRegionKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRestorePreview {
    pub total_sectors: Option<u64>,
    pub layout: Option<DiskLayoutModel>,
    pub layout_error: Option<String>,
    pub region_statuses: Vec<BackupRestoreRegionStatus>,
    pub restore_contract: Option<RestoreContract>,
    pub is_plain: bool,
}

fn partition_kind(role: Option<&str>) -> DiskRegionKind {
    match role {
        Some("boot") => DiskRegionKind::Boot,
        Some("share") => DiskRegionKind::Share,
        Some("encrypt") => DiskRegionKind::Encrypt,
        Some("boot_share_combined") => DiskRegionKind::Combined,
        Some("compatibility_reserve") => DiskRegionKind::Compatibility,
        Some("mbr_primary" | "mbr_logical" | "gpt_partition") => DiskRegionKind::Plain,
        _ => DiskRegionKind::Plain,
    }
}

fn partition_label(role: Option<&str>, index: u32) -> String {
    match role {
        Some("boot") => "启动区".into(),
        Some("share") => "交换区".into(),
        Some("encrypt") => "保密区".into(),
        Some("boot_share_combined") => "二合一区".into(),
        Some("compatibility_reserve") => "模式2兼容区".into(),
        Some("mbr_primary" | "mbr_logical" | "gpt_partition") | None => {
            format!("普通分区 P{index}")
        }
        Some(other) => format!("{} P{index}", other),
    }
}

fn restorable_complete_region(manifest: &Manifest, region_id: &str) -> bool {
    let Some(region) = manifest
        .regions
        .iter()
        .find(|region| region.id == region_id)
    else {
        return false;
    };
    let Some(total) = region.sector_count else {
        return false;
    };
    if total == 0 {
        return false;
    }

    let restorable_extent_ids = manifest
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.restore_policy == RestorePolicy::Restorable
                && artifact.completeness == ArtifactCompleteness::Complete
        })
        .flat_map(|artifact| artifact.source_extent_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();

    let mut ranges = manifest
        .extents
        .iter()
        .filter(|extent| {
            extent.region_id == region_id && restorable_extent_ids.contains(&extent.id)
        })
        .filter_map(|extent| {
            extent
                .start_lba
                .checked_add(extent.sector_count)
                .map(|end| (extent.start_lba, end))
        })
        .collect::<Vec<_>>();
    ranges.sort_unstable();

    let start = region.start_lba.unwrap_or(0);
    let end = match start.checked_add(total) {
        Some(end) => end,
        None => return false,
    };
    let mut cursor = start;
    for (range_start, range_end) in ranges {
        if range_end <= cursor || range_start > cursor {
            continue;
        }
        cursor = cursor.max(range_end.min(end));
        if cursor >= end {
            return true;
        }
    }
    false
}

fn build_layout(manifest: &Manifest, is_plain: bool) -> Result<DiskLayoutModel, String> {
    let total = manifest
        .geometry
        .total_sectors
        .ok_or_else(|| "备份未记录源盘总扇区数".to_string())?;
    let partitions = manifest
        .partitions
        .iter()
        .map(|partition| DiskLayoutSegment {
            label: partition_label(partition.role.as_deref(), partition.index),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            kind: partition_kind(partition.role.as_deref()),
        })
        .collect::<Vec<_>>();

    if !is_plain {
        let lce = manifest
            .regions
            .iter()
            .find(|region| region.role == "lba7_legacy_partition_compatibility_extent")
            .and_then(|region| Some((region.start_lba?, region.sector_count?)))
            .ok_or_else(|| "备份未记录可验证的 LCE 几何".to_string())?;
        return DiskLayoutModel::canonical_edp(total, partitions, lce.0, lce.1);
    }

    let restorable_extent_ids = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.restore_policy == RestorePolicy::Restorable)
        .flat_map(|artifact| artifact.source_extent_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let plain_region_ids = manifest
        .regions
        .iter()
        .filter(|region| region.role == "plain_partition_table")
        .map(|region| region.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let mut known = manifest
        .extents
        .iter()
        .filter(|extent| {
            plain_region_ids.contains(extent.region_id.as_str())
                && restorable_extent_ids.contains(&extent.id)
        })
        .map(|extent| DiskLayoutSegment {
            label: "分区表元数据".into(),
            start_lba: extent.start_lba,
            sector_count: extent.sector_count,
            kind: DiskRegionKind::Metadata,
        })
        .collect::<Vec<_>>();
    known.extend(partitions);
    DiskLayoutModel::canonical_from_known(total, known)
}

impl BackupRestorePreview {
    pub fn from_manifest(manifest: &Manifest) -> Self {
        let is_plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
        let restore_contract = manifest.restore_contract.clone();
        let mut region_statuses = Vec::new();

        if is_plain {
            region_statuses.push(BackupRestoreRegionStatus {
                label: "分区表结构".into(),
                kind: if restore_contract
                    .as_ref()
                    .is_some_and(|contract| contract.restores_partition_structure)
                {
                    BackupRestoreRegionKind::CompleteBytes
                } else {
                    BackupRestoreRegionKind::PartialOrInvalid
                },
                detail: Some("恢复分区表与分区几何".into()),
            });
        } else {
            let protocol_region = manifest
                .regions
                .iter()
                .find(|region| region.role == "protocol");
            region_statuses.push(BackupRestoreRegionStatus {
                label: "EDP 协议 LBA0-12".into(),
                kind: if restore_contract
                    .as_ref()
                    .is_some_and(|contract| contract.restores_edp_protocol)
                    && protocol_region
                        .is_some_and(|region| restorable_complete_region(manifest, &region.id))
                {
                    BackupRestoreRegionKind::CompleteBytes
                } else {
                    BackupRestoreRegionKind::PartialOrInvalid
                },
                detail: Some("13 sector".into()),
            });
        }

        if restore_contract
            .as_ref()
            .is_some_and(|contract| contract.restores_partition_structure)
        {
            region_statuses.extend(manifest.partitions.iter().map(|partition| {
                BackupRestoreRegionStatus {
                    label: partition_label(partition.role.as_deref(), partition.index),
                    kind: BackupRestoreRegionKind::StructureOnly,
                    detail: Some("数据内容未备份".into()),
                }
            }));
        }

        if !is_plain {
            let lce_region = manifest
                .regions
                .iter()
                .find(|region| region.role == "lba7_legacy_partition_compatibility_extent");
            region_statuses.push(BackupRestoreRegionStatus {
                label: "LCE".into(),
                kind: if lce_region
                    .is_some_and(|region| restorable_complete_region(manifest, &region.id))
                {
                    BackupRestoreRegionKind::CompleteBytes
                } else {
                    BackupRestoreRegionKind::PartialOrInvalid
                },
                detail: lce_region
                    .and_then(|region| region.sector_count)
                    .map(|count| format!("{count} sector")),
            });
        }

        region_statuses.push(BackupRestoreRegionStatus {
            label: "用户文件".into(),
            kind: if restore_contract
                .as_ref()
                .is_some_and(|contract| contract.restores_user_data)
            {
                BackupRestoreRegionKind::PartialOrInvalid
            } else {
                BackupRestoreRegionKind::OutOfScope
            },
            detail: Some("目录与用户文件不在备份范围".into()),
        });

        let (layout, layout_error) = match build_layout(manifest, is_plain) {
            Ok(layout) => (Some(layout), None),
            Err(error) => (None, Some(error)),
        };

        Self {
            total_sectors: manifest.geometry.total_sectors,
            layout,
            layout_error,
            region_statuses,
            restore_contract,
            is_plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edpb::{
        Artifact, BackupPurpose, CaptureLevel, ChunkStorage, ContainerVersion, DeviceGeometry,
        DeviceIdentity, Extent, ManifestPartition, Observation, Provenance, Region, SemanticStatus,
        SnapshotInfo,
    };

    fn storage() -> ChunkStorage {
        ChunkStorage {
            frame_offset: 0,
            data_offset: 0,
            stored_length: 0,
            original_length: 0,
            codec: "none".into(),
            sha256: String::new(),
        }
    }

    fn restorable_artifact(id: &str, extent_id: &str) -> Artifact {
        Artifact {
            id: id.into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id.into()],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            storage: storage(),
        }
    }

    fn base_manifest(
        device_state: &str,
        total_sectors: u64,
        restores_edp_protocol: bool,
    ) -> Manifest {
        Manifest {
            schema: "edpb.manifest.v3".into(),
            container_version: ContainerVersion { major: 1, minor: 0 },
            snapshot: SnapshotInfo {
                snapshot_id: "preview-test".into(),
                created_epoch: 0,
                capture_level: CaptureLevel::Metadata,
                device_state: device_state.into(),
            },
            backup_purpose: Some(BackupPurpose::MetadataOnly),
            restore_contract: Some(RestoreContract::metadata_only(restores_edp_protocol)),
            device: DeviceIdentity {
                vid: "1234".into(),
                pid: "5678".into(),
                device_id: "disk&ven_test&prod_test".into(),
                onlyid: Some("7001".into()),
            },
            identity: None,
            geometry: DeviceGeometry {
                logical_sector_size: 512,
                physical_sector_size: None,
                total_sectors: Some(total_sectors),
                capacity_bytes: Some(total_sectors * 512),
            },
            observation: Observation {
                disk_number: Some(6),
                platform: "test".into(),
                edpcli_version: "test".into(),
            },
            partitions: Vec::new(),
            regions: Vec::new(),
            extents: Vec::new(),
            artifacts: Vec::new(),
            provenance: Provenance {
                capture_source: "test".into(),
                source_format: "test".into(),
                notes: Vec::new(),
            },
        }
    }

    #[test]
    fn edp_preview_distinguishes_exact_protocol_lce_from_structure_only_partitions() {
        let mut manifest = base_manifest("mode0", 200_000, true);
        manifest.partitions = vec![
            ManifestPartition {
                index: 1,
                role: Some("boot".into()),
                partition_type: Some("edp:1".into()),
                start_lba: 63,
                sector_count: 20_417,
                filesystem_hint: Some("fat16".into()),
                volume_label_hint: Some("启动区".into()),
            },
            ManifestPartition {
                index: 2,
                role: Some("share".into()),
                partition_type: Some("edp:2".into()),
                start_lba: 20_480,
                sector_count: 20_000,
                filesystem_hint: None,
                volume_label_hint: Some("交换区".into()),
            },
            ManifestPartition {
                index: 3,
                role: Some("encrypt".into()),
                partition_type: Some("edp:4".into()),
                start_lba: 40_480,
                sector_count: 20_000,
                filesystem_hint: None,
                volume_label_hint: Some("保密区".into()),
            },
        ];
        manifest.regions = vec![
            Region {
                id: "region.protocol".into(),
                role: "protocol".into(),
                start_lba: Some(0),
                sector_count: Some(13),
                semantic_status: SemanticStatus::Identified,
            },
            Region {
                id: "region.lce".into(),
                role: "lba7_legacy_partition_compatibility_extent".into(),
                start_lba: Some(180_000),
                sector_count: Some(6),
                semantic_status: SemanticStatus::Identified,
            },
        ];
        manifest.extents = vec![
            Extent {
                id: "extent.protocol".into(),
                region_id: "region.protocol".into(),
                start_lba: 0,
                sector_count: 13,
                purpose: "protocol".into(),
            },
            Extent {
                id: "extent.lce".into(),
                region_id: "region.lce".into(),
                start_lba: 180_000,
                sector_count: 6,
                purpose: "lce".into(),
            },
        ];
        manifest.artifacts = vec![
            restorable_artifact("raw.protocol", "extent.protocol"),
            restorable_artifact("raw.lce", "extent.lce"),
        ];

        let preview = BackupRestorePreview::from_manifest(&manifest);
        preview
            .layout
            .as_ref()
            .expect("EDP layout")
            .validate_complete()
            .expect("complete disk layout");
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "EDP 协议 LBA0-12"
                && region.kind == BackupRestoreRegionKind::CompleteBytes
        }));
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "LCE" && region.kind == BackupRestoreRegionKind::CompleteBytes
        }));
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "启动区" && region.kind == BackupRestoreRegionKind::StructureOnly
        }));
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "用户文件" && region.kind == BackupRestoreRegionKind::OutOfScope
        }));
    }

    #[test]
    fn edp_preview_marks_missing_or_partial_lce_as_real_incompleteness() {
        let missing = base_manifest("mode0", 200_000, true);
        let preview = BackupRestorePreview::from_manifest(&missing);
        assert!(preview.layout.is_none());
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "LCE" && region.kind == BackupRestoreRegionKind::PartialOrInvalid
        }));

        let mut partial = base_manifest("mode0", 200_000, true);
        partial.regions.push(Region {
            id: "region.lce".into(),
            role: "lba7_legacy_partition_compatibility_extent".into(),
            start_lba: Some(180_000),
            sector_count: Some(6),
            semantic_status: SemanticStatus::Identified,
        });
        partial.extents.push(Extent {
            id: "extent.lce".into(),
            region_id: "region.lce".into(),
            start_lba: 180_000,
            sector_count: 6,
            purpose: "lce".into(),
        });
        let mut artifact = restorable_artifact("raw.lce", "extent.lce");
        artifact.completeness = ArtifactCompleteness::Partial;
        partial.artifacts.push(artifact);

        let preview = BackupRestorePreview::from_manifest(&partial);
        assert!(preview.layout.is_some());
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "LCE" && region.kind == BackupRestoreRegionKind::PartialOrInvalid
        }));
    }

    #[test]
    fn plain_preview_uses_partition_table_metadata_and_never_claims_user_data() {
        let mut manifest = base_manifest("plain", 20_000, false);
        manifest.device.device_id = "plain".into();
        manifest.device.onlyid = None;
        manifest.partitions = vec![ManifestPartition {
            index: 1,
            role: Some("mbr_primary".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba: 2_048,
            sector_count: 4_096,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: Some("DATA".into()),
        }];
        manifest.regions = vec![Region {
            id: "region.plain.partition_table".into(),
            role: "plain_partition_table".into(),
            start_lba: None,
            sector_count: None,
            semantic_status: SemanticStatus::Identified,
        }];
        manifest.extents = vec![Extent {
            id: "extent.mbr".into(),
            region_id: "region.plain.partition_table".into(),
            start_lba: 0,
            sector_count: 1,
            purpose: "mbr".into(),
        }];
        manifest.artifacts = vec![restorable_artifact("raw.mbr", "extent.mbr")];

        let preview = BackupRestorePreview::from_manifest(&manifest);
        assert!(preview.is_plain);
        preview
            .layout
            .as_ref()
            .expect("Plain layout")
            .validate_complete()
            .expect("complete disk layout");
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "分区表结构" && region.kind == BackupRestoreRegionKind::CompleteBytes
        }));
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "普通分区 P1" && region.kind == BackupRestoreRegionKind::StructureOnly
        }));
        assert!(preview.region_statuses.iter().any(|region| {
            region.label == "用户文件" && region.kind == BackupRestoreRegionKind::OutOfScope
        }));
        assert!(!preview
            .region_statuses
            .iter()
            .any(|region| region.label == "LCE"));
    }
}
