//! Canonical, UI-neutral media identity evidence and matching policy.
//!
//! This module is deliberately pure: it does not open disks, scan backup directories, or
//! authorize writes.  It describes evidence collected elsewhere and derives an explainable
//! relationship/confidence result.  Destructive operations must apply their own stricter policy.

use crate::domain::hardware::NativeTransport;
use crate::provision::DiskProvisionKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerialQuality {
    Usable,
    Suspicious,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialDigestEvidence {
    pub sha256: Option<String>,
    pub quality: SerialQuality,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareIdentityEvidence {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    /// Raw USB serial is retained only in memory so manifest v3 can persist it explicitly.
    /// Generic snapshot serialization (including elevation argv/lineage) must never carry it.
    #[serde(skip_serializing, skip_deserializing, default)]
    pub serial: Option<String>,
    /// Runtime digest used by non-secret identity pins and elevation resume payloads.
    pub serial_sha256: Option<String>,
    pub serial_quality: SerialQuality,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub transport: Option<NativeTransport>,
    pub total_sectors: Option<u64>,
    pub logical_sector_size: Option<u32>,
}

impl std::fmt::Debug for HardwareIdentityEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareIdentityEvidence")
            .field("vid", &self.vid)
            .field("pid", &self.pid)
            .field("serial", &self.serial.as_ref().map(|_| "<redacted>"))
            .field("serial_sha256", &self.serial_sha256)
            .field("serial_quality", &self.serial_quality)
            .field("vendor", &self.vendor)
            .field("product", &self.product)
            .field("revision", &self.revision)
            .field("transport", &self.transport)
            .field("total_sectors", &self.total_sectors)
            .field("logical_sector_size", &self.logical_sector_size)
            .finish()
    }
}

