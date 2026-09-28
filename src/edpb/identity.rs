use super::*;

pub(super) fn parse_hex_u16(value: &str) -> Option<u16> {
    let value = value.trim().trim_start_matches("0x");
    (value.len() <= 4)
        .then(|| u16::from_str_radix(value, 16).ok())
        .flatten()
}

pub(super) fn manifest_serial_quality(
    quality: crate::media_identity::SerialQuality,
) -> ManifestSerialQuality {
    use crate::media_identity::SerialQuality;
    match quality {
        SerialQuality::Usable => ManifestSerialQuality::Usable,
        SerialQuality::Suspicious => ManifestSerialQuality::Suspicious,
        SerialQuality::Missing => ManifestSerialQuality::Missing,
    }
}

pub(super) fn manifest_transport(value: crate::platform::NativeTransport) -> ManifestTransport {
    match value {
        crate::platform::NativeTransport::Uas => ManifestTransport::Uas,
        crate::platform::NativeTransport::Bot => ManifestTransport::Bot,
        crate::platform::NativeTransport::Unknown => ManifestTransport::Unknown,
    }
}

pub(super) fn manifest_provision_kind(
    value: crate::provision::DiskProvisionKind,
) -> ManifestProvisionKind {
    match value {
        crate::provision::DiskProvisionKind::Plain => ManifestProvisionKind::Plain,
        crate::provision::DiskProvisionKind::Mode0 => ManifestProvisionKind::Mode0,
        crate::provision::DiskProvisionKind::Mode1 => ManifestProvisionKind::Mode1,
        crate::provision::DiskProvisionKind::Mode2 => ManifestProvisionKind::Mode2,
        crate::provision::DiskProvisionKind::Mode3 => ManifestProvisionKind::Mode3,
    }
}

pub fn manifest_identity_from_snapshot(
    snapshot: &crate::media_identity::MediaIdentitySnapshot,
) -> ManifestIdentity {
    manifest_identity_from_snapshot_for_schema(snapshot, false)
}

pub(super) fn manifest_identity_from_snapshot_v2(
    snapshot: &crate::media_identity::MediaIdentitySnapshot,
) -> ManifestIdentity {
    manifest_identity_from_snapshot_for_schema(snapshot, true)
}

fn manifest_identity_from_snapshot_for_schema(
    snapshot: &crate::media_identity::MediaIdentitySnapshot,
    legacy_v2: bool,
) -> ManifestIdentity {
    ManifestIdentity {
        hardware: ManifestHardwareIdentity {
            vid: snapshot.hardware.vid,
            pid: snapshot.hardware.pid,
            serial: (!legacy_v2)
                .then(|| snapshot.hardware.serial.clone())
                .flatten(),
            serial_sha256: legacy_v2
                .then(|| snapshot.hardware.serial_sha256.clone())
                .flatten(),
            serial_quality: manifest_serial_quality(snapshot.hardware.serial_quality),
            vendor: snapshot.hardware.vendor.clone(),
            product: snapshot.hardware.product.clone(),
            revision: snapshot.hardware.revision.clone(),
            transport: snapshot.hardware.transport.map(manifest_transport),
            total_sectors: snapshot.hardware.total_sectors,
            logical_sector_size: snapshot.hardware.logical_sector_size,
        },
        protocol: ManifestProtocolIdentity {
            device_id: snapshot.protocol.device_id.clone(),
            onlyid: snapshot.protocol.onlyid.clone(),
            provision_kind: snapshot
                .protocol
                .provision_kind
                .map(manifest_provision_kind),
            lba4_identity_digest: snapshot.protocol.lba4_identity_digest.clone(),
        },
        derived: ManifestDerivedIdentity {
            device_id_candidates: snapshot.derived.device_id_candidates.clone(),
            legacy_derived_candidate: snapshot.derived.legacy_derived_candidate.clone(),
        },
    }
}

