use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use edpcli::application::media_identity::{
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentitySnapshot,
    ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::edpb::{
    canonical_media_identity, read_raw_protocol, verify_file, write_core_backup,
    write_core_backup_with_identity, CaptureLevel, CoreCapture, RestorePolicy,
    RAW_PROTOCOL_ARTIFACT_ID,
};
use edpcli::provision::DiskProvisionKind;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("edpcli_{tag}_{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn capture<'a>(data: &'a [u8]) -> CoreCapture<'a> {
    CoreCapture {
        snapshot_id: "1402259934-20260922T150000".into(),
        created_epoch: 1_790_000_000,
        disk_number: Some(6),
        vid: "0dd8".into(),
        pid: "2005".into(),
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        onlyid: Some("1402259934".into()),
        total_sectors: Some(122_880_000),
        logical_sector_size: 512,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "encrypted".into(),
        lba0_12: data,
    }
}

fn typed_identity(quality: SerialQuality, serial: Option<&str>) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            serial: serial.map(str::to_string),
            serial_sha256: None,
            serial_quality: quality,
            vendor: Some("Netac".into()),
            product: Some("OnlyDisk".into()),
            revision: Some("1.00".into()),
            transport: None,
            total_sectors: Some(122_880_000),
            logical_sector_size: Some(512),
        },
        protocol: ProtocolIdentityEvidence {
            device_id: Some("disk&ven_netac&prod_onlydisk".into()),
            onlyid: Some("1402259934".into()),
            provision_kind: Some(DiskProvisionKind::Mode0),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence {
            device_id_candidates: vec!["disk&ven_netac&prod_onlydisk".into()],
            legacy_derived_candidate: None,
        },
        observation: IdentityObservation::default(),
    }
}

#[test]
fn core_container_round_trips_raw_protocol_and_manifest() {
    let tmp = TempDir::new("edpb_roundtrip");
    let path = tmp.0.join("sample.edpb");
    let data: Vec<u8> = (0..13 * 512).map(|i| (i % 251) as u8).collect();

    write_core_backup(&path, &capture(&data)).unwrap();
    let verified = verify_file(&path).unwrap();
    assert_eq!(verified.manifest.snapshot.capture_level, CaptureLevel::Core);
    assert_eq!(
        verified.manifest.device.onlyid.as_deref(),
        Some("1402259934")
    );
    assert_eq!(
        verified.manifest.geometry.capacity_bytes,
        Some(62_914_560_000)
    );
    let artifact = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == RAW_PROTOCOL_ARTIFACT_ID)
        .unwrap();
    assert_eq!(artifact.restore_policy, RestorePolicy::Restorable);
    assert_eq!(read_raw_protocol(&path).unwrap(), data);
}

#[test]
fn chapter_18_b1_new_writer_uses_manifest_v3_metadata_restore_contract() {
    let tmp = TempDir::new("edpb_manifest_v3");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA5; 13 * 512];

    write_core_backup(&path, &capture(&data)).unwrap();
    let verified = verify_file(&path).unwrap();
    let json = serde_json::to_value(&verified.manifest).unwrap();

    assert_eq!(json["schema"], "edpb.manifest.v3");
    assert_eq!(json["backup_purpose"], "metadata_only");
    assert_eq!(
        json["restore_contract"]["restores_partition_structure"],
        true
    );
    assert_eq!(json["restore_contract"]["restores_edp_protocol"], true);
    assert_eq!(json["restore_contract"]["restores_filesystem"], false);
    assert_eq!(json["restore_contract"]["restores_user_data"], false);
    assert_eq!(
        json["restore_contract"]["post_restore_assessment_required"],
        true
    );
    assert_eq!(
        json["identity"]["protocol"]["device_id"],
        "disk&ven_netac&prod_onlydisk"
    );
    assert_eq!(json["identity"]["protocol"]["onlyid"], "1402259934");
    assert!(
        json["identity"]["hardware"].is_object(),
        "v3 must formally carry typed hardware identity"
    );
    assert!(
        json["identity"]["derived"].is_object(),
        "v3 must formally carry typed derived evidence"
    );
}