impl Default for HardwareIdentityEvidence {
    fn default() -> Self {
        Self {
            vid: None,
            pid: None,
            serial: None,
            serial_sha256: None,
            serial_quality: SerialQuality::Missing,
            vendor: None,
            product: None,
            revision: None,
            transport: None,
            total_sectors: None,
            logical_sector_size: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProtocolIdentityEvidence {
    /// Only an EDP device_id actually observed/verified from the EDP protocol belongs here.
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub provision_kind: Option<DiskProvisionKind>,
    pub lba4_identity_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DerivedProtocolEvidence {
    /// Hardware-derived EDP device_id candidates. These are not observed protocol identity.
    pub device_id_candidates: Vec<String>,
    /// Plain manifests classify their device_id projection as a derived candidate.
    pub legacy_derived_candidate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IdentityObservation {
    pub platform: Option<String>,
    /// Ephemeral selector/session locator. It is observation metadata, never a permanent ID.
    pub disk_selector: Option<String>,
    pub captured_epoch: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MediaIdentitySnapshot {
    pub hardware: HardwareIdentityEvidence,
    pub protocol: ProtocolIdentityEvidence,
    pub derived: DerivedProtocolEvidence,
    pub observation: IdentityObservation,
}

impl MediaIdentitySnapshot {
    /// Construct canonical Plain identity without inventing an observed EDP protocol identity.
    pub(crate) fn strong_backup_group_key(&self) -> Option<String> {
        let sectors = self.hardware.total_sectors?;
        let logical_sector_size = self.hardware.logical_sector_size?;
        let vid = self.hardware.vid?;
        let pid = self.hardware.pid?;
        let geometry =
            format!("vid:{vid:04x}:pid:{pid:04x}:sectors:{sectors}:lss:{logical_sector_size}");

        if self.hardware.serial_quality == SerialQuality::Usable {
            if let Some(serial) = self.hardware.serial.as_deref() {
                return Some(format!(
                    "serial:{}:{geometry}",
                    crate::sha256::sha256_hex(serial.as_bytes())
                ));
            }
        }
        match (
            self.protocol.device_id.as_deref(),
            self.protocol.onlyid.as_deref(),
        ) {
            (Some(device_id), Some(onlyid)) => Some(format!("edp:{device_id}:{onlyid}:{geometry}")),
            _ => None,
        }
    }

    pub fn plain(
        hardware: HardwareIdentityEvidence,
        derived: DerivedProtocolEvidence,
        observation: IdentityObservation,
    ) -> Self {
        Self {
            hardware,
            protocol: ProtocolIdentityEvidence {
                device_id: None,
                onlyid: None,
                provision_kind: Some(DiskProvisionKind::Plain),
                lba4_identity_digest: None,
            },
            derived,
            observation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaIdentityPin {
    pub snapshot: MediaIdentitySnapshot,
    pub protocol_image_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaIdentityPinConflict {
    SerialChangedOrLost,
    VidPidChangedOrLost,
    GeometryChangedOrLost,
    ProtocolImageChanged,
}

/// Compact, non-secret projection safe to carry through a TUI elevation argv.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaIdentityResumePin {
    pub serial_sha256: Option<String>,
    pub serial_quality: SerialQuality,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub total_sectors: Option<u64>,
    pub logical_sector_size: Option<u32>,
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub protocol_image_sha256: String,
}

impl MediaIdentityResumePin {
    pub fn from_pin(pin: &MediaIdentityPin) -> Self {
        Self {
            serial_sha256: serial_digest_for_pin(&pin.snapshot.hardware),
            serial_quality: pin.snapshot.hardware.serial_quality,
            vid: pin.snapshot.hardware.vid,
            pid: pin.snapshot.hardware.pid,
            total_sectors: pin.snapshot.hardware.total_sectors,
            logical_sector_size: pin.snapshot.hardware.logical_sector_size,
            device_id: pin.snapshot.protocol.device_id.clone(),
            onlyid: pin.snapshot.protocol.onlyid.clone(),
            protocol_image_sha256: pin.protocol_image_sha256.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let valid_sha =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid_sha(&self.protocol_image_sha256)
            || self
                .serial_sha256
                .as_deref()
                .is_some_and(|value| !valid_sha(value))
            || (self.serial_quality == SerialQuality::Usable && self.serial_sha256.is_none())
        {
            return Err("invalid identity pin digest");
        }
        Ok(())
    }

    pub fn verify(
        &self,
        observed: &MediaIdentitySnapshot,
        protocol_image: &[u8],
    ) -> Result<(), MediaIdentityPinConflict> {
        if self.serial_quality == SerialQuality::Usable
            && (observed.hardware.serial_quality != SerialQuality::Usable
                || self.serial_sha256 != serial_digest_for_pin(&observed.hardware))
        {
            return Err(MediaIdentityPinConflict::SerialChangedOrLost);
        }
        if self.vid != observed.hardware.vid || self.pid != observed.hardware.pid {
            return Err(MediaIdentityPinConflict::VidPidChangedOrLost);
        }
        if self.total_sectors != observed.hardware.total_sectors
            || self.logical_sector_size != observed.hardware.logical_sector_size
        {
            return Err(MediaIdentityPinConflict::GeometryChangedOrLost);
        }
        if self.device_id != observed.protocol.device_id
            || self.onlyid != observed.protocol.onlyid
            || self.protocol_image_sha256 != crate::sha256::sha256_hex(protocol_image)
        {
            return Err(MediaIdentityPinConflict::ProtocolImageChanged);
        }
        Ok(())
    }
}

impl MediaIdentityPin {
    pub fn new(snapshot: MediaIdentitySnapshot, protocol_image: &[u8]) -> Self {
        Self {
            snapshot,
            protocol_image_sha256: crate::sha256::sha256_hex(protocol_image),
        }
    }

    pub fn verify(
        &self,
        observed: &MediaIdentitySnapshot,
        protocol_image: &[u8],
    ) -> Result<(), MediaIdentityPinConflict> {
        if self.snapshot.hardware.serial_quality == SerialQuality::Usable
            && (observed.hardware.serial_quality != SerialQuality::Usable
                || serial_digest_for_pin(&self.snapshot.hardware)
                    != serial_digest_for_pin(&observed.hardware))
        {
            return Err(MediaIdentityPinConflict::SerialChangedOrLost);
        }
        if self.snapshot.hardware.vid != observed.hardware.vid
            || self.snapshot.hardware.pid != observed.hardware.pid
        {
            return Err(MediaIdentityPinConflict::VidPidChangedOrLost);
        }
        if self.snapshot.hardware.total_sectors != observed.hardware.total_sectors
            || self.snapshot.hardware.logical_sector_size != observed.hardware.logical_sector_size
        {
            return Err(MediaIdentityPinConflict::GeometryChangedOrLost);
        }
        if self.protocol_image_sha256 != crate::sha256::sha256_hex(protocol_image) {
            return Err(MediaIdentityPinConflict::ProtocolImageChanged);
        }
        Ok(())
    }
}

fn serial_digest_for_pin(hardware: &HardwareIdentityEvidence) -> Option<String> {
    hardware
        .serial_sha256
        .clone()
        .or_else(|| serial_digest_evidence(hardware.serial.as_deref()).sha256)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRelationship {
    SamePhysicalMedia,
    SameEdpInstance,
    SameControlledLineage,
    ProbableSameMedia,
    ModelOnlyMatch,
    Ambiguous,
    DifferentMedia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityConfidence {
    PhysicalStrong,
    EdpInstanceStrong,
    ControlledLineage,
    HardwareProfileMatch,
    AmbiguousInsufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityEvidenceKind {
    UsbSerialDigest,
    VidPid,
    VendorProductRevision,
    Geometry,
    ObservedDeviceId,
    Onlyid,
    Lba4Identity,
    DerivedDeviceIdCandidate,
    ControlledLineage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityEvidenceOutcome {
    Match,
    Compatible,
    ChangedExpected,
    Missing,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentityEvidenceStrength {
    Weak,
    Medium,
    Strong,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityEvidenceResult {
    pub kind: IdentityEvidenceKind,
    pub outcome: IdentityEvidenceOutcome,
    pub strength: IdentityEvidenceStrength,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityConflictKind {
    UsableSerialMismatch,
    VidPidMismatch,
    GeometryMismatch,
    SerialCollisionSuspected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityConflictSeverity {
    PhysicalHard,
    AuthorizationHard,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityConflict {
    pub kind: IdentityConflictKind,
    pub severity: IdentityConflictSeverity,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMatch {
    pub relationship: MediaRelationship,
    pub confidence: IdentityConfidence,
    pub evidence: Vec<IdentityEvidenceResult>,
    pub conflicts: Vec<IdentityConflict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupAffinity {
    Confirmed,
    Possible,
    Unrelated,
}

/// Read-side grouping policy only. This policy never authorizes destructive writes.
pub struct BackupAffinityPolicy;

impl BackupAffinityPolicy {
    pub fn classify(identity_match: &IdentityMatch) -> BackupAffinity {
        if identity_match.relationship == MediaRelationship::DifferentMedia {
            return BackupAffinity::Unrelated;
        }
        match identity_match.confidence {
            IdentityConfidence::PhysicalStrong
            | IdentityConfidence::EdpInstanceStrong
            | IdentityConfidence::ControlledLineage => BackupAffinity::Confirmed,
            IdentityConfidence::HardwareProfileMatch => BackupAffinity::Possible,
            IdentityConfidence::AmbiguousInsufficient => BackupAffinity::Unrelated,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreGeometryRequirements {
    pub total_sectors: u64,
    pub logical_sector_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRejection {
    UsableSerialMismatch,
    VidPidMismatch,
    GeometryMismatch,
    GeometryUnavailable,
    WeakHardwareBinding,
    DifferentMedia,
    InsufficientPhysicalEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAuthorizationDecision {
    Authorized,
    Reject(RestoreRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsableSerialComparison {
    RawMatch,
    RawMismatch,
    Missing,
}

fn compare_usable_serials(
    a: &HardwareIdentityEvidence,
    b: &HardwareIdentityEvidence,
) -> UsableSerialComparison {
    if a.serial_quality != SerialQuality::Usable || b.serial_quality != SerialQuality::Usable {
        return UsableSerialComparison::Missing;
    }
    if let (Some(left), Some(right)) = (a.serial.as_deref(), b.serial.as_deref()) {
        return if left == right {
            UsableSerialComparison::RawMatch
        } else {
            UsableSerialComparison::RawMismatch
        };
    }
    UsableSerialComparison::Missing
}

/// Destructive restore requires matching, usable hardware evidence and exact geometry.
/// Protocol identity and controlled lineage are independent evidence, never write grants.
pub struct RestoreAuthorizationPolicy;

impl RestoreAuthorizationPolicy {
    pub fn evaluate(
        backup: &MediaIdentitySnapshot,
        target: &MediaIdentitySnapshot,
        identity_match: &IdentityMatch,
        geometry: RestoreGeometryRequirements,
        _lineage: Option<&ControlledLineageEvidence>,
    ) -> RestoreAuthorizationDecision {
        use RestoreAuthorizationDecision::{Authorized, Reject};

        if identity_match
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == IdentityConflictKind::UsableSerialMismatch)
        {
            return Reject(RestoreRejection::UsableSerialMismatch);
        }
        if identity_match
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == IdentityConflictKind::VidPidMismatch)
        {
            return Reject(RestoreRejection::VidPidMismatch);
        }
        if backup
            .hardware
            .total_sectors
            .is_some_and(|value| value != geometry.total_sectors)
            || target
                .hardware
                .total_sectors
                .is_some_and(|value| value != geometry.total_sectors)
            || backup
                .hardware
                .logical_sector_size
                .is_some_and(|value| value != geometry.logical_sector_size)
            || target
                .hardware
                .logical_sector_size
                .is_some_and(|value| value != geometry.logical_sector_size)
            || identity_match
                .conflicts
                .iter()
                .any(|conflict| conflict.kind == IdentityConflictKind::GeometryMismatch)
        {
            return Reject(RestoreRejection::GeometryMismatch);
        }
        if backup.hardware.total_sectors != Some(geometry.total_sectors)
            || target.hardware.total_sectors != Some(geometry.total_sectors)
            || backup.hardware.logical_sector_size != Some(geometry.logical_sector_size)
            || target.hardware.logical_sector_size != Some(geometry.logical_sector_size)
        {
            return Reject(RestoreRejection::GeometryUnavailable);
        }
        if identity_match.relationship == MediaRelationship::DifferentMedia {
            return Reject(RestoreRejection::DifferentMedia);
        }
        let serial_comparison = compare_usable_serials(&backup.hardware, &target.hardware);
        if backup.hardware.vid.is_none()
            || backup.hardware.pid.is_none()
            || backup.hardware.vid != target.hardware.vid
            || backup.hardware.pid != target.hardware.pid
            || matches!(serial_comparison, UsableSerialComparison::Missing)
        {
            return Reject(RestoreRejection::WeakHardwareBinding);
        }
        if !matches!(serial_comparison, UsableSerialComparison::RawMatch)
            || identity_match.relationship != MediaRelationship::SamePhysicalMedia
        {
            return Reject(RestoreRejection::InsufficientPhysicalEvidence);
        }
        Authorized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlledLineageEvidence {
    pub linked: bool,
}

/// Normalize a serial only for comparison/hashing. The raw value is never returned.
pub fn serial_digest_evidence(raw: Option<&str>) -> SerialDigestEvidence {
    let Some(raw) = raw else {
        return SerialDigestEvidence {
            sha256: None,
            quality: SerialQuality::Missing,
        };
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        return SerialDigestEvidence {
            sha256: None,
            quality: SerialQuality::Missing,
        };
    }

    let folded = normalized.to_ascii_lowercase();
    let all_zero = normalized.bytes().all(|byte| byte == b'0');
    let all_f = normalized.bytes().all(|byte| byte == b'f' || byte == b'F');
    let placeholder = matches!(
        folded.as_str(),
        "unknown"
            | "none"
            | "n/a"
            | "na"
            | "null"
            | "default"
            | "serial"
            | "serialnumber"
            | "not available"
            | "notavailable"
    );
    let quality = if all_zero || all_f || placeholder || normalized.len() < 4 {
        SerialQuality::Suspicious
    } else {
        SerialQuality::Usable
    };

    SerialDigestEvidence {
        sha256: Some(crate::sha256::sha256_hex(normalized.as_bytes())),
        quality,
    }
}

fn same_nonempty(a: Option<&str>, b: Option<&str>) -> bool {
    matches!(
        (a.map(str::trim), b.map(str::trim)),
        (Some(left), Some(right)) if !left.is_empty() && left == right
    )
}

fn option_equal<T: PartialEq>(a: Option<&T>, b: Option<&T>) -> bool {
    matches!((a, b), (Some(left), Some(right)) if left == right)
}

fn different_if_both<T: PartialEq>(a: Option<&T>, b: Option<&T>) -> bool {
    matches!((a, b), (Some(left), Some(right)) if left != right)
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn hardware_profile_matches(a: &HardwareIdentityEvidence, b: &HardwareIdentityEvidence) -> bool {
    let vid_pid = option_equal(a.vid.as_ref(), b.vid.as_ref())
        && option_equal(a.pid.as_ref(), b.pid.as_ref());
    let model = normalized_text(a.vendor.as_deref()) == normalized_text(b.vendor.as_deref())
        && normalized_text(a.product.as_deref()) == normalized_text(b.product.as_deref())
        && normalized_text(a.revision.as_deref()) == normalized_text(b.revision.as_deref())
        && normalized_text(a.vendor.as_deref()).is_some()
        && normalized_text(a.product.as_deref()).is_some();
    let geometry = option_equal(a.total_sectors.as_ref(), b.total_sectors.as_ref())
        && option_equal(
            a.logical_sector_size.as_ref(),
            b.logical_sector_size.as_ref(),
        );
    vid_pid && model && geometry
}

fn push_evidence(
    evidence: &mut Vec<IdentityEvidenceResult>,
    kind: IdentityEvidenceKind,
    outcome: IdentityEvidenceOutcome,
    strength: IdentityEvidenceStrength,
    explanation: impl Into<String>,
) {
    evidence.push(IdentityEvidenceResult {
        kind,
        outcome,
        strength,
        explanation: explanation.into(),
    });
}

/// Pure evidence matcher. It never authorizes destructive I/O.
pub fn match_media_identity(
    a: &MediaIdentitySnapshot,
    b: &MediaIdentitySnapshot,
    lineage: Option<&ControlledLineageEvidence>,
) -> IdentityMatch {
    let mut evidence = Vec::new();
    let mut conflicts = Vec::new();

    let serial_comparison = compare_usable_serials(&a.hardware, &b.hardware);
    let serial_match = matches!(serial_comparison, UsableSerialComparison::RawMatch);
    let serial_mismatch = matches!(serial_comparison, UsableSerialComparison::RawMismatch);

    if serial_mismatch {
        let explanation = "usable raw USB serials differ";
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::UsbSerialDigest,
            IdentityEvidenceOutcome::Conflict,
            IdentityEvidenceStrength::Hard,
            explanation,
        );
        conflicts.push(IdentityConflict {
            kind: IdentityConflictKind::UsableSerialMismatch,
            severity: IdentityConflictSeverity::PhysicalHard,
            explanation: explanation.into(),
        });
    } else if serial_match {
        let explanation = "usable raw USB serials match";
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::UsbSerialDigest,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Hard,
            explanation,
        );
    } else {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::UsbSerialDigest,
            IdentityEvidenceOutcome::Missing,
            IdentityEvidenceStrength::Weak,
            "usable USB serial evidence is unavailable",
        );
    }

    let vid_mismatch = different_if_both(a.hardware.vid.as_ref(), b.hardware.vid.as_ref());
    let pid_mismatch = different_if_both(a.hardware.pid.as_ref(), b.hardware.pid.as_ref());
    if vid_mismatch || pid_mismatch {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::VidPid,
            IdentityEvidenceOutcome::Conflict,
            IdentityEvidenceStrength::Hard,
            "USB VID/PID differ",
        );
        conflicts.push(IdentityConflict {
            kind: IdentityConflictKind::VidPidMismatch,
            severity: IdentityConflictSeverity::PhysicalHard,
            explanation: "USB VID/PID mismatch".into(),
        });
    } else if option_equal(a.hardware.vid.as_ref(), b.hardware.vid.as_ref())
        && option_equal(a.hardware.pid.as_ref(), b.hardware.pid.as_ref())
    {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::VidPid,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Strong,
            "USB VID/PID match",
        );
    }

    let total_mismatch = different_if_both(
        a.hardware.total_sectors.as_ref(),
        b.hardware.total_sectors.as_ref(),
    );
    let sector_size_mismatch = different_if_both(
        a.hardware.logical_sector_size.as_ref(),
        b.hardware.logical_sector_size.as_ref(),
    );
    if total_mismatch || sector_size_mismatch {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::Geometry,
            IdentityEvidenceOutcome::Conflict,
            IdentityEvidenceStrength::Hard,
            "media geometry differs",
        );
        conflicts.push(IdentityConflict {
            kind: IdentityConflictKind::GeometryMismatch,
            severity: IdentityConflictSeverity::AuthorizationHard,
            explanation: "sector geometry mismatch; destructive authorization must fail closed"
                .into(),
        });
    } else if option_equal(
        a.hardware.total_sectors.as_ref(),
        b.hardware.total_sectors.as_ref(),
    ) && option_equal(
        a.hardware.logical_sector_size.as_ref(),
        b.hardware.logical_sector_size.as_ref(),
    ) {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::Geometry,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Medium,
            "media geometry matches",
        );
    }

    if serial_mismatch || vid_mismatch || pid_mismatch {
        return IdentityMatch {
            relationship: MediaRelationship::DifferentMedia,
            confidence: IdentityConfidence::AmbiguousInsufficient,
            evidence,
            conflicts,
        };
    }

    if serial_match && (total_mismatch || sector_size_mismatch) {
        conflicts.push(IdentityConflict {
            kind: IdentityConflictKind::SerialCollisionSuspected,
            severity: IdentityConflictSeverity::Ambiguous,
            explanation:
                "matching usable serial conflicts with media geometry; possible serial collision"
                    .into(),
        });
        return IdentityMatch {
            relationship: MediaRelationship::Ambiguous,
            confidence: IdentityConfidence::AmbiguousInsufficient,
            evidence,
            conflicts,
        };
    }

    let device_id_same = same_nonempty(
        a.protocol.device_id.as_deref(),
        b.protocol.device_id.as_deref(),
    );
    let onlyid_same = same_nonempty(a.protocol.onlyid.as_deref(), b.protocol.onlyid.as_deref());
    let onlyid_changed = matches!(
        (a.protocol.onlyid.as_deref(), b.protocol.onlyid.as_deref()),
        (Some(left), Some(right)) if !left.is_empty() && !right.is_empty() && left != right
    );

    if device_id_same {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::ObservedDeviceId,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Strong,
            "observed EDP device_id matches",
        );
    }
    if onlyid_same {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::Onlyid,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Strong,
            "EDP onlyid matches",
        );
    } else if onlyid_changed {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::Onlyid,
            IdentityEvidenceOutcome::ChangedExpected,
            IdentityEvidenceStrength::Medium,
            "EDP onlyid changed; reprovisioning may legitimately change it",
        );
    }

    if serial_match {
        return IdentityMatch {
            relationship: MediaRelationship::SamePhysicalMedia,
            confidence: IdentityConfidence::PhysicalStrong,
            evidence,
            conflicts,
        };
    }

    if device_id_same && onlyid_same {
        return IdentityMatch {
            relationship: MediaRelationship::SameEdpInstance,
            confidence: IdentityConfidence::EdpInstanceStrong,
            evidence,
            conflicts,
        };
    }

    if lineage.is_some_and(|lineage| lineage.linked) {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::ControlledLineage,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Strong,
            "host-side controlled lineage links both observations",
        );
        return IdentityMatch {
            relationship: MediaRelationship::SameControlledLineage,
            confidence: IdentityConfidence::ControlledLineage,
            evidence,
            conflicts,
        };
    }

    if hardware_profile_matches(&a.hardware, &b.hardware) {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::VendorProductRevision,
            IdentityEvidenceOutcome::Compatible,
            IdentityEvidenceStrength::Medium,
            "hardware model and geometry match but no unique hardware serial is available",
        );
        return IdentityMatch {
            relationship: MediaRelationship::ModelOnlyMatch,
            confidence: IdentityConfidence::HardwareProfileMatch,
            evidence,
            conflicts,
        };
    }

    if device_id_same {
        return IdentityMatch {
            relationship: MediaRelationship::ProbableSameMedia,
            confidence: IdentityConfidence::AmbiguousInsufficient,
            evidence,
            conflicts,
        };
    }

    IdentityMatch {
        relationship: MediaRelationship::Ambiguous,
        confidence: IdentityConfidence::AmbiguousInsufficient,
        evidence,
        conflicts,
    }
}
