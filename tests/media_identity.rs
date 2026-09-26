use std::io;
use std::time::Duration;

use edpcli::application::media_identity::{
    match_media_identity, serial_digest_evidence, ControlledLineageEvidence,
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityConfidence, IdentityObservation,
    MediaIdentitySnapshot, MediaRelationship, ProtocolIdentityEvidence, SerialQuality,
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
