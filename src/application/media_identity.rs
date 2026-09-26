//! Canonical, UI-neutral media identity evidence and matching policy.
//!
//! This module is deliberately pure: it does not open disks, scan backup directories, or
//! authorize writes.  It describes evidence collected elsewhere and derives an explainable
//! relationship/confidence result.  Destructive operations must apply their own stricter policy.

use sha2::{Digest, Sha256};

use crate::platform::NativeTransport;
use crate::provision::DiskProvisionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareIdentityEvidence {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    /// SHA-256 of the normalized serial. Raw serials must never be stored here.
    pub serial_sha256: Option<String>,
    pub serial_quality: SerialQuality,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub transport: Option<NativeTransport>,
    pub total_sectors: Option<u64>,
    pub logical_sector_size: Option<u32>,
}

impl Default for HardwareIdentityEvidence {
    fn default() -> Self {
        Self {
            vid: None,
            pid: None,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtocolIdentityEvidence {
    /// Only an EDP device_id actually observed/verified from the EDP protocol belongs here.
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub provision_kind: Option<DiskProvisionKind>,
    pub lba4_identity_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DerivedProtocolEvidence {
    /// Hardware-derived EDP device_id candidates. These are not observed protocol identity.
    pub device_id_candidates: Vec<String>,
    /// v1 Plain backups historically stored a derived candidate in the legacy device_id slot.
    pub legacy_derived_candidate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdentityObservation {
    pub platform: Option<String>,
    /// Ephemeral selector/session locator. It is observation metadata, never a permanent ID.
    pub disk_selector: Option<String>,
    pub captured_epoch: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaIdentitySnapshot {
    pub hardware: HardwareIdentityEvidence,
    pub protocol: ProtocolIdentityEvidence,
    pub derived: DerivedProtocolEvidence,
    pub observation: IdentityObservation,
}

impl MediaIdentitySnapshot {
    /// Construct canonical Plain identity without inventing an observed EDP protocol identity.
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
        sha256: Some(format!("{:x}", Sha256::digest(normalized.as_bytes()))),
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

    let usable_serials = a.hardware.serial_quality == SerialQuality::Usable
        && b.hardware.serial_quality == SerialQuality::Usable
        && a.hardware.serial_sha256.is_some()
        && b.hardware.serial_sha256.is_some();
    let serial_match = usable_serials
        && a.hardware.serial_sha256.as_deref() == b.hardware.serial_sha256.as_deref();
    let serial_mismatch = usable_serials && !serial_match;

    if serial_mismatch {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::UsbSerialDigest,
            IdentityEvidenceOutcome::Conflict,
            IdentityEvidenceStrength::Hard,
            "usable USB serial digests differ",
        );
        conflicts.push(IdentityConflict {
            kind: IdentityConflictKind::UsableSerialMismatch,
            severity: IdentityConflictSeverity::PhysicalHard,
            explanation: "usable USB serial digest mismatch".into(),
        });
    } else if serial_match {
        push_evidence(
            &mut evidence,
            IdentityEvidenceKind::UsbSerialDigest,
            IdentityEvidenceOutcome::Match,
            IdentityEvidenceStrength::Hard,
            "usable USB serial digests match",
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
                "matching serial digest conflicts with media geometry; possible serial collision"
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
