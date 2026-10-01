use super::*;

#[test]
fn password_intent_matrix_never_waits_for_a_source_password_when_rebuild_is_explicit() {
    use password_model::{
        decide_password_intent, PasswordIntent as Intent, SourcePasswordState as Source,
        TargetPasswordMode as Target,
    };

    let unverified = [Source::Verifying, Source::Failed, Source::Unknown];
    let verified = [Source::VerifiedDefault, Source::VerifiedUser];

    assert_eq!(
        decide_password_intent(
            Source::NotApplicable,
            Target::Explicit,
            "",
            crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
            false
        ),
        Intent::InitializeNew
    );
    assert_eq!(
        decide_password_intent(
            Source::NotApplicable,
            Target::Explicit,
            "",
            crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT,
            true
        ),
        Intent::InitializeNew
    );
    for source in unverified {
        assert_eq!(
            decide_password_intent(source, Target::Explicit, "old", "new", true),
            Intent::Rebuild,
            "{source:?}: an explicit rebuild target password does not need source verification"
        );
        assert_eq!(
            decide_password_intent(source, Target::Passthrough, "old", "", true),
            Intent::BlockedNeedsExplicitPassword,
            "{source:?}: destructive rebuild cannot silently reuse an unverified source password"
        );
    }
    for source in verified {
        assert_eq!(
            decide_password_intent(source, Target::Passthrough, "old", "", true),
            Intent::Rebuild,
            "{source:?}: verified source password may be reused for an explicitly authorized rebuild"
        );
        assert_eq!(
            decide_password_intent(source, Target::Explicit, "old", "new", true),
            Intent::Rebuild
        );
    }
    assert_eq!(
        decide_password_intent(Source::Verifying, Target::Passthrough, "old", "", false),
        Intent::Waiting
    );
    assert_eq!(
        decide_password_intent(
            Source::VerifiedUser,
            Target::Explicit,
            "same",
            "same",
            false
        ),
        Intent::Passthrough
    );
    assert_eq!(
        decide_password_intent(Source::VerifiedUser, Target::Explicit, "old", "new", false),
        Intent::Rewrap
    );
    assert_eq!(
        decide_password_intent(Source::Unknown, Target::Explicit, "old", "new", false),
        Intent::BlockedNeedsFormat
    );
}

fn plain_row(partition: Option<(u64, u64, Option<&str>)>) -> crate::disk_scan::Row {
    use crate::application::media_identity::{
        DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentityPin,
        MediaIdentitySnapshot,
    };
    use crate::partition_table::{
        PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
        PhysicalPartition,
    };

    let size = 64_000_000_000u64;
    let table = PartitionTableSnapshot {
        kind: PartitionTableKind::Mbr,
        partitions: partition
            .map(|(start_lba, sector_count, filesystem)| PhysicalPartition {
                index: 1,
                start_lba,
                sector_count,
                source: PartitionSource::Mbr {
                    partition_type: 0x06,
                    primary_slot: Some(1),
                },
                filesystem: filesystem.map(str::to_string),
            })
            .into_iter()
            .collect(),
        table_extents: vec![PartitionTableExtent {
            label: "MBR".into(),
            start_lba: 0,
            sector_count: 1,
        }],
        issues: Vec::new(),
    };
    let snapshot = MediaIdentitySnapshot::plain(
        HardwareIdentityEvidence {
            total_sectors: Some(size / crate::common::SECTOR as u64),
            logical_sector_size: Some(crate::common::SECTOR as u32),
            ..HardwareIdentityEvidence::default()
        },
        DerivedProtocolEvidence::default(),
        IdentityObservation::default(),
    );
    crate::disk_scan::Row {
        disk: 6,
        size,
        vid: "1234".into(),
        pid: "5678".into(),
        proto: "USB".into(),
        serial: None,
        hardware_model: None,
        device_id: Some("disk&ven_test&prod_plain".into()),
        identity_pin: Some(MediaIdentityPin::new(
            snapshot,
            &vec![0; crate::common::METADATA_IMAGE_LEN],
        )),
        onlyid: None,
        dept: Some("输电运检中心".into()),
        user: Some("测试用户".into()),
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
        partition_table: Some(table),
        partition_table_error: None,
        lce: None,
    }
}

fn mode0_plain_state(partition: Option<(u64, u64, Option<&str>)>) -> AppState {
    let mut state = AppState::new();
    state.replace_devices(vec![plain_row(partition)]);
    assert_eq!(state.begin_provision_for_selected_device(), Ok(6));
    assert!(state.provision_select_scheme_index(0));
    assert_eq!(state.provision_begin_selected(), ProvisionKind::Mode0);
    state.provision_enter_form_workspace();
    state
}

#[test]
fn plain_boot_preflight_auto_formats_required_rebuilds_and_preserves_exact_extent() {
    use crate::provision::PartitionRole;
    use preflight::{
        ProvisionFormatDisposition as FormatDisposition, ProvisionPreflightKind as Kind,
    };

    let cases = [
        ("no partition", None, false),
        (
            "normal 2048 start",
            Some((2_048, 20_417, Some("FAT16"))),
            false,
        ),
        ("one sector short", Some((63, 20_416, Some("FAT16"))), false),
        ("one sector late", Some((64, 20_416, Some("FAT16"))), false),
        ("exact exfat", Some((63, 20_417, Some("exFAT"))), false),
        ("exact unknown fs", Some((63, 20_417, None)), false),
        ("exact fat16", Some((63, 20_417, Some("FAT16"))), true),
    ];

    for (name, partition, exact) in cases {
        let mut state = mode0_plain_state(partition);
        let preflight = state.provision_preflight().unwrap();
        assert_eq!(
            preflight.partition(PartitionRole::Boot).unwrap().kind,
            if exact { Kind::Preserve } else { Kind::Rebuild },
            "{name}"
        );
        assert_eq!(
            preflight.format_disposition(PartitionRole::Boot),
            Some(if exact {
                FormatDisposition::Preserve
            } else {
                FormatDisposition::RequiredRebuild
            }),
            "{name}: boot format disposition"
        );
        for role in [PartitionRole::Share, PartitionRole::Encrypt] {
            assert_eq!(
                preflight.partition(role).unwrap().kind,
                Kind::Rebuild,
                "{name}: {role:?}"
            );
            assert_eq!(
                preflight.format_disposition(role),
                Some(FormatDisposition::RequiredRebuild),
                "{name}: {role:?} must auto-format"
            );
        }

        if exact {
            state.provision_mut().form.format_boot = true;
            let preflight = state.provision_preflight().unwrap();
            assert_eq!(
                preflight.partition(PartitionRole::Boot).unwrap().kind,
                Kind::Rebuild,
                "explicit format must override exact preservation"
            );
            assert_eq!(
                preflight.format_disposition(PartitionRole::Boot),
                Some(FormatDisposition::UserRequestedRebuild)
            );
        }
    }
}

#[test]
fn plain_mode0_request_enables_required_formats_without_manual_checkboxes() {
    let mut state = mode0_plain_state(None);
    let request = state.provision_request().unwrap();
    assert!(request.format.boot);
    assert!(request.format.share);
    assert!(request.format.encrypt);
    let form = &state.provision_mut().form;
    assert!(!form.format_boot);
    assert!(!form.format_share);
    assert!(!form.format_encrypt);
}