pub(super) fn inferred_manifest_identity(capture: &CoreCapture<'_>) -> ManifestIdentity {
    let plain = capture.device_state.eq_ignore_ascii_case("plain");
    ManifestIdentity {
        hardware: ManifestHardwareIdentity {
            vid: parse_hex_u16(&capture.vid),
            pid: parse_hex_u16(&capture.pid),
            serial: None,
            serial_sha256: None,
            serial_quality: ManifestSerialQuality::Missing,
            vendor: None,
            product: None,
            revision: None,
            transport: None,
            total_sectors: capture.total_sectors,
            logical_sector_size: Some(capture.logical_sector_size),
        },
        protocol: ManifestProtocolIdentity {
            device_id: (!plain).then(|| capture.device_id.clone()),
            onlyid: (!plain).then(|| capture.onlyid.clone()).flatten(),
            provision_kind: plain.then_some(ManifestProvisionKind::Plain),
            lba4_identity_digest: None,
        },
        derived: ManifestDerivedIdentity {
            device_id_candidates: if capture.device_id.is_empty() {
                Vec::new()
            } else {
                vec![capture.device_id.clone()]
            },
            legacy_derived_candidate: plain.then(|| capture.device_id.clone()),
        },
    }
}

pub(super) fn canonical_serial_quality(
    value: ManifestSerialQuality,
) -> crate::media_identity::SerialQuality {
    match value {
        ManifestSerialQuality::Usable => crate::media_identity::SerialQuality::Usable,
        ManifestSerialQuality::Suspicious => crate::media_identity::SerialQuality::Suspicious,
        ManifestSerialQuality::Missing => crate::media_identity::SerialQuality::Missing,
    }
}

pub(super) fn canonical_transport(value: ManifestTransport) -> crate::platform::NativeTransport {
    match value {
        ManifestTransport::Uas => crate::platform::NativeTransport::Uas,
        ManifestTransport::Bot => crate::platform::NativeTransport::Bot,
        ManifestTransport::Unknown => crate::platform::NativeTransport::Unknown,
    }
}

pub(super) fn canonical_provision_kind(
    value: ManifestProvisionKind,
) -> crate::provision::DiskProvisionKind {
    match value {
        ManifestProvisionKind::Plain => crate::provision::DiskProvisionKind::Plain,
        ManifestProvisionKind::Mode0 => crate::provision::DiskProvisionKind::Mode0,
        ManifestProvisionKind::Mode1 => crate::provision::DiskProvisionKind::Mode1,
        ManifestProvisionKind::Mode2 => crate::provision::DiskProvisionKind::Mode2,
        ManifestProvisionKind::Mode3 => crate::provision::DiskProvisionKind::Mode3,
    }
}