#[test]
fn chapter_18_b1_v3_persists_raw_serial_and_never_persists_new_serial_digest() {
    let tmp = TempDir::new("edpb_manifest_v3_raw_serial");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA6; 13 * 512];
    let identity = typed_identity(SerialQuality::Usable, Some("NETAC-RAW-SERIAL-001"));

    write_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();
    let verified = verify_file(&path).unwrap();
    let json = serde_json::to_value(&verified.manifest).unwrap();

    assert_eq!(json["schema"], "edpb.manifest.v3");
    assert_eq!(
        json["identity"]["hardware"]["serial"],
        "NETAC-RAW-SERIAL-001"
    );
    assert!(
        json["identity"]["hardware"].get("serial_sha256").is_none(),
        "v3 must not persist a newly generated serial_sha256 field"
    );
    let canonical = canonical_media_identity(&verified.manifest).unwrap();
    assert_eq!(
        canonical.hardware.serial.as_deref(),
        Some("NETAC-RAW-SERIAL-001")
    );
    assert_eq!(canonical.hardware.serial_sha256, None);
}

#[test]
fn current_manifest_preserves_each_serial_quality_without_digest_fallback() {
    let tmp = TempDir::new("edpb_v3_serial_quality");
    let data = vec![0xA6; 13 * 512];
    for (quality, serial) in [
        (SerialQuality::Usable, Some("NETAC-RAW-SERIAL-001")),
        (SerialQuality::Suspicious, Some("000000")),
        (SerialQuality::Missing, None),
    ] {
        let path = tmp.0.join(format!("{quality:?}.edpb"));
        let identity = typed_identity(quality, serial);
        write_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();
        let canonical = canonical_media_identity(&verify_file(&path).unwrap().manifest).unwrap();
        assert_eq!(canonical.hardware.serial_quality, quality);
        assert_eq!(canonical.hardware.serial.as_deref(), serial);
        assert_eq!(canonical.hardware.serial_sha256, None);
    }
}

#[test]
fn authenticated_containers_reject_retired_manifest_schemas_and_capture_level() {
    let tmp = TempDir::new("edpb_retired_schema");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA6; 13 * 512];
    for schema in ["edpb.manifest.v1", "edpb.manifest.v2"] {
        let path = tmp.0.join(format!("{schema}.edpb"));
        write_core_backup(&path, &capture(&data)).unwrap();
        super::edpb_manifest::mutate(&path, |manifest| manifest["schema"] = schema.into());
        let error = verify_file(&path).unwrap_err();
        assert!(
            error.contains("unsupported EDPB manifest schema"),
            "{error}"
        );
        assert!(read_raw_protocol(&path).is_err());
    }
    write_core_backup(&path, &capture(&data)).unwrap();
    super::edpb_manifest::mutate(&path, |manifest| {
        manifest["snapshot"]["capture_level"] = "legacy_migrated".into();
    });
    let error = verify_file(&path).unwrap_err();
    assert!(
        error.contains("unknown variant `legacy_migrated`"),
        "{error}"
    );
}

#[test]
fn current_manifest_rejects_old_serial_digest_field_and_authority_note() {
    let tmp = TempDir::new("edpb_retired_serial_digest");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA6; 13 * 512];
    write_core_backup(&path, &capture(&data)).unwrap();
    super::edpb_manifest::mutate(&path, |manifest| {
        manifest["identity"]["hardware"]["serial_sha256"] = "ab".repeat(32).into();
    });
    let error = verify_file(&path).unwrap_err();
    assert!(error.contains("unknown field `serial_sha256`"), "{error}");

    let path = tmp.0.join("digest-note.edpb");
    write_core_backup(&path, &capture(&data)).unwrap();
    super::edpb_manifest::mutate(&path, |manifest| {
        manifest["provenance"]["notes"]
            .as_array_mut()
            .unwrap()
            .push(format!("hardware_serial_sha256={}", "ab".repeat(32)).into());
    });
    let error = verify_file(&path).unwrap_err();
    assert!(
        error.contains("must not carry serial digest notes"),
        "{error}"
    );
}

