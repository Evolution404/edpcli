use edpcli::cli_args::{parse_args, usage_text, Parsed};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn tui_is_an_explicit_v2_entrypoint_without_changing_bare_cli() {
    assert!(matches!(
        parse_args(&[]).expect("bare edpcli"),
        Parsed::List { .. }
    ));
    assert!(matches!(
        parse_args(&args(&["tui"])).expect("tui should parse"),
        Parsed::Tui
    ));
    assert!(usage_text().contains("tui"));
}

#[test]
fn tui_work_does_not_bump_the_package_version_before_release() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "2.0.1");
}

#[test]
fn tui_frontend_has_no_direct_platform_or_raw_write_dependency() {
    let source = include_str!("../src/tui/mod.rs");
    for forbidden in [
        "crate::platform::",
        "reopen_rdwr",
        "prepare_write",
        "write_sector",
        "atomic_write",
    ] {
        assert!(
            !source.contains(forbidden),
            "TUI frontend must use application/service layer, found {forbidden}"
        );
    }
}

#[test]
fn application_boundary_is_exported_for_cli_and_tui() {
    let lib = include_str!("../src/lib.rs");
    assert!(
        lib.contains("pub mod application;"),
        "application/service boundary must be a first-class module"
    );
    assert!(
        lib.contains("pub mod tui;"),
        "TUI must be exposed as a first-class frontend module"
    );
}
