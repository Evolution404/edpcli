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
fn bare_interactive_terminal_defaults_to_tui_without_breaking_non_tty_cli() {
    let source = include_str!("../src/cli.rs");
    assert!(
        source.contains("should_default_to_tui"),
        "CLI entrypoint must have an explicit bare-invocation TTY routing policy"
    );
    assert!(
        source.contains("io::stdin().is_terminal()") && source.contains("io::stdout().is_terminal()"),
        "bare edpcli must gate automatic TUI launch on stdin/stdout terminal capability"
    );
    assert!(
        source.contains("if should_default_to_tui"),
        "TTY routing must happen before the ordinary argv parser keeps bare CLI as list"
    );
}

#[test]
fn tui_startup_elevates_before_entering_the_alternate_screen() {
    let source = include_str!("../src/tui/mod.rs");
    assert!(
        source.contains("ensure_elevated_before_tui"),
        "TUI must acquire administrator rights before entering raw/alternate-screen mode"
    );
    assert!(
        source.contains("crate::elevate::ensure_elevated"),
        "TUI startup must reuse the shared platform elevation path"
    );
}

#[test]
fn tui_theme_uses_the_same_semantic_color_family_as_cli_output() {
    let source = include_str!("../src/tui/render.rs");
    for color in [
        "Color::Cyan",
        "Color::Green",
        "Color::Yellow",
        "Color::Red",
        "Color::Magenta",
    ] {
        assert!(
            source.contains(color),
            "TUI semantic palette is missing {color}"
        );
    }
}

#[test]
fn tui_remains_within_the_cli_v2_compatible_release_line() {
    let major = env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .expect("package version major")
        .parse::<u64>()
        .expect("numeric package version major");
    assert_eq!(
        major, 2,
        "a future major-version bump must explicitly revisit TUI/CLI compatibility contracts"
    );
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