#[test]
fn new_plain_manifest_never_promotes_legacy_candidate_to_observed_device_id() {
    let tmp = TempDir::new("edpb_plain_identity");
    let path = tmp.0.join("plain.edpb");
    let data = vec![0u8; 13 * 512];
    let mut plain = capture(&data);
    plain.device_state = "plain".into();
    plain.onlyid = None;

    write_core_backup(&path, &plain).unwrap();
    let verified = verify_file(&path).unwrap();
    let json = serde_json::to_value(&verified.manifest).unwrap();

    assert_eq!(json["schema"], "edpb.manifest.v3");
    assert!(
        json["identity"]["protocol"]["device_id"].is_null(),
        "Plain must not have an observed EDP device_id"
    );
    assert!(
        json["identity"]["protocol"]["onlyid"].is_null(),
        "Plain must not have an observed EDP onlyid"
    );
    assert_eq!(
        json["identity"]["derived"]["legacy_derived_candidate"],
        "disk&ven_netac&prod_onlydisk"
    );
}

#[test]
fn verify_rejects_payload_corruption() {
    let tmp = TempDir::new("edpb_corrupt_payload");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA5; 13 * 512];
    let manifest = write_core_backup(&path, &capture(&data)).unwrap();
    let artifact = &manifest.artifacts[0];
    let mut file = fs::read(&path).unwrap();
    file[artifact.storage.data_offset as usize + 17] ^= 0x5A;
    fs::write(&path, file).unwrap();
    let err = verify_file(&path).unwrap_err();
    assert!(err.contains("SHA-256"), "{err}");
}

#[test]
fn verify_rejects_manifest_corruption() {
    let tmp = TempDir::new("edpb_corrupt_manifest");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0x5A; 13 * 512];
    write_core_backup(&path, &capture(&data)).unwrap();
    let mut file = fs::read(&path).unwrap();
    let manifest_offset = u64::from_le_bytes(file[16..24].try_into().unwrap()) as usize;
    file[manifest_offset + 5] ^= 1;
    fs::write(&path, file).unwrap();
    let err = verify_file(&path).unwrap_err();
    assert!(err.contains("manifest SHA-256"), "{err}");
}

#[test]
fn writer_refuses_wrong_extension_and_short_protocol_image() {
    let tmp = TempDir::new("edpb_inputs");
    let data = vec![0u8; 13 * 512];
    let err = write_core_backup(&tmp.0.join("legacy.bin"), &capture(&data)).unwrap_err();
    assert!(err.contains(".edpb"));

    let short = vec![0u8; 13 * 512 - 1];
    let err = write_core_backup(&tmp.0.join("short.edpb"), &capture(&short)).unwrap_err();
    assert!(err.contains("LBA0-12"));
}

#[test]
fn verified_snapshot_keeps_manifest_and_bytes_after_path_replacement_and_in_place_edit() {
    let tmp = TempDir::new("edpb_snapshot_identity");
    let path = tmp.0.join("sample.edpb");
    let original = vec![0x11; 13 * 512];
    write_core_backup(&path, &capture(&original)).unwrap();
    let reader = edpcli::edpb::VerifiedBackupReader::open(&path).unwrap();
    let original_digest = reader.verified().file_sha256.clone();
    let replacement = tmp.0.join("new.edpb");
    let changed = vec![0x22; 13 * 512];
    write_core_backup(&replacement, &capture(&changed)).unwrap();
    // Windows cannot atomically replace an open destination; snapshots close it.
    fs::remove_file(&path).unwrap();
    fs::rename(&replacement, &path).unwrap();
    assert_eq!(reader.read_raw_protocol().unwrap(), original);
    assert_eq!(reader.verified().file_sha256, original_digest);
    assert_eq!(read_raw_protocol(&path).unwrap(), changed);
    fs::write(&path, b"corrupt in-place modification").unwrap();
    assert!(verify_file(&path).is_err());
    assert_eq!(reader.read_raw_protocol().unwrap(), original);
}

