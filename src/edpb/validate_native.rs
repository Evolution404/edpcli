//! Native 1024B/2048B/4096B EDPB v4: evidence-only format validation.
//! Kept separate from the legacy 512B recoverable v3 contract.
use super::*;

pub(super) fn validate_schema(manifest: &Manifest) -> Result<(), String> {
    let native_evidence = manifest.schema == "edpb.manifest.v4";
    if (native_evidence && !matches!(manifest.geometry.logical_sector_size, 1024 | 2048 | 4096))
        || (!native_evidence && manifest.geometry.logical_sector_size != 512)
    {
        return Err("EDPB schema and verified logical sector size disagree".into());
    }
    if manifest.backup_purpose != BackupPurpose::MetadataOnly {
        return Err("EDPB must declare metadata_only backup purpose".into());
    }
    let contract = &manifest.restore_contract;
    let plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
    if native_evidence {
        if plain || manifest.snapshot.capture_level != CaptureLevel::Metadata {
            return Err("Native evidence currently supports EDP metadata only".into());
        }
        if contract.restores_partition_structure
            || contract.restores_edp_protocol
            || contract.restores_filesystem
            || contract.restores_user_data
            || !contract.post_restore_assessment_required
            || manifest
                .artifacts
                .iter()
                .any(|a| a.restore_policy == RestorePolicy::Restorable)
        {
            return Err("Native v4 is evidence-only; restore authorization forbidden".into());
        }
    } else if !contract.restores_partition_structure
        || contract.restores_filesystem
        || contract.restores_user_data
        || !contract.post_restore_assessment_required
        || contract.restores_edp_protocol == plain
    {
        return Err("EDPB v3 metadata restore contract mismatch".into());
    }
    Ok(())
}

pub(super) fn validate_required_lce(manifest: &Manifest) -> Result<(), String> {
    let lce = manifest
        .artifacts
        .iter()
        .find(|a| a.id == "raw.lba7_compatibility")
        .ok_or("Native EDP evidence requires complete native LCE artifact")?;
    if lce.kind != "raw_sectors"
        || lce.restore_policy != RestorePolicy::EvidenceOnly
        || lce.completeness != ArtifactCompleteness::Complete
        || lce.source_extent_ids.len() != 1
    {
        return Err("Native LCE must be complete nonrestorable raw evidence".into());
    }
    Ok(())
}

pub(super) fn validate_native_evidence<'a>(
    manifest: &Manifest,
    read: &mut impl FnMut(&str) -> Result<&'a [u8], String>,
) -> Result<(), String> {
    let total = manifest
        .geometry
        .total_sectors
        .ok_or("Native EDP missing native capacity")?;
    let protocol = crate::protocol::image::NativeProtocolImage::from_native_bytes(
        manifest.geometry.logical_sector_size,
        read(RAW_PROTOCOL_ARTIFACT_ID)?.to_vec(),
    )
    .map_err(|error| error.to_string())?;
    let projected = protocol.protocol_projection();
    let did = &manifest.device.device_id;
    crate::domain::geometry::parse_partition_geometry_with_sector_bytes(
        &projected,
        did,
        total,
        manifest.geometry.logical_sector_size,
    )?;
    let pointer = crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
        &projected,
        did,
        total,
        manifest.geometry.logical_sector_size,
    )?;
    let lce = manifest
        .artifacts
        .iter()
        .find(|a| a.id == "raw.lba7_compatibility")
        .ok_or("Native LCE missing")?;
    let extent = manifest
        .extents
        .iter()
        .find(|e| e.id == lce.source_extent_ids[0])
        .ok_or("Native LCE extent missing")?;
    if extent.start_lba != pointer.start_lba || extent.sector_count != pointer.sector_count {
        return Err("4Kn LCE physical extent differs from verified LBA7 pointer".into());
    }
    Ok(())
}
