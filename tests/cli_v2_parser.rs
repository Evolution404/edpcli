use edpcli::cli_args::{parse_args, BackupAction, InspectMode, Parsed, ProvisionAction};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn provision_verify_source_accepts_only_explicit_disk_and_verified_backup_path() {
    match parse_args(&args(&[
        "provision",
        "verify-source",
        "--disk",
        "4",
        "--backup",
        "/tmp/test.edpb",
    ]))
    .unwrap()
    {
        Parsed::Provision(ProvisionAction::VerifySource { disk, backup }) => {
            assert_eq!(disk, 4);
            assert_eq!(backup, "/tmp/test.edpb");
        }
        _ => panic!("must be read-only source verify"),
    }
    for bad in [
        vec!["provision", "verify-source", "--disk", "4"],
        vec!["provision", "verify-source", "--backup", "/tmp/test.edpb"],
        vec![
            "provision",
            "verify-source",
            "--disk",
            "4",
            "--backup",
            "/tmp/test.edpb",
            "--yes",
        ],
        vec![
            "provision",
            "verify-source",
            "--disk",
            "4",
            "--backup",
            "/tmp/test.edpb",
            "--out",
            "/dev/disk4",
        ],
    ] {
        assert!(parse_args(&args(&bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn demo_command_parses_default_scene_and_listing() {
    assert!(matches!(
        parse_args(&args(&["demo"])).unwrap(),
        Parsed::Demo {
            scene: None,
            list_scenes: false
        }
    ));
    assert!(matches!(
        parse_args(&args(&["demo", "--scene", "inspect-lba8"])).unwrap(),
        Parsed::Demo { scene: Some(scene), list_scenes: false } if scene == "inspect-lba8"
    ));
    assert!(matches!(
        parse_args(&args(&["demo", "--list-scenes"])).unwrap(),
        Parsed::Demo {
            scene: None,
            list_scenes: true
        }
    ));
    assert!(parse_args(&args(&["demo", "--scene"])).is_err());
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
        "--target",
        "mode1",
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
            assert_eq!(opts.boot_label, "启动区");
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
    assert!(parse_args(&args(&["provision", "plan", "--target", "mode4"])).is_err());
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
        vec!["provision", "plan", "--disk", "4", "--target", "mode4"],
        vec![
            "provision",
            "plan",
            "--disk",
            "4",
            "--target",
            "plain",
            "--target",
            "mode1",
        ],
    ] {
        assert!(parse_args(&args(&invalid)).is_err(), "{invalid:?}");
    }
}

#[test]
fn provision_plain_rejects_all_password_credentials() {
    for invalid in [
        "--share-source-password",
        "--share-target-password",
        "--encrypt-source-password",
        "--encrypt-target-password",
    ] {
        assert!(
            parse_args(&args(&[
                "provision",
                "plan",
                "--disk",
                "4",
                "--target",
                "plain",
                invalid,
                "AnyPass1!",
            ]))
            .is_err(),
            "Plain must reject password option {invalid}"
        );
    }
}

#[test]
fn provision_label_prefills_from_target_unless_cli_overrides_it() {
    let base = [
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode1",
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
fn provision_preserve_flag_is_explicit_and_separate_from_format_selection() {
    use edpcli::cli_args::{parse_args, Parsed, ProvisionAction};
    fn parsed(args: &[&str]) -> edpcli::cli_args::ProvisionNewOpts {
        let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match parse_args(&args).expect("official CLI grammar") {
            Parsed::Provision(ProvisionAction::Plan(opts)) => *opts,
            Parsed::Provision(ProvisionAction::Write { opts, .. }) => *opts,
            _ => panic!("expected official provisioning"),
        }
    }
    let base = ["provision", "plan", "--disk", "4", "--target", "mode0"];
    let default = parsed(&base);
    assert!(
        !default.preserve_unformatted,
        "legacy default remains rebuild"
    );
    assert!(!default.format_boot && !default.format_share && !default.format_encrypt);
    let explicit = parsed(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode0",
        "--preserve-unformatted",
        "--format-boot",
    ]);
    assert!(explicit.preserve_unformatted);
    assert!(explicit.format_boot && !explicit.format_share && !explicit.format_encrypt);
    let write = [
        "provision",
        "write",
        "--disk",
        "4",
        "--target",
        "mode1",
        "--preserve-unformatted",
    ];
    let args = write.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(matches!(
        parse_args(&args).expect("write preserve flag"),
        Parsed::Provision(ProvisionAction::Write { opts, .. }) if opts.preserve_unformatted
    ));
    for invalid in [
        vec![
            "provision",
            "plan",
            "--disk",
            "4",
            "--target",
            "plain",
            "--preserve-unformatted",
        ],
        vec![
            "provision",
            "plan",
            "--disk",
            "4",
            "--target",
            "mode0",
            "--preserve-unformatted=true",
        ],
        vec![
            "provision",
            "plan",
            "--disk",
            "4",
            "--target",
            "mode0",
            "--preserve-unformatted",
            "--preserve-unformatted",
        ],
    ] {
        let args = invalid.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(parse_args(&args).is_err(), "must reject: {invalid:?}");
    }
}
#[test]
fn provision_parses_complete_pass_info_policy_overrides() {
    let base = ["provision", "plan", "--disk", "4", "--target", "mode1"];
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
        "--target",
        "mode1",
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
            assert!(opts.share_target_password.is_empty()); // absent: retain on existing disk
            assert!(opts.encrypt_source_password.is_empty());
            assert!(opts.encrypt_target_password.is_empty()); // new disk gets OEM default when rebuilt
            assert_eq!(opts.boot_label, "启动区");
            assert!(!opts.format_boot && !opts.format_share && !opts.format_encrypt);
            assert_eq!(
                opts.boot_fs,
                edpcli::application::filesystem::FilesystemKind::Fat16
            );
            assert_eq!(
                opts.share_fs,
                edpcli::application::filesystem::FilesystemKind::ExFat
            );
            assert_eq!(
                opts.encrypt_fs,
                edpcli::application::filesystem::FilesystemKind::ExFat
            );
        }
        _ => panic!("expected provision plan"),
    }
}

#[test]
fn provision_actions_allow_capacity_and_identity_prefill() {
    let plan = parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode1",
    ]))
    .expect("plan should allow source/system prefill");
    let Parsed::Provision(ProvisionAction::Plan(plan_opts)) = plan else {
        panic!("expected provision plan");
    };

    let image = parse_args(&args(&[
        "provision",
        "image",
        "--disk",
        "4",
        "--target",
        "mode1",
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
        "--target",
        "mode1",
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
        "--target",
        "mode0",
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
        "fat32",
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
                edpcli::application::filesystem::FilesystemKind::ExFat
            );
            assert_eq!(
                opts.share_fs,
                edpcli::application::filesystem::FilesystemKind::Fat16
            );
            assert_eq!(
                opts.encrypt_fs,
                edpcli::application::filesystem::FilesystemKind::Fat32
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
        "--target",
        "mode0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
    ];
    for filesystem in ["fat12", "ntfs"] {
        let mut command = base.to_vec();
        command.extend(["--boot-fs", filesystem]);
        assert!(parse_args(&args(&command)).is_err());
    }
}

#[test]
fn partition_labels_are_independent_with_explicit_overrides() {
    let parsed = parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode0",
        "--share-mib",
        "64",
        "--encrypt-mib",
        "128",
        "--user",
        "USER06",
        "--dept",
        "江苏省电力有限公司",
        "--boot-label",
        "独立启动",
        "--share-label",
        "独立交换",
    ]))
    .unwrap();
    let Parsed::Provision(ProvisionAction::Plan(opts)) = parsed else {
        panic!("expected provision plan");
    };
    assert_eq!(opts.boot_label, "独立启动");
    assert_eq!(opts.share_label, "独立交换");
    assert_eq!(opts.encrypt_label, "保密区");
}

