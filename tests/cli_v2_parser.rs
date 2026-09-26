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
        "--share-source-password",
        "ProofPass1!",
        "--share-target-password",
        "ProofPass1!",
        "--encrypt-source-password",
        "ProofPass1!",
        "--encrypt-target-password",
        "ProofPass1!",
    ];
    let mut plan = vec!["provision", "plan"];
    plan.extend(common);
    match parse_args(&args(&plan)).expect("provision plan") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.target.mode_number(), Some(1));
            assert_eq!(opts.disk, Some(4));
            assert_eq!(opts.volume_label, "启动区");
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

    let removed = parse_args(&args(&[
        "provision",
        "convert",
        "--disk",
        "4",
        "--write",
        "--yes",
    ]))
    .err()
    .expect("obsolete provision convert must stay removed");
    assert!(removed.contains("plan / image / write"), "{removed}");
    assert!(parse_args(&args(&["provision", "plan", "--onlyid", "1"])).is_err());
    assert!(parse_args(&args(&["provision", "convert", "--yes"])).is_err());
    assert!(parse_args(&args(&["provision", "plan", "--mode", "4"])).is_err());
}

#[test]
fn provision_write_accepts_backup_dir_but_plan_and_image_do_not() {
    let write = args(&[
        "provision",
        "write",
        "--disk",
        "4",
        "--target",
        "plain",
        "--partition",
        "2048:fill:exfat:DATA",
        "--backup-dir",
        "/tmp/edpcli-provision-backup",
        "--yes",
    ]);
    match parse_args(&write).expect("provision write with explicit backup dir") {
        Parsed::Provision(ProvisionAction::Write { backup_dir, .. }) => {
            assert_eq!(backup_dir.as_deref(), Some("/tmp/edpcli-provision-backup"));
        }
        _ => panic!("expected provision write"),
    }

    let plan = args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "plain",
        "--backup-dir",
        "/tmp/edpcli-provision-backup",
    ]);
    assert!(parse_args(&plan).is_err());

    let image = args(&[
        "provision",
        "image",
        "--disk",
        "4",
        "--target",
        "plain",
        "--backup-dir",
        "/tmp/edpcli-provision-backup",
        "--out",
        "/tmp/plain.img",
    ]);
    assert!(parse_args(&image).is_err());
}

#[test]
fn provision_plain_is_a_typed_target_and_never_mode4() {
    assert!(parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "plain"
    ]))
    .is_ok());

    assert!(parse_args(&args(&[
        "provision",
        "write",
        "--disk",
        "4",
        "--target",
        "plain",
        "--partition",
        "2048:64MiB:exfat:DATA",
        "--partition",
        "200000:fill:fat16:TOOLS",
        "--yes",
    ]))
    .is_ok());

    assert!(parse_args(&args(&[
        "provision",
        "image",
        "--disk",
        "4",
        "--target",
        "plain",
        "--out",
        "/tmp/plain.img",
    ]))
    .is_ok());

    assert!(parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode2"
    ]))
    .is_ok());

    for invalid in [
        vec!["provision", "plan", "--disk", "4", "--target", "mode4"],
        vec!["provision", "plan", "--disk", "4", "--mode", "4"],
        vec![
            "provision",
            "plan",
            "--disk",
            "4",
            "--target",
            "plain",
            "--mode",
            "1",
        ],
    ] {
        assert!(parse_args(&args(&invalid)).is_err(), "{invalid:?}");
    }
}

