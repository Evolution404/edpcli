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

fn near_hard_limit(actual: usize, hard_limit: usize) -> bool {
    actual.saturating_mul(5) >= hard_limit.saturating_mul(4)
}

#[test]
fn soft_size_budget_warns_before_existing_hard_limits() {
    assert!(!near_hard_limit(79, 100));
    assert!(near_hard_limit(80, 100));
    for (path, hard_limit) in [
        ("src/tui/mod.rs", 400),
        ("src/tui/render.rs", 1_200),
        ("src/tui/keymap.rs", 420),
        ("src/tui/keymap/help.rs", 320),
        ("src/tui/help_overlay.rs", 150),
        ("src/tui/status.rs", 140),
        ("src/tui/overview.rs", 180),
        ("src/tui/ui/modal.rs", 100),
        ("src/tui/ui/confirmation.rs", 180),
        ("src/tui/restore_confirmation_render.rs", 260),
        ("src/tui/ui/workspace_overview.rs", 160),
        ("src/tui/devices/state.rs", 400),
        ("src/tui/disk_layout_state.rs", 120),
        ("src/tui/navigation_state.rs", 300),
        ("src/tui/table_state.rs", 650),
        ("src/tui/resume.rs", 180),
        ("src/tui/inspect/state.rs", 350),
        ("src/tui/inspect/field_navigation.rs", 220),
        ("src/tui/inspect/lifecycle_state.rs", 180),
        ("src/tui/inspect/preview_state.rs", 200),
        ("src/tui/inspect/search_state.rs", 450),
        ("src/tui/inspect/jump_state.rs", 200),
        ("src/tui/inspect/detail_render.rs", 420),
        ("src/tui/inspect/field_table_render.rs", 180),
        ("src/tui/provision/state.rs", 400),
        ("src/tui/provision/execution_state.rs", 220),
        ("src/tui/provision/scheme_picker_state.rs", 120),
        ("src/tui/provision/scheme_picker_render.rs", 160),
        ("src/tui/provision/field_presentation.rs", 340),
        ("src/tui/provision/field_layout.rs", 180),
        ("src/cli_args.rs", 400),
        ("src/cli_args/parse_support.rs", 100),
        ("src/cli_args/help.rs", 160),
        ("src/cli.rs", 250),
        ("src/cli/prompter.rs", 180),
        ("src/cli_args/provision.rs", 700),
        ("src/application/inspect_tree/build.rs", 250),
        ("src/application/inspect_tree/topology.rs", 350),
        ("src/application/post_restore/layout_projection.rs", 160),
        ("src/tui/restore_result_state.rs", 320),
        ("src/tui/restore_result_partition_layout.rs", 260),
        ("src/tui/restore_result_verification.rs", 180),
        ("src/provision/reprovision/plan.rs", 400),
        ("src/edpb/identity.rs", 400),
    ] {
        let actual = lines(path);
        assert!(
            actual < hard_limit,
            "{path} exceeds hard limit {hard_limit}"
        );
        if near_hard_limit(actual, hard_limit) {
            eprintln!("[architecture soft budget] {path}: {actual}/{hard_limit} lines");
        }
    }
}

#[test]
fn complete_dependency_direction_is_guarded() {
    exists("src/disk_scan_render.rs");
    exists("src/text_width.rs");
    let lower_layer_forbidden = [
        "crate::application",
        "crate::tui",
        "crate::cli",
        "crate::ui",
        "crate::inspect_cli",
        "crate::metainfo_cli",
        "crate::inspect_adapter",
        "ratatui",
        "crossterm",
    ];
    for directory in [
        "src/protocol",
        "src/provision",
        "src/platform",
        "src/diskio",
        "src/edpb",
    ] {
        assert_sources_exclude(rust_sources_under(directory), &lower_layer_forbidden);
    }
    for path in [
        "src/media_identity.rs",
        "src/media_identity_observer.rs",
        "src/partition_table.rs",
        "src/backup_coverage.rs",
        "src/disk_layout.rs",
        "src/inspect_target.rs",
        "src/backup_metadata.rs",
        "src/filesystem/analysis/mod.rs",
        "src/backup_catalog.rs",
        "src/disk_scan.rs",
    ] {
        assert_sources_exclude(rust_sources_under(path), &lower_layer_forbidden);
    }
    let application_forbidden = [
        "crate::tui",
        "crate::cli",
        "crate::ui",
        "crate::inspect_cli",
        "crate::metainfo_cli",
        "ratatui",
        "crossterm",
    ];
    let mut application = rust_sources_under("src/application");
    application.extend(rust_sources_under("src/application.rs"));
    assert_sources_exclude(application, &application_forbidden);
}

#[test]
fn tui_does_not_import_protocol_implementation_modules() {
    assert_sources_exclude(rust_sources_under("src/tui"), &["crate::protocol::"]);
}

#[test]
fn provision_stage_writes_are_centralized_in_transitions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in rust_sources_under("src/tui") {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let relative = relative.to_string_lossy();
        if relative == "src/tui/provision/transitions.rs" || relative == "src/tui/demo/mod.rs" {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !source.contains("self.provision.stage = ProvisionStage::"),
            "{} writes ProvisionStage outside transitions",
            relative
        );
        assert!(
            !source.contains("provision_mut().stage = ProvisionStage::"),
            "{} writes ProvisionStage outside transitions",
            relative
        );
    }
}

