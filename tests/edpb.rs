use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use edpcli::edpb::{
    read_raw_protocol, verify_file, write_core_backup, CaptureLevel, CoreCapture, RestorePolicy,
    RAW_PROTOCOL_ARTIFACT_ID,
};

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
