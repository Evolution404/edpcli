use edpcli::application::support::{
    EdpCliError, EdpCliResult, EXIT_BACKUP, METADATA_IMAGE_LEN, METADATA_LAST_LBA, SECTOR,
};
use edpcli::edpb::*;
use edpcli::{diskio, edpb, provision};
// Compile the audited planner directly without changing its production visibility.
mod backup_metadata {
    pub use edpcli::application::backup::*;
}
fn err(code: i32, message: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, message)
}
#[path = "../../../src/application/write/restore_plan.rs"]
mod restore_plan;
fn main() {
    let root = std::env::temp_dir().join(format!("edpcli-container-audit-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let raw=std::fs::read("tests/fixtures/protocol/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin").unwrap();
    let core = CoreCapture {
        snapshot_id: "audit".into(),
        created_epoch: 1,
        disk_number: Some(6),
        vid: "0dd8".into(),
        pid: "2005".into(),
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        onlyid: Some("1402259934".into()),
        total_sectors: Some(122880000),
        logical_sector_size: 512,
        edpcli_version: "audit".into(),
        device_state: "edp".into(),
        lba0_12: &raw,
    };
    let mut capture = MetadataCapture {
        core,
        partitions: vec![],
        regions: vec![],
        extents: vec![],
        artifacts: vec![],
        notes: vec![],
    };
    for i in 0..1024 {
        capture.artifacts.push(ArtifactInput {
            id: format!("audit-{i}"),
            kind: "audit".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![],
            derivation: None,
            restore_policy: RestorePolicy::EvidenceOnly,
            completeness: ArtifactCompleteness::Complete,
            data: vec![1],
        });
    }
    let path = root.join("many.edpb");
    let manifest = write_metadata_backup(&path, &capture).unwrap();
    let error = verify_file(&path).unwrap_err();
    assert_eq!(manifest.artifacts.len(), 1025);
    assert!(error.contains("artifact count"));
    println!("writer accepted 1025 artifacts; reader rejected: {error}");
    capture.artifacts.clear();
    capture.regions.push(Region {
        id: "region.audit.userdata".into(),
        role: "ordinary_file_payload".into(),
        start_lba: Some(2048),
        sector_count: Some(1),
        semantic_status: SemanticStatus::Identified,
    });
    capture.extents.push(Extent {
        id: "extent.audit.userdata".into(),
        region_id: "region.audit.userdata".into(),
        start_lba: 2048,
        sector_count: 1,
        purpose: "ordinary_file_payload".into(),
    });
    capture.artifacts.push(ArtifactInput {
        id: "raw.audit.userdata".into(),
        kind: "raw_sectors".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec!["extent.audit.userdata".into()],
        derivation: None,
        restore_policy: RestorePolicy::Restorable,
        completeness: ArtifactCompleteness::Partial,
        data: vec![0x55; 512],
    });
    let path = root.join("userdata.edpb");
    write_metadata_backup(&path, &capture).unwrap();
    let reader = VerifiedBackupReader::open(&path).unwrap();
    assert_eq!(
        reader.read_artifact("raw.audit.userdata").unwrap().len(),
        512
    );
    let transaction = restore_plan::build_metadata_restore_plan(
        &reader,
        Some(&raw),
        &raw,
        Some("disk&ven_netac&prod_onlydisk"),
        122880000,
    )
    .unwrap();
    assert_eq!(
        transaction.writes().get(&2048).unwrap().bytes,
        vec![0x55; 512]
    );
    println!("audited restore planner admitted that partial user-data extent into its in-memory write transaction; no device opened or written");
    println!("metadata-only writer and verifier accepted restorable ordinary_file_payload at LBA2048, completeness=partial");
    capture.regions.clear();
    capture.extents.clear();
    capture.artifacts.clear();
    capture.notes = vec!["a".repeat(4 * 1024 * 1024)];
    let path = root.join("notes.edpb");
    write_metadata_backup(&path, &capture).unwrap();
    let error = verify_file(&path).unwrap_err();
    assert!(error.contains("manifest"));
    println!("writer accepted >4MiB manifest; reader rejected: {error}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        println!(
            "backup mode under current umask: {:o}",
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777
        );
    }
    // Model the exact final exists-check + rename primitive in lineage, using temporary files only.
    let final_path = root.join("lineage.json");
    let temp_path = root.join("lineage.tmp");
    std::fs::write(&temp_path, b"second").unwrap();
    assert!(!final_path.exists());
    std::fs::write(&final_path, b"first").unwrap();
    std::fs::rename(&temp_path, &final_path).unwrap();
    assert_eq!(std::fs::read(&final_path).unwrap(), b"second");
    println!("Unix check-then-rename overwrites concurrently inserted immutable record (primitive repro)");
    std::fs::remove_dir_all(&root).unwrap();
}
