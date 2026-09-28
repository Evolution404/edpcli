use super::*;

pub(super) fn validate_manifest_graph(manifest: &Manifest) -> Result<(), String> {
    validate_manifest_identity(manifest)?;
    match manifest.schema.as_str() {
        "edpb.manifest.v3" => {
            if manifest.backup_purpose != Some(BackupPurpose::MetadataOnly) {
                return Err("EDPB manifest v3 must declare metadata_only backup purpose".into());
            }
            let contract = manifest
                .restore_contract
                .as_ref()
                .ok_or_else(|| "EDPB manifest v3 missing restore contract".to_string())?;
            if !contract.restores_partition_structure
                || contract.restores_filesystem
                || contract.restores_user_data
                || !contract.post_restore_assessment_required
            {
                return Err(
                    "EDPB manifest v3 restore contract violates metadata-only semantics".into(),
                );
            }
            let plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
            if contract.restores_edp_protocol == plain {
                return Err(
                    "EDPB manifest v3 EDP restore contract conflicts with device state".into(),
                );
            }
        }
        "edpb.manifest.v1" | "edpb.manifest.v2"
            if manifest.backup_purpose.is_some()
                || manifest.restore_contract.is_some()
                || !manifest.partitions.is_empty() =>
        {
            return Err("historical EDPB manifest must not masquerade as v3".into());
        }
        "edpb.manifest.v1" | "edpb.manifest.v2" => {}
        _ => {}
    }
    if manifest.container_version.major != FORMAT_MAJOR {
        return Err(format!(
            "unsupported EDPB major version: {}",
            manifest.container_version.major
        ));
    }
    let plain_metadata_v3 = manifest.schema == "edpb.manifest.v3"
        && manifest.snapshot.device_state.eq_ignore_ascii_case("plain")
        && manifest.snapshot.capture_level == CaptureLevel::Metadata;
    let mut partition_indexes = BTreeSet::new();
    for partition in &manifest.partitions {
        if partition.index == 0 || !partition_indexes.insert(partition.index) {
            return Err("EDPB partition metadata contains invalid/duplicate index".into());
        }
        if partition.sector_count == 0 {
            return Err(format!(
                "EDPB partition {} has zero sectors",
                partition.index
            ));
        }
        let end = partition
            .start_lba
            .checked_add(partition.sector_count)
            .ok_or_else(|| format!("EDPB partition {} range overflow", partition.index))?;
        if manifest
            .geometry
            .total_sectors
            .is_some_and(|total| end > total)
        {
            return Err(format!(
                "EDPB partition {} exceeds source device geometry",
                partition.index
            ));
        }
    }
    if plain_metadata_v3 {
        if manifest.partitions.is_empty() {
            return Err("EDPB Plain metadata backup contains no typed partition entries".into());
        }
        if manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.id == RAW_PROTOCOL_ARTIFACT_ID)
            || manifest
                .regions
                .iter()
                .any(|region| region.id == PROTOCOL_REGION_ID)
            || manifest
                .extents
                .iter()
                .any(|extent| extent.id == RAW_PROTOCOL_EXTENT_ID)
        {
            return Err(
                "EDPB Plain metadata backup must not store fixed LBA0-12 as protocol".into(),
            );
        }
        if manifest.artifacts.iter().any(|artifact| {
            artifact.id.contains("filesystem")
                || artifact.kind.contains("filesystem")
                || artifact.id.contains("directory")
                || artifact.kind.contains("directory")
        }) {
            return Err(
                "EDPB Plain metadata backup must not contain filesystem/directory data".into(),
            );
        }
        if !manifest.artifacts.iter().any(|artifact| {
            artifact.kind == "raw_sectors" && artifact.restore_policy == RestorePolicy::Restorable
        }) {
            return Err(
                "EDPB Plain metadata backup has no restorable partition-table artifact".into(),
            );
        }
    }
    let region_ids: BTreeSet<&str> = manifest.regions.iter().map(|v| v.id.as_str()).collect();
    if region_ids.len() != manifest.regions.len() {
        return Err("duplicate EDPB region id".into());
    }
    let extent_ids: BTreeSet<&str> = manifest.extents.iter().map(|v| v.id.as_str()).collect();
    if extent_ids.len() != manifest.extents.len() {
        return Err("duplicate EDPB extent id".into());
    }
    for extent in &manifest.extents {
        if !region_ids.contains(extent.region_id.as_str()) {
            return Err(format!("Extent {} references missing Region", extent.id));
        }
    }
    let artifact_ids: BTreeSet<&str> = manifest.artifacts.iter().map(|v| v.id.as_str()).collect();
    if artifact_ids.len() != manifest.artifacts.len() {
        return Err("duplicate EDPB artifact id".into());
    }
    for artifact in &manifest.artifacts {
        for extent in &artifact.source_extent_ids {
            if !extent_ids.contains(extent.as_str()) {
                return Err(format!(
                    "Artifact {} references missing Extent",
                    artifact.id
                ));
            }
        }
        if let Some(derivation) = &artifact.derivation {
            for source in &derivation.source_artifact_ids {
                if !artifact_ids.contains(source.as_str()) {
                    return Err(format!("Artifact {} has missing source", artifact.id));
                }
            }
        }
    }

    if !plain_metadata_v3 {
        let protocol_region = manifest
            .regions
            .iter()
            .find(|region| region.id == PROTOCOL_REGION_ID)
            .ok_or_else(|| "EDPB manifest missing protocol Region".to_string())?;
        if protocol_region.start_lba != Some(0) || protocol_region.sector_count != Some(13) {
            return Err("EDPB protocol Region geometry mismatch".into());
        }
        let protocol_extent = manifest
            .extents
            .iter()
            .find(|extent| extent.id == RAW_PROTOCOL_EXTENT_ID)
            .ok_or_else(|| "EDPB manifest missing protocol Extent".to_string())?;
        if protocol_extent.start_lba != 0 || protocol_extent.sector_count != 13 {
            return Err("EDPB protocol Extent geometry mismatch".into());
        }
    }
    for extent in &manifest.extents {
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or_else(|| format!("Extent {} source range overflow", extent.id))?;
        if let Some(total) = manifest.geometry.total_sectors {
            if end > total {
                return Err(format!(
                    "Extent {} exceeds source device geometry",
                    extent.id
                ));
            }
        }
    }
    if !plain_metadata_v3 {
        let raw_protocol = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.id == RAW_PROTOCOL_ARTIFACT_ID)
            .ok_or_else(|| "EDPB manifest missing raw protocol Artifact".to_string())?;
        let expected_protocol_bytes = 13u64
            .checked_mul(manifest.geometry.logical_sector_size as u64)
            .ok_or_else(|| "EDPB protocol byte length overflow".to_string())?;
        if raw_protocol.storage.original_length != expected_protocol_bytes
            || raw_protocol.restore_policy != RestorePolicy::Restorable
        {
            return Err("EDPB raw protocol Artifact contract mismatch".into());
        }
    }
    for artifact in &manifest.artifacts {
        if artifact.kind == "raw_sectors" && artifact.source_extent_ids.len() == 1 {
            let extent = manifest
                .extents
                .iter()
                .find(|extent| extent.id == artifact.source_extent_ids[0])
                .ok_or_else(|| format!("Artifact {} source Extent missing", artifact.id))?;
            let expected = extent
                .sector_count
                .checked_mul(manifest.geometry.logical_sector_size as u64)
                .ok_or_else(|| format!("Artifact {} raw length overflow", artifact.id))?;
            if artifact.storage.original_length != expected {
                return Err(format!(
                    "Artifact {} raw extent length mismatch",
                    artifact.id
                ));
            }
        }
    }
    Ok(())
}
