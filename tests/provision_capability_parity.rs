use std::fs;
use std::path::Path;

fn source(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn cli_and_tui_share_provision_prepare_commit_and_export_entrypoints() {
    let cli = source("src/cli.rs");
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
    let args = source("src/cli_args.rs");
    let application = source("src/application/provision.rs");
    let tui_state = source("src/tui/provision/state.rs");

    assert!(args.contains("mode0|mode1|mode2|mode3|plain"));
    assert!(args.contains("--partition"));
    assert!(args.contains("Plain 不是 mode4"));
    assert!(!args.contains(r#""4" => Ok"#));

    assert!(application.contains("enum ProvisionRequest"));
    assert!(application.contains("Official(Box<OfficialProvisionRequest>)"));
    assert!(application.contains("Plain(PlainProvisionRequest)"));
    assert!(application.contains("enum PreparedProvision"));
    assert!(tui_state.contains("./edp-plain.img"));
}

#[test]
fn plain_export_is_available_in_tui_review_flow() {
    let state = source("src/tui/provision/state.rs");
    let render = source("src/tui/render.rs");

    assert!(
        !state.contains("ProvisionPrepared::Plain(_) => return None"),
        "Plain export must not be blocked by TUI state"
    );
    assert!(render.contains("e 导出镜像"));
    assert!(!render.contains("e 导出镜像（新盘计划）"));
}
