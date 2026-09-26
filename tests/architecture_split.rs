use std::fs;
use std::path::{Path, PathBuf};

fn lines(path: &str) -> usize {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
        .lines()
        .count()
}

fn exists(path: &str) {
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path).is_file(),
        "missing architecture module {path}"
    );
}

fn rust_sources_under(path: &str) -> Vec<PathBuf> {
    fn collect(path: &Path, out: &mut Vec<PathBuf>) {
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path.to_path_buf());
            }
            return;
        }
        let mut entries = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
            .map(|entry| entry.expect("read dir entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                collect(&entry, out);
            } else if entry.extension().is_some_and(|ext| ext == "rs") {
                out.push(entry);
            }
        }
    }

    let mut out = Vec::new();
    collect(&Path::new(env!("CARGO_MANIFEST_DIR")).join(path), &mut out);
    out
}

fn assert_sources_exclude(paths: impl IntoIterator<Item = PathBuf>, forbidden: &[&str]) {
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "{} violates architecture import direction with {needle}",
                path.display()
            );
        }
    }
}

#[test]
fn large_modules_are_split_by_domain_boundary() {
    for path in [
        "src/application/provision/prepare.rs",
        "src/application/provision/commit.rs",
        "src/application/provision/export.rs",
        "src/diskio/device.rs",
        "src/diskio/transaction.rs",
        "src/diskio/backup_config.rs",
        "src/diskio/backup_catalog.rs",
        "src/diskio/backup_create.rs",
        "src/tui/provision/state.rs",
        "src/tui/provision/form.rs",
        "src/tui/provision/plain_editor.rs",
        "src/tui/provision/fields.rs",
        "src/tui/provision/validation.rs",
        "src/tui/provision/layout.rs",
        "src/tui/provision/editor.rs",
        "src/tui/provision/render.rs",
        "src/tui/provision/task.rs",
        "src/tui/inspect/state.rs",
        "src/tui/inspect/render.rs",
        "src/tui/backups/state.rs",
        "src/tui/backups/render.rs",
        "src/tui/devices/render.rs",
        "src/inspect/model.rs",
        "src/inspect_adapter.rs",
        "src/application/inspect_text.rs",
        "src/inspect/metadata.rs",
        "src/inspect/catalog.rs",
        "src/inspect/lba_adapter.rs",
        "src/inspect/lba_early.rs",
        "src/inspect/lba_middle.rs",
        "src/inspect/lba_late.rs",
        "src/inspect/render.rs",
    ] {
        exists(path);
    }

    assert!(lines("src/application/provision.rs") < 1_000);
    assert!(lines("src/diskio.rs") < 500);
    assert!(lines("src/tui/state.rs") < 3_500);
    assert!(lines("src/tui/render.rs") < 1_500);
    assert!(lines("src/tui/task.rs") < 1_000);
    assert!(lines("src/inspect.rs") < 150);
    assert!(lines("src/inspect_adapter.rs") < 150);
    assert!(lines("src/application/inspect_text.rs") < 260);
    assert!(lines("src/inspect/model.rs") < 650);
    assert!(lines("src/inspect/metadata.rs") < 150);
    assert!(lines("src/inspect/catalog.rs") < 50);
    let inspect_model =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inspect/model.rs"))
            .expect("read inspect field model");
    assert!(!inspect_model.contains("crate::diskio"));
    assert!(!inspect_model.contains("BackupMeta"));
    assert!(lines("src/inspect/lba_adapter.rs") < 100);
    for path in [
        "src/inspect/lba_early.rs",
        "src/inspect/lba_middle.rs",
        "src/inspect/lba_late.rs",
    ] {
        assert!(
            lines(path) < 500,
            "LBA presentation adapter is oversized: {path}"
        );
    }
    assert!(lines("src/inspect/render.rs") < 400);
    assert!(
        lines("src/tui/provision/state.rs") < 520,
        "Provision orchestration state must not absorb form/capacity/plain model again"
    );
    assert!(
        lines("src/tui/provision/editor.rs") < 300,
        "Provision edit actions must stay bounded"
    );
    let editor_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/editor.rs"),
    )
    .expect("read provision editor");
    for name in [
        "provision_fill_selected_capacity",
        "provision_plain_add_partition",
    ] {
        assert!(
            editor_source.contains(name),
            "editor module is missing {name}"
        );
    }
    assert!(
        lines("src/tui/provision/layout.rs") < 500,
        "Provision layout presentation must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/provision/validation.rs") < 500,
        "Provision validation adapter must not duplicate canonical protocol or domain parsers"
    );
    assert!(
        lines("src/tui/provision/fields.rs") < 900,
        "Provision field navigation and input editing must stay responsibility-bounded"
    );
    let field_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/fields.rs"),
    )
    .expect("read provision fields module");
    let orchestration_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/state.rs"),
    )
    .expect("read provision orchestration state");
    for name in [
        "provision_field_slot",
        "provision_push_char",
        "provision_delete_char",
    ] {
        assert!(
            field_source.contains(name),
            "fields module is missing {name}"
        );
        assert!(
            !orchestration_source.contains(&format!("fn {name}(")),
            "orchestration state must not reabsorb {name}"
        );
    }
    assert!(
        lines("src/tui/provision/form.rs") < 650,
        "Provision form model must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/provision/plain_editor.rs") < 250,
        "Plain editor must contain only pure partition editing and form conversion"
    );
    let provision_form =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/form.rs"))
            .expect("read provision form model");
    for forbidden in [
        "AppState",
        "crate::application",
        "crate::platform",
        "crate::diskio",
    ] {
        assert!(
            !provision_form.contains(forbidden),
            "Provision form model must stay pure and independent of orchestration/I/O: {forbidden}"
        );
    }
    let plain_editor = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/plain_editor.rs"),
    )
    .expect("read plain partition editor");
    for forbidden in [
        "AppState",
        "crate::application",
        "crate::platform",
        "crate::diskio",
    ] {
        assert!(
            !plain_editor.contains(forbidden),
            "Plain partition editor must remain a pure form adapter: {forbidden}"
        );
    }
    let semantic =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocol/semantic.rs"))
            .expect("read protocol semantic layer");
    for forbidden in [
        "crate::inspect",
        "crate::application",
        "crate::tui",
        "crate::cli",
    ] {
        assert!(
            !semantic.contains(forbidden),
            "protocol semantic layer must not depend on presentation/application layer: {forbidden}"
        );
    }
}