#[test]
fn library_root_exposes_stable_interfaces_only() {
    let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("read library root");
    for module in ["backup_cli", "build_info", "elevate", "plist"] {
        assert!(
            source.contains(&format!("pub(crate) mod {module};")),
            "{module} is an internal implementation module"
        );
    }
    for module in [
        "application",
        "cli",
        "cli_args",
        "command_spec",
        "completion",
        "diskio",
        "edpb",
        "inspect",
        "platform",
        "protocol",
        "provision",
        "tui",
    ] {
        assert!(
            source.contains(&format!("pub mod {module};")),
            "{module} is a maintained external integration surface"
        );
    }
}

#[test]
fn large_modules_are_split_by_domain_boundary() {
    for path in [
        "src/application/provision/prepare.rs",
        "src/application/provision/commit.rs",
        "src/application/provision/export.rs",
        "src/application/provision/progress_projection.rs",
        "src/diskio/device.rs",
        "src/diskio/transaction.rs",
        "src/diskio/backup_config.rs",
        "src/diskio/backup_catalog.rs",
        "src/diskio/backup_create.rs",
        "src/tui/provision/state.rs",
        "src/tui/provision/execution_state.rs",
        "src/tui/provision/scheme_picker_state.rs",
        "src/tui/provision/form.rs",
        "src/tui/provision/plain_editor.rs",
        "src/tui/provision/fields.rs",
        "src/tui/provision/field_model.rs",
        "src/tui/provision/field_presentation.rs",
        "src/tui/provision/field_layout.rs",
        "src/tui/provision/field_input.rs",
        "src/tui/provision/password_verification.rs",
        "src/tui/provision/validation.rs",
        "src/tui/provision/layout.rs",
        "src/tui/provision/editor.rs",
        "src/tui/provision/render.rs",
        "src/tui/provision/scheme_picker_render.rs",
        "src/tui/provision/form_render.rs",
        "src/tui/provision/review_render.rs",
        "src/tui/provision/result_render.rs",
        "src/tui/provision/result_partition_layout.rs",
        "src/tui/provision/result_interaction.rs",
        "src/tui/provision/result_geometry.rs",
        "src/tui/provision/result_model.rs",
        "src/tui/ui/operation_result.rs",
        "src/tui/ui/result_supplement.rs",
        "src/tui/ui/result_table.rs",
        "src/tui/disk_region_list.rs",
        "src/tui/restore_result_state.rs",
        "src/tui/restore_result_render.rs",
        "src/tui/restore_result_partition_layout.rs",
        "src/tui/restore_result_verification.rs",
        "src/tui/wizard_result_render.rs",
        "src/tui/operation_progress_render.rs",
        "src/tui/operation_progress_status.rs",
        "src/tui/progress_transport.rs",
        "src/tui/runtime_updates.rs",
        "src/tui/resume.rs",
        "src/tui/runtime_input.rs",
        "src/tui/runtime_input/inspect.rs",
        "src/tui/runtime_input/provision.rs",
        "src/tui/runtime_input/backup_batch.rs",
        "src/tui/runtime_input/backup_prune.rs",
        "src/tui/runtime_input/backup_wizard.rs",
        "src/tui/runtime_input/shell.rs",
        "src/tui/provision/task.rs",
        "src/tui/inspect/state.rs",
        "src/tui/inspect/lifecycle_state.rs",
        "src/tui/inspect/field_navigation.rs",
        "src/tui/inspect/search_state.rs",
        "src/tui/inspect/jump_state.rs",
        "src/tui/inspect/detail_state.rs",
        "src/tui/inspect/tree_state.rs",
        "src/tui/inspect/sector_state.rs",
        "src/tui/inspect/preview_state.rs",
        "src/tui/inspect/render.rs",
        "src/tui/inspect/tree_render.rs",
        "src/tui/inspect/detail_render.rs",
        "src/tui/inspect/field_table_render.rs",
        "src/tui/inspect/sector_render.rs",
        "src/tui/backups/state.rs",
        "src/tui/backups/render.rs",
        "src/tui/devices/render.rs",
        "src/tui/devices/state.rs",
        "src/tui/dispatch.rs",
        "src/tui/controller.rs",
        "src/tui/controller/provision.rs",
        "src/tui/disk_layout_state.rs",
        "src/tui/navigation_state.rs",
        "src/tui/task_gate.rs",
        "src/tui/table_state.rs",
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
    assert!(lines("src/application/provision/progress_projection.rs") < 120);
    assert!(lines("src/diskio.rs") < 500);
    assert!(lines("src/tui/state.rs") < 2_200);
    assert!(
        lines("src/tui/navigation_state.rs") < 300,
        "workspace/pane navigation state must stay isolated from AppState business state"
    );
    assert!(
        lines("src/tui/disk_layout_state.rs") < 120,
        "shared DiskLayout interaction state must stay isolated from AppState orchestration"
    );
    assert!(
        lines("src/tui/devices/state.rs") < 400,
        "Devices workspace state must stay isolated from AppState orchestration"
    );
    assert!(
        lines("src/tui/table_state.rs") < 650,
        "shared table interaction state must stay isolated from AppState orchestration"
    );
    assert!(lines("src/tui/render.rs") < 1_500);
    assert!(lines("src/tui/task.rs") < 800);
    assert!(
        lines("src/tui/task_gate.rs") < 180,
        "generation/single-flight task gates must stay isolated from business worker routing"
    );
    assert!(
        lines("src/tui/mod.rs") < 400,
        "TUI module root must remain lifecycle-oriented; action dispatch belongs in dispatch.rs"
    );
    assert!(
        lines("src/tui/resume.rs") < 180,
        "TUI elevation-resume argv encoding must stay isolated from terminal lifecycle"
    );
    assert!(lines("src/tui/runtime_updates.rs") < 160);
    assert!(lines("src/tui/runtime_input.rs") < 180);
    for path in [
        "src/tui/runtime_input/inspect.rs",
        "src/tui/runtime_input/provision.rs",
        "src/tui/runtime_input/backup_batch.rs",
        "src/tui/runtime_input/backup_prune.rs",
        "src/tui/runtime_input/backup_wizard.rs",
        "src/tui/runtime_input/shell.rs",
    ] {
        assert!(
            lines(path) < 300,
            "runtime input handler is oversized: {path}"
        );
    }
    assert!(
        lines("src/tui/dispatch.rs") < 600,
        "TUI dispatch module must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/controller.rs") < 600,
        "shared TUI action controller must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/controller/provision.rs") < 350,
        "Provision action routing must stay isolated from the shared controller"
    );
    assert!(
        lines("src/tui/inspect/state.rs") < 350,
        "Inspect workspace state must stay type/focus-oriented"
    );
    assert!(
        lines("src/tui/inspect/field_navigation.rs") < 220,
        "Inspect field navigation must stay isolated from workspace state"
    );
    assert!(
        lines("src/tui/inspect/lifecycle_state.rs") < 180,
        "Inspect begin/request/finish lifecycle must stay isolated from navigation state"
    );
    assert!(
        lines("src/tui/inspect/preview_state.rs") < 200,
        "Inspect preview loading must stay isolated from workspace orchestration"
    );
    assert!(
        lines("src/tui/inspect/search_state.rs") < 450,
        "Inspect search/prompt state must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/jump_state.rs") < 200,
        "Inspect jump state must stay isolated from structured search"
    );
    assert!(
        lines("src/tui/inspect/detail_state.rs") < 500,
        "Inspect detail/pane state must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/tree_state.rs") < 600,
        "Inspect tree state must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/sector_state.rs") < 450,
        "Sector Inspector state must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/render.rs") < 400,
        "Inspect workspace renderer must stay layout/orchestration-oriented"
    );
    assert!(
        lines("src/tui/inspect/tree_render.rs") < 180,
        "Inspect tree renderer must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/detail_render.rs") < 420,
        "Inspect detail renderer must stay responsibility-bounded"
    );
    assert!(
        lines("src/tui/inspect/field_table_render.rs") < 180,
        "Inspect field table rendering must stay isolated from object evidence rendering"
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
        lines("src/tui/provision/state.rs") < 400,
        "Provision orchestration state must not absorb form/capacity/plain model again"
    );
    assert!(
        lines("src/tui/provision/execution_state.rs") < 220,
        "Provision export/confirm/write lifecycle must stay isolated from form orchestration"
    );
    assert!(lines("src/tui/provision/scheme_picker_state.rs") < 120);
    assert!(lines("src/tui/provision/scheme_picker_render.rs") < 160);
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
        lines("src/tui/provision/fields.rs") < 350,
        "Provision field navigation and input editing must stay responsibility-bounded"
    );
    assert!(lines("src/tui/provision/field_model.rs") < 120);
    assert!(lines("src/tui/provision/field_presentation.rs") < 340);
    assert!(lines("src/tui/provision/field_layout.rs") < 180);
    assert!(lines("src/tui/provision/field_input.rs") < 250);
    assert!(lines("src/tui/provision/password_verification.rs") < 200);
    assert!(lines("src/tui/provision/key_domains.rs") < 80);
    assert!(
        lines("src/tui/provision/render.rs") < 400,
        "Provision root renderer must stay layout/orchestration-oriented"
    );
    assert!(lines("src/tui/provision/form_render.rs") < 300);
    assert!(lines("src/tui/provision/review_render.rs") < 170);
    assert!(lines("src/tui/provision/result_render.rs") < 340);
    assert!(lines("src/tui/provision/result_partition_layout.rs") < 260);
    assert!(lines("src/tui/provision/result_interaction.rs") < 180);
    assert!(lines("src/tui/provision/result_geometry.rs") < 120);
    assert!(lines("src/tui/provision/result_model.rs") < 120);
    assert!(lines("src/tui/ui/operation_result.rs") < 340);
    assert!(lines("src/tui/ui/result_supplement.rs") < 100);
    assert!(lines("src/tui/ui/result_table.rs") < 140);
    assert!(lines("src/tui/disk_region_list.rs") < 100);
    assert!(lines("src/tui/restore_result_state.rs") < 320);
    assert!(lines("src/tui/restore_result_render.rs") < 120);
    assert!(lines("src/tui/restore_result_partition_layout.rs") < 260);
    assert!(lines("src/tui/restore_result_verification.rs") < 180);
    assert!(lines("src/tui/wizard_result_render.rs") < 120);
    assert!(lines("src/tui/operation_progress_render.rs") < 260);
    assert!(lines("src/tui/operation_progress_status.rs") < 120);
    assert!(lines("src/tui/progress_transport.rs") < 180);
    let field_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/fields.rs"),
    )
    .expect("read provision fields module");
    let orchestration_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/state.rs"),
    )
    .expect("read provision orchestration state");
    let input_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/provision/field_input.rs"),
    )
    .expect("read provision field input module");
    for name in ["provision_push_char", "provision_delete_char"] {
        assert!(
            input_source.contains(name),
            "field input module is missing {name}"
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
    assert!(!field_source.contains("fn provision_visible_fields("));
    assert!(!field_source.contains("fn provision_push_char("));
    assert!(!field_source.contains("fn provision_source_password_verify_request("));
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
fn cli_entry_is_split_by_command_domain() {
    for path in [
        "src/cli_args/provision.rs",
        "src/cli_args/inspect.rs",
        "src/cli_args/backup.rs",
        "src/cli_args/help.rs",
        "src/cli_args/parse_support.rs",
        "src/cli/commands/provision.rs",
        "src/cli/commands/backup.rs",
        "src/cli/prompter.rs",
    ] {
        exists(path);
    }
    assert!(lines("src/cli_args.rs") < 400);
    assert!(lines("src/cli_args/parse_support.rs") < 100);
    assert!(lines("src/cli_args/help.rs") < 160);
    assert!(lines("src/cli.rs") < 250);
    assert!(lines("src/cli/prompter.rs") < 180);
    let args = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli_args.rs"))
        .expect("read CLI parser root");
    assert!(!args.contains("fn parse_new_provision_opts("));
    assert!(!args.contains("fn parse_lbas("));
    let cli = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli.rs"))
        .expect("read CLI command root");
    assert!(!cli.contains("fn provision_flow("));
    assert!(!cli.contains("fn real_flow("));
}

#[test]
fn application_inspect_is_split_by_read_responsibility() {
    for path in [
        "src/application/inspect/model.rs",
        "src/application/inspect/request.rs",
        "src/application/inspect/decode.rs",
        "src/application/inspect/source.rs",
        "src/application/inspect/export.rs",
        "src/application/inspect/service.rs",
        "src/application/inspect_tree/model.rs",
        "src/application/inspect_tree/build.rs",
        "src/application/inspect_tree/topology.rs",
        "src/application/inspect_tree/search.rs",
    ] {
        exists(path);
    }
    assert!(lines("src/application/inspect.rs") < 160);
    assert!(lines("src/application/inspect_tree.rs") < 80);
    let inspect = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/inspect.rs"),
    )
    .expect("read inspect service root");
    assert!(!inspect.contains("fn run_advanced_source("));
    assert!(!inspect.contains("fn sector_meta_text("));
}

#[test]
fn reprovision_domain_is_split_by_responsibility() {
    for path in [
        "src/provision/reprovision/model.rs",
        "src/provision/reprovision/parsing.rs",
        "src/provision/reprovision/prefill.rs",
        "src/provision/reprovision/geometry.rs",
        "src/provision/reprovision/disposition.rs",
        "src/provision/reprovision/plan.rs",
    ] {
        exists(path);
    }
    assert!(lines("src/provision/reprovision.rs") < 100);
    assert!(lines("src/provision/reprovision/plan.rs") < 400);
    let root = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provision/reprovision.rs"),
    )
    .expect("read reprovision root");
    assert!(!root.contains("fn parse_existing_provision("));
    assert!(!root.contains("fn prefill_for_target_mode("));
    assert!(!root.contains("fn decide_partition_action("));
}