#[test]
fn provision_label_prefills_from_target_unless_cli_overrides_it() {
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
        "--share-source-password",
        "ProofPass1!",
        "--share-target-password",
        "ProofPass1!",
        "--encrypt-source-password",
        "ProofPass1!",
        "--encrypt-target-password",
        "ProofPass1!",
    ];
    match parse_args(&args(&base)).expect("default provision label") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert!(opts.label.is_empty());
            assert_eq!(opts.force_change_password, None);
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
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.force_change_password, Some(true))
        }
        _ => panic!("expected provision plan"),
    }

    let mut disabled = base.to_vec();
    disabled.push("--no-force-change-password");
    match parse_args(&args(&disabled)).expect("explicitly disable force-change policy") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.force_change_password, Some(false))
        }
        _ => panic!("expected provision plan"),
    }

    let mut conflicting = base.to_vec();
    conflicting.extend(["--force-change-password", "--no-force-change-password"]);
    assert!(parse_args(&args(&conflicting)).is_err());
}

#[test]
fn provision_parses_complete_pass_info_policy_overrides() {
    let base = ["provision", "plan", "--disk", "4", "--mode", "1"];
    let mut values = base.to_vec();
    values.extend([
        "--force-change-password",
        "--cancel-password-complexity-check",
        "--share-max-password-errors",
        "7",
        "--encrypt-max-password-errors",
        "9",
    ]);
    let Parsed::Provision(ProvisionAction::Plan(opts)) = parse_args(&args(&values)).unwrap() else {
        panic!("expected provision plan");
    };
    assert_eq!(opts.force_change_password, Some(true));
    assert_eq!(opts.cancel_password_complexity_check, Some(true));
    assert_eq!(opts.max_share_password_errors, Some(7));
    assert_eq!(opts.max_encrypt_password_errors, Some(9));

    let mut enforce = base.to_vec();
    enforce.push("--enforce-password-complexity-check");
    let Parsed::Provision(ProvisionAction::Plan(opts)) = parse_args(&args(&enforce)).unwrap()
    else {
        panic!("expected provision plan");
    };
    assert_eq!(opts.cancel_password_complexity_check, Some(false));

    let mut inline_bool = base.to_vec();
    inline_bool.push("--cancel-password-complexity-check=true");
    assert!(parse_args(&args(&inline_bool)).is_err());

    let mut invalid = base.to_vec();
    invalid.extend(["--share-max-password-errors", "256"]);
    assert!(parse_args(&args(&invalid)).is_err());
}

#[test]
fn provision_password_and_volume_label_have_product_defaults() {
    let args = args(&[
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
    ]);
    match parse_args(&args).expect("default password and volume label") {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert!(opts.share_source_password.is_empty());
            assert_eq!(opts.share_target_password, "0000aaaa");
            assert!(opts.encrypt_source_password.is_empty());
            assert_eq!(opts.encrypt_target_password, "0000aaaa");
            assert_eq!(opts.volume_label, "启动区");
            assert!(!opts.format_boot && !opts.format_share && !opts.format_encrypt);
            assert_eq!(
                opts.boot_fs,
                edpcli::provision::OfficialFilesystemFormat::Fat16
            );
            assert_eq!(
                opts.share_fs,
                edpcli::provision::OfficialFilesystemFormat::ExFat
            );
            assert_eq!(
                opts.encrypt_fs,
                edpcli::provision::OfficialFilesystemFormat::ExFat
            );
        }
        _ => panic!("expected provision plan"),
    }
}

