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
        "src/tui/provision/render.rs",
        "src/tui/provision/task.rs",
        "src/tui/inspect/state.rs",
        "src/tui/inspect/render.rs",
        "src/tui/backups/state.rs",
        "src/tui/backups/render.rs",
        "src/tui/devices/render.rs",
    ] {
        exists(path);
    }

    assert!(lines("src/application/provision.rs") < 1_000);
    assert!(lines("src/diskio.rs") < 500);
    assert!(lines("src/tui/state.rs") < 3_500);
    assert!(lines("src/tui/render.rs") < 1_500);
    assert!(lines("src/tui/task.rs") < 1_000);
    assert!(
        lines("src/tui/provision/state.rs") < 2_160,
        "Provision orchestration state must not absorb form/capacity/plain model again"
    );
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
    assert!(source.contains("commit_provision_on_disk"));
    assert!(source.contains("backup_create_on_disk"));
    assert!(source.contains("restore_on_disk"));
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