#[test]
fn edpb_container_is_split_by_protocol_responsibility() {
    for path in [
        "src/edpb/model.rs",
        "src/edpb/codec.rs",
        "src/edpb/identity.rs",
        "src/edpb/write.rs",
        "src/edpb/read.rs",
        "src/edpb/validate.rs",
        "src/edpb/legacy.rs",
    ] {
        exists(path);
    }
    assert!(lines("src/edpb.rs") < 100);
    for path in [
        "src/edpb/write.rs",
        "src/edpb/read.rs",
        "src/edpb/validate.rs",
    ] {
        assert!(
            lines(path) < 400,
            "EDPB responsibility module oversized: {path}"
        );
    }
    let root = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/edpb.rs"))
        .expect("read EDPB root");
    assert!(!root.contains("fn verify_file("));
    assert!(!root.contains("fn write_container("));
}

#[test]
fn workspace_modules_do_not_import_platform_or_diskio_directly() {
    for path in [
        "src/tui/provision/state.rs",
        "src/tui/provision/execution_state.rs",
        "src/tui/provision/scheme_picker_state.rs",
        "src/tui/provision/render.rs",
        "src/tui/provision/scheme_picker_render.rs",
        "src/tui/inspect/state.rs",
        "src/tui/inspect/search_state.rs",
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = rust_sources_under("src/cli")
        .into_iter()
        .chain([root.join("src/cli.rs")])
        .map(|path| fs::read_to_string(path).expect("read CLI source"))
        .collect::<String>();
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
    let cli = fs::read_to_string(root.join("src/cli/commands/provision.rs"))
        .expect("read CLI provision command");
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
        "src/filesystem/analysis/mod.rs",
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
    let fat = fs::read_to_string(root.join("src/filesystem/analysis/fat.rs"))
        .expect("read FAT parser source");
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/inspect/service.rs"),
    )
    .expect("read application inspect service");
    assert_sources_exclude(
        rust_sources_under("src/application/inspect"),
        &[
            "struct AdvancedBackupReader",
            "crate::edpb::verify_file",
            "crate::edpb::read_raw_protocol",
            "FileDev::open_rdonly",
        ],
    );

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

    let app_state = state
        .split("pub struct AppState {")
        .nth(1)
        .and_then(|tail| tail.split("impl Default for AppState").next())
        .expect("AppState section");
    assert!(app_state.contains("devices: DevicesState"));
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
            !app_state.contains(legacy_field),
            "device-owned field must live in DevicesState: {legacy_field}"
        );
    }
    for owned_field in [
        "rows: Vec<crate::disk_scan::Row>",
        "table_view: super::super::table_layout::TableViewData",
        "scan_pending: bool",
        "pane_focus: crate::tui::pane::PaneFocus",
        "info_selected: DeviceInfoNodeKey",
        "info_expanded: BTreeSet<DeviceInfoNodeKey>",
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
        "prune: Option<BackupPruneState>",
        "pane_focus: crate::tui::pane::PaneFocus",
    ] {
        assert!(
            backups.contains(owned_field),
            "BackupsState must own field: {owned_field}"
        );
    }
    assert!(!backups.contains("BackupCreateChoiceState"));
    assert!(!backups.contains("create_choice:"));
}