#[test]
fn sparse_container_over_budget_is_rejected_before_allocating_payload() {
    let tmp = TempDir::new("edpb_sparse_budget");
    let path = tmp.0.join("huge.edpb");
    let file = fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 * 1024 + 1).unwrap();
    let error = verify_file(&path).unwrap_err();
    assert!(error.contains("exceeds read budget"), "{error}");
}

// Rebuild locators and manifest hashes so malformed inputs reach the resource
// and arithmetic checks, rather than failing the outer SHA-256 check first.
fn rewrite_container(
    path: &std::path::Path,
    template: &[u8],
    manifest: &serde_json::Value,
    manifest_offset: u64,
) {
    use sha2::{Digest, Sha256};
    use std::io::{Seek, SeekFrom, Write};
    let mut header = template[..96].to_vec();
    let mut footer = template[template.len() - 80..].to_vec();
    let bytes = serde_json::to_vec(manifest).unwrap();
    let footer_offset = manifest_offset + bytes.len() as u64;
    let file_len = footer_offset + 80;
    for part in [&mut header, &mut footer] {
        part[16..24].copy_from_slice(&manifest_offset.to_le_bytes());
        part[24..32].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    }
    header[32..40].copy_from_slice(&footer_offset.to_le_bytes());
    footer[32..40].copy_from_slice(&file_len.to_le_bytes());
    let hash = Sha256::digest(&bytes);
    header[48..80].copy_from_slice(&hash);
    footer[40..72].copy_from_slice(&hash);
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&header).unwrap();
    let mut frame = template[96..160].to_vec();
    frame[16..24].copy_from_slice(
        &manifest["artifacts"][0]["storage"]["stored_length"]
            .as_u64()
            .unwrap()
            .to_le_bytes(),
    );
    file.write_all(&frame).unwrap();
    file.seek(SeekFrom::Start(manifest_offset)).unwrap();
    file.write_all(&bytes).unwrap();
    file.write_all(&footer).unwrap();
}

#[test]
fn verifier_rejects_artifact_count_length_and_chunk_offset_overflow() {
    let tmp = TempDir::new("edpb_budget_boundaries");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0x11; 13 * 512];
    write_core_backup(&path, &capture(&data)).unwrap();
    let template = fs::read(&path).unwrap();
    let original = serde_json::to_value(verify_file(&path).unwrap().manifest).unwrap();
    let offset = u64::from_le_bytes(template[16..24].try_into().unwrap());
    let mut too_many = original.clone();
    too_many["artifacts"] = serde_json::Value::Array(vec![original["artifacts"][0].clone(); 1025]);
    rewrite_container(&path, &template, &too_many, offset);
    assert!(verify_file(&path)
        .unwrap_err()
        .contains("artifact count exceeds read budget"));
    let mut oversized = original.clone();
    let length = 64 * 1024 * 1024 + 1u64;
    oversized["artifacts"][0]["storage"]["stored_length"] = length.into();
    rewrite_container(&path, &template, &oversized, 160 + length);
    assert!(verify_file(&path)
        .unwrap_err()
        .contains("artifact exceeds read budget"));
    let mut overflow = original;
    overflow["artifacts"][0]["storage"]["frame_offset"] = u64::MAX.into();
    overflow["artifacts"][0]["storage"]["data_offset"] = 63u64.into();
    rewrite_container(&path, &template, &overflow, offset);
    assert!(verify_file(&path)
        .unwrap_err()
        .contains("chunk offset mismatch"));
}

