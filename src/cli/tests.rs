use super::*;
use crate::protocol::sectors::EdpfPartition;

#[test]
fn yes_flag_preserves_the_secret_input_channel() {
    struct SecretOnly;
    impl Prompter for SecretOnly {
        fn prompt_line(&mut self, _: &str) -> String {
            panic!("secret must never pass through ordinary input");
        }
        fn prompt_secret(&mut self, message: &str) -> crate::provision::SecretBytes {
            assert_eq!(message, "password");
            crate::provision::SecretBytes::new(b"test-secret")
        }
        fn confirm_yes(&mut self, _: &str) -> bool {
            false
        }
    }
    let mut prompt = AlwaysYes(SecretOnly);
    assert_eq!(prompt.prompt_secret("password").as_bytes(), b"test-secret");
}

#[test]
fn yes_flag_does_not_bypass_independent_key_domain_confirmation() {
    struct Reject;
    impl Prompter for Reject {
        fn prompt_line(&mut self, _message: &str) -> String {
            String::new()
        }

        fn prompt_secret(&mut self, _msg: &str) -> crate::provision::SecretBytes {
            panic!("unexpected secret prompt")
        }

        fn confirm_yes(&mut self, _message: &str) -> bool {
            false
        }
    }

    let mut prompt = AlwaysYes(Reject);
    assert!(prompt.confirm_write_yes("ordinary write"));
    assert!(!prompt.confirm_post_restore_format_yes("format restored partition"));
    assert!(!prompt.confirm_reinitialize_yes("replace encrypted key domain"));
}

#[test]
fn finish_preserves_business_error_exit_code() {
    assert_eq!(
        finish(Err(EdpCliError::new(EXIT_IO, "expected failure"))),
        EXIT_IO
    );
}

#[test]
fn target_plan_summary_reports_exact_geometry_and_data_fate() {
    use crate::filesystem::FilesystemKind;
    use crate::protocol::edpf::EdpPartitionType;
    use crate::provision::{
        OfficialPartitionMode, PartitionRole, PassthroughBasis, PasswordDisposition,
        RegionDisposition, SourcePasswordKnowledge, TargetPartitionGeometry, TargetPartitionPlan,
        TargetPasswordPolicy, TargetProvisionPlan,
    };

    let plan = TargetProvisionPlan {
        mode: OfficialPartitionMode::BootShareCombined,
        partitions: vec![
            TargetPartitionPlan {
                geometry: TargetPartitionGeometry {
                    role: PartitionRole::BootShareCombined,
                    partition_type: EdpPartitionType::Share,
                    start_lba: 63,
                    sector_count: 100,
                    physically_encrypted: false,
                    filesystem: Some(FilesystemKind::ExFat),
                },
                disposition: RegionDisposition::Rebuild,
                password_disposition: Some(PasswordDisposition::Rebuild),
                source_password_knowledge: None,
                target_password_policy: Some(TargetPasswordPolicy::InitializeNew),
                reason: "geometry changed".into(),
                preserved_record: None,
            },
            TargetPartitionPlan {
                geometry: TargetPartitionGeometry {
                    role: PartitionRole::Encrypt,
                    partition_type: EdpPartitionType::Encrypt,
                    start_lba: 1000,
                    sector_count: 200,
                    physically_encrypted: true,
                    filesystem: Some(FilesystemKind::ExFat),
                },
                disposition: RegionDisposition::PreserveOpaque,
                password_disposition: Some(PasswordDisposition::Passthrough(
                    PassthroughBasis::OpaqueCompatible,
                )),
                source_password_knowledge: Some(SourcePasswordKnowledge::Unknown),
                target_password_policy: Some(TargetPasswordPolicy::PreserveOpaque),
                reason: "exact source match".into(),
                preserved_record: None,
            },
        ],
        unallocated_sectors: 737,
    };

    let lines = target_plan_summary_lines(&plan);
    assert!(lines
        .iter()
        .any(|line| line.contains("start=63 end=162 sectors=100")));
    assert!(lines
        .iter()
        .any(|line| line.contains("Rebuild") && line.contains("K_new")));
    assert!(lines
        .iter()
        .any(|line| line.contains("start=1000 end=1199 sectors=200")));
    assert!(lines.iter().any(|line| line.contains("Passthrough")
        && line.contains("opaque-compatible")
        && line.contains("original key material")));
    assert!(lines
        .iter()
        .any(|line| line.contains("source=Unknown") && line.contains("password=passthrough")));
    assert!(lines
        .iter()
        .any(|line| line.contains("unallocated=737 sectors")));
}