#[test]
fn inspect_tree_and_detail_renderers_are_split_from_workspace_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let render =
        fs::read_to_string(root.join("src/tui/inspect/render.rs")).expect("read inspect renderer");
    let tree = fs::read_to_string(root.join("src/tui/inspect/tree_render.rs"))
        .expect("read inspect tree renderer");
    let detail = fs::read_to_string(root.join("src/tui/inspect/detail_render.rs"))
        .expect("read inspect detail renderer");

    assert!(!render.contains("fn draw_inspect_tree_pane"));
    assert!(tree.contains("fn draw_inspect_tree_pane"));
    assert!(!render.contains("fn draw_inspect_object_panes"));
    assert!(detail.contains("fn draw_inspect_object_panes"));
}

#[test]
fn inspect_cached_decode_enriches_topology_nodes_instead_of_rebuilding_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tree_state = fs::read_to_string(root.join("src/tui/inspect/tree_state.rs"))
        .expect("read inspect tree state");
    let build = fs::read_to_string(root.join("src/application/inspect_tree/build.rs"))
        .expect("read inspect tree build");

    assert!(tree_state.contains("enrich_sector_node("));
    assert!(
        !tree_state.contains("standalone_sector_node_with_fields("),
        "TUI cache hydration must enrich canonical topology nodes rather than reconstruct them"
    );
    assert!(build.contains("pub fn enrich_sector_node"));
    assert!(build.contains("pub fn standalone_sector_node_with_fields"));
}

