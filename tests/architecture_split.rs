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
        "src/tui/inspect/sector_state.rs",
        "src/tui/inspect/render.rs",
        "src/tui/inspect/sector_render.rs",
        "src/tui/backups/state.rs",
        "src/tui/backups/render.rs",
        "src/tui/devices/render.rs",
        "src/tui/devices/state.rs",
        "src/tui/dispatch.rs",
        "src/tui/controller.rs",
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
    assert!(
        lines("src/tui/mod.rs") < 1_800,
        "TUI module root must remain lifecycle-oriented; action dispatch belongs in dispatch.rs"
    );
    assert!(
        lines("src/tui/dispatch.rs") < 600,
        "TUI dispatch module must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/controller.rs") < 750,
        "shared TUI action controller must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/state.rs") < 1_900,
        "Inspect workspace state must not absorb Sector Inspector state again"
    );
    assert!(
        lines("src/tui/inspect/sector_state.rs") < 450,
        "Sector Inspector state must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/render.rs") < 750,
        "Inspect workspace renderer must not absorb Sector Inspector rendering again"
    );
    assert!(
        lines("src/tui/inspect/sector_render.rs") < 350,
        "Sector Inspector renderer must stay responsibility-bounded"
    );
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
    for name in ["provision_push_char", "provision_delete_char"] {
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
        field_source.contains("ProvisionFieldId"),
        "fields module must use typed field identifiers"
    );
    assert!(
        !field_source.contains("provision_field_slot"),
        "fields module must not reintroduce numeric field slots"
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
fn real_usb_k6_verify_is_read_only_and_identity_bound() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/real_usb_k6_verify.rs"),
    )
    .expect("real USB K6 verifier must exist");

    for required in [
        "EXPECTED_VID",
        "EXPECTED_PID",
        "EXPECTED_TOTAL_SECTORS",
        "EXPECTED_DEVICE_ID",
        "guard_usb_disk",
        "FileDev::open_rdonly",
        "stream_file_payload",
        "DEFAULT_KEY_DOMAIN_PASSWORD",
        "aggregate_sha256",
    ] {
        assert!(
            source.contains(required),
            "missing K6 verifier safety token: {required}"
        );
    }
    for forbidden in [
        "open_rdwr",
        "write_sector(",
        "execute_write_transaction",
        "atomic_write",
        "Command::new",
    ] {
        assert!(
            !source.contains(forbidden),
            "K6 verifier must stay read-only: found {forbidden}"
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
fn app_state_owns_devices_through_devices_substate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let devices =
        fs::read_to_string(root.join("src/tui/devices/state.rs")).expect("read devices state");

    assert!(state.contains("devices: DevicesState"));
    assert!(state.contains("#[path = \"devices/state.rs\"]"));
    for legacy_field in [
        "devices: Vec<crate::disk_scan::Row>",
        "device_table_view: super::table_layout::TableViewData",
        "device_scan_pending: bool",
        "devices_pane_focus: crate::tui::pane::PaneFocus",
        "device_summary_selected: usize",
        "device_summary_expanded: u8",
    ] {
        assert!(
            !state.contains(legacy_field),
            "device-owned field must live in DevicesState: {legacy_field}"
        );
    }
    for owned_field in [
        "rows: Vec<crate::disk_scan::Row>",
        "table_view: super::super::table_layout::TableViewData",
        "scan_pending: bool",
        "pane_focus: crate::tui::pane::PaneFocus",
        "summary_selected: usize",
        "summary_expanded: u8",
    ] {
        assert!(
            devices.contains(owned_field),
            "DevicesState must own field: {owned_field}"
        );
    }
}

#[test]
fn app_state_owns_backups_through_backups_substate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let backups =
        fs::read_to_string(root.join("src/tui/backups/state.rs")).expect("read backups state");

    let app_state = state
        .split("pub struct AppState {")
        .nth(1)
        .and_then(|tail| tail.split("impl Default for AppState").next())
        .expect("AppState section");
    assert!(app_state.contains("backups: BackupsState"));
    for legacy_field in [
        "backups: Vec<crate::application::BackupWorkspaceItem>",
        "backup_verify_run: Option<BackupVerifyRunState>",
        "backup_table_view: super::table_layout::TableViewData",
        "backup_scan_pending: bool",
        "backup_delete: Option<BackupDeleteState>",
        "backup_batch_delete: Option<BackupBatchDeleteState>",
        "backup_selection: std::collections::BTreeSet<std::path::PathBuf>",
        "backup_create_choice: Option<BackupCreateChoiceState>",
        "backup_prune: Option<BackupPruneState>",
        "backups_pane_focus: crate::tui::pane::PaneFocus",
    ] {
        assert!(
            !app_state.contains(legacy_field),
            "backup-owned field must live in BackupsState: {legacy_field}"
        );
    }
    for owned_field in [
        "rows: Vec<crate::application::BackupWorkspaceItem>",
        "verify_run: Option<BackupVerifyRunState>",
        "table_view: super::super::table_layout::TableViewData",
        "scan_pending: bool",
        "delete: Option<BackupDeleteState>",
        "batch_delete: Option<BackupBatchDeleteState>",
        "selection: std::collections::BTreeSet<std::path::PathBuf>",
        "create_choice: Option<BackupCreateChoiceState>",
        "prune: Option<BackupPruneState>",
        "pane_focus: crate::tui::pane::PaneFocus",
    ] {
        assert!(
            backups.contains(owned_field),
            "BackupsState must own field: {owned_field}"
        );
    }
}