#[test]
fn default_tui_requires_bare_interactive_terminal() {
    assert!(should_default_to_tui_for_test(true, true, true));
    assert!(!should_default_to_tui_for_test(false, true, true));
    assert!(!should_default_to_tui_for_test(true, false, true));
    assert!(!should_default_to_tui_for_test(true, true, false));
}

#[test]
fn parse_bare_and_subcommands() {
    assert!(matches!(
        parse_args(&[]).unwrap(),
        Parsed::List {
            backup_dir: None,
            include_virtual: false
        }
    ));
    assert!(matches!(
        parse_args(&["help".into()]).unwrap(),
        Parsed::Help { topic: None }
    ));
    assert!(matches!(
        parse_args(&["version".into()]).unwrap(),
        Parsed::Version { detailed: true }
    ));
    assert!(parse_args(&["apply".into()]).is_err());
    match parse_args(&["info".into(), "backup.bin".into()]).unwrap() {
        Parsed::Info(opts) => {
            assert_eq!(opts.backup.as_deref(), Some("backup.bin"));
        }
        _ => panic!("应解析为 info"),
    }
    assert!(
        parse_args(&["convert".into()]).is_err(),
        "removed top-level offline convert command must stay absent"
    );
    match parse_args(&[
        "inspect".into(),
        "meta".into(),
        "--lba".into(),
        "6,7,12".into(),
        "--backup-dir".into(),
        "/tmp/bak".into(),
    ])
    .unwrap()
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, crate::cli_args::InspectMode::Meta);
            assert_eq!(opts.lbas, vec![6, 7, 12]);
            assert_eq!(opts.backup_dir.as_deref(), Some("/tmp/bak"));
        }
        _ => panic!("应解析为 inspect meta"),
    }
    match parse_args(&[
        "inspect".into(),
        "raw".into(),
        "x.bin".into(),
        "--lba".into(),
        "9".into(),
        "--id".into(),
        "disk&ven_x&prod_y".into(),
    ])
    .unwrap()
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, crate::cli_args::InspectMode::Raw);
            assert_eq!(opts.backup.as_deref(), Some("x.bin"));
            assert_eq!(opts.lbas, vec![9]);
            assert_eq!(opts.device_id.as_deref(), Some("disk&ven_x&prod_y"));
        }
        _ => panic!("应解析为 inspect raw backup"),
    }
    match parse_args(&[
        "backup".into(),
        "prune".into(),
        "--keep".into(),
        "0".into(),
        "--yes".into(),
        "--backup-dir=/tmp/bak".into(),
    ])
    .unwrap()
    {
        Parsed::Backup {
            action: BackupAction::Prune,
            keep,
            yes,
            backup_dir,
        } => {
            assert_eq!(keep, 0);
            assert!(yes);
            assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
        }
        _ => panic!("应解析为 backup prune"),
    }
    match parse_args(&["backup".into(), "verify".into(), "2".into()]).unwrap() {
        Parsed::Backup {
            action: BackupAction::Verify { target },
            keep,
            yes,
            ..
        } => {
            assert_eq!(target.as_deref(), Some("2"));
            assert_eq!(keep, 2);
            assert!(!yes);
        }
        _ => panic!("应解析为 backup verify"),
    }
    match parse_args(&[
        "backup".into(),
        "delete".into(),
        "2,3,4".into(),
        "--yes".into(),
        "--backup-dir".into(),
        "/tmp/bak".into(),
    ])
    .unwrap()
    {
        Parsed::Backup {
            action: BackupAction::Delete { targets },
            yes,
            backup_dir,
            ..
        } => {
            assert_eq!(targets, vec!["2,3,4"]);
            assert!(yes);
            assert_eq!(backup_dir.as_deref(), Some("/tmp/bak"));
        }
        _ => panic!("应解析为 backup delete"),
    }
    assert!(matches!(
        parse_args(&["backup".into(), "create".into(), "--disk=4".into()]).unwrap(),
        Parsed::Backup {
            action: BackupAction::Create { disk: Some(4) },
            ..
        }
    ));
}

