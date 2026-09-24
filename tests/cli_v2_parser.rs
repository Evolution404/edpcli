use edpcli::cli_args::{parse_args, BackupAction, InspectMode, Parsed, ProvisionAction};

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
fn provision_parses_all_four_product_actions_and_rejects_ambiguous_flags() {
    let common = [
        "--disk",
        "4",
        "--mode",
        "1",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--label-id",
        "1402259934",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
        "--label",
        "江苏电力!SAFE6",
        "--password",
        "ProofPass1!",
    ];
    let mut plan = vec!["provision", "plan"];
    plan.extend(common);
    match parse_args(&args(&plan)).expect("provision plan") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.mode, 1);
            assert_eq!(opts.disk, Some(4));
            assert_eq!(opts.volume_label, "SAFE6");
        }
        _ => panic!("expected provision plan"),
    }

    let mut image = vec!["provision", "image"];
    image.extend(common);
    image.extend(["--out", "/tmp/edp.img"]);
    assert!(matches!(
        parse_args(&args(&image)).unwrap(),
        Parsed::Provision(ProvisionAction::Image { .. })
    ));

    let mut write = vec!["provision", "write"];
    write.extend(common);
    write.push("--yes");
    assert!(matches!(
        parse_args(&args(&write)).unwrap(),
        Parsed::Provision(ProvisionAction::Write { yes: true, .. })
    ));

    assert!(matches!(
        parse_args(&args(&[
            "provision",
            "convert",
            "--disk",
            "4",
            "--write",
            "--yes"
        ]))
        .unwrap(),
        Parsed::Provision(ProvisionAction::Convert {
            disk: Some(4),
            write: true,
            yes: true,
            ..
        })
    ));
    assert!(parse_args(&args(&["provision", "plan", "--onlyid", "1"])).is_err());
    assert!(parse_args(&args(&["provision", "convert", "--yes"])).is_err());
    assert!(parse_args(&args(&["provision", "plan", "--mode", "4"])).is_err());
}

#[test]
fn provision_label_defaults_to_jiangsu_safe6_but_cli_can_override_it() {
    let base = [
        "provision",
        "plan",
        "--disk",
        "4",
        "--mode",
        "1",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--label-id",
        "1402259934",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
        "--password",
        "ProofPass1!",
    ];
    match parse_args(&args(&base)).expect("default provision label") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.label, "江苏电力!SAFE6");
            assert!(!opts.force_change_password);
        }
        _ => panic!("expected provision plan"),
    }

    let mut custom = base.to_vec();
    custom.extend(["--label", "自定义标签!SAFE6"]);
    match parse_args(&args(&custom)).expect("custom provision label") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.label, "自定义标签!SAFE6")
        }
        _ => panic!("expected provision plan"),
    }

    let mut forced = base.to_vec();
    forced.push("--force-change-password");
    match parse_args(&args(&forced)).expect("force-change provision policy") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => assert!(opts.force_change_password),
        _ => panic!("expected provision plan"),
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
fn inspect_requires_mode_and_explicit_lba_flag() {
    match parse_args(&args(&["inspect", "meta", "backup.bin", "--lba", "6,7,12"])).expect("inspect")
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, InspectMode::Meta);
            assert_eq!(opts.backup.as_deref(), Some("backup.bin"));
            assert_eq!(opts.lbas, vec![6, 7, 12]);
        }
        _ => panic!("expected inspect"),
    }

    let error = parse_args(&args(&["inspect", "raw", "7"]))
        .err()
        .expect("bare LBA must be rejected");
    assert!(error.contains("--lba"), "{error}");
    assert!(parse_args(&args(&["inspect", "--lba", "7"])).is_err());
    assert!(parse_args(&args(&["inspect", "raw", "--lba", "7", "--hex"])).is_err());
}

#[test]
fn inspect_accepts_u64_ranges_and_count() {
    match parse_args(&args(&[
        "inspect",
        "decode",
        "--lba",
        "240250283-240250288",
    ]))
    .unwrap()
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, InspectMode::Decode);
            assert_eq!(opts.lbas.len(), 6);
            assert_eq!(opts.lbas[0], 240250283);
            assert_eq!(opts.lbas[5], 240250288);
        }
        _ => panic!("expected inspect decode"),
    }

    match parse_args(&args(&[
        "inspect",
        "raw",
        "--lba",
        "4294967296",
        "--count",
        "2",
    ]))
    .unwrap()
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, InspectMode::Raw);
            assert_eq!(opts.lbas, vec![4_294_967_296, 4_294_967_297]);
        }
        _ => panic!("expected inspect raw"),
    }

    assert!(parse_args(&args(&["inspect", "meta", "--lba", "5,6", "--count", "2"])).is_err());
    assert!(parse_args(&args(&["inspect", "meta", "--lba", "9-7"])).is_err());
}

#[test]
fn inspect_enforces_65536_sector_limit_and_u64_count_overflow() {
    let max = parse_args(&args(&["inspect", "raw", "--lba", "1000000-1065535"]))
        .expect("65536-sector range must be accepted");
    match max {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.lbas.len(), 65_536);
            assert_eq!(opts.lbas.first(), Some(&1_000_000));
            assert_eq!(opts.lbas.last(), Some(&1_065_535));
        }
        _ => panic!("expected inspect raw"),
    }

    let too_many = parse_args(&args(&["inspect", "raw", "--lba", "1000000-1065536"]))
        .err()
        .expect("65537-sector range must be rejected");
    assert!(too_many.contains("65536"), "{too_many}");

    let too_large_count = parse_args(&args(&["inspect", "raw", "--lba", "1", "--count", "65537"]))
        .err()
        .expect("count above 65536 must be rejected");
    assert!(too_large_count.contains("65536"), "{too_large_count}");

    let overflow = parse_args(&args(&[
        "inspect",
        "raw",
        "--lba",
        "18446744073709551615",
        "--count",
        "2",
    ]))
    .err()
    .expect("u64 count expansion overflow must be rejected");
    assert!(overflow.contains("溢出"), "{overflow}");
}

#[test]
fn inspect_backup_dir_alone_never_selects_a_backup_source() {
    match parse_args(&args(&[
        "inspect",
        "meta",
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