#[test]
fn workspace_modules_do_not_import_platform_or_diskio_directly() {
    for path in [
        "src/tui/provision/state.rs",
        "src/tui/provision/render.rs",
        "src/tui/inspect/state.rs",
        "src/tui/inspect/render.rs",
        "src/tui/backups/state.rs",
        "src/tui/backups/render.rs",
        "src/tui/devices/render.rs",
    ] {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(
            !source.contains("crate::platform"),
            "{path} bypasses application boundary"
        );
        assert!(
            !source.contains("crate::diskio"),
            "{path} bypasses application boundary"
        );
    }
}

#[test]
fn entire_tui_uses_application_boundary_for_platform_and_raw_disk_access() {
    let tui = rust_sources_under("src/tui");
    assert_sources_exclude(
        tui,
        &[
            "crate::platform",
            "crate::diskio",
            "FileDev::open_rdonly",
            "raw_path(",
            "SystemClock",
        ],
    );
}

#[test]
fn cli_uses_application_boundary_for_raw_disk_access() {
    let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli.rs"))
        .expect("read cli");
    for forbidden in [
        "crate::diskio",
        "FileDev::open_rdonly",
        "raw_path(",
        "SystemClock",
    ] {
        assert!(
            !source.contains(forbidden),
            "cli must acquire raw disks through application service: {forbidden}"
        );
    }
    assert!(source.contains("prepare_provision_on_disk"));
    assert!(source.contains("commit_provision_with_backup_on_disk"));
    assert!(source.contains("backup_create_on_disk"));
    assert!(source.contains("restore_on_disk"));
}

#[test]
fn provision_write_frontends_cannot_bypass_mandatory_application_backup() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let application = fs::read_to_string(root.join("src/application/provision.rs"))
        .expect("read provision application");
    let cli = fs::read_to_string(root.join("src/cli.rs")).expect("read cli");
    let tui = fs::read_to_string(root.join("src/tui/provision/task.rs"))
        .expect("read tui provision task");

    let start = application
        .find("pub fn commit_provision_with_backup_on_disk")
        .expect("application must own mandatory provision backup chain");
    let body = &application[start..];
    let backup = body
        .find("backup_create_on_disk")
        .expect("mandatory chain must create backup");
    let commit = body
        .find("commit_provision_on_disk")
        .expect("mandatory chain must commit after backup");
    assert!(
        backup < commit,
        "backup must complete before provision commit"
    );

    assert!(cli.contains("commit_provision_with_backup_on_disk"));
    assert!(!cli.contains("match crate::application::provision::commit_provision_on_disk"));
    assert!(tui.contains("commit_provision_with_backup_on_disk"));
    assert!(!tui.contains("_backup_dir: PathBuf"));
    assert!(!tui.contains("commit_provision_on_disk(&runner, &prepared)"));
}

