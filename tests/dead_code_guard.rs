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

    for path in ["src/edpb/write.rs", "src/edpb/legacy.rs"] {
        assert!(!source(path).contains("fn write_legacy_migrated_backup("));
    }
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/migrate_legacy_backups.rs")
            .exists(),
        "removed legacy backup migration example must not return"
    );
}

#[test]
fn migrated_edpb_read_semantics_remain_supported() {
    let edpb = source("src/edpb/model.rs");
    assert!(
        edpb.contains("LegacyMigrated"),
        "already-migrated EDPB manifests must remain deserializable"
    );
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
    for path in ["src/edpb/write.rs", "src/edpb/legacy.rs"] {
        assert!(
            !source(path).contains("fn write_legacy_"),
            "historical fixture writer returned to production: {path}"
        );
    }
    assert!(!source("src/infrastructure/backup_store/catalog.rs").contains("fn parse_backup_name("));
    assert!(!source("src/infrastructure/backup_store/create.rs").contains("fn create_backup("));
    assert!(!source("src/diskio/device.rs").contains("fn read_lba("));
    assert!(!source("src/identify.rs").contains("fn detect_transport("));
    assert!(!source("src/tui/clipboard.rs").contains("fn copy_text("));
}
