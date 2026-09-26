use std::io;
use std::time::Duration;

use edpcli::application::media_identity::{
    match_media_identity, serial_digest_evidence, ControlledLineageEvidence,
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityConfidence, IdentityObservation,
    MediaIdentityPin, MediaIdentityPinConflict, MediaIdentityResumePin, MediaIdentitySnapshot,
    MediaRelationship, ProtocolIdentityEvidence, RestoreAuthorizationDecision,
    RestoreAuthorizationPolicy, RestoreGeometryRequirements, RestoreRejection, SerialQuality,
};
use edpcli::application::media_identity_observer::observe_media_identity_readonly;
use edpcli::diskio::SectorDev;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::provision::DiskProvisionKind;
use edpcli::sysinfo::CmdRunner;

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

fn authorize(
    backup: &MediaIdentitySnapshot,
    target: &MediaIdentitySnapshot,
) -> RestoreAuthorizationDecision {
    let matched = match_media_identity(backup, target, None);
    RestoreAuthorizationPolicy::evaluate(
        backup,
        target,
        &matched,
        RestoreGeometryRequirements {
            total_sectors: 1_000_000,
            logical_sector_size: 512,
        },
        None,
    )
}

#[test]
fn restore_authorization_rejects_same_protocol_clone_with_different_usable_serial() {
    let source = snapshot(
        hardware(Some("SOURCE-USB-SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let clone = snapshot(
        hardware(Some("CLONED-USB-SERIAL-002"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    assert_eq!(
        authorize(&source, &clone),
        RestoreAuthorizationDecision::Reject(RestoreRejection::UsableSerialMismatch)
    );
}

#[test]
fn restore_authorization_rejects_vid_pid_and_geometry_conflicts() {
    let source = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let wrong_vid = snapshot(
        hardware(Some("SERIAL-001"), 0x9999, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let wrong_pid = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x9999, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let wrong_size = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_001),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    assert_eq!(
        authorize(&source, &wrong_vid),
        RestoreAuthorizationDecision::Reject(RestoreRejection::VidPidMismatch)
    );
    assert_eq!(
        authorize(&source, &wrong_pid),
        RestoreAuthorizationDecision::Reject(RestoreRejection::VidPidMismatch)
    );
    assert_eq!(
        authorize(&source, &wrong_size),
        RestoreAuthorizationDecision::Reject(RestoreRejection::GeometryMismatch)
    );
}

#[test]
fn restore_authorization_requires_usable_serial_even_for_same_edp_instance() {
    let source = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    assert_eq!(
        authorize(&source, &source),
        RestoreAuthorizationDecision::Reject(RestoreRejection::WeakHardwareBinding)
    );
}

#[test]
fn restore_authorization_keeps_physical_and_protocol_identity_separate() {
    let source = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let reprovisioned = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    assert_eq!(
        authorize(&source, &reprovisioned),
        RestoreAuthorizationDecision::Authorized
    );
    let weak = snapshot(
        hardware(None, 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("99"),
        DiskProvisionKind::Mode2,
    );
    let lineage = ControlledLineageEvidence { linked: true };
    let matched = match_media_identity(&source, &weak, Some(&lineage));
    assert_eq!(
        RestoreAuthorizationPolicy::evaluate(
            &source,
            &weak,
            &matched,
            RestoreGeometryRequirements {
                total_sectors: 1_000_000,
                logical_sector_size: 512
            },
            Some(&lineage)
        ),
        RestoreAuthorizationDecision::Reject(RestoreRejection::WeakHardwareBinding)
    );
}

#[test]
fn provision_pin_hides_raw_serial_and_rejects_reopen_clone() {
    let source = snapshot(
        hardware(Some("SOURCE-USB-SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let clone = snapshot(
        hardware(Some("CLONED-USB-SERIAL-002"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let pin = MediaIdentityPin::new(source.clone(), &[0x42; 13 * 512]);
    assert!(!format!("{pin:?}").contains("SOURCE-USB-SERIAL-001"));
    assert_eq!(
        pin.verify(&clone, &[0x42; 13 * 512]),
        Err(MediaIdentityPinConflict::SerialChangedOrLost)
    );
    assert_eq!(
        pin.verify(&source, &[0x41; 13 * 512]),
        Err(MediaIdentityPinConflict::ProtocolImageChanged)
    );
}

#[test]
fn elevation_pin_contains_only_digest_and_rejects_reopened_clone() {
    let source = snapshot(
        hardware(Some("RAW-USB-SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let clone = snapshot(
        hardware(Some("CLONED-USB-SERIAL-002"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    let pin =
        MediaIdentityResumePin::from_pin(&MediaIdentityPin::new(source.clone(), &[0; 13 * 512]));
    let argv_value = serde_json::to_string(&pin).unwrap();
    assert!(!argv_value.contains("RAW-USB-SERIAL-001"));
    assert!(pin.validate().is_ok());
    assert_eq!(
        pin.verify(&clone, &[0; 13 * 512]),
        Err(MediaIdentityPinConflict::SerialChangedOrLost)
    );
}

struct ObservationRunner;

impl CmdRunner for ObservationRunner {
    fn check_output(&self, _cmd: &[&str], _timeout: Duration) -> io::Result<String> {
        Err(io::Error::other(
            "platform geometry unavailable in unit fixture",
        ))
    }

    fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
        Some(HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: NativeTransport::Uas,
            inquiry: Some(InquiryInfo {
                vendor: "AIGO".into(),
                product: "U335".into(),
                revision: "PMAP".into(),
            }),
        })
    }

    fn hardware_serial(&self, _disk: u32) -> Option<String> {
        Some("RAW-SERIAL-MUST-NOT-ESCAPE".into())
    }
}

struct ReadOnlyAuditDev {
    image: Vec<u8>,
    reads: usize,
    writes: usize,
    reopens: usize,
}

impl ReadOnlyAuditDev {
    fn plain() -> Self {
        Self {
            image: vec![0u8; 13 * 512],
            reads: 0,
            writes: 0,
            reopens: 0,
        }
    }
}

impl SectorDev for ReadOnlyAuditDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.reads += 1;
        let start = lba as usize * 512;
        let end = start + 512;
        self.image
            .get(start..end)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| io::Error::other("out of fixture range"))
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
        self.writes += 1;
        Err(io::Error::other("identity observation attempted a write"))
    }

    fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
        self.reopens += 1;
        Err(io::Error::other(
            "identity observation attempted a read-write reopen",
        ))
    }
}

#[test]
fn readonly_observation_collects_plain_identity_without_any_write_transition() {
    let runner = ObservationRunner;
    let mut dev = ReadOnlyAuditDev::plain();

    let observed =
        observe_media_identity_readonly(&runner, 6, &mut dev).expect("read-only observation");

    assert_eq!(
        dev.reads, 13,
        "identity observation should read LBA0-12 once"
    );
    assert_eq!(
        dev.writes, 0,
        "identity observation must perform zero writes"
    );
    assert_eq!(
        dev.reopens, 0,
        "identity observation must never reopen read-write"
    );
    assert_eq!(observed.protocol_image.len(), 13 * 512);
    assert_eq!(observed.snapshot.protocol.device_id, None);
    assert_eq!(observed.snapshot.protocol.onlyid, None);
    assert_eq!(
        observed.snapshot.protocol.provision_kind,
        Some(DiskProvisionKind::Plain)
    );
    assert!(!observed.snapshot.derived.device_id_candidates.is_empty());
    assert_eq!(
        observed.snapshot.hardware.serial_quality,
        SerialQuality::Usable
    );
    assert!(observed.snapshot.hardware.serial_sha256.is_some());

    let debug = format!("{:?}", observed.snapshot);
    assert!(
        !debug.contains("RAW-SERIAL-MUST-NOT-ESCAPE"),
        "raw USB serial must never enter canonical snapshot/debug output"
    );
}
