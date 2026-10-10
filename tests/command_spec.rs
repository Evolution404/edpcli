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
            "demo",
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
    assert_eq!(
        provision.action_names(),
        vec!["verify-source", "plan", "image", "write"]
    );
    assert!(!provision.action_names().contains(&"convert"));
    assert!(provision
        .option_names(Some("verify-source"))
        .contains(&"--backup"));

    for action in ["plan", "image", "write"] {
        let options = provision.option_names(Some(action));
        assert!(options.contains(&"--disk"), "{action} missing --disk");
        assert!(options.contains(&"--target"), "{action} missing --target");
        assert!(
            !options.contains(&"--mode"),
            "{action} exposes removed --mode"
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
    let help = source("src/cli_args/help.rs");
    let completion = source("src/completion.rs");
    assert!(help.contains("command_spec::top_level_specs"));
    assert!(help.contains("command_spec::command"));
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
    assert!(!source("Cargo.toml").contains("mode1 重制"));
    assert!(!source("src/lib.rs").contains("mode1 重制"));
}

#[test]
fn all_parser_flag_arms_match_catalog_names_and_value_shapes() {
    use std::collections::BTreeMap;
    use syn::visit::Visit;

    #[derive(Default)]
    struct Calls(bool);
    impl<'ast> Visit<'ast> for Calls {
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if let syn::Expr::Path(path) = &*call.func {
                self.0 |= path.path.is_ident("take_value");
            }
            syn::visit::visit_expr_call(self, call);
        }
    }
    #[derive(Default)]
    struct Flags(BTreeMap<String, bool>);
    impl<'ast> Visit<'ast> for Flags {
        fn visit_arm(&mut self, arm: &'ast syn::Arm) {
            fn names(pattern: &syn::Pat, out: &mut Vec<String>) {
                match pattern {
                    syn::Pat::Lit(literal) => {
                        if let syn::Lit::Str(value) = &literal.lit {
                            let value = value.value();
                            if value.starts_with("--") {
                                out.push(value);
                            }
                        }
                    }
                    syn::Pat::Or(pattern) => {
                        for case in &pattern.cases {
                            names(case, out);
                        }
                    }
                    _ => {}
                }
            }
            let mut found = Vec::new();
            names(&arm.pat, &mut found);
            if !found.is_empty() {
                let mut calls = Calls::default();
                calls.visit_expr(&arm.body);
                for name in found {
                    if let Some(previous) = self.0.insert(name.clone(), calls.0) {
                        assert_eq!(previous, calls.0, "inconsistent parser shape for {name}");
                    }
                }
            }
            syn::visit::visit_arm(self, arm);
        }
    }
    let mut parsed = Flags::default();
    for path in [
        "src/cli_args.rs",
        "src/cli_args/backup.rs",
        "src/cli_args/info.rs",
        "src/cli_args/inspect.rs",
        "src/cli_args/provision.rs",
    ] {
        parsed.visit_file(&syn::parse_file(&source(path)).unwrap());
    }
    // --version is a global alias for the version command. Elevation/resume
    // arguments are parsed in their own internal channel, outside this grammar.
    assert_eq!(parsed.0.remove("--version"), Some(false));
    let mut catalog = BTreeMap::new();
    for command in command_spec::top_level_specs() {
        for option in command
            .options
            .iter()
            .chain(command.actions.iter().flat_map(|action| action.options))
        {
            if let Some(previous) = catalog.insert(option.name.to_string(), option.takes_value) {
                assert_eq!(previous, option.takes_value);
            }
        }
    }
    assert_eq!(
        parsed.0, catalog,
        "public grammar and catalog must have identical names and argument shapes"
    );
}

#[test]
fn sensitive_options_are_present_in_grammar_and_absent_from_completion() {
    let provision = command_spec::command("provision").unwrap();
    let sensitive: Vec<_> = provision
        .options
        .iter()
        .filter(|option| option.sensitive)
        .collect();
    assert_eq!(sensitive.len(), 4);
    for shell in [
        edpcli::completion::Shell::Zsh,
        edpcli::completion::Shell::Bash,
        edpcli::completion::Shell::Fish,
    ] {
        let script = edpcli::completion::script(shell);
        for option in &sensitive {
            assert!(option.takes_value);
            assert!(!option.completion_visible);
            assert!(!script.contains(option.name.trim_start_matches("--")));
        }
    }
}
