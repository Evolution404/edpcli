use super::*;

/// Convert the current manifest into the canonical media identity.
pub fn canonical_media_identity(
    manifest: &Manifest,
) -> Result<crate::media_identity::MediaIdentitySnapshot, String> {
    use crate::media_identity::{
        DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation,
        MediaIdentitySnapshot, ProtocolIdentityEvidence,
    };

    validate_manifest_identity(manifest)?;

    let identity = &manifest.identity;
    Ok(MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: identity.hardware.vid,
            pid: identity.hardware.pid,
            serial: identity.hardware.serial.clone(),
            serial_sha256: None,
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
    })
}
