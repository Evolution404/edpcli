use std::fs;
use std::path::Path;

fn source(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn cli_and_tui_share_provision_prepare_commit_and_export_entrypoints() {
    let cli = source("src/cli/commands/provision.rs");
    let task = source("src/tui/provision/task.rs");

    for entrypoint in [
        "prepare_provision",
        "commit_provision",
        "export_provision_image",
    ] {
        assert!(cli.contains(entrypoint), "CLI missing {entrypoint}");
        assert!(task.contains(entrypoint), "TUI missing {entrypoint}");
    }

    for forbidden in [
        "prepare_target_provision(",
        "prepare_plain_provision(",
        "commit_new_provision(",
        "commit_plain_provision(",
        "export_sparse_provision_image(",
    ] {
        assert!(
            !cli.contains(forbidden),
            "CLI bypasses unified application path with {forbidden}"
        );
        assert!(
            !task.contains(forbidden),
            "TUI bypasses unified application path with {forbidden}"
        );
    }
}

#[test]
fn plain_is_a_first_class_target_and_not_mode4() {
    let help = source("src/cli_args/help.rs");
    let parser = source("src/cli_args/provision.rs");
    let application = source("src/application/provision.rs");
    let tui_execution = source("src/tui/provision/execution_state.rs");

    assert!(help.contains("mode0|mode1|mode2|mode3|plain"));
    assert!(help.contains("--partition"));
    assert!(help.contains("Plain 不是 mode4"));
    assert!(!parser.contains(r#""4" => Ok"#));

    assert!(application.contains("enum ProvisionRequest"));
    assert!(application.contains("Official(Box<OfficialProvisionRequest>)"));
    assert!(application.contains("Plain(PlainProvisionRequest)"));
    assert!(application.contains("enum PreparedProvision"));
    assert!(tui_execution.contains("./edp-plain.img"));
}

#[test]
fn optional_format_choices_are_not_forced_on_by_rebuild_actions() {
    let prepare = source("src/application/provision/prepare.rs");
    assert!(prepare.contains("let format_options = request.format.clone();"));
    assert!(
        !prepare.contains("format_options.boot = target_plan.partitions"),
        "Rebuild must not silently override the user's optional format checkbox"
    );
    assert!(
        !prepare.contains("format_options.share = target_plan.partitions")
            && !prepare.contains("format_options.encrypt = target_plan.partitions"),
        "all three format checkboxes must remain user-controlled"
    );
}

#[test]
fn plain_export_is_available_in_tui_review_flow() {
    let execution = source("src/tui/provision/execution_state.rs");
    let keymap = source("src/tui/keymap.rs");
    let controller = source("src/tui/controller/provision.rs");

    assert!(
        !execution.contains("ProvisionPrepared::Plain(_) => return None"),
        "Plain export must not be blocked by TUI state"
    );
    assert!(keymap.contains("KeyCode::Char('e') => Some(TuiAction::Export)"));
    assert!(controller.contains("ProvisionStage::Review => match action"));
    assert!(controller.contains("TuiAction::Export =>"));
    assert!(controller.contains("state.provision_begin_export();"));
}