#[test]
fn oversized_manifest_is_rejected_before_deserialization() {
    use std::io::{Seek, SeekFrom, Write};
    let tmp = TempDir::new("edpb_manifest_budget");
    let path = tmp.0.join("sample.edpb");
    write_core_backup(&path, &capture(&vec![0; 13 * 512])).unwrap();
    let template = fs::read(&path).unwrap();
    let length = 4 * 1024 * 1024 + 1u64;
    let footer_offset = 96 + length;
    let mut header = template[..96].to_vec();
    let mut footer = template[template.len() - 80..].to_vec();
    for part in [&mut header, &mut footer] {
        part[16..24].copy_from_slice(&96u64.to_le_bytes());
        part[24..32].copy_from_slice(&length.to_le_bytes());
    }
    header[32..40].copy_from_slice(&footer_offset.to_le_bytes());
    footer[32..40].copy_from_slice(&(footer_offset + 80).to_le_bytes());
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(&header).unwrap();
    file.seek(SeekFrom::Start(footer_offset)).unwrap();
    file.write_all(&footer).unwrap();
    assert!(verify_file(&path)
        .unwrap_err()
        .contains("manifest exceeds read budget"));
}

#[test]
fn writer_rejects_user_payload_restore_and_excessive_manifest() {
    use edpcli::edpb::*;
    let tmp = TempDir::new("restore_authority");
    let data = vec![0; 13 * 512];
    let mut metadata = MetadataCapture {
        core: capture(&data),
        partitions: vec![],
        regions: vec![Region {
            id: "userdata".into(),
            role: "ordinary_file_payload".into(),
            start_lba: Some(2048),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        }],
        extents: vec![Extent {
            id: "userdata".into(),
            region_id: "userdata".into(),
            start_lba: 2048,
            sector_count: 1,
            purpose: "ordinary_file_payload".into(),
        }],
        artifacts: vec![ArtifactInput {
            id: "userdata".into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec!["userdata".into()],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Partial,
            data: vec![0; 512],
        }],
        notes: vec![],
    };
    let path = tmp.0.join("bad.edpb");
    assert!(write_metadata_backup(&path, &metadata).is_err());
    assert!(!path.exists());
    metadata.artifacts[0].completeness = ArtifactCompleteness::Complete;
    assert!(write_metadata_backup(&path, &metadata).is_err());
    assert!(!path.exists());
    metadata.regions.clear();
    metadata.extents.clear();
    metadata.artifacts.clear();
    metadata.notes.push("x".repeat(4 * 1024 * 1024));
    assert!(write_metadata_backup(&path, &metadata).is_err());
    assert!(!path.exists());
}