#[test]
fn real_usb_password_hil_keeps_secrets_off_argv_and_is_default_off() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/real_usb_password_hil.rs"),
    )
    .expect("real USB password HIL must exist");

    for required in [
        "EDPCLI_REAL_USB_PASSWORD_HIL",
        "guard_usb_disk",
        "commit_provision_with_backup_on_disk",
        "--stdin-secrets",
        "--generate-secrets",
        "getrandom::fill",
        "/dev/tty",
        "stty",
        "impl Drop for SecretBundle",
    ] {
        assert!(
            source.contains(required),
            "missing HIL safety token: {required}"
        );
    }
    for forbidden in [
        "SharePass1!",
        "EncryptPass1!",
        "EncryptPass2!",
        "0000aaaa",
        "--share-source-password",
        "--share-target-password",
        "--encrypt-source-password",
        "--encrypt-target-password",
    ] {
        assert!(
            !source.contains(forbidden),
            "HIL source must not contain plaintext password/CLI secret token: {forbidden}"
        );
    }
}

#[test]
fn inspect_cli_uses_typed_application_error_kinds() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inspect_cli.rs"))
            .expect("read inspect cli");
    assert!(source.contains("InspectErrorKind"));
    assert!(source.contains("fn inspect_error_code(error: &InspectError)"));
    assert!(
        !source.contains("message.contains("),
        "inspect CLI must not infer control flow from localized error text"
    );
}

#[test]
fn semantic_consumers_do_not_depend_on_inspect_presentation() {
    exists("src/protocol/semantic.rs");

    for path in ["src/metainfo.rs", "src/provision/validate.rs"] {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(
            !source.contains("crate::inspect"),
            "{path} must consume typed protocol semantics instead of inspect presentation"
        );
    }
}

#[test]
fn inspect_presentation_stays_downstream_of_protocol_and_application_domains() {
    let mut domain_sources = rust_sources_under("src/protocol");
    domain_sources.extend(rust_sources_under("src/provision"));
    domain_sources.extend(rust_sources_under("src/application"));
    assert_sources_exclude(domain_sources, &["crate::inspect::", "crate::inspect{"]);

    let adapter =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inspect_adapter.rs"))
            .expect("read inspect adapter");
    assert!(!adapter.contains("crate::application"));
    assert!(!adapter.contains("mod render;"));

    for path in [
        "src/inspect/model.rs",
        "src/inspect/metadata.rs",
        "src/inspect/lba_adapter.rs",
        "src/inspect/lba_early.rs",
        "src/inspect/lba_middle.rs",
        "src/inspect/lba_late.rs",
    ] {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(
            !source.contains("crate::application"),
            "{path} imports application"
        );
        assert!(
            !source.contains("render_fields("),
            "{path} imports display rendering"
        );
        assert!(
            !source.contains("render_hex("),
            "{path} imports display rendering"
        );
    }
}

#[test]
fn inspect_key_status_uses_typed_errors() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inspect_target.rs"))
            .expect("read inspect target");
    assert!(source.contains("default_file_key_checked"));
    assert!(!source.contains("error.contains(\"FileKeyCRC\")"));
}

#[test]
fn critical_io_paths_have_no_panicking_shortcuts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/application/evidence.rs",
        "src/diskio/device.rs",
        "src/diskio/transaction.rs",
        "src/backup_deep.rs",
    ] {
        let source = fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        for forbidden in [".unwrap(", ".expect(", "panic!", "unreachable!"] {
            assert!(
                !source.contains(forbidden),
                "{path} must return an error instead of using {forbidden}"
            );
        }
    }
    let fat =
        fs::read_to_string(root.join("src/backup_deep/fat.rs")).expect("read FAT parser source");
    let parse = fat
        .split("pub(super) fn parse(")
        .nth(1)
        .expect("FAT parser");
    let length_guard = parse.find("boot.len() != 512").expect("boot length guard");
    let first_boot_access = parse.find("boot[510..512]").expect("boot signature access");
    assert!(length_guard < first_boot_access);
    let validator = fs::read_to_string(root.join("src/provision/validate.rs"))
        .expect("read provision validator source");
    assert!(validator.contains("checked_sector(bytes, lba as usize)"));
    let plist = fs::read_to_string(root.join("src/plist.rs")).expect("read plist source");
    let production = plist.split("#[cfg(test)]").next().expect("plist parser");
    assert!(!production.contains(".unwrap("));
}

#[test]
fn protocol_semantic_does_not_depend_on_presentation_or_application_layers() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocol/semantic.rs"))
            .expect("read protocol semantic");
    for forbidden in [
        "crate::inspect",
        "crate::metainfo",
        "crate::application",
        "crate::tui",
    ] {
        assert!(
            !source.contains(forbidden),
            "protocol semantic must not depend on higher layer {forbidden}"
        );
    }
}

#[test]
fn raw_write_flows_use_target_session_for_safety_transition() {
    exists("src/application/target_session.rs");

    for path in [
        "src/application/provision/commit.rs",
        "src/application/write.rs",
    ] {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(
            !source.contains("sysinfo::prepare_write"),
            "{path} must enter write state through application::target_session"
        );
        assert!(
            !source.contains("reopen_rdwr("),
            "{path} must reopen raw devices through application::target_session"
        );
    }
}

