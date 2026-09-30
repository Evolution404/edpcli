use super::*;

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