fn validate_typed_identity_projection(
    manifest: &Manifest,
    identity: &ManifestIdentity,
) -> Result<(), String> {
    if let Some(vid) = identity.hardware.vid {
        if parse_hex_u16(&manifest.device.vid) != Some(vid) {
            return Err("EDPB typed VID conflicts with legacy device projection".into());
        }
    }
    if let Some(pid) = identity.hardware.pid {
        if parse_hex_u16(&manifest.device.pid) != Some(pid) {
            return Err("EDPB typed PID conflicts with legacy device projection".into());
        }
    }
    if let Some(total) = identity.hardware.total_sectors {
        if manifest.geometry.total_sectors != Some(total) {
            return Err("EDPB typed total_sectors conflicts with geometry".into());
        }
    }
    if let Some(sector_size) = identity.hardware.logical_sector_size {
        if manifest.geometry.logical_sector_size != sector_size {
            return Err("EDPB typed logical sector size conflicts with geometry".into());
        }
    }

    let typed_plain = identity.protocol.provision_kind == Some(ManifestProvisionKind::Plain);
    if typed_plain {
        if identity.protocol.device_id.is_some() || identity.protocol.onlyid.is_some() {
            return Err(
                "EDPB Plain typed protocol identity must not contain device_id/onlyid".into(),
            );
        }
        if manifest.device.onlyid.is_some() {
            return Err("EDPB Plain legacy projection must not contain onlyid".into());
        }
        let projection_is_derived = identity.derived.legacy_derived_candidate.as_deref()
            == Some(manifest.device.device_id.as_str())
            || identity
                .derived
                .device_id_candidates
                .iter()
                .any(|candidate| candidate == &manifest.device.device_id);
        if !projection_is_derived {
            return Err(
                "EDPB Plain legacy device_id must be classified as derived candidate".into(),
            );
        }
    } else {
        if let Some(device_id) = identity.protocol.device_id.as_deref() {
            if device_id != manifest.device.device_id {
                return Err("EDPB typed device_id conflicts with legacy device projection".into());
            }
        }
        if let Some(onlyid) = identity.protocol.onlyid.as_deref() {
            if manifest.device.onlyid.as_deref() != Some(onlyid) {
                return Err("EDPB typed onlyid conflicts with legacy device projection".into());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_manifest_identity(manifest: &Manifest) -> Result<(), String> {
    match manifest.schema.as_str() {
        "edpb.manifest.v1" => {
            if manifest.identity.is_some() {
                return Err("EDPB manifest v1 must not carry typed identity".into());
            }
            legacy_hardware_serial_digest(manifest)?;
            Ok(())
        }
        "edpb.manifest.v2" => {
            let identity = manifest
                .identity
                .as_ref()
                .ok_or_else(|| "EDPB manifest v2 missing typed identity".to_string())?;
            if identity.hardware.serial.is_some() {
                return Err("EDPB manifest v2 must not carry a raw USB serial".into());
            }
            match identity.hardware.serial_quality {
                ManifestSerialQuality::Missing => {
                    if identity.hardware.serial_sha256.is_some() {
                        return Err(
                            "EDPB typed identity marks serial missing but stores a digest".into(),
                        );
                    }
                }
                ManifestSerialQuality::Usable | ManifestSerialQuality::Suspicious => {
                    let digest = identity.hardware.serial_sha256.as_deref().ok_or_else(|| {
                        "EDPB typed identity serial quality requires a digest".to_string()
                    })?;
                    if !valid_sha256_hex(digest) {
                        return Err("EDPB typed hardware serial digest is malformed".into());
                    }
                }
            }
            if let Some(legacy_digest) = legacy_hardware_serial_digest(manifest)? {
                if identity
                    .hardware
                    .serial_sha256
                    .as_deref()
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                    != Some(legacy_digest.as_str())
                {
                    return Err(
                        "EDPB typed identity conflicts with legacy hardware serial evidence".into(),
                    );
                }
            }
            validate_typed_identity_projection(manifest, identity)
        }
        "edpb.manifest.v3" => {
            let identity = manifest
                .identity
                .as_ref()
                .ok_or_else(|| "EDPB manifest v3 missing typed identity".to_string())?;
            if identity.hardware.serial_sha256.is_some() {
                return Err("EDPB manifest v3 must not carry serial_sha256".into());
            }
            if legacy_hardware_serial_digest(manifest)?.is_some() {
                return Err("EDPB manifest v3 must not carry legacy serial digest notes".into());
            }
            match identity.hardware.serial_quality {
                ManifestSerialQuality::Missing => {
                    if identity.hardware.serial.is_some() {
                        return Err(
                            "EDPB v3 marks USB serial missing but stores a raw value".into()
                        );
                    }
                }
                ManifestSerialQuality::Usable | ManifestSerialQuality::Suspicious => {
                    let serial = identity.hardware.serial.as_deref().ok_or_else(|| {
                        "EDPB v3 serial quality requires a raw USB serial".to_string()
                    })?;
                    if serial.is_empty() {
                        return Err("EDPB v3 raw USB serial must not be empty".into());
                    }
                }
            }
            validate_typed_identity_projection(manifest, identity)
        }
        other => Err(format!("unsupported EDPB manifest schema: {other}")),
    }
}

/// Convert historical manifest v1/v2 or current manifest v3 into canonical identity.
///
/// Free-text serial-digest parsing is confined to the historical v1 adapter. Manifest v3 carries
/// the reviewed raw serial field explicitly and never synthesizes a new persisted digest.
pub fn canonical_media_identity(
    manifest: &Manifest,
) -> Result<crate::media_identity::MediaIdentitySnapshot, String> {
    use crate::media_identity::{
        DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation,
        MediaIdentitySnapshot, ProtocolIdentityEvidence, SerialQuality,
    };

    validate_manifest_identity(manifest)?;

    if matches!(
        manifest.schema.as_str(),
        "edpb.manifest.v2" | "edpb.manifest.v3"
    ) {
        let identity = manifest
            .identity
            .as_ref()
            .ok_or_else(|| format!("{} missing typed identity", manifest.schema))?;
        let manifest_v3 = manifest.schema == "edpb.manifest.v3";
        return Ok(MediaIdentitySnapshot {
            hardware: HardwareIdentityEvidence {
                vid: identity.hardware.vid,
                pid: identity.hardware.pid,
                serial: manifest_v3
                    .then(|| identity.hardware.serial.clone())
                    .flatten(),
                serial_sha256: (!manifest_v3)
                    .then(|| identity.hardware.serial_sha256.clone())
                    .flatten(),
                serial_quality: canonical_serial_quality(identity.hardware.serial_quality),
                vendor: identity.hardware.vendor.clone(),
                product: identity.hardware.product.clone(),
                revision: identity.hardware.revision.clone(),
                transport: identity.hardware.transport.map(canonical_transport),
                total_sectors: identity.hardware.total_sectors,
                logical_sector_size: identity.hardware.logical_sector_size,
            },
            protocol: ProtocolIdentityEvidence {
                device_id: identity.protocol.device_id.clone(),
                onlyid: identity.protocol.onlyid.clone(),
                provision_kind: identity
                    .protocol
                    .provision_kind
                    .map(canonical_provision_kind),
                lba4_identity_digest: identity.protocol.lba4_identity_digest.clone(),
            },
            derived: DerivedProtocolEvidence {
                device_id_candidates: identity.derived.device_id_candidates.clone(),
                legacy_derived_candidate: identity.derived.legacy_derived_candidate.clone(),
            },
            observation: IdentityObservation {
                platform: Some(manifest.observation.platform.clone()),
                disk_selector: manifest
                    .observation
                    .disk_number
                    .map(|disk| format!("disk{disk}")),
                captured_epoch: Some(manifest.snapshot.created_epoch),
            },
        });
    }

    let serial_sha256 = legacy_hardware_serial_digest(manifest)?;
    let is_plain = manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
    Ok(MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: parse_hex_u16(&manifest.device.vid),
            pid: parse_hex_u16(&manifest.device.pid),
            serial: None,
            serial_quality: if serial_sha256.is_some() {
                SerialQuality::Usable
            } else {
                SerialQuality::Missing
            },
            serial_sha256,
            vendor: None,
            product: None,
            revision: None,
            transport: None,
            total_sectors: manifest.geometry.total_sectors,
            logical_sector_size: Some(manifest.geometry.logical_sector_size),
        },
        protocol: ProtocolIdentityEvidence {
            device_id: (!is_plain).then(|| manifest.device.device_id.clone()),
            onlyid: (!is_plain)
                .then(|| manifest.device.onlyid.clone())
                .flatten(),
            provision_kind: is_plain.then_some(crate::provision::DiskProvisionKind::Plain),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence {
            device_id_candidates: Vec::new(),
            legacy_derived_candidate: is_plain.then(|| manifest.device.device_id.clone()),
        },
        observation: IdentityObservation {
            platform: Some(manifest.observation.platform.clone()),
            disk_selector: manifest
                .observation
                .disk_number
                .map(|disk| format!("disk{disk}")),
            captured_epoch: Some(manifest.snapshot.created_epoch),
        },
    })
}
