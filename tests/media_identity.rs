use std::io;
use std::time::Duration;

use edpcli::application::media_identity::{
    match_media_identity, serial_digest_evidence, ControlledLineageEvidence,
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityConfidence, IdentityObservation,
    MediaIdentityPin, MediaIdentityPinConflict, MediaIdentityResumePin, MediaIdentitySnapshot,
    MediaRelationship, ProtocolIdentityEvidence, RestoreAuthorizationDecision,
    RestoreAuthorizationPolicy, RestoreGeometryRequirements, RestoreRejection, SerialQuality,
};
#[cfg(target_os = "macos")]
use edpcli::application::media_identity_observer::media_identity_from_protocol_image;
use edpcli::application::media_identity_observer::observe_media_identity_readonly;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::ports::CmdRunner;
use edpcli::ports::SectorDev;
use edpcli::provision::DiskProvisionKind;

fn hardware(serial: Option<&str>, vid: u16, pid: u16, sectors: u64) -> HardwareIdentityEvidence {
    let raw_serial = serial.map(str::to_string);
    let serial = serial_digest_evidence(serial);
    HardwareIdentityEvidence {
        vid: Some(vid),
        pid: Some(pid),
        serial: raw_serial,
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
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
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
fn matching_digest_only_evidence_cannot_authorize_restore() {
    let mut source = snapshot(
        hardware(Some("SERIAL-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    source.hardware.serial = None;
    assert!(source.hardware.serial_sha256.is_some());
    let matched = match_media_identity(&source, &source, None);
    assert_ne!(matched.relationship, MediaRelationship::SamePhysicalMedia);
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
    fn check_output(&self, cmd: &[&str], _timeout: Duration) -> io::Result<String> {
        if cmd == ["diskutil", "info", "-plist", "disk6"] {
            return Ok(
                r#"<plist version="1.0"><dict><key>DiskSize</key><integer>64000000000</integer></dict></plist>"#
                    .into(),
            );
        }
        Err(io::Error::other(
            "platform query unavailable in unit fixture",
        ))
    }

    fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
        Some(HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: NativeTransport::Uas,
            windows_pnp_instance_id: None,
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
        let mut image = vec![0u8; 13 * 512];
        let total_sectors = 64_000_000_000u64 / 512;
        let entry = 0x1be;
        image[entry + 4] = 0x07;
        image[entry + 8..entry + 12].copy_from_slice(&2048u32.to_le_bytes());
        image[entry + 12..entry + 16]
            .copy_from_slice(&u32::try_from(total_sectors - 2048).unwrap().to_le_bytes());
        image[510..512].copy_from_slice(&[0x55, 0xaa]);
        Self {
            image,
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

#[cfg(target_os = "macos")]
struct StaleEdpRunner;

#[cfg(target_os = "macos")]
impl CmdRunner for StaleEdpRunner {
    fn check_output(&self, cmd: &[&str], _timeout: Duration) -> io::Result<String> {
        if cmd == ["diskutil", "info", "-plist", "disk26"] {
            return Ok(
                r#"<plist version="1.0"><dict><key>DiskSize</key><integer>125829120000</integer></dict></plist>"#
                    .into(),
            );
        }
        Err(io::Error::other(
            "platform query unavailable in stale fixture",
        ))
    }

    fn hardware_probe(&self, _disk: u32) -> Option<HardwareProbe> {
        Some(HardwareProbe {
            vid: Some(0x3535),
            pid: Some(0x6300),
            transport: NativeTransport::Uas,
            windows_pnp_instance_id: None,
            inquiry: Some(InquiryInfo {
                vendor: "AIGO".into(),
                product: "U335".into(),
                revision: "PMAP".into(),
            }),
        })
    }

    fn hardware_serial(&self, _disk: u32) -> Option<String> {
        Some("AIGO-STALE-EDP-PLAIN".into())
    }
}

#[cfg(target_os = "macos")]
struct StaleEdpPlainDev {
    protocol: Vec<u8>,
    boot: Vec<u8>,
    reads: usize,
    writes: usize,
}

#[cfg(target_os = "macos")]
impl SectorDev for StaleEdpPlainDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.reads += 1;
        if lba < 13 {
            let start = lba as usize * 512;
            return Ok(self.protocol[start..start + 512].to_vec());
        }
        if lba == 2_048 {
            return Ok(self.boot.clone());
        }
        Ok(vec![0u8; 512])
    }

    fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
        self.writes += 1;
        Err(io::Error::other(
            "stale identity observation attempted a write",
        ))
    }
}

#[cfg(target_os = "macos")]
#[test]
fn formatted_plain_layout_overrides_stale_valid_edp_protocol_for_readonly_identity() {
    const TOTAL: u64 = 245_760_000;
    let runner = StaleEdpRunner;
    let original = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/protocol/mode1/aigo_u335_20260916_lba0_12.bin"
    ))
    .to_vec();
    assert_eq!(original.len(), 13 * 512);
    let original_identity =
        media_identity_from_protocol_image(&runner, 26, &original).expect("original EDP identity");
    assert_eq!(
        original_identity.protocol.provision_kind,
        Some(DiskProvisionKind::Mode1),
        "authentic EDP fixture must remain recognized before the MBR is replaced"
    );

    let mut protocol = original;
    protocol[..512].fill(0);
    let entry = 0x1be;
    protocol[entry + 4] = 0x07;
    protocol[entry + 8..entry + 12].copy_from_slice(&2_048u32.to_le_bytes());
    protocol[entry + 12..entry + 16]
        .copy_from_slice(&u32::try_from(TOTAL - 2_048).unwrap().to_le_bytes());
    protocol[510..512].copy_from_slice(&[0x55, 0xaa]);

    let fs = edpcli::application::filesystem::build_empty_exfat(
        2_048,
        TOTAL - 2_048,
        0x1234_5678,
        "PLAIN",
    )
    .expect("build strict exFAT boot");
    let boot = fs.sectors().get(&0).expect("exFAT boot sector").to_vec();
    let mut dev = StaleEdpPlainDev {
        protocol,
        boot,
        reads: 0,
        writes: 0,
    };

    let observed =
        observe_media_identity_readonly(&runner, 26, &mut dev).expect("readonly stale-media audit");
    assert_eq!(
        observed.snapshot.protocol.provision_kind,
        Some(DiskProvisionKind::Plain)
    );
    assert_eq!(observed.snapshot.protocol.device_id, None);
    assert_eq!(observed.snapshot.protocol.onlyid, None);
    assert_eq!(dev.writes, 0);
    assert!(
        dev.reads > 13,
        "runtime Plain proof must inspect the live filesystem boot"
    );
}

#[test]
fn readonly_observation_never_writes_and_classifies_plain_with_fixture_geometry() {
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
    // ObservationRunner stubs macOS `diskutil` geometry. Linux and Windows obtain
    // whole-disk geometry through native platform APIs instead of CmdRunner, so this
    // synthetic fixture cannot truthfully prove Plain classification on those targets.
    // The cross-platform zero-write/read-only assertions above remain active everywhere.
    #[cfg(target_os = "macos")]
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
    assert_eq!(
        observed.snapshot.hardware.serial.as_deref(),
        Some("RAW-SERIAL-MUST-NOT-ESCAPE")
    );

    let serialized = serde_json::to_string(&observed.snapshot).unwrap();
    assert!(
        !serialized.contains("RAW-SERIAL-MUST-NOT-ESCAPE"),
        "raw USB serial must not enter generic snapshot serialization"
    );
    let debug = format!("{:?}", observed.snapshot);
    assert!(
        !debug.contains("RAW-SERIAL-MUST-NOT-ESCAPE"),
        "raw USB serial must never enter canonical snapshot/debug output"
    );
}

#[test]
fn raw_only_serial_survives_resume_projection() {
    let mut source = snapshot(
        hardware(Some("RAW-ONLY-001"), 0x1234, 0x5678, 1_000_000),
        Some("edp"),
        Some("42"),
        DiskProvisionKind::Mode0,
    );
    source.hardware.serial_sha256 = None;
    let image = [0; 13 * 512];
    let pin = MediaIdentityResumePin::from_pin(&MediaIdentityPin::new(source.clone(), &image));
    assert!(pin.validate().is_ok());
    assert!(pin.verify(&source, &image).is_ok());
    assert!(!serde_json::to_string(&pin).unwrap().contains("RAW-ONLY"));
}
