use edpcli::backup_deep::{assess_partition, AnalysisStatus};
use edpcli::backup_metadata::PartitionGeometry;
use edpcli::edpb::{ArtifactCompleteness, RestorePolicy};

fn partition() -> PartitionGeometry {
    PartitionGeometry {
        index: 1,
        partition_type: 2,
        partition_count: 3,
        need_disturb: 1,
        need_encrypt: 1,
        start_sector: 20480,
        sector_size: 512,
        partition_size: 118477684736,
        sector_count: 231401728,
        user_key_crc: 69825373,
        file_key_crc: 1099700748,
        encrypt_mode: 2,
    }
}

#[test]
fn encrypted_metadata_never_becomes_an_empty_filesystem() {
    let p = partition();
    // Even a coincidental plaintext signature is not a verified decoded view.
    let mut raw = vec![0; 512];
    raw[3..11].copy_from_slice(b"NTFS    ");
    let report = assess_partition(&p, Some(&raw));
    assert_eq!(report.status, AnalysisStatus::Locked);
    assert!(report.filesystem.is_none());
    assert!(report.total_bytes.is_none());
    assert!(report.used_bytes.is_none());
    assert!(report.free_bytes.is_none());
    assert!(report.file_count.is_none());
    assert!(report.directory_count.is_none());
    assert!(report.entries.is_none());
    assert_eq!(report.partition_bytes, p.partition_size);
    let artifact = report.into_artifact().unwrap();
    assert_eq!(artifact.restore_policy, RestorePolicy::DerivedOnly);
    assert_eq!(artifact.completeness, ArtifactCompleteness::NotCaptured);
    assert_eq!(
        artifact.derivation.unwrap().source_artifact_ids,
        ["raw.protocol.lba0_12", "raw.partition.1.prefix"]
    );
    let json: serde_json::Value = serde_json::from_slice(&artifact.data).unwrap();
    assert!(json["entries"].is_null());
    assert_eq!(json["status"], "locked");
}

#[test]
fn missing_and_short_prefixes_are_explicit_and_never_zero_filled() {
    let p = partition();
    let missing = assess_partition(&p, None);
    assert_eq!(missing.status, AnalysisStatus::NotCaptured);
    let artifact = missing.into_artifact().unwrap();
    assert_eq!(artifact.source_extent_ids, ["extent.protocol.lba0_12"]);
    assert_eq!(
        assess_partition(&p, Some(&[0; 511])).status,
        AnalysisStatus::ParseFailed
    );
}

#[test]
fn plaintext_metadata_does_not_claim_a_complete_inventory() {
    let mut p = partition();
    p.need_encrypt = 0;
    p.need_disturb = 0;
    p.partition_type = 4;
    let mut raw = [0; 512];
    raw[3..11].copy_from_slice(b"NTFS    ");
    let report = assess_partition(&p, Some(&raw));
    assert_eq!(report.status, AnalysisStatus::Unsupported);
    assert!(report.entries.is_none());
    p.need_disturb = 1;
    assert_eq!(
        assess_partition(&p, Some(&raw)).status,
        AnalysisStatus::Unsupported
    );
}