#[test]
fn mode0_parser_leaves_boot_and_identity_for_target_prefill() {
    let parsed = parse_args(&args(&[
        "provision",
        "plan",
        "--disk",
        "4",
        "--target",
        "mode0",
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
            action: BackupAction::Create { disk: Some(4) },
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
fn inspect_default_sweep_and_mixed_list_are_parsed_by_the_current_cli() {
    match parse_args(&args(&["inspect", "decode"])).unwrap() {
        Parsed::Inspect(opts) => {
            assert!(opts.lbas.is_empty(), "execution selects the protocol sweep")
        }
        _ => panic!("expected inspect decode"),
    }
    match parse_args(&args(&["inspect", "decode", "--lba", "7,12,20-22,7"])).unwrap() {
        Parsed::Inspect(opts) => assert_eq!(opts.lbas, [7, 12, 20, 21, 22]),
        _ => panic!("expected inspect decode"),
    }
    for rest in [&["--count", "2"][..], &["--lba", "0", "--count", "0"][..]] {
        let mut argv = args(&["inspect", "decode"]);
        argv.extend(args(rest));
        assert!(parse_args(&argv).is_err());
    }
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
fn development_provision_options_are_rejected_and_not_advertised() {
    let spec = edpcli::command_spec::command("provision").unwrap();
    for action in ["plan", "image", "write"] {
        for (option, value) in [
            ("--mode", "1"),
            ("--volume-label", "old label"),
            ("--password", "old password"),
        ] {
            let error = parse_args(&args(&[
                "provision",
                action,
                "--target",
                "mode1",
                option,
                value,
            ]))
            .err()
            .expect("removed development option must be rejected");
            assert!(error.contains(option), "{error}");
            assert!(!spec.option_names(Some(action)).contains(&option));
        }
    }
}

#[test]
fn controlled_password_input_is_catalogued_and_exclusive() {
    let args: Vec<String> = [
        "provision",
        "plan",
        "--target",
        "mode0",
        "--prompt-passwords",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let Parsed::Provision(ProvisionAction::Plan(opts)) = parse_args(&args).unwrap() else {
        panic!("wrong command")
    };
    assert!(opts.prompt_passwords);
    let mut conflicting = args;
    conflicting.extend(["--share-target-password".into(), "secret".into()]);
    assert!(parse_args(&conflicting).is_err());
    for name in [
        "--share-source-password",
        "--share-target-password",
        "--encrypt-source-password",
        "--encrypt-target-password",
    ] {
        let spec = edpcli::command_spec::option("provision", Some("plan"), name).unwrap();
        assert!(spec.sensitive && spec.takes_value && !spec.completion_visible);
    }
}

#[test]
fn provision_offline_native_image_parses_complete_4kn_geometry_and_fails_closed() {
    let good = [
        "provision",
        "image",
        "--target",
        "plain",
        "--sector-bytes",
        "4096",
        "--total-sectors",
        "40000",
        "--out",
        "virtual.img",
        "--partition",
        "2048:16MiB:fat16:TEST",
    ];
    let Parsed::Provision(ProvisionAction::NativeImage {
        out,
        total_sectors,
        sector_bytes,
        partitions,
    }) = parse_args(&args(&good)).unwrap()
    else {
        panic!("native image should remain a distinct offline-only CLI action")
    };
    assert_eq!(out, "virtual.img");
    assert_eq!(total_sectors, 40000);
    assert_eq!(sector_bytes, 4096);
    assert_eq!(partitions.len(), 1);
    assert_eq!(partitions[0].start_lba, 2048);
    assert_eq!(
        partitions[0].filesystem,
        edpcli::application::filesystem::FilesystemKind::Fat16
    );
    for bad in [
        vec!["--disk", "4"],
        vec!["--yes"],
        vec!["--target", "mode0"],
        vec!["--format-boot"],
        vec!["--encrypt-target-password", "test"],
    ] {
        let mut argv = good.to_vec();
        argv.extend(bad);
        assert!(
            parse_args(&args(&argv)).is_err(),
            "unsafe extra flags must fail: {argv:?}"
        );
    }
    let mut invalid_size = good.to_vec();
    invalid_size[5] = "2048";
    assert!(parse_args(&args(&invalid_size)).is_err());
    let mut missing_geometry = good.to_vec();
    missing_geometry.drain(4..6);
    assert!(parse_args(&args(&missing_geometry)).is_err());
    let mut missing_volume = good.to_vec();
    missing_volume.drain(6..8);
    assert!(parse_args(&args(&missing_volume)).is_err());
    let mut repeated = good.to_vec();
    repeated.extend(["--sector-bytes", "4096"]);
    assert!(parse_args(&args(&repeated)).is_err());
}
#[test]
fn provision_4kn_edp_virtual_demo_requires_explicit_opt_in_and_rejects_live_flags() {
    for mode in ["mode0", "mode1", "mode2", "mode3"] {
        for (algorithm, expected) in [
            ("sms4", edpcli::provision::FileKeyWrapMode::Sm4),
            ("aes", edpcli::provision::FileKeyWrapMode::A7f0),
            ("aes-cross", edpcli::provision::FileKeyWrapMode::Aes128Ecb),
        ] {
            let argv = [
                "provision",
                "image",
                "--target",
                mode,
                "--sector-bytes",
                "4096",
                "--total-sectors",
                "262144",
                "--out",
                "demo.img",
                "--synthetic-demo",
                "--algorithm",
                algorithm,
            ];
            let Parsed::Provision(ProvisionAction::NativeEdpDemoImage {
                out,
                total_sectors,
                mode: actual,
                algorithm: parsed_mode,
            }) = parse_args(&args(&argv)).unwrap()
            else {
                panic!("offline demo must have a separate no-hardware CLI action")
            };
            assert_eq!(out, "demo.img");
            assert_eq!(total_sectors, 262144);
            assert_eq!(parsed_mode, expected);
            assert_eq!(
                actual,
                edpcli::provision::ProvisionTarget::from_mode_number(
                    mode.strip_prefix("mode").unwrap().parse().unwrap()
                )
                .unwrap()
                .official_mode()
                .unwrap()
            );
        }
    }
    let base = [
        "provision",
        "image",
        "--target",
        "mode0",
        "--sector-bytes",
        "4096",
        "--total-sectors",
        "262144",
        "--out",
        "demo.img",
    ];
    assert!(parse_args(&args(&base)).is_err());
    for forbidden in [
        vec!["--disk", "4"],
        vec!["--yes"],
        vec!["--partition", "63:32MiB:fat16"],
        vec!["--encrypt-target-password", "PRIVATE"],
    ] {
        let mut argv = base.to_vec();
        argv.push("--synthetic-demo");
        argv.extend(forbidden);
        assert!(parse_args(&args(&argv)).is_err());
    }
    let mut bad_size = base.to_vec();
    bad_size[5] = "512";
    bad_size.push("--synthetic-demo");
    assert!(parse_args(&args(&bad_size)).is_err());
}

#[test]
fn source_bound_native_mode1_image_is_offline_only_and_forbids_any_disk_target() {
    let args_good = [
        "provision",
        "image",
        "--source-backup",
        "original-native.edpb",
        "--target",
        "mode1",
        "--out",
        "mode1-virtual.img",
    ];
    let Parsed::Provision(ProvisionAction::NativeMode1BackupImage { backup, out }) =
        parse_args(&args(&args_good)).unwrap()
    else {
        panic!("native Mode1 source conversion must have a dedicated offline CLI action");
    };
    assert_eq!(backup, "original-native.edpb");
    assert_eq!(out, "mode1-virtual.img");
    for forbidden in [
        vec!["--disk", "4"],
        vec!["--yes"],
        vec!["--sector-bytes", "4096"],
        vec!["--total-sectors", "262144"],
        vec!["--encrypt-target-password", "SECRET"],
        vec!["--format-encrypt"],
        vec!["--partition", "63:100MiB:exfat"],
        vec!["--synthetic-demo"],
    ] {
        let mut argv = args_good.to_vec();
        argv.extend(forbidden);
        assert!(
            parse_args(&args(&argv)).is_err(),
            "forbidden flags: {argv:?}"
        );
    }
    for mode in ["plain", "mode0", "mode2", "mode3", "mode4"] {
        let mut argv = args_good.to_vec();
        argv[5] = mode;
        assert!(parse_args(&args(&argv)).is_err(), "unexpected mode: {mode}");
    }
    let mut duplicate = args_good.to_vec();
    duplicate.extend(["--source-backup", "another.edpb"]);
    assert!(parse_args(&args(&duplicate)).is_err());
    let mut no_backup = args_good.to_vec();
    no_backup.drain(2..4);
    // Legacy image has its own parser; omitting the backup never turns it
    // into this isolated, source-authenticated native conversion action.
    assert!(matches!(
        parse_args(&args(&no_backup)),
        Ok(Parsed::Provision(ProvisionAction::Image { .. }))
    ));
}

#[test]
fn native_mode1_plan_routes_through_readonly_source_bound_action_only() {
    let good = [
        "provision",
        "plan",
        "--target",
        "mode1",
        "--disk",
        "4",
        "--source-backup",
        "snapshot.edpb",
    ];
    let Parsed::Provision(ProvisionAction::NativeMode1Plan { disk, backup }) =
        parse_args(&args(&good)).unwrap()
    else {
        panic!("native source-backed plan must be its own read-only action")
    };
    assert_eq!(disk, 4);
    assert_eq!(backup, "snapshot.edpb");
    for extra in [
        vec!["--yes"],
        vec!["--out", "export.img"],
        vec!["--format-share"],
        vec!["--share-fs", "exfat"],
        vec!["--encrypt-target-password", "SECRET"],
        vec!["--backup-dir", "unsafe"],
        vec!["--sector-bytes", "4096"],
    ] {
        let mut argv = good.to_vec();
        argv.extend(extra);
        assert!(parse_args(&args(&argv)).is_err(), "unsafe extras: {argv:?}");
    }
    for mode in ["plain", "mode0", "mode2", "mode3"] {
        let mut args_other = good.to_vec();
        args_other[3] = mode;
        assert!(parse_args(&args(&args_other)).is_err());
    }
    let mut no_disk = good.to_vec();
    no_disk.drain(4..6);
    assert!(parse_args(&args(&no_disk)).is_err());
    let mut duplicate = good.to_vec();
    duplicate.extend(["--source-backup", "other.edpb"]);
    assert!(parse_args(&args(&duplicate)).is_err());
}