#[test]
fn tui_renderers_do_not_assume_parent_surface_palette_colors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in rust_sources_under("src/tui") {
        if path.ends_with("theme.rs") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for forbidden in [
            "palette().background",
            "palette().canvas",
            "palette().surface",
            "palette().surface_raised",
            "palette().surface_active",
            "palette().surface_focus",
            "Color::",
            ".fg(",
            ".bg(",
            "Modifier::REVERSED",
            "focused_panel()",
            ".border_style(panel())",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} must consume surface styles through theme APIs or inherit the parent buffer; found {forbidden}",
                path.display()
            );
        }
    }
    let layout =
        fs::read_to_string(root.join("src/tui/disk_layout.rs")).expect("read disk layout renderer");
    assert!(layout.contains("disk_region_half_block"));
    let table = fs::read_to_string(root.join("src/tui/ui/table.rs")).expect("read shared table");
    let card = fs::read_to_string(root.join("src/tui/ui/card.rs")).expect("read shared card");
    assert!(table.contains("table_surface(focused)"));
    assert!(card.contains("pane_surface(focused)"));
}

#[test]
fn media_write_yes_prompt_has_one_ui_owner_and_backup_management_does_not_reuse_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let confirmation = fs::read_to_string(root.join("src/tui/ui/confirmation.rs"))
        .expect("read confirmation component");
    let root_render = fs::read_to_string(root.join("src/tui/render.rs")).expect("read root render");
    let provision_render = fs::read_to_string(root.join("src/tui/provision/render.rs"))
        .expect("read provision render");
    let backup_render =
        fs::read_to_string(root.join("src/tui/backups/render.rs")).expect("read backup render");
    let backup_state =
        fs::read_to_string(root.join("src/tui/backups/state.rs")).expect("read backup state");
    let batch_input = fs::read_to_string(root.join("src/tui/runtime_input/backup_batch.rs"))
        .expect("read batch input");
    let prune_input = fs::read_to_string(root.join("src/tui/runtime_input/backup_prune.rs"))
        .expect("read prune input");

    assert!(confirmation.contains("输入 YES 确认写入"));
    assert!(confirmation.contains("Enter\", theme.accent().add_modifier(Modifier::BOLD)"));
    assert!(confirmation.contains("开始恢复"));
    assert!(root_render.contains("render_write_confirmation_modal"));
    assert!(provision_render.contains("render_write_confirmation_modal"));
    assert!(backup_render.contains("render_action_confirmation_modal"));
    for (name, source) in [
        ("root renderer", root_render.as_str()),
        ("provision renderer", provision_render.as_str()),
        ("backup renderer", backup_render.as_str()),
    ] {
        assert!(
            !source.contains("输入 YES"),
            "{name} must delegate the write-authorization prompt to the shared component"
        );
    }
    for (name, source) in [
        ("backup state", backup_state.as_str()),
        ("backup batch input", batch_input.as_str()),
        ("backup prune input", prune_input.as_str()),
    ] {
        assert!(
            !source.contains("YES"),
            "{name} must not reuse media-write authorization for backup-file management"
        );
    }
}