#[test]
fn elevation_reexec_pins_explicit_disk_to_platform_selector() {
    let selector = crate::platform::disk_selector_value(6);

    let mut split = vec![
        "backup".to_string(),
        "--disk".to_string(),
        "6".to_string(),
        "--yes".to_string(),
    ];
    DeviceSelector::new(Some(6)).pin_argv(&mut split, 6);
    assert_eq!(split[2], selector);
    assert_eq!(
        split.iter().filter(|arg| arg.as_str() == "--disk").count(),
        1
    );

    let mut inline = vec![
        "backup".to_string(),
        "restore".to_string(),
        "--disk=6".to_string(),
    ];
    DeviceSelector::new(Some(6)).pin_argv(&mut inline, 6);
    assert_eq!(inline[2], format!("--disk={selector}"));

    let mut automatic = vec![
        "backup".to_string(),
        "restore".to_string(),
        "--yes".to_string(),
    ];
    DeviceSelector::new(None).pin_argv(&mut automatic, 6);
    assert_eq!(
        automatic,
        vec!["backup", "restore", "--yes", "--disk", &selector]
    );
}

#[test]
fn list_requests_elevation_only_for_permission_denied_rows() {
    let base = Row {
        disk: 6,
        size: 62_914_560_000,
        vid: "0dd8".into(),
        pid: "2005".into(),
        proto: "USB".into(),
        serial: None,
        hardware_model: None,
        device_id: None,
        identity_pin: None,
        onlyid: None,
        dept: None,
        user: None,
        label: None,
        force_change_password: None,
        cancel_password_complexity_check: None,
        max_share_password_errors: None,
        max_encrypt_password_errors: None,
        n_baks: 0,
        n_possible_baks: 0,
        denied: false,
        probe_error: None,
        provision_kind: crate::provision::DiskProvisionKind::Plain,
        partitions: None,
        partition_table: None,
        partition_table_error: None,
        lce: None,
    };
    assert!(!list_needs_elevation(
        std::slice::from_ref(&base),
        false,
        false
    ));

    let denied = Row {
        denied: true,
        ..base
    };
    assert!(list_needs_elevation(
        std::slice::from_ref(&denied),
        false,
        false
    ));
    assert!(!list_needs_elevation(
        std::slice::from_ref(&denied),
        true,
        false
    ));
    assert!(!list_needs_elevation(&[denied], false, true));
}

