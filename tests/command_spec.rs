use std::fs;
use std::path::Path;

use edpcli::command_spec;

fn source(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn command_schema_is_the_public_surface_catalog() {
    let names: Vec<_> = command_spec::top_level_specs()
        .iter()
        .map(|command| command.name)
        .collect();
    assert_eq!(
        names,
        [
            "list",
            "tui",
            "info",
            "backup",
            "provision",
            "inspect",
            "completion",
            "version",
            "help",
        ]
    );

    let provision = command_spec::command("provision").expect("provision spec");
    assert_eq!(provision.action_names(), vec!["plan", "image", "write"]);
    assert!(!provision.action_names().contains(&"convert"));

    for action in ["plan", "image", "write"] {
        let options = provision.option_names(Some(action));
        assert!(options.contains(&"--disk"), "{action} missing --disk");
        assert!(options.contains(&"--target"), "{action} missing --target");
        assert!(
            options.contains(&"--mode"),
            "{action} missing legacy --mode"
        );
        assert!(
            options.contains(&"--partition"),
            "{action} missing --partition"
        );
    }
    assert!(provision.option_names(Some("image")).contains(&"--out"));
    assert!(provision.option_names(Some("write")).contains(&"--yes"));
}

#[test]
fn help_and_completion_consume_command_schema() {
    let cli_args = source("src/cli_args.rs");
    let completion = source("src/completion.rs");
    assert!(cli_args.contains("command_spec::top_level_specs"));
    assert!(cli_args.contains("command_spec::command"));
    assert!(completion.contains("command_spec::top_level_specs"));
    assert!(completion.contains("command_spec::command"));

    let usage = edpcli::cli_args::usage_text();
    for spec in command_spec::top_level_specs() {
        assert!(usage.contains(spec.name), "help missing {}", spec.name);
    }

    for shell in [
        edpcli::completion::Shell::Zsh,
        edpcli::completion::Shell::Bash,
        edpcli::completion::Shell::Fish,
    ] {
        let rendered = edpcli::completion::script(shell);
        for spec in command_spec::top_level_specs() {
            assert!(
                rendered.contains(spec.name),
                "{shell:?} missing {}",
                spec.name
            );
        }
        assert!(
            !rendered.contains("convert"),
            "{shell:?} leaked removed convert"
        );
        let (target_syntax, partition_syntax) = match shell {
            edpcli::completion::Shell::Fish => ("-l target", "-l partition"),
            _ => ("--target", "--partition"),
        };
        assert!(
            rendered.contains(target_syntax),
            "{shell:?} missing target option"
        );
        assert!(
            rendered.contains(partition_syntax),
            "{shell:?} missing partition option"
        );
    }
}

#[test]
fn product_descriptions_no_longer_advertise_offline_convert() {
    assert!(!source("Cargo.toml").contains("免密转换"));
    assert!(!source("src/lib.rs").contains("免密转换"));
}