#[test]
fn help_and_status_information_architecture_has_single_owners() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let render = fs::read_to_string(root.join("src/tui/render.rs")).expect("read root renderer");
    let keymap = fs::read_to_string(root.join("src/tui/keymap.rs")).expect("read keymap");
    let help_registry =
        fs::read_to_string(root.join("src/tui/keymap/help.rs")).expect("read help registry");
    let help = fs::read_to_string(root.join("src/tui/help_overlay.rs")).expect("read help overlay");
    let status = fs::read_to_string(root.join("src/tui/status.rs")).expect("read status model");
    let shell = fs::read_to_string(root.join("src/tui/shell/mod.rs")).expect("read shell");
    let controller =
        fs::read_to_string(root.join("src/tui/controller.rs")).expect("read controller");

    assert!(render.contains("status::dynamic_status"));
    assert!(render.contains("help_overlay::draw_help_overlay"));
    assert!(!render.contains("制盘方案：j/k"));
    assert!(!render.contains("检查字段表："));
    assert!(!render.contains("y 单元格 · Y 整行"));
    assert!(!render.contains("Tab/Shift-Tab 或 gt/gT"));
    assert!(keymap.contains("pub use help::"));
    assert!(help_registry.contains("pub const DEVICES_HELP"));
    assert!(help_registry.contains("pub const BACKUPS_HELP"));
    assert!(help_registry.contains("pub const PROVISION_HELP"));
    assert!(help_registry.contains("pub const GLOBAL_HELP"));
    assert!(!help_registry.contains("pub const NORMAL_HELP"));
    assert!(help.contains("PICKER_HELP"));
    assert!(help.contains("TABLE_HELP"));
    assert!(status.contains("pub(super) fn dynamic_status"));
    assert!(shell.contains("\"? 帮助\""));
    assert!(shell.contains("pub fn message_bar"));
    assert!(!shell.contains("pub fn footer"));
    let dispatch = controller
        .split_once("pub(super) fn dispatch_action")
        .map(|(_, dispatch)| dispatch)
        .expect("dispatch_action");
    let global_help = dispatch
        .find("if action == TuiAction::Help")
        .expect("global help dispatch");
    let picker = dispatch
        .find("if state.provision_scheme_picker_open()")
        .expect("picker dispatch");
    assert!(
        global_help < picker,
        "global ? help must be dispatched before business overlays can swallow it"
    );
}

#[test]
fn device_and_backup_overviews_share_one_layout_and_one_kind_counter() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let devices = fs::read_to_string(root.join("src/tui/devices/list_render.rs"))
        .expect("read device list renderer");
    let backups =
        fs::read_to_string(root.join("src/tui/backups/render.rs")).expect("read backup renderer");
    let overview =
        fs::read_to_string(root.join("src/tui/overview.rs")).expect("read overview model");
    let component = fs::read_to_string(root.join("src/tui/ui/workspace_overview.rs"))
        .expect("read overview component");

    assert!(devices.contains("ui::workspace_overview"));
    assert!(backups.contains("ui::workspace_overview"));
    assert!(devices.contains("ProvisionKindCounts::from_kinds"));
    assert!(backups.contains("ProvisionKindCounts::from_kinds"));
    assert!(overview.contains("if count > 0"));
    assert!(component.contains("搜索 · 实时过滤"));
    for stale in [
        "h/l 激活",
        "</> 移列",
        "0/$ 首尾列",
        "H/L 视口",
        "s 排序",
        "S 默认",
    ] {
        assert!(
            !backups.contains(stale),
            "backup table title must not advertise shortcuts: {stale}"
        );
    }
}

#[test]
fn modal_surface_is_a_shared_theme_primitive_not_a_business_local_clear_block() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let modal = fs::read_to_string(root.join("src/tui/ui/modal.rs")).expect("read modal primitive");
    let picker = fs::read_to_string(root.join("src/tui/provision/scheme_picker_render.rs"))
        .expect("read scheme picker");

    assert!(modal.contains("theme.modal_background()"));
    assert!(modal.contains("theme.modal_border()"));
    assert!(modal.contains("buffer.set_style"));
    assert!(modal.contains("Clear"));
    assert!(picker.contains("ui::render_modal"));
    assert!(!picker.contains("Clear"));
    assert!(!picker.contains("Block::default"));
}

