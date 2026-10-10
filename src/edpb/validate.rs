use super::*;

pub(super) fn validate_manifest_graph(manifest: &Manifest) -> Result<(), String> {
    validate_manifest_identity(manifest)?;
    super::limits::validate_payload_lengths(
        manifest.artifacts.iter().map(|a| a.storage.stored_length),
    )?;
    let native_evidence = matches!(
        manifest.schema.as_str(),
        "edpb.manifest.v4" | "edpb.manifest.v5"
    );
    let plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
    super::validate_native::validate_schema(manifest)?;
    if manifest.container_version.major != FORMAT_MAJOR {
        return Err(format!(
            "unsupported EDPB major version: {}",
            manifest.container_version.major
        ));
    }
    let plain_metadata = manifest.snapshot.device_state.eq_ignore_ascii_case("plain")
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
    if plain_metadata {
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

    if native_evidence {
        super::validate_raw_extents::validate_unique_native_raw_extents(manifest)?;
    }

    if !plain_metadata {
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
    if !plain_metadata {
        let raw_protocol = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.id == RAW_PROTOCOL_ARTIFACT_ID)
            .ok_or_else(|| "EDPB manifest missing raw protocol Artifact".to_string())?;
        let expected_protocol_bytes = 13u64
            .checked_mul(manifest.geometry.logical_sector_size as u64)
            .ok_or_else(|| "EDPB protocol byte length overflow".to_string())?;
        let required_policy = if manifest.schema == "edpb.manifest.v4" {
            RestorePolicy::EvidenceOnly
        } else {
            RestorePolicy::Restorable
        };
        if raw_protocol.storage.original_length != expected_protocol_bytes
            || raw_protocol.restore_policy != required_policy
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
    if native_evidence {
        super::validate_native::validate_required_lce(manifest)?;
    }
    let mut restore_ranges = Vec::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|a| a.restore_policy == RestorePolicy::Restorable)
    {
        if artifact.kind != "raw_sectors"
            || artifact.completeness != ArtifactCompleteness::Complete
            || artifact.source_extent_ids.len() != 1
        {
            return Err(format!(
                "Artifact {} is not complete raw metadata",
                artifact.id
            ));
        }
        let extent = manifest
            .extents
            .iter()
            .find(|e| e.id == artifact.source_extent_ids[0])
            .ok_or("restorable extent missing")?;
        let region = manifest
            .regions
            .iter()
            .find(|r| r.id == extent.region_id)
            .ok_or("restorable region missing")?;
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or("restorable extent overflow")?;
        if extent.sector_count == 0 || region.semantic_status != SemanticStatus::Identified {
            return Err("unidentified/empty restorable metadata".into());
        }
        if let (Some(start), Some(count)) = (region.start_lba, region.sector_count) {
            if extent.start_lba < start
                || start
                    .checked_add(count)
                    .is_none_or(|region_end| end > region_end)
            {
                return Err("restorable extent outside region".into());
            }
        }
        let authorized = match (region.role.as_str(), extent.purpose.as_str()) {
            ("protocol", "raw_protocol_snapshot") => {
                (!plain || manifest.snapshot.capture_level == CaptureLevel::Core)
                    && extent.start_lba == 0
                    && extent.sector_count == 13
                    && artifact.id == RAW_PROTOCOL_ARTIFACT_ID
            }
            (
                "lba7_legacy_partition_compatibility_extent",
                "lba7_compatibility_extent_ciphertext",
            ) => !plain && artifact.id == "raw.lba7_compatibility",
            ("lba4_lba12_backup_mirror", "historical_lba4_lba12_mirror") => {
                !plain
                    && artifact.id == "raw.tail.metadata_mirror_512k"
                    && manifest
                        .geometry
                        .total_sectors
                        .and_then(|t| t.checked_sub(1024))
                        == Some(extent.start_lba)
                    && extent.sector_count == 9
            }
            ("historical_restore_node_mirror", "historical_restore_node_mirror") => {
                !plain
                    && artifact.id == "raw.tail.restore_node_end4"
                    && manifest
                        .geometry
                        .total_sectors
                        .and_then(|t| t.checked_sub(4))
                        == Some(extent.start_lba)
                    && extent.sector_count == 1
            }
            ("native_partition_header_evidence", "native_partition_header_evidence") => {
                super::validate_native::authorized_v5_partition_header(manifest, artifact, extent)
            }
            ("plain_partition_table", _) => plain_metadata,
            _ => false,
        };
        if !authorized {
            return Err(format!(
                "Artifact {} has unauthorized restore semantics",
                artifact.id
            ));
        }
        if artifact.storage.codec != "none"
            || artifact.storage.stored_length != artifact.storage.original_length
        {
            return Err("raw metadata storage length/codec mismatch".into());
        }
        restore_ranges.push((extent.start_lba, end));
    }
    restore_ranges.sort_unstable();
    if restore_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("restorable metadata ranges overlap".into());
    }
    Ok(())
}

