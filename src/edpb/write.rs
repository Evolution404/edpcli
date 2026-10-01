use super::*;

pub(super) fn base_manifest(
    capture: &CoreCapture<'_>,
    identity: Option<&crate::media_identity::MediaIdentitySnapshot>,
    schema: &str,
) -> Manifest {
    let capacity_bytes = capture
        .total_sectors
        .and_then(|sectors| sectors.checked_mul(capture.logical_sector_size as u64));
    let typed_identity = matches!(schema, "edpb.manifest.v2" | "edpb.manifest.v3").then(|| {
        identity
            .map(|snapshot| {
                if schema == "edpb.manifest.v2" {
                    manifest_identity_from_snapshot_v2(snapshot)
                } else {
                    manifest_identity_from_snapshot(snapshot)
                }
            })
            .unwrap_or_else(|| inferred_manifest_identity(capture))
    });
    let manifest_v3 = schema == "edpb.manifest.v3";
    let restores_edp_protocol = !capture.device_state.eq_ignore_ascii_case("plain");
    Manifest {
        schema: schema.into(),
        container_version: ContainerVersion {
            major: FORMAT_MAJOR,
            minor: FORMAT_MINOR,
        },
        snapshot: SnapshotInfo {
            snapshot_id: capture.snapshot_id.clone(),
            created_epoch: capture.created_epoch,
            capture_level: CaptureLevel::Core,
            device_state: capture.device_state.clone(),
        },
        backup_purpose: manifest_v3.then_some(BackupPurpose::MetadataOnly),
        restore_contract: manifest_v3
            .then(|| RestoreContract::metadata_only(restores_edp_protocol)),
        device: DeviceIdentity {
            vid: capture.vid.clone(),
            pid: capture.pid.clone(),
            device_id: capture.device_id.clone(),
            onlyid: capture.onlyid.clone(),
        },
        identity: typed_identity,
        geometry: DeviceGeometry {
            logical_sector_size: capture.logical_sector_size,
            physical_sector_size: None,
            total_sectors: capture.total_sectors,
            capacity_bytes,
        },
        observation: Observation {
            disk_number: capture.disk_number,
            platform: std::env::consts::OS.to_string(),
            edpcli_version: capture.edpcli_version.clone(),
        },
        partitions: Vec::new(),
        regions: vec![Region {
            id: PROTOCOL_REGION_ID.into(),
            role: "protocol".into(),
            start_lba: Some(0),
            sector_count: Some(13),
            semantic_status: SemanticStatus::Identified,
        }],
        extents: vec![Extent {
            id: RAW_PROTOCOL_EXTENT_ID.into(),
            region_id: PROTOCOL_REGION_ID.into(),
            start_lba: 0,
            sector_count: 13,
            purpose: "raw_protocol_snapshot".into(),
        }],
        artifacts: Vec::new(),
        provenance: Provenance {
            capture_source: "physical_device".into(),
            source_format: "raw_device".into(),
            notes: vec!["raw evidence is authoritative; derived data must never replace it".into()],
        },
    }
}

pub(super) fn validate_core_capture(capture: &CoreCapture<'_>) -> Result<(), String> {
    if capture.logical_sector_size == 0 {
        return Err("logical_sector_size must not be zero".into());
    }
    let expected_len = 13usize
        .checked_mul(capture.logical_sector_size as usize)
        .ok_or_else(|| "LBA0-12 length overflow".to_string())?;
    if capture.lba0_12.len() != expected_len {
        return Err(format!(
            "LBA0-12 length is {} bytes, expected {} bytes",
            capture.lba0_12.len(),
            expected_len
        ));
    }
    Ok(())
}