#[test]
fn app_state_owns_inspect_through_inspect_substate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let inspect =
        fs::read_to_string(root.join("src/tui/inspect/state.rs")).expect("read inspect state");

    let app_state = state
        .split("pub struct AppState {")
        .nth(1)
        .and_then(|tail| tail.split("impl Default for AppState").next())
        .expect("AppState section");
    assert!(app_state.contains("inspect: InspectState"));
    assert!(
        !app_state.contains("advanced_inspect: Option<AdvancedInspectState>"),
        "Inspect workspace root state must not live directly in AppState"
    );
    assert!(inspect.contains("pub struct InspectState"));
    assert!(inspect.contains("advanced: Option<AdvancedInspectState>"));
    assert!(
        !state.contains("self.advanced_inspect"),
        "state facade must access Inspect workspace state through InspectState"
    );
    assert!(
        !inspect.contains("self.advanced_inspect"),
        "Inspect methods must access workspace state through InspectState"
    );
}

#[test]
fn production_and_demo_share_one_tui_action_controller() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let controller = fs::read_to_string(root.join("src/tui/controller.rs"))
        .expect("read shared TUI action controller");
    let dispatch =
        fs::read_to_string(root.join("src/tui/dispatch.rs")).expect("read production TUI dispatch");
    let demo = fs::read_to_string(root.join("src/tui/demo/mod.rs")).expect("read demo TUI loop");

    assert!(controller.contains("pub(super) fn dispatch_action"));
    assert!(dispatch.contains("controller::dispatch_action"));
    assert!(demo.contains("controller::dispatch_action"));
    assert!(
        !demo.contains("fn handle_action("),
        "demo must not maintain a second TUI action router"
    );
}

#[test]
fn infrastructure_does_not_depend_on_application_layer() {
    let mut infrastructure = rust_sources_under("src/diskio");
    for path in ["src/edpb.rs", "src/disk_scan.rs"] {
        infrastructure.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(path));
    }
    assert_sources_exclude(infrastructure, &["crate::application"]);
    for path in [
        "src/media_identity.rs",
        "src/media_identity_observer.rs",
        "src/partition_table.rs",
        "src/backup_coverage.rs",
        "src/disk_layout.rs",
    ] {
        assert_sources_exclude(rust_sources_under(path), &["crate::application"]);
    }
}

#[test]
fn chapter_15_identity_write_boundaries_remain_separate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = |path: &str| {
        fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
    };
    let restore = source("src/application/write.rs");
    let selector = source("src/selectors.rs");
    let observer = source("src/media_identity_observer.rs");
    let matcher = source("src/media_identity.rs");
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
    assert!(prepare.contains("prepare_migrations"));
    assert!(prepare.contains("build_migrated_filesystem"));
    assert!(!commit.contains("Migrate 当前 unsupported"));
    assert!(commit.contains("Migrate 写集合缺少目标文件系统引导扇区"));
}