/// Validates semantic authority using the exact immutable artifact bytes.
pub(super) fn validate_restore_evidence<'a>(
    manifest: &Manifest,
    mut read: impl FnMut(&str) -> Result<&'a [u8], String>,
) -> Result<(), String> {
    if matches!(
        manifest.schema.as_str(),
        "edpb.manifest.v4" | "edpb.manifest.v5"
    ) {
        return super::validate_native::validate_native_evidence(manifest, &mut read);
    }
    let plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
    if plain && manifest.snapshot.capture_level == CaptureLevel::Metadata {
        let total = manifest
            .geometry
            .total_sectors
            .ok_or("Plain metadata requires source geometry")?;
        let mut sectors = std::collections::BTreeMap::new();
        for artifact in manifest
            .artifacts
            .iter()
            .filter(|a| a.restore_policy == RestorePolicy::Restorable)
        {
            let extent = manifest
                .extents
                .iter()
                .find(|e| e.id == artifact.source_extent_ids[0])
                .ok_or("Plain extent missing")?;
            for (offset, sector) in read(&artifact.id)?.as_chunks::<512>().0.iter().enumerate() {
                sectors.insert(extent.start_lba + offset as u64, sector.to_vec());
            }
        }
        let (table, _) = crate::partition_table::capture_partition_table(total, |lba| {
            sectors
                .get(&lba)
                .cloned()
                .ok_or_else(|| format!("Plain backup missing table sector {lba}"))
        })?;
        let expected: BTreeSet<_> = table
            .table_extents
            .iter()
            .map(|e| (e.start_lba, e.sector_count))
            .collect();
        let actual: BTreeSet<_> = manifest
            .artifacts
            .iter()
            .filter(|a| a.restore_policy == RestorePolicy::Restorable)
            .map(|a| {
                let e = manifest
                    .extents
                    .iter()
                    .find(|e| e.id == a.source_extent_ids[0])
                    .expect("graph checked");
                (e.start_lba, e.sector_count)
            })
            .collect();
        if actual != expected {
            return Err("Plain restorable extents differ from validated partition table".into());
        }
        if table.partitions.len() != manifest.partitions.len()
            || table
                .partitions
                .iter()
                .zip(&manifest.partitions)
                .any(|(a, b)| {
                    a.index as u32 != b.index
                        || a.start_lba != b.start_lba
                        || a.sector_count != b.sector_count
                })
        {
            return Err("Plain partition geometry differs from evidence".into());
        }
    } else if let Some(artifact) = manifest
        .artifacts
        .iter()
        .find(|a| a.restore_policy == RestorePolicy::Restorable && a.id == "raw.lba7_compatibility")
    {
        let total = manifest
            .geometry
            .total_sectors
            .ok_or("LCE requires source geometry")?;
        let geometry = crate::backup_metadata::parse_lba7_compatibility_geometry(
            read(RAW_PROTOCOL_ARTIFACT_ID)?,
            &manifest.device.device_id,
            total,
        )?;
        let extent = manifest
            .extents
            .iter()
            .find(|e| e.id == artifact.source_extent_ids[0])
            .ok_or("LCE extent missing")?;
        if extent.start_lba != geometry.start_lba || extent.sector_count != geometry.sector_count {
            return Err("LCE extent differs from protocol pointer".into());
        }
    }
    Ok(())
}