#[test]
fn parse_usage_errors() {
    assert!(parse_args(&["bogus".into()]).is_err());
    assert!(parse_args(&["run".into()]).is_err());
    assert!(parse_args(&["restore".into()]).is_err());
    assert!(parse_args(&["meta".into()]).is_err());
    assert!(matches!(
        parse_args(&["backup".into()]).unwrap(),
        Parsed::Backup {
            action: BackupAction::List,
            ..
        }
    ));
    assert!(parse_args(&["backup".into(), "rm".into()]).is_err());
    assert!(parse_args(&["backup".into(), "delete".into(), "--yes".into(),]).is_err());
    assert!(parse_args(&[
        "backup".into(),
        "prune".into(),
        "--keep".into(),
        "-1".into()
    ])
    .is_err());
    assert!(parse_args(&[
        "backup".into(),
        "verify".into(),
        "a.bin".into(),
        "b.bin".into()
    ])
    .is_err());
    assert!(parse_args(&[
        "backup".into(),
        "verify".into(),
        "a.bin".into(),
        "--onlyid".into(),
        "1".into(),
    ])
    .is_err());
    assert!(parse_args(&[
        "backup".into(),
        "list".into(),
        "--onlyid".into(),
        "abc".into(),
    ])
    .is_err());
    assert!(parse_args(&["backup".into(), "list".into(), "--yes".into()]).is_err());
    assert!(parse_args(&[
        "inspect".into(),
        "raw".into(),
        "--disk".into(),
        "4".into(),
        "x.bin".into(),
    ])
    .is_err());
    assert!(parse_args(&[
        "inspect".into(),
        "meta".into(),
        "--onlyid".into(),
        "1402259934".into(),
    ])
    .is_err());
    assert!(parse_args(&[
        "inspect".into(),
        "raw".into(),
        "--lba".into(),
        "7".into(),
        "--hex".into(),
    ])
    .is_err());
    assert!(parse_args(&[
        "backup".into(),
        "verify".into(),
        "--index".into(),
        "1".into(),
    ])
    .is_err());
    // 哨兵旗标被剥离
    assert!(matches!(
        parse_args(&[
            "backup".into(),
            "restore".into(),
            "--disk".into(),
            "6".into(),
            "--_elevated".into()
        ])
        .unwrap(),
        Parsed::Backup { .. }
    ));
}

#[test]
fn include_virtual_is_explicit_and_strictly_scoped() {
    match parse_args(&["list".into(), "--include-virtual".into()]).unwrap() {
        Parsed::List {
            include_virtual: true,
            ..
        } => {}
        _ => panic!("expected virtual device list opt-in"),
    }
    match parse_args(&["tui".into(), "--include-virtual".into()]).unwrap() {
        Parsed::Tui {
            include_virtual: true,
        } => {}
        _ => panic!("expected virtual device TUI opt-in"),
    }
    assert!(parse_args(&[
        "list".into(),
        "--include-virtual".into(),
        "--include-virtual".into()
    ])
    .is_err());
    assert!(parse_args(&[
        "tui".into(),
        "--include-virtual".into(),
        "--include-virtual".into()
    ])
    .is_err());
    assert!(parse_args(&["list".into(), "--include-virtual=true".into()]).is_err());
    assert!(parse_args(&["tui".into(), "--include-virtual=true".into()]).is_err());
    assert!(parse_args(&[
        "backup".into(),
        "restore".into(),
        "--include-virtual".into()
    ])
    .is_err());
}

#[test]
fn boolean_flags_reject_inline_values() {
    for argv in [
        vec!["backup", "prune", "--yes=0"],
        vec!["backup", "delete", "1", "--yes=no"],
        vec!["backup", "restore", "backup.bin", "--yes=false"],
    ] {
        let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
        let err = parse_args(&args).err().expect("布尔旗标带值必须报错");
        assert!(err.contains("不接受参数值"), "{err}");
    }
}

#[test]
fn boolean_flags_reject_duplicates() {
    for argv in [
        vec!["backup", "prune", "--yes", "--yes"],
        vec!["backup", "restore", "backup.bin", "--yes", "--yes"],
    ] {
        let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
        let err = parse_args(&args).err().expect("布尔旗标重复必须报错");
        assert!(err.contains("重复"), "{err}");
    }
}

#[test]
fn inspect_accepts_arbitrary_u64_lbas_ranges_and_count() {
    match parse_args(&[
        "inspect".into(),
        "decode".into(),
        "--lba".into(),
        "240250283-240250288".into(),
    ])
    .unwrap()
    {
        Parsed::Inspect(opts) => {
            assert_eq!(opts.mode, crate::cli_args::InspectMode::Decode);
            assert_eq!(
                opts.lbas,
                vec![240250283, 240250284, 240250285, 240250286, 240250287, 240250288]
            );
        }
        _ => panic!("应解析为 inspect decode"),
    }

    match parse_args(&[
        "inspect".into(),
        "raw".into(),
        "--lba".into(),
        "4294967296".into(),
        "--count".into(),
        "2".into(),
    ])
    .unwrap()
    {
        Parsed::Inspect(opts) => assert_eq!(opts.lbas, vec![4_294_967_296, 4_294_967_297]),
        _ => panic!("应解析为 inspect raw"),
    }

    assert!(parse_args(&["inspect".into(), "--lba".into(), "7".into()]).is_err());
    assert!(parse_args(&[
        "inspect".into(),
        "meta".into(),
        "--lba".into(),
        "12-7".into(),
    ])
    .is_err());
}

