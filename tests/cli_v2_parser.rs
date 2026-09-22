use edpcli::cli_args::{parse_args, BackupAction, Parsed};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn bare_edpcli_defaults_to_list() {
    assert!(matches!(
        parse_args(&[]).expect("bare edpcli should parse"),
        Parsed::List { .. }
    ));
}

#[test]
fn info_replaces_meta_and_accepts_disk_or_backup_file() {
    match parse_args(&args(&["info", "--disk", "4"])).expect("info disk") {
        Parsed::Info(opts) => assert_eq!(opts.disk, Some(4)),
        _ => panic!("expected info"),
    }

    match parse_args(&args(&["info", "backup.bin"])).expect("info backup") {
        Parsed::Info(opts) => assert_eq!(opts.backup.as_deref(), Some("backup.bin")),
        _ => panic!("expected info"),
    }
}

#[test]
fn apply_owns_dry_run_instead_of_run_command() {
    match parse_args(&args(&["apply", "--dry-run", "--disk", "4"])).expect("dry-run") {
        Parsed::Apply { opts, dry_run, .. } => {
            assert!(dry_run);
            assert_eq!(opts.disk, Some(4));
        }
        _ => panic!("expected apply"),
    }
}

#[test]
fn backup_v2_actions_parse_without_onlyid_or_index_ui() {
    assert!(matches!(
        parse_args(&args(&["backup", "create", "--disk", "4"])).expect("backup create"),
        Parsed::Backup {
            action: BackupAction::Create {
                disk: Some(4),
                deep: false
            },
            ..
        }
    ));
    assert!(matches!(
        parse_args(&args(&["backup", "restore", "2", "--disk", "4", "--yes"]))
            .expect("backup restore"),
        Parsed::Backup {
            action: BackupAction::Restore {
                ref target,
                disk: Some(4)
            },
            yes: true,
            ..
        } if target.as_deref() == Some("2")
    ));
    assert!(matches!(
        parse_args(&args(&["backup", "delete", "2,4,5", "--yes"])).expect("backup delete"),
        Parsed::Backup {
            action: BackupAction::Delete { ref targets },
            yes: true,
            ..
        } if targets == &["2,4,5".to_string()]
    ));
}

#[test]
fn inspect_requires_explicit_lba_flag() {
    match parse_args(&args(&[
        "inspect",
        "backup.bin",
        "--lba",
        "6,7,12",
        "--hex",
    ]))
    .expect("inspect")
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.backup.as_deref(), Some("backup.bin"));
            assert_eq!(opts.lbas, vec![6, 7, 12]);
            assert!(opts.hex);
        }
        _ => panic!("expected inspect"),
    }

    let error = parse_args(&args(&["inspect", "7"]))
        .err()
        .expect("bare LBA must be rejected");
    assert!(error.contains("--lba"), "{error}");
}

#[test]
fn inspect_backup_dir_alone_never_selects_a_backup_source() {
    match parse_args(&args(&[
        "inspect",
        "--backup-dir",
        "/tmp/backups",
        "--lba",
        "7",
    ]))
    .expect("inspect device source")
    {
        Parsed::Inspect(opts) => {
            assert!(opts.backup.is_none());
            assert_eq!(opts.backup_dir.as_deref(), Some("/tmp/backups"));
            assert_eq!(opts.lbas, vec![7]);
        }
        _ => panic!("expected inspect"),
    }
}

#[test]
fn removed_v1_grammar_returns_migration_errors_not_compatibility_paths() {
    for (argv, replacement) in [
        (&["run"][..], "apply --dry-run"),
        (&["meta"][..], "info"),
        (&["metainfo"][..], "info"),
        (&["restore"][..], "backup restore"),
        (&["backup", "rm"][..], "backup delete"),
    ] {
        let error = parse_args(&args(argv))
            .err()
            .expect("removed v1 grammar must not parse");
        assert!(error.contains(replacement), "{argv:?}: {error}");
    }
}

#[test]
fn v2_help_names_only_the_new_top_level_commands() {
    let help = edpcli::cli_args::usage_text();
    for command in [
        "list",
        "info",
        "apply",
        "backup",
        "inspect",
        "convert",
        "completion",
        "version",
    ] {
        assert!(help.contains(command), "missing {command}: {help}");
    }
    for removed in ["\n  run ", "\n  restore ", "\n  meta ", "\n  metainfo "] {
        assert!(
            !help.contains(removed),
            "legacy command leaked into help: {removed}\n{help}"
        );
    }
}

#[test]
fn deep_backup_is_explicit_and_rejects_duplicate_flags() {
    assert!(matches!(
        parse_args(&args(&["backup", "create", "--deep", "--disk", "5"])).unwrap(),
        Parsed::Backup {
            action: BackupAction::Create {
                disk: Some(5),
                deep: true
            },
            ..
        }
    ));
    assert!(parse_args(&args(&["backup", "create", "--deep", "--deep"])).is_err());
    assert!(parse_args(&args(&["backup", "create", "--deep=false"])).is_err());
}