#[test]
fn provision_actions_allow_capacity_and_identity_prefill() {
    let plan = parse_args(&args(&["provision", "plan", "--disk", "4", "--mode", "1"]))
        .expect("plan should allow source/system prefill");
    let Parsed::Provision(ProvisionAction::Plan(plan_opts)) = plan else {
        panic!("expected provision plan");
    };

    let image = parse_args(&args(&[
        "provision",
        "image",
        "--disk",
        "4",
        "--mode",
        "1",
        "--out",
        "/tmp/edp.img",
    ]))
    .expect("image should allow source/system prefill");
    let Parsed::Provision(ProvisionAction::Image {
        opts: image_opts, ..
    }) = image
    else {
        panic!("expected provision image");
    };

    let write = parse_args(&args(&[
        "provision",
        "write",
        "--disk",
        "4",
        "--mode",
        "1",
        "--share-start-sector",
        "63",
        "--encrypt-start-sector",
        "4020480",
        "--yes",
    ]))
    .expect("write should allow source/system prefill");
    let Parsed::Provision(ProvisionAction::Write {
        opts: write_opts,
        yes,
        ..
    }) = write
    else {
        panic!("expected provision write");
    };
    assert!(yes);
    assert_eq!(write_opts.share_start_lba, Some(63));
    assert_eq!(write_opts.encrypt_start_lba, Some(4_020_480));

    for opts in [&plan_opts, &image_opts, &write_opts] {
        assert_eq!(opts.share_mib, None);
        assert_eq!(opts.share_sectors, None);
        assert_eq!(opts.encrypt_mib, None);
        assert_eq!(opts.encrypt_sectors, None);
        assert!(opts.label_id.is_empty());
        assert!(opts.user.is_empty());
        assert!(opts.dept.is_empty());
        assert!(opts.label.is_empty());
    }
}

#[test]
fn provision_format_flags_and_independent_labels_parse() {
    let parsed = parse_args(&args(&[
        "provision",
        "write",
        "--disk",
        "4",
        "--mode",
        "0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
        "--format-boot",
        "--format-share",
        "--format-encrypt",
        "--boot-label",
        "启动",
        "--share-label",
        "交换",
        "--encrypt-label",
        "保密",
        "--boot-fs",
        "exfat",
        "--share-fs",
        "fat16",
        "--encrypt-fs",
        "exfat",
    ]))
    .unwrap();
    match parsed {
        Parsed::Provision(ProvisionAction::Write { opts, .. }) => {
            assert!(opts.format_boot && opts.format_share && opts.format_encrypt);
            assert_eq!(
                (
                    &opts.boot_label[..],
                    &opts.share_label[..],
                    &opts.encrypt_label[..]
                ),
                ("启动", "交换", "保密")
            );
            assert_eq!(
                opts.boot_fs,
                edpcli::provision::OfficialFilesystemFormat::ExFat
            );
            assert_eq!(
                opts.share_fs,
                edpcli::provision::OfficialFilesystemFormat::Fat16
            );
        }
        _ => panic!("expected provision write"),
    }
}

#[test]
fn provision_cli_rejects_filesystems_without_a_writer() {
    let base = [
        "provision",
        "plan",
        "--disk",
        "4",
        "--mode",
        "0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
    ];
    for filesystem in ["fat32", "ntfs"] {
        let mut command = base.to_vec();
        command.extend(["--boot-fs", filesystem]);
        assert!(parse_args(&args(&command)).is_err());
    }
}

#[test]
fn legacy_volume_label_is_a_fallback_for_each_partition_label() {
    let parsed = parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--mode",
        "0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
        "--volume-label",
        "共同卷标",
        "--share-label",
        "独立交换",
    ]))
    .unwrap();
    let Parsed::Provision(ProvisionAction::Plan(opts)) = parsed else {
        panic!("expected provision plan");
    };
    assert_eq!(opts.boot_label, "共同卷标");
    assert_eq!(opts.share_label, "独立交换");
    assert_eq!(opts.encrypt_label, "共同卷标");
}

#[test]
fn mode0_parser_leaves_boot_and_identity_for_target_prefill() {
    let parsed = parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--mode",
        "0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
    ]))
    .expect("mode0 defaults");
    match parsed {
        Parsed::Provision(ProvisionAction::Plan(opts)) => {
            assert_eq!(opts.boot_mib, None);
            assert_eq!(opts.boot_sectors, None);
            assert!(opts.label_id.is_empty());
        }
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
        (&["run"][..], "provision plan"),
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
    for command in ["list", "info", "backup", "inspect", "completion", "version"] {
        assert!(help.contains(command), "missing {command}: {help}");
    }
    for removed in [
        "\n  run ",
        "\n  restore ",
        "\n  meta ",
        "\n  metainfo ",
        "\nconvert ",
        "offline-convert",
    ] {
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