// Virtual 4Kn EDP media: encrypted fixed 512B protocol payload in each
// native sector and a separate 4096B LCE block. No physical writes.
fn virtual_native_4kn_metadata() -> (Vec<u8>, Vec<u8>) {
    use edpcli::protocol::crypto::{a7f0_full, crc32_bare, xor_rolling};
    let did = "disk&ven_test&prod_native";
    let mut projection = vec![0u8; 13 * 512];
    // Independent native MBR geometry must agree with EDPF source entry0.
    // Mode0's FAT16 visible partition uses the official 0x0e type.
    projection[446 + 4] = 0x0e;
    projection[454..458].copy_from_slice(&63u32.to_le_bytes());
    projection[458..462].copy_from_slice(&400u32.to_le_bytes());
    projection[510..512].copy_from_slice(&[0x55, 0xaa]);
    let crc = crc32_bare(did.as_bytes());
    let mut lba7 = [0u8; 512];
    let mut lba12 = [0u8; 512];
    for (index, (kind, start, blocks)) in [(1u32, 63u64, 400u64), (2, 512, 1500), (4, 2012, 1000)]
        .into_iter()
        .enumerate()
    {
        let p = index * 0x60;
        lba12[p..p + 4].copy_from_slice(b"EDPF");
        lba12[p + 8..p + 12].copy_from_slice(&3u32.to_le_bytes());
        lba12[p + 12..p + 16].copy_from_slice(&kind.to_le_bytes());
        lba12[p + 20..p + 24].copy_from_slice(&u32::from(kind != 1).to_le_bytes());
        lba12[p + 24..p + 32].copy_from_slice(&start.to_le_bytes());
        lba12[p + 32..p + 40].copy_from_slice(&4096u64.to_le_bytes());
        lba12[p + 40..p + 48].copy_from_slice(&(blocks * 4096).to_le_bytes());
        let q = index * 0x40;
        lba7[q..q + 4].copy_from_slice(b"EDPF");
        lba7[q + 8..q + 12].copy_from_slice(&3u32.to_le_bytes());
        lba7[q + 12..q + 16].copy_from_slice(&kind.to_le_bytes());
        lba7[q + 20..q + 24].copy_from_slice(&u32::from(kind != 1).to_le_bytes());
        lba7[q + 24..q + 32]
            .copy_from_slice(&(if index == 0 { 63u64 } else { 10_000u64 }).to_le_bytes());
        lba7[q + 32..q + 40].copy_from_slice(&4096u64.to_le_bytes());
        lba7[q + 40..q + 48]
            .copy_from_slice(&(if index == 0 { 400u64 * 4096 } else { 4096u64 }).to_le_bytes());
    }
    projection[7 * 512..8 * 512].copy_from_slice(&xor_rolling(&lba7, (crc & 0xffff) ^ (crc >> 16)));
    projection[12 * 512..13 * 512].copy_from_slice(&a7f0_full(&lba12, &crc.to_le_bytes(), 0));
    let mut native =
        edpcli::protocol::image::NativeProtocolImage::from_protocol_zero_tailed(&projection, 4096)
            .unwrap();
    let mut entire = native.native_bytes().to_vec();
    entire[11 * 4096 + 512..12 * 4096].fill(0x5d); // opaque official LBA11 tail must survive
    native = edpcli::protocol::image::NativeProtocolImage::from_native_bytes(4096, entire).unwrap();
    (native.native_bytes().to_vec(), vec![0xe7; 4096])
}

