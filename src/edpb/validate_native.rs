//! Versioned native-sector EDPB contracts.
//! v4 is immutable evidence-only; v5 describes restorable *metadata only*.
//! Neither schema string nor manifest flags grant a write lease.
use super::*;

pub(super) fn validate_schema(manifest: &Manifest) -> Result<(), String> {
    let native_evidence = matches!(
        manifest.schema.as_str(),
        "edpb.manifest.v4" | "edpb.manifest.v5"
    );
    let native_restorable = manifest.schema == "edpb.manifest.v5";
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
    if manifest.schema == "edpb.manifest.v4" {
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
    } else if native_restorable {
        if plain || manifest.snapshot.capture_level != CaptureLevel::Metadata {
            return Err("Native v5 recovery requires complete EDP metadata capture".into());
        }
        if !contract.restores_partition_structure
            || !contract.restores_edp_protocol
            || contract.restores_filesystem
            || contract.restores_user_data
            || !contract.post_restore_assessment_required
            || !manifest.artifacts.iter().any(|a| {
                a.id == RAW_PROTOCOL_ARTIFACT_ID
                    && a.kind == "raw_sectors"
                    && a.restore_policy == RestorePolicy::Restorable
                    && a.completeness == ArtifactCompleteness::Complete
            })
            || !manifest.artifacts.iter().any(|a| {
                a.id == "raw.lba7_compatibility"
                    && a.kind == "raw_sectors"
                    && a.restore_policy == RestorePolicy::Restorable
                    && a.completeness == ArtifactCompleteness::Complete
            })
            || manifest.partitions.is_empty()
            || manifest
                .partitions
                .iter()
                .enumerate()
                .any(|(slot, partition)| {
                    let expected_id = format!("raw.partition_header.{slot}");
                    !manifest.artifacts.iter().any(|a| {
                        a.id == expected_id
                            && a.kind == "raw_sectors"
                            && a.restore_policy == RestorePolicy::Restorable
                            && a.completeness == ArtifactCompleteness::Complete
                            && a.source_extent_ids.len() == 1
                            && manifest.extents.iter().any(|extent| {
                                extent.id == a.source_extent_ids[0]
                                    && extent.start_lba == partition.start_lba
                                    && extent.sector_count == 1
                            })
                    })
                })
            || manifest.artifacts.iter().any(|a| {
                a.kind == "raw_sectors"
                    && (a.restore_policy != RestorePolicy::Restorable
                        || a.completeness != ArtifactCompleteness::Complete
                        || a.source_extent_ids.len() != 1
                        || (a.id != RAW_PROTOCOL_ARTIFACT_ID
                            && a.id != "raw.lba7_compatibility"
                            && !manifest
                                .partitions
                                .iter()
                                .enumerate()
                                .any(|(slot, _)| a.id == format!("raw.partition_header.{slot}"))))
            })
        {
            return Err(
                "Native v5 requires complete restorable raw metadata, no filesystem or user data"
                    .into(),
            );
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
        || lce.restore_policy
            != if manifest.schema == "edpb.manifest.v5" {
                RestorePolicy::Restorable
            } else {
                RestorePolicy::EvidenceOnly
            }
        || lce.completeness != ArtifactCompleteness::Complete
        || lce.source_extent_ids.len() != 1
    {
        return Err(
            "Native LCE policy, completeness or raw extent is inconsistent with schema".into(),
        );
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
        return Err("Native LCE physical extent differs from verified LBA7 pointer".into());
    }
    Ok(())
}

/// A native partition header is restorable only when it is exactly the single
/// first native block of the matching, typed v5 partition. Never permit this
/// exemption for v3/v4 or for arbitrary user-data sector extents.
pub(super) fn authorized_v5_partition_header(
    manifest: &Manifest,
    artifact: &Artifact,
    extent: &Extent,
) -> bool {
    manifest.schema == "edpb.manifest.v5"
        && extent.sector_count == 1
        && manifest
            .partitions
            .iter()
            .enumerate()
            .any(|(slot, partition)| {
                artifact.id == format!("raw.partition_header.{slot}")
                    && extent.start_lba == partition.start_lba
            })
}

#[cfg(test)]
mod versioned_contract_tests {
    use super::*;

    fn artifact(
        id: &str,
        extent: &str,
        sector: u32,
        blocks: u64,
        policy: RestorePolicy,
    ) -> Artifact {
        Artifact {
            id: id.into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent.into()],
            derivation: None,
            restore_policy: policy,
            completeness: ArtifactCompleteness::Complete,
            storage: ChunkStorage {
                frame_offset: 96,
                data_offset: 160,
                stored_length: blocks * u64::from(sector),
                original_length: blocks * u64::from(sector),
                codec: "none".into(),
                sha256: "0".repeat(64),
            },
        }
    }

    fn fixture(sector: u32) -> Manifest {
        let prefix = vec![0u8; 13 * sector as usize];
        let capture = CoreCapture {
            snapshot_id: "native-contract-test".into(),
            created_epoch: 1_790_000_000,
            disk_number: None,
            vid: "1234".into(),
            pid: "5678".into(),
            device_id: "test-native-device".into(),
            onlyid: None,
            total_sectors: Some(12000),
            logical_sector_size: sector,
            edpcli_version: "test".into(),
            device_state: "edp".into(),
            lba0_12: &prefix,
        };
        let mut manifest = super::super::write::base_manifest(&capture, None);
        manifest.snapshot.capture_level = CaptureLevel::Metadata;
        manifest.regions.push(Region {
            id: "region.lce".into(),
            role: "legacy".into(),
            start_lba: Some(10000),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        });
        manifest.extents.push(Extent {
            id: "extent.lce".into(),
            region_id: "region.lce".into(),
            start_lba: 10000,
            sector_count: 1,
            purpose: "native_lce".into(),
        });
        manifest.partitions.push(ManifestPartition {
            index: 1,
            role: Some("encrypt".into()),
            partition_type: Some("4".into()),
            start_lba: 2048,
            sector_count: 4096,
            filesystem_hint: None,
            volume_label_hint: None,
        });
        manifest.regions.push(Region {
            id: "region.partition_header.0".into(),
            role: "native_partition_header_evidence".into(),
            start_lba: Some(2048),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        });
        manifest.extents.push(Extent {
            id: "extent.partition_header.0".into(),
            region_id: "region.partition_header.0".into(),
            start_lba: 2048,
            sector_count: 1,
            purpose: "native_partition_header_evidence".into(),
        });
        manifest.artifacts.push(artifact(
            RAW_PROTOCOL_ARTIFACT_ID,
            RAW_PROTOCOL_EXTENT_ID,
            sector,
            13,
            RestorePolicy::EvidenceOnly,
        ));
        manifest.artifacts.push(artifact(
            "raw.partition_header.0",
            "extent.partition_header.0",
            sector,
            1,
            RestorePolicy::EvidenceOnly,
        ));
        manifest.artifacts.push(artifact(
            "raw.lba7_compatibility",
            "extent.lce",
            sector,
            1,
            RestorePolicy::EvidenceOnly,
        ));
        manifest
    }

    #[test]
    fn v4_remains_evidence_only_even_if_restore_flags_are_forged() {
        for sector in [1024, 2048, 4096] {
            let mut v4 = fixture(sector);
            validate_schema(&v4).unwrap();
            validate_required_lce(&v4).unwrap();
            v4.restore_contract = RestoreContract::metadata_only(true);
            assert!(validate_schema(&v4).is_err(), "sector={sector}");
            v4.restore_contract.restores_partition_structure = false;
            v4.restore_contract.restores_edp_protocol = false;
            v4.artifacts[0].restore_policy = RestorePolicy::Restorable;
            assert!(validate_schema(&v4).is_err(), "sector={sector}");
        }
    }

    #[test]
    fn v5_requires_complete_native_metadata_without_user_data() {
        for sector in [1024, 2048, 4096] {
            let mut v5 = fixture(sector);
            v5.schema = "edpb.manifest.v5".into();
            v5.restore_contract = RestoreContract::metadata_only(true);
            for artifact in &mut v5.artifacts {
                artifact.restore_policy = RestorePolicy::Restorable;
            }
            validate_schema(&v5).unwrap();
            validate_required_lce(&v5).unwrap();

            let mut missing_lce = v5.clone();
            missing_lce.artifacts.pop();
            assert!(validate_schema(&missing_lce).is_err(), "sector={sector}");
            let mut wrong_policy = v5.clone();
            wrong_policy.artifacts[2].restore_policy = RestorePolicy::EvidenceOnly;
            assert!(validate_schema(&wrong_policy).is_err(), "sector={sector}");
            assert!(validate_required_lce(&wrong_policy).is_err());
            let mut partial_protocol = v5.clone();
            partial_protocol.artifacts[0].completeness = ArtifactCompleteness::Partial;
            assert!(
                validate_schema(&partial_protocol).is_err(),
                "sector={sector}"
            );
            let mut missing_header = v5.clone();
            missing_header.artifacts.remove(1);
            assert!(validate_schema(&missing_header).is_err(), "sector={sector}");
            let mut moved_header = v5.clone();
            moved_header
                .extents
                .iter_mut()
                .find(|extent| extent.id == "extent.partition_header.0")
                .unwrap()
                .start_lba += 1;
            assert!(validate_schema(&moved_header).is_err(), "sector={sector}");
            let mut extra_raw = v5.clone();
            extra_raw.artifacts.push(artifact(
                "raw.unowned_user_data",
                "extent.partition_header.0",
                sector,
                1,
                RestorePolicy::Restorable,
            ));
            assert!(validate_schema(&extra_raw).is_err(), "sector={sector}");
            let mut user_data = v5.clone();
            user_data.restore_contract.restores_user_data = true;
            assert!(validate_schema(&user_data).is_err(), "sector={sector}");
            let mut filesystem = v5.clone();
            filesystem.restore_contract.restores_filesystem = true;
            assert!(validate_schema(&filesystem).is_err(), "sector={sector}");
            let mut plain = v5.clone();
            plain.snapshot.device_state = "plain".into();
            assert!(validate_schema(&plain).is_err(), "sector={sector}");
        }

        let mut invalid_sector = fixture(512);
        invalid_sector.schema = "edpb.manifest.v5".into();
        invalid_sector.restore_contract = RestoreContract::metadata_only(true);
        assert!(validate_schema(&invalid_sector).is_err());
    }
}