#[test]
fn provision_stage_renderers_are_split_from_workspace_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let render = fs::read_to_string(root.join("src/tui/provision/render.rs"))
        .expect("read provision renderer");
    for (path, marker) in [
        ("form_render.rs", "fn draw_provision_form"),
        ("review_render.rs", "fn draw_provision_review"),
    ] {
        let source = fs::read_to_string(root.join("src/tui/provision").join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(source.contains(marker), "{path} must own {marker}");
        assert!(!render.contains(marker), "{marker} leaked into render.rs");
    }
    let shared = fs::read_to_string(root.join("src/tui/operation_progress_render.rs"))
        .expect("read shared operation progress renderer");
    assert!(shared.contains("fn draw_operation_progress"));
    assert!(!root.join("src/tui/provision/running_render.rs").exists());
    assert!(!root.join("src/tui/provision/selection_render.rs").exists());
    let provision_state =
        fs::read_to_string(root.join("src/tui/provision/state.rs")).expect("read provision state");
    let table_state =
        fs::read_to_string(root.join("src/tui/table_state.rs")).expect("read table state");
    let table_layout =
        fs::read_to_string(root.join("src/tui/table_layout.rs")).expect("read table layout");
    for forbidden in [
        "ProvisionStage::SelectDisk",
        "provision_select_disk",
        "ProvisionDevices",
    ] {
        assert!(
            !provision_state.contains(forbidden)
                && !table_state.contains(forbidden)
                && !table_layout.contains(forbidden),
            "redundant provision device-selection surface returned: {forbidden}"
        );
    }
    assert!(!root.join("src/tui/runtime_input/backup_choice.rs").exists());
    assert!(!root.join("src/tui/backups/result_render.rs").exists());
}

#[test]
fn inspect_tree_model_and_navigation_are_split_from_workspace_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state =
        fs::read_to_string(root.join("src/tui/inspect/state.rs")).expect("read inspect state");
    let tree = fs::read_to_string(root.join("src/tui/inspect/tree_state.rs"))
        .expect("read inspect tree state");

    for marker in [
        "pub struct AdvancedInspectTreeRow",
        "pub fn advanced_inspect_tree_rows",
        "pub fn advanced_inspect_move_tree",
        "pub fn advanced_inspect_toggle_selected",
    ] {
        assert!(
            !state.contains(marker),
            "{marker} leaked back into inspect/state.rs"
        );
        assert!(
            tree.contains(marker),
            "{marker} missing from inspect/tree_state.rs"
        );
    }
}

#[test]
fn inspect_detail_and_pane_state_is_split_from_workspace_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state =
        fs::read_to_string(root.join("src/tui/inspect/state.rs")).expect("read inspect state");
    let detail = fs::read_to_string(root.join("src/tui/inspect/detail_state.rs"))
        .expect("read inspect detail state");

    for marker in [
        "pub fn advanced_inspect_detail_rows",
        "pub fn advanced_inspect_detail_toggle_selected",
        "pub fn advanced_inspect_focused_content_len",
        "pub fn advanced_inspect_move_focused_vertical",
    ] {
        assert!(
            !state.contains(marker),
            "{marker} leaked back into inspect/state.rs"
        );
        assert!(
            detail.contains(marker),
            "{marker} missing from inspect/detail_state.rs"
        );
    }
}

#[test]
fn inspect_search_and_jump_state_is_split_from_workspace_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state =
        fs::read_to_string(root.join("src/tui/inspect/state.rs")).expect("read inspect state");
    let search = fs::read_to_string(root.join("src/tui/inspect/search_state.rs"))
        .expect("read inspect search state");
    let jump = fs::read_to_string(root.join("src/tui/inspect/jump_state.rs"))
        .expect("read inspect jump state");

    for marker in [
        "pub fn advanced_inspect_begin_jump",
        "pub fn advanced_inspect_begin_search",
        "pub fn advanced_inspect_search_next",
    ] {
        assert!(
            !state.contains(marker),
            "{marker} leaked back into inspect/state.rs"
        );
        assert!(
            search.contains(marker),
            "{marker} missing from inspect/search_state.rs"
        );
    }
    let jump_marker = "pub fn advanced_inspect_jump_lba";
    assert!(
        !state.contains(jump_marker) && !search.contains(jump_marker),
        "{jump_marker} must stay isolated from workspace/search state"
    );
    assert!(
        jump.contains(jump_marker),
        "{jump_marker} missing from inspect/jump_state.rs"
    );
}

#[test]
fn app_state_owns_inspect_through_inspect_substate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let inspect =
        fs::read_to_string(root.join("src/tui/inspect/state.rs")).expect("read inspect state");
    let table_state =
        fs::read_to_string(root.join("src/tui/table_state.rs")).expect("read table state");

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
        state.contains("self.inspect.advanced") || table_state.contains("self.inspect.advanced"),
        "AppState facade modules must access Inspect workspace field through InspectState"
    );
    assert!(
        inspect.contains("self.inspect.advanced"),
        "Inspect methods must access workspace field through InspectState"
    );
}

