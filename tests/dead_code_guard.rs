use std::fs;
use std::path::Path;

fn source(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn removed_dead_and_legacy_write_helpers_do_not_return() {
    let diskio = source("src/diskio.rs");
    for removed in [
        "fn wildcard_match(",
        "fn read_lba_file(",
        "fn backup_label_id(",
        "fn backup_is_nopwd(",
        "fn sha256_sidecar_path(",
        "fn read_backup_sha256(",
    ] {
        assert!(!diskio.contains(removed), "diskio resurrected {removed}");
    }

    let application = source("src/application/provision.rs");
    assert!(!application.contains("fn prepare_new_provision("));

    let reprovision = source("src/provision/reprovision.rs");
    assert!(!reprovision.contains("fn force_change_password_from_sectors("));

    let edpb = source("src/edpb.rs");
    assert!(!edpb.contains("fn write_legacy_migrated_backup("));
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/migrate_legacy_backups.rs")
            .exists(),
        "removed legacy backup migration example must not return"
    );
}

#[test]
fn migrated_edpb_read_semantics_remain_supported() {
    let edpb = source("src/edpb.rs");
    assert!(
        edpb.contains("LegacyMigrated"),
        "already-migrated EDPB manifests must remain deserializable"
    );
}