// One internal entry point carries the complete EDPB capture graph and identity provenance.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_container(
    path: &Path,
    capture: &CoreCapture<'_>,
    capture_level: CaptureLevel,
    extra_partitions: &[ManifestPartition],
    extra_regions: &[Region],
    extra_extents: &[Extent],
    extra_artifacts: &[ArtifactInput],
    extra_notes: &[String],
    identity: Option<&crate::media_identity::MediaIdentitySnapshot>,
    schema: &str,
) -> Result<Manifest, String> {
    validate_core_capture(capture)?;
    if path.extension().and_then(|v| v.to_str()) != Some(EXTENSION) {
        return Err(format!("EDPB file must use .{EXTENSION} extension"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create backup directory failed: {e}"))?;
    }

    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("create EDPB failed {}: {e}", path.display()))?;

    let result = (|| -> Result<Manifest, String> {
        file.write_all(&[0u8; HEADER_SIZE])
            .map_err(|e| format!("write EDPB header failed: {e}"))?;

        let mut manifest = base_manifest(capture, identity, schema);
        manifest.snapshot.capture_level = capture_level;
        manifest.partitions.extend_from_slice(extra_partitions);
        let plain_metadata_v3 = schema == "edpb.manifest.v3"
            && capture.device_state.eq_ignore_ascii_case("plain")
            && capture_level == CaptureLevel::Metadata;
        if plain_metadata_v3 {
            manifest.regions.clear();
            manifest.extents.clear();
        }
        manifest.regions.extend_from_slice(extra_regions);
        manifest.extents.extend_from_slice(extra_extents);
        manifest.provenance.notes.extend_from_slice(extra_notes);

        let mut inputs =
            Vec::with_capacity(extra_artifacts.len() + usize::from(!plain_metadata_v3));
        if !plain_metadata_v3 {
            inputs.push(ArtifactInput {
                id: RAW_PROTOCOL_ARTIFACT_ID.into(),
                kind: "raw_sectors".into(),
                media_type: "application/octet-stream".into(),
                source_extent_ids: vec![RAW_PROTOCOL_EXTENT_ID.into()],
                derivation: None,
                restore_policy: RestorePolicy::Restorable,
                completeness: ArtifactCompleteness::Complete,
                data: capture.lba0_12.to_vec(),
            });
        }
        inputs.extend_from_slice(extra_artifacts);

        for input in inputs {
            let frame_offset = file
                .stream_position()
                .map_err(|e| format!("read EDPB chunk position failed: {e}"))?;
            file.write_all(&make_chunk_header(&input.data))
                .map_err(|e| format!("write EDPB chunk header failed: {e}"))?;
            let data_offset = frame_offset + CHUNK_HEADER_SIZE as u64;
            file.write_all(&input.data)
                .map_err(|e| format!("write EDPB chunk {} failed: {e}", input.id))?;
            manifest.artifacts.push(Artifact {
                id: input.id,
                kind: input.kind,
                media_type: input.media_type,
                source_extent_ids: input.source_extent_ids,
                derivation: input.derivation,
                restore_policy: input.restore_policy,
                completeness: input.completeness,
                storage: ChunkStorage {
                    frame_offset,
                    data_offset,
                    stored_length: input.data.len() as u64,
                    original_length: input.data.len() as u64,
                    codec: "none".into(),
                    sha256: hex(&sha256_bytes(&input.data)),
                },
            });
        }

        let manifest_offset = file
            .stream_position()
            .map_err(|e| format!("read EDPB write position failed: {e}"))?;
        validate_manifest_graph(&manifest)?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("serialize EDPB manifest failed: {e}"))?;
        let manifest_sha = sha256_bytes(&manifest_bytes);
        file.write_all(&manifest_bytes)
            .map_err(|e| format!("write EDPB manifest failed: {e}"))?;
        let footer_offset = file
            .stream_position()
            .map_err(|e| format!("read EDPB footer position failed: {e}"))?;
        let file_size = footer_offset + FOOTER_SIZE as u64;
        file.write_all(&make_footer(
            manifest_offset,
            manifest_bytes.len() as u64,
            file_size,
            &manifest_sha,
        ))
        .map_err(|e| format!("write EDPB footer failed: {e}"))?;

        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("seek EDPB header failed: {e}"))?;
        file.write_all(&make_header(
            capture.created_epoch,
            manifest_offset,
            manifest_bytes.len() as u64,
            footer_offset,
            &manifest_sha,
        ))
        .map_err(|e| format!("rewrite EDPB header failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync EDPB failed: {e}"))?;
        Ok(manifest)
    })();

    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

pub fn write_core_backup(path: &Path, capture: &CoreCapture<'_>) -> Result<Manifest, String> {
    write_container(
        path,
        capture,
        CaptureLevel::Core,
        &[],
        &[],
        &[],
        &[],
        &[],
        None,
        "edpb.manifest.v3",
    )
}

pub fn write_core_backup_with_identity(
    path: &Path,
    capture: &CoreCapture<'_>,
    identity: &crate::media_identity::MediaIdentitySnapshot,
) -> Result<Manifest, String> {
    write_container(
        path,
        capture,
        CaptureLevel::Core,
        &[],
        &[],
        &[],
        &[],
        &[],
        Some(identity),
        "edpb.manifest.v3",
    )
}

#[doc(hidden)]
pub fn write_legacy_v2_core_backup_with_identity(
    path: &Path,
    capture: &CoreCapture<'_>,
    identity: &crate::media_identity::MediaIdentitySnapshot,
) -> Result<Manifest, String> {
    write_container(
        path,
        capture,
        CaptureLevel::Core,
        &[],
        &[],
        &[],
        &[],
        &[],
        Some(identity),
        "edpb.manifest.v2",
    )
}

pub fn write_core_backup_with_notes(
    path: &Path,
    capture: &CoreCapture<'_>,
    notes: &[String],
) -> Result<Manifest, String> {
    write_container(
        path,
        capture,
        CaptureLevel::Core,
        &[],
        &[],
        &[],
        &[],
        notes,
        None,
        "edpb.manifest.v3",
    )
}

pub fn write_metadata_backup(
    path: &Path,
    capture: &MetadataCapture<'_>,
) -> Result<Manifest, String> {
    write_container(
        path,
        &capture.core,
        CaptureLevel::Metadata,
        &capture.partitions,
        &capture.regions,
        &capture.extents,
        &capture.artifacts,
        &capture.notes,
        None,
        "edpb.manifest.v3",
    )
}

pub fn write_metadata_backup_with_identity(
    path: &Path,
    capture: &MetadataCapture<'_>,
    identity: &crate::media_identity::MediaIdentitySnapshot,
) -> Result<Manifest, String> {
    write_container(
        path,
        &capture.core,
        CaptureLevel::Metadata,
        &capture.partitions,
        &capture.regions,
        &capture.extents,
        &capture.artifacts,
        &capture.notes,
        Some(identity),
        "edpb.manifest.v3",
    )
}