#[test]
fn inspect_disk_and_backup_sources_use_evidence_source() {
    exists("src/application/evidence.rs");
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/inspect.rs"),
    )
    .expect("read application inspect");

    for forbidden in [
        "struct AdvancedBackupReader",
        "crate::edpb::verify_file",
        "crate::edpb::read_raw_protocol",
        "FileDev::open_rdonly",
    ] {
        assert!(
            !source.contains(forbidden),
            "application inspect must acquire source evidence through EvidenceSource: {forbidden}"
        );
    }
    assert!(source.contains("EvidenceSource::open_backup"));
    assert!(source.contains("EvidenceSource::open_disk"));
}

#[test]
fn typed_write_events_are_rendered_outside_application_layer() {
    let write =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/write.rs"))
            .expect("read application write");
    assert!(!write.contains("crate::ui::"));
    assert!(!write.contains("render_event_text"));

    let application =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application.rs"))
            .expect("read application root");
    assert!(!application.contains("std::io::stdout"));
    assert!(!application.contains("render_event_text"));

    let ui = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui.rs"))
        .expect("read ui renderer");
    assert!(ui.contains("pub fn render_write_event"));
}

#[test]
fn domain_and_application_import_direction_is_guarded() {
    let mut domain = rust_sources_under("src/provision");
    domain.extend(rust_sources_under("src/protocol"));
    assert_sources_exclude(domain, &["crate::tui", "crate::cli"]);

    let mut application = rust_sources_under("src/application");
    application.extend(rust_sources_under("src/application.rs"));
    assert_sources_exclude(
        application,
        &["ratatui", "crossterm", "crate::tui", "crate::cli"],
    );

    let validator =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provision/validate.rs"))
            .expect("read provision validator");
    assert!(
        !validator.contains("crate::inspect"),
        "provision validator must not depend on inspect presentation"
    );
}

#[test]
fn chapter_15_identity_write_boundaries_remain_separate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = |path: &str| {
        fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
    };
    let restore = source("src/application/write.rs");
    let selector = source("src/selectors.rs");
    let observer = source("src/application/media_identity_observer.rs");
    let matcher = source("src/application/media_identity.rs");
    let edpb = source("src/edpb.rs");
    let backup_writer = source("src/diskio/backup_create.rs");
    let lineage = source("src/application/provision/identity_lineage.rs");

    let authorize = restore
        .split("fn authorize_restore(")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn read_image(").next())
        .expect("restore authorization boundary");
    assert!(authorize.contains("RestoreAuthorizationPolicy::evaluate"));
    assert!(!authorize.contains("BackupAffinityPolicy"));
    assert!(!authorize.contains("identity_grade >="));
    assert!(!selector.contains("for_onlyid"));
    assert!(!selector.contains("matches_onlyid"));
    assert!(!restore.contains("for_onlyid"));
    assert!(!restore.contains("matches_onlyid"));

    let writer = edpb
        .split("const LEGACY_HARDWARE_SERIAL_NOTE_PREFIX")
        .next()
        .expect("EDPB writer before legacy adapter");
    assert!(!writer.contains("hardware_serial_sha256="));
    assert!(!backup_writer.contains("hardware_serial_sha256="));
    assert!(edpb.contains("fn legacy_hardware_serial_digest("));
    assert!(edpb.contains("edpb.manifest.v2"));

    for forbidden in ["prepare_write(", "reopen_rdwr(", "write_sector("] {
        assert!(
            !observer.contains(forbidden),
            "observer contains {forbidden}"
        );
        assert!(!matcher.contains(forbidden), "matcher contains {forbidden}");
    }
    assert!(observer.contains("MediaIdentitySnapshot::plain("));
    assert!(lineage.contains("backup_dir.join(\".edpcli/identity-lineage/v1\")"));
    assert!(!lineage.contains("write_sector("));

    for path in [
        "src/tui/devices/render.rs",
        "src/tui/backups/render.rs",
        "src/tui/inspect/render.rs",
    ] {
        let renderer = source(path);
        for forbidden in [
            "FileDev::open_",
            "verify_file(",
            "read_raw_protocol(",
            "find_backups(",
            "observe_media_identity_readonly(",
        ] {
            assert!(!renderer.contains(forbidden), "{path} contains {forbidden}");
        }
    }

    let prepare = source("src/application/provision/prepare.rs");
    let commit = source("src/application/provision/commit.rs");
    assert!(prepare.contains("RegionDisposition::Migrate =>"));
    assert!(commit.contains("RegionDisposition::Migrate =>"));
    assert!(prepare.contains("K6"));
    assert!(commit.contains("unsupported"));
}
