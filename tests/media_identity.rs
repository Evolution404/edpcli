use edpcli::application::media_identity::{
    match_media_identity, serial_digest_evidence, ControlledLineageEvidence,
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityConfidence, IdentityObservation,
    MediaIdentitySnapshot, MediaRelationship, ProtocolIdentityEvidence, SerialQuality,
};
use edpcli::platform::NativeTransport;
use edpcli::provision::DiskProvisionKind;

fn hardware(serial: Option<&str>, vid: u16, pid: u16, sectors: u64) -> HardwareIdentityEvidence {
    let serial = serial_digest_evidence(serial);
    HardwareIdentityEvidence {
        vid: Some(vid),
        pid: Some(pid),
        serial_sha256: serial.sha256,
        serial_quality: serial.quality,
        vendor: Some("AIGO".into()),
        product: Some("U335".into()),
        revision: Some("1.00".into()),
        transport: Some(NativeTransport::Uas),
        total_sectors: Some(sectors),
        logical_sector_size: Some(512),
    }
}

fn snapshot(
    hardware: HardwareIdentityEvidence,
    device_id: Option<&str>,
    onlyid: Option<&str>,
    kind: DiskProvisionKind,
) -> MediaIdentitySnapshot {
    MediaIdentitySnapshot {
        hardware,
        protocol: ProtocolIdentityEvidence {
            device_id: device_id.map(str::to_string),
            onlyid: onlyid.map(str::to_string),
            provision_kind: Some(kind),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    }
}

#[test]
fn plain_canonical_identity_has_no_observed_protocol_ids() {
    let plain = MediaIdentitySnapshot::plain(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        DerivedProtocolEvidence {
            device_id_candidates: vec!["disk&ven_aigo&prod_u335".into()],
            legacy_derived_candidate: None,
        },
        IdentityObservation::default(),
    );
    assert_eq!(plain.protocol.device_id, None);
    assert_eq!(plain.protocol.onlyid, None);
    assert_eq!(
        plain.protocol.provision_kind,
        Some(DiskProvisionKind::Plain)
    );
    assert_eq!(
        plain.derived.device_id_candidates,
        vec!["disk&ven_aigo&prod_u335"]
    );
}

#[test]
fn same_usable_serial_plain_to_edp_is_physical_strong() {
    let plain = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        None,
        None,
        DiskProvisionKind::Plain,
    );
    let edp = snapshot(
        hardware(Some(" SERIAL-001 "), 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let matched = match_media_identity(&plain, &edp, None);
    assert_eq!(matched.relationship, MediaRelationship::SamePhysicalMedia);
    assert_eq!(matched.confidence, IdentityConfidence::PhysicalStrong);
}

#[test]
fn different_usable_serial_overrides_matching_protocol_identity() {
    let a = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let b = snapshot(
        hardware(Some("SERIAL-002"), 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let matched = match_media_identity(&a, &b, None);
    assert_eq!(matched.relationship, MediaRelationship::DifferentMedia);
}

#[test]
fn same_edp_instance_without_serial_is_b_level() {
    let a = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let b = a.clone();
    let matched = match_media_identity(&a, &b, None);
    assert_eq!(matched.relationship, MediaRelationship::SameEdpInstance);
    assert_eq!(matched.confidence, IdentityConfidence::EdpInstanceStrong);
}

#[test]
fn changed_onlyid_does_not_mean_different_physical_media() {
    let a = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let b = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    let matched = match_media_identity(&a, &b, None);
    assert_ne!(matched.relationship, MediaRelationship::DifferentMedia);
    assert_ne!(matched.relationship, MediaRelationship::SameEdpInstance);
}

#[test]
fn controlled_lineage_with_compatible_weak_hardware_is_c_level() {
    let a = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        None,
        None,
        DiskProvisionKind::Plain,
    );
    let b = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    let lineage = ControlledLineageEvidence { linked: true };
    let matched = match_media_identity(&a, &b, Some(&lineage));
    assert_eq!(
        matched.relationship,
        MediaRelationship::SameControlledLineage
    );
    assert_eq!(matched.confidence, IdentityConfidence::ControlledLineage);
}

#[test]
fn lineage_never_overrides_serial_conflict() {
    let a = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        None,
        None,
        DiskProvisionKind::Plain,
    );
    let b = snapshot(
        hardware(Some("SERIAL-002"), 0x1234, 0x5678, 1_000_000),
        Some("disk&ven_aigo&prod_u335"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    let lineage = ControlledLineageEvidence { linked: true };
    let matched = match_media_identity(&a, &b, Some(&lineage));
    assert_eq!(matched.relationship, MediaRelationship::DifferentMedia);
}

#[test]
fn same_model_and_capacity_without_serial_is_only_d_level() {
    let a = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        None,
        None,
        DiskProvisionKind::Plain,
    );
    let b = a.clone();
    let matched = match_media_identity(&a, &b, None);
    assert_eq!(matched.relationship, MediaRelationship::ModelOnlyMatch);
    assert_eq!(matched.confidence, IdentityConfidence::HardwareProfileMatch);
}

#[test]
fn suspicious_serial_never_creates_physical_strong() {
    let suspicious = serial_digest_evidence(Some("0000000000000000"));
    assert_eq!(suspicious.quality, SerialQuality::Suspicious);

    let a = snapshot(
        hardware(Some("0000000000000000"), 0x1234, 0x5678, 1_000_000),
        None,
        None,
        DiskProvisionKind::Plain,
    );
    let b = a.clone();
    let matched = match_media_identity(&a, &b, None);
    assert_ne!(matched.confidence, IdentityConfidence::PhysicalStrong);
}