#[test]
fn single_value_flags_reject_duplicates() {
    for argv in [
        vec!["backup", "restore", "--disk=4", "--disk=6"],
        vec!["backup", "create", "--disk=4", "--disk=6"],
        vec!["backup", "prune", "--keep", "1", "--keep", "2"],
        vec!["info", "--disk", "4", "--disk", "6"],
        vec!["inspect", "meta", "--lba", "1", "--lba", "2"],
    ] {
        let args: Vec<String> = argv.into_iter().map(str::to_string).collect();
        let err = parse_args(&args).err().expect("单值旗标重复必须报错");
        assert!(err.contains("重复"), "{err}");
    }
}

#[test]
fn disk_table_rendering() {
    let parts = vec![
        EdpfPartition {
            ptype: 1,
            active: 1,
            enc: 0,
            start_lba: 32,
            size_bytes: 16_384,
        },
        EdpfPartition {
            ptype: 2,
            active: 1,
            enc: 1,
            start_lba: 63,
            size_bytes: 59_750_819_680,
        },
        EdpfPartition {
            ptype: 4,
            active: 0,
            enc: 1,
            start_lba: 116_707_328,
            size_bytes: 3_143_761_920,
        },
    ];
    let mut rows = vec![
        Row {
            disk: 4,
            size: 64_000_000_000,
            vid: "0951".into(),
            pid: "1666".into(),
            proto: "USB".into(),
            serial: None,
            hardware_model: None,
            device_id: None,
            identity_pin: None,
            onlyid: None,
            dept: None,
            user: None,
            label: None,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
            n_baks: 0,
            n_possible_baks: 0,
            denied: false,
            probe_error: None,
            provision_kind: crate::provision::DiskProvisionKind::Plain,
            partitions: None,
            partition_table: None,
            partition_table_error: None,
            lce: None,
        },
        Row {
            disk: 6,
            size: 62_914_560_000,
            vid: "0dd8".into(),
            pid: "2005".into(),
            proto: "USB".into(),
            serial: None,
            hardware_model: None,
            device_id: Some("disk&ven_netac&prod_onlydisk".into()),
            identity_pin: None,
            onlyid: Some("1402259934".into()),
            dept: Some("国网江苏省电力有限公司泰州供电公司".into()),
            user: Some("宋旭琳".into()),
            label: Some("江苏电力!SAFE6".into()),
            force_change_password: Some(false),
            cancel_password_complexity_check: Some(false),
            max_share_password_errors: Some(255),
            max_encrypt_password_errors: Some(255),
            n_baks: 3,
            n_possible_baks: 2,
            denied: false,
            probe_error: None,
            provision_kind: crate::provision::DiskProvisionKind::Mode0,
            partitions: Some(parts),
            partition_table: None,
            partition_table_error: None,
            lce: None,
        },
        Row {
            disk: 7,
            size: 500_107_862_016,
            vid: "xxxx".into(),
            pid: "xxxx".into(),
            proto: "Thunderbolt".into(),
            serial: None,
            hardware_model: None,
            device_id: None,
            identity_pin: None,
            onlyid: None,
            dept: None,
            user: None,
            label: None,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
            n_baks: 0,
            n_possible_baks: 0,
            denied: false,
            probe_error: None,
            provision_kind: crate::provision::DiskProvisionKind::Plain,
            partitions: None,
            partition_table: None,
            partition_table_error: None,
            lce: None,
        },
    ];
    for row in rows.iter_mut().filter(|row| row.proto == "USB") {
        use crate::application::media_identity::{
            DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation,
            MediaIdentityPin, MediaIdentitySnapshot, ProtocolIdentityEvidence,
        };
        let hardware = HardwareIdentityEvidence {
            total_sectors: Some(row.size / crate::common::SECTOR as u64),
            logical_sector_size: Some(crate::common::SECTOR as u32),
            ..HardwareIdentityEvidence::default()
        };
        let snapshot = if row.provision_kind == crate::provision::DiskProvisionKind::Plain {
            MediaIdentitySnapshot::plain(
                hardware,
                DerivedProtocolEvidence::default(),
                IdentityObservation::default(),
            )
        } else {
            MediaIdentitySnapshot {
                hardware,
                protocol: ProtocolIdentityEvidence {
                    device_id: row.device_id.clone(),
                    onlyid: row.onlyid.clone(),
                    provision_kind: Some(row.provision_kind),
                    lba4_identity_digest: None,
                },
                derived: DerivedProtocolEvidence::default(),
                observation: IdentityObservation::default(),
            }
        };
        row.identity_pin = Some(MediaIdentityPin::new(
            snapshot,
            &vec![0; crate::common::METADATA_IMAGE_LEN],
        ));
    }
    let out = print_disk_table(&rows);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "外接盘 3 个:");
    let disk4_line = lines.iter().find(|l| l.contains("disk4")).unwrap();
    assert!(disk4_line.contains("普通盘"));
    let disk6_line = lines.iter().find(|l| l.contains("disk6")).unwrap();
    assert!(
        disk6_line.contains("mode0 · 缺省三分区")
            && disk6_line.contains("宋旭琳")
            && disk6_line.contains("泰州供电公司"),
        "{}",
        disk6_line
    );
    // EDPF 明细行: 类型 + 大小 + LBA 范围
    let edpf = lines.iter().find(|l| l.contains("EDPF")).unwrap();
    let share_capacity = crate::common::fmt_capacity((116_700_881 - 63 + 1) * 512);
    let encrypt_capacity = crate::common::fmt_capacity((122_847_487 - 116_707_328 + 1) * 512);
    let boot_capacity = crate::common::fmt_capacity((63 - 32 + 1) * 512);
    assert!(
        edpf.contains(&format!("Share {share_capacity} (LBA 63~116,700,881)")),
        "{}",
        edpf
    );
    assert!(
        edpf.contains(&format!(
            "Encrypt {encrypt_capacity} (LBA 116,707,328~122,847,487)"
        )),
        "{}",
        edpf
    );
    assert!(
        edpf.contains(&format!("Boot {boot_capacity} (LBA 32~63)")),
        "{}",
        edpf
    );
    let meta = lines.iter().find(|l| l.contains("onlyid")).unwrap();
    assert!(
        meta.contains("onlyid=1402259934")
            && meta.contains("备份 3 份")
            && meta.contains("可能相关 2 份")
    );
    let disk7_line = lines.iter().find(|l| l.contains("disk7")).unwrap();
    assert!(disk7_line.contains("非 USB") && disk7_line.contains("不支持"));
    assert_eq!(print_disk_table(&[]).trim(), "未检测到外接盘。");
}

#[test]
fn disk_menu_is_numbered() {
    use crate::platform::ExtDisk;
    let disks = vec![
        ExtDisk {
            n: 4,
            size: 64_000_000_000,
            vid: "0951".into(),
            pid: "1666".into(),
            proto: "USB".into(),
        },
        ExtDisk {
            n: 6,
            size: 62_914_560_000,
            vid: "0dd8".into(),
            pid: "2005".into(),
            proto: "USB".into(),
        },
    ];
    let m = disk_menu_str(&disks);
    assert!(
        m.contains("编号") && m.contains("设备") && m.contains("VID:PID"),
        "{}",
        m
    );
    assert!(
        m.lines()
            .any(|line| line.contains("1") && line.contains("disk4")),
        "{}",
        m
    );
    assert!(
        m.lines()
            .any(|line| line.contains("2") && line.contains("disk6")),
        "{}",
        m
    );
    assert!(
        m.contains("disk4")
            && m.contains(&crate::common::fmt_capacity(64_000_000_000))
            && m.contains("0951:1666")
    );
}
