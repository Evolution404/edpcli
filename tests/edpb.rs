use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use edpcli::application::media_identity::{
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentitySnapshot,
    ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::edpb::{
    canonical_media_identity, read_raw_protocol, verify_file, write_core_backup,
    write_core_backup_with_identity, write_legacy_v1_core_backup_with_notes,
    write_legacy_v2_core_backup_with_identity, CaptureLevel, CoreCapture, RestorePolicy,
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

fn typed_identity(quality: SerialQuality, digest: Option<&str>) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            serial: None,
            serial_sha256: digest.map(str::to_string),
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
    let mut identity = typed_identity(SerialQuality::Usable, Some(&"ab".repeat(32)));
    identity.hardware.serial = Some("NETAC-RAW-SERIAL-001".into());

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
fn v1_encrypted_and_mode1_backups_remain_readable() {
    let tmp = TempDir::new("edpb_v1_edp");
    let data = vec![0x39; 13 * 512];

    for state in ["encrypted", "mode1"] {
        let path = tmp.0.join(format!("{state}.edpb"));
        let mut legacy = capture(&data);
        legacy.device_state = state.into();
        write_legacy_v1_core_backup_with_notes(&path, &legacy, &[]).unwrap();

        let before = fs::read(&path).unwrap();
        let verified = verify_file(&path).unwrap();
        assert_eq!(verified.manifest.schema, "edpb.manifest.v1");
        let identity = canonical_media_identity(&verified.manifest).unwrap();
        assert_eq!(
            identity.protocol.device_id.as_deref(),
            Some("disk&ven_netac&prod_onlydisk")
        );
        assert_eq!(identity.protocol.onlyid.as_deref(), Some("1402259934"));
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "reading a historical EDPB must never rewrite it"
        );
    }
}

#[test]
fn v1_plain_device_id_is_legacy_derived_candidate_not_observed_protocol_id() {
    let tmp = TempDir::new("edpb_v1_plain");
    let path = tmp.0.join("plain.edpb");
    let data = vec![0u8; 13 * 512];
    let mut legacy = capture(&data);
    legacy.device_state = "plain".into();
    legacy.onlyid = None;
    write_legacy_v1_core_backup_with_notes(&path, &legacy, &[]).unwrap();

    let verified = verify_file(&path).unwrap();
    let identity = canonical_media_identity(&verified.manifest).unwrap();
    assert_eq!(identity.protocol.device_id, None);
    assert_eq!(identity.protocol.onlyid, None);
    assert_eq!(
        identity.protocol.provision_kind,
        Some(DiskProvisionKind::Plain)
    );
    assert_eq!(
        identity.derived.legacy_derived_candidate.as_deref(),
        Some("disk&ven_netac&prod_onlydisk")
    );
}

#[test]
fn v1_hardware_serial_note_is_legacy_fallback_only() {
    let tmp = TempDir::new("edpb_v1_serial");
    let path = tmp.0.join("legacy.edpb");
    let data = vec![0x42; 13 * 512];
    let digest = "ab".repeat(32);
    let note = format!("hardware_serial_sha256={digest}");
    write_legacy_v1_core_backup_with_notes(&path, &capture(&data), &[note]).unwrap();

    let verified = verify_file(&path).unwrap();
    let identity = canonical_media_identity(&verified.manifest).unwrap();
    assert_eq!(identity.hardware.serial_quality, SerialQuality::Usable);
    assert_eq!(
        identity.hardware.serial_sha256.as_deref(),
        Some(digest.as_str())
    );
}

#[test]
fn v2_round_trips_missing_suspicious_and_usable_serial_evidence() {
    let tmp = TempDir::new("edpb_v2_serial_quality");
    let data = vec![0x51; 13 * 512];
    let cases = [
        (SerialQuality::Missing, None),
        (SerialQuality::Suspicious, Some("cd".repeat(32))),
        (SerialQuality::Usable, Some("ef".repeat(32))),
    ];

    for (index, (quality, digest)) in cases.into_iter().enumerate() {
        let path = tmp.0.join(format!("{index}.edpb"));
        let identity = typed_identity(quality, digest.as_deref());
        write_legacy_v2_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();
        let verified = verify_file(&path).unwrap();
        assert_eq!(verified.manifest.schema, "edpb.manifest.v2");
        let decoded = canonical_media_identity(&verified.manifest).unwrap();
        assert_eq!(decoded.hardware.serial_quality, quality);
        assert_eq!(decoded.hardware.serial, None);
        assert_eq!(decoded.hardware.serial_sha256, digest);
        assert!(
            verified
                .manifest
                .provenance
                .notes
                .iter()
                .all(|note| !note.starts_with("hardware_serial_sha256=")),
            "v2 writer must not persist serial identity in free-text notes"
        );
    }
}

#[test]
fn typed_and_legacy_serial_conflict_is_invalid_fail_closed() {
    let tmp = TempDir::new("edpb_typed_legacy_conflict");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0x61; 13 * 512];
    let identity = typed_identity(SerialQuality::Usable, Some(&"11".repeat(32)));
    write_legacy_v2_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();

    let mut verified = verify_file(&path).unwrap();
    verified
        .manifest
        .provenance
        .notes
        .push(format!("hardware_serial_sha256={}", "22".repeat(32)));
    let error = canonical_media_identity(&verified.manifest).unwrap_err();
    assert!(error.contains("conflicts"), "{error}");
}

#[test]
fn legacy_migrated_capture_level_remains_deserializable() {
    let level: CaptureLevel = serde_json::from_str("\"legacy_migrated\"").unwrap();
    assert_eq!(level, CaptureLevel::LegacyMigrated);
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
