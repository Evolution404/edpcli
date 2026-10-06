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
        "fn sha256_sidecar_path(",
        "fn read_backup_sha256(",
    ] {
        assert!(!diskio.contains(removed), "diskio resurrected {removed}");
    }

    let application = source("src/application/provision.rs");
    assert!(!application.contains("fn prepare_new_provision("));

    let reprovision = source("src/provision/reprovision.rs");
    assert!(!reprovision.contains("fn force_change_password_from_sectors("));

    assert!(!source("src/edpb/write.rs").contains("fn write_legacy_migrated_backup("));
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/migrate_legacy_backups.rs")
            .exists(),
        "removed legacy backup migration example must not return"
    );
}

#[test]
fn removed_edpb_history_adapters_do_not_return() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!root.join("src/edpb/legacy.rs").exists());
    assert!(!root.join("tests/support/historical_edpb.rs").exists());
    assert!(!source("src/edpb/model.rs").contains("LegacyMigrated"));
    assert!(!source("src/edpb/identity/canonical.rs").contains("legacy_hardware_serial_digest"));
    assert!(!source("src/media_identity.rs").contains("LegacyDigestMatch"));
}

#[test]
fn development_compatibility_shims_and_fixture_writers_do_not_return() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for removed in [
        "src/application/ports.rs",
        "src/tui/event.rs",
        "src/diskio/backup_catalog.rs",
        "src/diskio/backup_create.rs",
        "src/diskio/backup_config.rs",
        "src/crypto.rs",
        "src/sectors.rs",
        "src/sysinfo.rs",
    ] {
        assert!(
            !root.join(removed).exists(),
            "obsolete module returned: {removed}"
        );
    }
    assert!(
        !source("src/edpb/write.rs").contains("fn write_legacy_"),
        "historical fixture writer returned to production"
    );
    assert!(!source("src/infrastructure/backup_store/catalog.rs").contains("fn parse_backup_name("));
    assert!(!source("src/infrastructure/backup_store/create.rs").contains("fn create_backup("));
    assert!(!source("src/diskio/device.rs").contains("fn read_lba("));
    assert!(!source("src/identify.rs").contains("fn detect_transport("));
    assert!(!source("src/tui/clipboard.rs").contains("fn copy_text("));
}
