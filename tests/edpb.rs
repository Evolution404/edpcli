use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use edpcli::application::media_identity::{
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentitySnapshot,
    ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::edpb::{
    canonical_media_identity, read_raw_protocol, verify_file, write_core_backup,
    write_core_backup_with_identity, write_legacy_v1_core_backup_with_notes, CaptureLevel,
    CoreCapture, RestorePolicy, RAW_PROTOCOL_ARTIFACT_ID,
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
fn new_writer_uses_manifest_v2_with_typed_protocol_identity() {
    let tmp = TempDir::new("edpb_manifest_v2");
    let path = tmp.0.join("sample.edpb");
    let data = vec![0xA5; 13 * 512];

    write_core_backup(&path, &capture(&data)).unwrap();
    let verified = verify_file(&path).unwrap();
    let json = serde_json::to_value(&verified.manifest).unwrap();

    assert_eq!(json["schema"], "edpb.manifest.v2");
    assert_eq!(
        json["identity"]["protocol"]["device_id"],
        "disk&ven_netac&prod_onlydisk"
    );
    assert_eq!(json["identity"]["protocol"]["onlyid"], "1402259934");
    assert!(
        json["identity"]["hardware"].is_object(),
        "v2 must formally carry typed hardware identity"
    );
    assert!(
        json["identity"]["derived"].is_object(),
        "v2 must formally carry typed derived evidence"
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

    assert_eq!(json["schema"], "edpb.manifest.v2");
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
fn v1_encrypted_and_passwordless_backups_remain_readable() {
    let tmp = TempDir::new("edpb_v1_edp");
    let data = vec![0x39; 13 * 512];

    for state in ["encrypted", "passwordless"] {
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
        write_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();
        let verified = verify_file(&path).unwrap();
        let decoded = canonical_media_identity(&verified.manifest).unwrap();
        assert_eq!(decoded.hardware.serial_quality, quality);
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
    write_core_backup_with_identity(&path, &capture(&data), &identity).unwrap();

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