#[test]
fn app_state_owns_global_shell_state_through_shell_substate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let shell = fs::read_to_string(root.join("src/tui/shell/state.rs")).expect("read shell state");

    let app_state = state
        .split("pub struct AppState {")
        .nth(1)
        .and_then(|tail| tail.split("impl Default for AppState").next())
        .expect("AppState section");
    assert!(app_state.contains("shell: ShellState"));
    for legacy_field in [
        "demo_mode: bool",
        "workspace: Workspace",
        "critical_operation: bool",
        "exit_pending: bool",
        "navigation: NavigationStack",
        "notice: Option<String>",
        "notice_at: Option<std::time::Instant>",
        "animation_frame: u64",
        "selected: usize",
        "item_count: usize",
        "input_mode: InputMode",
        "input_buffer: String",
        "search_query: String",
        "search_matches: Vec<usize>",
        "search_cursor: usize",
        "wizard: Option<WizardState>",
        "pinned_disk: Option<u32>",
        "disk_layout_tail: crate::tui::disk_layout::TailExpansion",
        "disk_layout_selected: usize",
    ] {
        assert!(
            !app_state
                .lines()
                .any(|line| line.trim() == format!("{legacy_field},")),
            "global shell field must live in ShellState: {legacy_field}"
        );
        assert!(
            shell.contains(legacy_field),
            "ShellState must own global field: {legacy_field}"
        );
    }
}

#[test]
fn app_state_is_only_shell_plus_four_workspace_states() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/tui/state.rs")).expect("read TUI state");
    let app_state = state
        .split("pub struct AppState {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect("AppState body");
    let fields = app_state
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(':') && line.ends_with(','))
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        [
            "shell: ShellState,",
            "devices: DevicesState,",
            "inspect: InspectState,",
            "backups: BackupsState,",
            "provision: ProvisionState,",
        ]
    );
}

#[test]
fn tui_event_loop_does_not_interpret_workspace_stages() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("src/tui/mod.rs")).expect("read TUI module root");
    let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
    assert!(production.contains("runtime_input::handle_key"));
    for stage in [
        "AdvancedInspectStage",
        "ProvisionStage",
        "BackupBatchDeleteStage",
        "BackupPruneStage",
        "WizardStage",
    ] {
        assert!(
            !production.contains(stage),
            "event loop still interprets {stage}"
        );
    }
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
    let edpb_writer = source("src/edpb/write.rs");
    let edpb_legacy = source("src/edpb/legacy.rs");
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

    assert!(!edpb_writer.contains("hardware_serial_sha256="));
    assert!(!backup_writer.contains("hardware_serial_sha256="));
    assert!(edpb_legacy.contains("fn legacy_hardware_serial_digest("));
    assert!(edpb_writer.contains("edpb.manifest.v3"));
    assert!(edpb_writer.contains("write_legacy_v2_core_backup_with_identity"));

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

#[test]
fn passive_capacity_display_uses_one_global_unit_system() {
    for path in [
        "src/disk_scan_render.rs",
        "src/ui.rs",
        "src/inspect_cli.rs",
        "src/backup_cli.rs",
        "src/application/identity.rs",
        "src/metainfo.rs",
        "src/inspect/model.rs",
        "src/tui/render.rs",
        "src/tui/devices/state.rs",
        "src/tui/devices/presentation.rs",
        "src/tui/disk_layout.rs",
        "src/tui/provision/layout.rs",
        "src/tui/provision/render.rs",
        "src/tui/provision/scheme_picker_render.rs",
    ] {
        let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(
            source.contains("fmt_capacity"),
            "{path} must route passive capacity text through the global formatter"
        );
        for forbidden in [
            "1_073_741_824.0",
            "1_000_000_000.0",
            "/ 1024.0 / 1024.0 / 1024.0",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} hardcodes capacity conversion {forbidden}"
            );
        }
    }
}


#[test]
fn post_restore_layout_projection_is_application_owned_and_nonfatal() {
    let projection = include_str!("../src/application/post_restore/layout_projection.rs");
    let restore = include_str!("../src/application/write.rs");

    assert!(projection.contains("parse_existing_provision"));
    assert!(projection.contains("DiskRegionKind::from_partition_role"));
    assert!(projection.contains("DiskLayoutModel::canonical_edp"));
    assert!(projection.contains("DiskLayoutModel::canonical_plain_plan"));
    assert!(
        !projection.contains("crate::tui"),
        "post-restore layout projection must stay UI-neutral"
    );

    let restore_tail = restore
        .split("let layout = super::post_restore::project_restored_layout_readonly")
        .nth(1)
        .expect("restore flow must retain a typed layout projection result");
    let outcome_section = restore_tail
        .split("Ok(super::post_restore::MetadataRestoreOutcome")
        .nth(1)
        .expect("restore flow must still return MetadataRestoreOutcome");
    assert!(
        outcome_section.contains("layout,"),
        "projection Result must be carried into the outcome"
    );
    assert!(
        !restore_tail
            .split("Ok(super::post_restore::MetadataRestoreOutcome")
            .next()
            .unwrap_or_default()
            .contains("?;"),
        "layout projection failure must not reclassify a verified restore as failed"
    );
}


#[test]
fn restore_result_has_one_selection_source_of_truth() {
    let state = include_str!("../src/tui/state.rs");
    let result_state = include_str!("../src/tui/restore_result_state.rs");
    let render = include_str!("../src/tui/render.rs");

    assert!(
        !state.contains("post_restore_selected"),
        "legacy restore result row selection must not return"
    );
    assert!(
        result_state.contains("post_restore_workbench.selected_partition"),
        "restore actions must resolve selection through the shared result workbench"
    );
    assert!(
        render.contains("WizardStage::PostRestore => unreachable!"),
        "legacy inline PostRestore renderer must remain unreachable"
    );
}