#[test]
fn native_4kn_edpb_v4_evidence_roundtrip_and_restore_guard() {
    use edpcli::edpb::{
        write_metadata_backup, ArtifactCompleteness, ArtifactInput, Extent, MetadataCapture,
        Region, RestorePolicy, SemanticStatus, VerifiedBackupReader,
    };
    let tmp = TempDir::new("native4kn_evidence");
    let path = tmp.0.join("native.edpb");
    let (protocol, lce) = virtual_native_4kn_metadata();
    let core = CoreCapture {
        snapshot_id: "virtual-native-4kn".into(),
        created_epoch: 1_790_000_000,
        disk_number: None,
        vid: "3535".into(),
        pid: "0901".into(),
        device_id: "disk&ven_test&prod_native".into(),
        onlyid: None,
        total_sectors: Some(12_000),
        logical_sector_size: 4096,
        edpcli_version: env!("CARGO_PKG_VERSION").into(),
        device_state: "edp".into(),
        lba0_12: &protocol,
    };
    let region = Region {
        id: "region.lba7_compatibility_extent".into(),
        role: "lba7_legacy_partition_compatibility_extent".into(),
        start_lba: Some(10_000),
        sector_count: Some(1),
        semantic_status: SemanticStatus::Identified,
    };
    let extent = Extent {
        id: "extent.lba7_compatibility".into(),
        region_id: region.id.clone(),
        start_lba: 10_000,
        sector_count: 1,
        purpose: "lba7_compatibility_extent_ciphertext".into(),
    };
    let artifact = ArtifactInput {
        id: "raw.lba7_compatibility".into(),
        kind: "raw_sectors".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec![extent.id.clone()],
        derivation: None,
        restore_policy: RestorePolicy::EvidenceOnly,
        completeness: ArtifactCompleteness::Complete,
        data: lce.clone(),
    };
    let mut extra = [artifact];
    let build = |artifact: &[ArtifactInput]| MetadataCapture {
        core: core.clone(),
        partitions: vec![],
        regions: vec![region.clone()],
        extents: vec![extent.clone()],
        artifacts: artifact.to_vec(),
        notes: vec![],
    };
    let manifest = write_metadata_backup(&path, &build(&extra)).expect("4Kn v4 write evidence");
    assert_eq!(manifest.schema, "edpb.manifest.v4");
    assert_eq!(manifest.geometry.logical_sector_size, 4096);
    assert!(!manifest.restore_contract.restores_edp_protocol);
    assert!(!manifest.restore_contract.restores_partition_structure);
    assert!(manifest
        .artifacts
        .iter()
        .all(|item| item.restore_policy != RestorePolicy::Restorable));
    let verified = VerifiedBackupReader::open(&path).expect("reopen and hash-verify v4");
    assert_eq!(verified.read_raw_protocol().unwrap(), protocol);
    assert_eq!(
        verified.read_artifact("raw.lba7_compatibility").unwrap(),
        lce
    );
    assert!(
        verified.read_raw_protocol().unwrap()[11 * 4096 + 512..12 * 4096]
            .iter()
            .all(|b| *b == 0x5d)
    );
    let entry = edpcli::infrastructure::backup_store::catalog::scan_backup_file(&path).unwrap();
    assert!(entry.size_ok);
    assert!(matches!(
        entry.integrity_status,
        edpcli::infrastructure::backup_store::catalog::BackupIntegrityStatus::Verified
    ));
    assert_eq!(entry.provision_kind, Some(DiskProvisionKind::Mode0));
    use edpcli::application::evidence::{EvidenceSource, SectorReader};
    let mut source = EvidenceSource::open_backup(&path).expect("read-only v4 Inspect backing");
    assert_eq!(source.total_sectors(), 12_000);
    assert_eq!(source.logical_sector_bytes(), 4096);
    assert_eq!(
        source.protocol(),
        edpcli::protocol::image::NativeProtocolImage::from_native_bytes(4096, protocol.clone())
            .unwrap()
            .protocol_projection()
    );
    assert_eq!(
        source.read_native_sector(11).unwrap(),
        &protocol[11 * 4096..12 * 4096]
    );
    assert_eq!(
        source.read_sector(11).unwrap(),
        &protocol[11 * 4096..11 * 4096 + 512]
    );
    assert_eq!(source.read_native_sector(10_000).unwrap(), lce);
    assert!(source.read_native_sector(10_001).is_err());

    // Full EDPB v4 archive -> verified integrity -> EvidenceSource identity,
    // native 13-block snapshot -> strict EDPF/LBA7/LCE replay. The replay
    // content comes directly from the SAME reopened EDPB file, not a
    // test-supplied LCE vector or a synthetic reader.
    use edpcli::application::filesystem::FilesystemKind;
    use edpcli::protocol::edpf::EdpPartitionType;
    use edpcli::provision::{
        NativeEdpLayoutPlan, OfficialPartitionMode, PartitionRole, TargetPartitionGeometry,
    };
    let parts = [
        TargetPartitionGeometry {
            role: PartitionRole::Boot,
            partition_type: EdpPartitionType::Boot,
            start_lba: 63,
            sector_count: 400,
            physically_encrypted: false,
            filesystem: Some(FilesystemKind::Fat16),
        },
        TargetPartitionGeometry {
            role: PartitionRole::Share,
            partition_type: EdpPartitionType::Share,
            start_lba: 512,
            sector_count: 1500,
            physically_encrypted: true,
            filesystem: Some(FilesystemKind::ExFat),
        },
        TargetPartitionGeometry {
            role: PartitionRole::Encrypt,
            partition_type: EdpPartitionType::Encrypt,
            start_lba: 2012,
            sector_count: 1000,
            physically_encrypted: true,
            filesystem: Some(FilesystemKind::ExFat),
        },
    ];
    let plan = NativeEdpLayoutPlan::from_confirmed_geometry(
        OfficialPartitionMode::DefaultThreePartition,
        12_000,
        4096,
        &parts,
        10_000,
        1,
    )
    .unwrap();
    assert!(!plan.may_write());
    let verified_writes = source.verified_native_replay(&plan).unwrap();
    assert_eq!(verified_writes.len(), 14);
    assert_eq!(verified_writes.last().unwrap().relative_lba, 0);
    for native_lba in 0..13u64 {
        let block = verified_writes
            .iter()
            .find(|item| item.relative_lba == native_lba)
            .unwrap();
        assert_eq!(
            block.data,
            protocol[native_lba as usize * 4096..(native_lba as usize + 1) * 4096]
        );
    }
    assert_eq!(
        verified_writes
            .iter()
            .find(|item| item.relative_lba == 10_000)
            .unwrap()
            .data,
        lce
    );
    let snapshot = source.native_protocol_image().unwrap();
    plan.verify_source_replay_readback(
        snapshot,
        "disk&ven_test&prod_native",
        &[lce.clone()],
        &verified_writes,
    )
    .unwrap();
    let mut changed = verified_writes.clone();
    changed
        .iter_mut()
        .find(|block| block.relative_lba == 11)
        .unwrap()
        .data[4095] ^= 1;
    assert!(plan
        .verify_source_replay_readback(
            snapshot,
            "disk&ven_test&prod_native",
            &[lce.clone()],
            &changed
        )
        .is_err());
    let mut changed_lce = verified_writes.clone();
    changed_lce
        .iter_mut()
        .find(|block| block.relative_lba == 10_000)
        .unwrap()
        .data[4095] ^= 1;
    assert!(plan
        .verify_source_replay_readback(
            snapshot,
            "disk&ven_test&prod_native",
            &[lce.clone()],
            &changed_lce
        )
        .is_err());
    // Contradictory geometry is rejected instead of becoming a new writer
    // authority; the archive's source LCE points at 10000, not 10001.
    let wrong_lce_plan = NativeEdpLayoutPlan::from_confirmed_geometry(
        OfficialPartitionMode::DefaultThreePartition,
        12_000,
        4096,
        &parts,
        10_001,
        1,
    )
    .unwrap();
    assert!(source.verified_native_replay(&wrong_lce_plan).is_err());
    let bad = tmp.0.join("tampered.edpb");
    let mut altered = fs::read(&path).unwrap();
    let payload = verified
        .verified()
        .manifest
        .artifacts
        .iter()
        .find(|a| a.id == "raw.lba7_compatibility")
        .unwrap();
    altered[payload.storage.data_offset as usize + 500] ^= 0x01;
    fs::write(&bad, &altered).unwrap();
    assert!(VerifiedBackupReader::open(&bad)
        .unwrap_err()
        .contains("SHA-256"));
    extra[0].restore_policy = RestorePolicy::Restorable;
    assert!(write_metadata_backup(&tmp.0.join("forbidden.edpb"), &build(&extra)).is_err());
    assert!(write_metadata_backup(&tmp.0.join("missing.edpb"), &build(&[])).is_err());
    extra[0].restore_policy = RestorePolicy::EvidenceOnly;
    let mut corrupted = protocol.clone();
    corrupted[7 * 4096] ^= 0x01;
    let mut invalid = build(&extra);
    invalid.core.lba0_12 = &corrupted;
    assert!(write_metadata_backup(&tmp.0.join("invalid-pointer.edpb"), &invalid).is_err());
    let mut invalid_size = build(&extra);
    invalid_size.core.logical_sector_size = 512;
    assert!(write_metadata_backup(&tmp.0.join("invalid-size.edpb"), &invalid_size).is_err());
}
