//! Read-only collection of canonical media identity evidence.
//!
//! This module may probe hardware and read protocol sectors, but it deliberately has no path to
//! prepare/unmount a disk, reopen a handle read-write, or write a sector. Destructive callers must
//! separately apply their authorization policy and the existing write safety state machine.

use sha2::{Digest, Sha256};

use crate::common::{
    EdpCliError, EdpCliResult, EXIT_IO, METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR,
};
use crate::diskio::{self, SectorDev};
use crate::identify::{generate_candidates, identify};
use crate::platform::{HardwareProbe, NativeTransport};
use crate::provision::DiskProvisionKind;
use crate::sysinfo::{self, CmdRunner};

use super::media_identity::{
    serial_digest_evidence, DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation,
    MediaIdentitySnapshot, ProtocolIdentityEvidence,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadonlyMediaObservation {
    pub snapshot: MediaIdentitySnapshot,
    pub protocol_image: Vec<u8>,
}

/// Read exactly LBA0..12 in protocol order without any write-capable transition.
pub fn read_protocol_image_readonly(dev: &mut dyn SectorDev) -> EdpCliResult<Vec<u8>> {
    let mut image = Vec::with_capacity(METADATA_IMAGE_LEN);
    for lba in 0..METADATA_SECTOR_COUNT as u32 {
        let sector = dev.read_sector(lba).map_err(|error| {
            EdpCliError::new(EXIT_IO, format!("错误: 读取 LBA{lba} 失败: {error}"))
        })?;
        if sector.len() != SECTOR {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: LBA{lba} 读取 {}B，预期完整扇区 {SECTOR}B",
                    sector.len()
                ),
            ));
        }
        image.extend_from_slice(&sector);
    }
    Ok(image)
}

fn merged_hardware_probe(runner: &dyn CmdRunner, disk: u32) -> Option<HardwareProbe> {
    let native = runner.hardware_probe(disk);
    let fallback_needed = native.as_ref().is_none_or(|probe| {
        probe.vid.is_none()
            || probe.pid.is_none()
            || probe.transport == NativeTransport::Unknown
            || probe
                .inquiry
                .as_ref()
                .is_none_or(|inquiry| inquiry.vendor.trim().is_empty())
    });
    let fallback = fallback_needed
        .then(|| crate::platform::fallback_hardware_probe(runner, disk))
        .flatten();

    match (native, fallback) {
        (Some(mut primary), Some(secondary)) => {
            if primary.vid.is_none() {
                primary.vid = secondary.vid;
            }
            if primary.pid.is_none() {
                primary.pid = secondary.pid;
            }
            if primary.transport == NativeTransport::Unknown {
                primary.transport = secondary.transport;
            }
            if primary
                .inquiry
                .as_ref()
                .is_none_or(|inquiry| inquiry.vendor.trim().is_empty())
            {
                primary.inquiry = secondary.inquiry;
            }
            Some(primary)
        }
        (Some(primary), None) => Some(primary),
        (None, Some(fallback)) => Some(fallback),
        (None, None) => None,
    }
}

fn lba4_digest(lba4: &[u8]) -> Option<String> {
    lba4.iter()
        .any(|byte| *byte != 0)
        .then(|| format!("{:x}", Sha256::digest(lba4)))
}

pub fn media_identity_from_protocol_image(
    runner: &dyn CmdRunner,
    disk: u32,
    protocol_image: &[u8],
) -> EdpCliResult<MediaIdentitySnapshot> {
    if protocol_image.len() != METADATA_IMAGE_LEN {
        return Err(EdpCliError::new(
            EXIT_IO,
            format!(
                "错误: 身份观察镜像长度 {}B，预期 {}B",
                protocol_image.len(),
                METADATA_IMAGE_LEN
            ),
        ));
    }
    let lba4 = &protocol_image[4 * SECTOR..5 * SECTOR];
    let lba7 = &protocol_image[7 * SECTOR..8 * SECTOR];

    let probe = merged_hardware_probe(runner, disk);
    let serial = runner.hardware_serial(disk);
    let serial = serial_digest_evidence(serial.as_deref());
    let total_sectors = sysinfo::disk_total_sectors(runner, disk);
    let hardware = HardwareIdentityEvidence {
        vid: probe.as_ref().and_then(|value| value.vid),
        pid: probe.as_ref().and_then(|value| value.pid),
        serial_sha256: serial.sha256,
        serial_quality: serial.quality,
        vendor: probe
            .as_ref()
            .and_then(|value| value.inquiry.as_ref())
            .map(|value| value.vendor.trim().to_string())
            .filter(|value| !value.is_empty()),
        product: probe
            .as_ref()
            .and_then(|value| value.inquiry.as_ref())
            .map(|value| value.product.trim().to_string())
            .filter(|value| !value.is_empty()),
        revision: probe
            .as_ref()
            .and_then(|value| value.inquiry.as_ref())
            .map(|value| value.revision.trim().to_string())
            .filter(|value| !value.is_empty()),
        transport: probe.as_ref().map(|value| value.transport),
        total_sectors,
        logical_sector_size: Some(SECTOR as u32),
    };

    let candidates = generate_candidates(runner, disk);
    let identified = identify(runner, disk, lba7);
    let onlyid = diskio::lba4_label_id_from(lba4);
    let lba4_identity_digest = lba4_digest(lba4);
    let derived = DerivedProtocolEvidence {
        device_id_candidates: candidates,
        legacy_derived_candidate: None,
    };
    let observation = IdentityObservation {
        platform: Some(std::env::consts::OS.to_string()),
        disk_selector: Some(format!("disk{disk}")),
        captured_epoch: None,
    };

    let snapshot = if let Some(device_id) = identified.device_id {
        let detected = DiskProvisionKind::from_metadata(protocol_image, &device_id);
        let provision_kind = (detected != DiskProvisionKind::Plain).then_some(detected);
        MediaIdentitySnapshot {
            hardware,
            protocol: ProtocolIdentityEvidence {
                device_id: Some(device_id),
                onlyid,
                provision_kind,
                lba4_identity_digest,
            },
            derived,
            observation,
        }
    } else if lba4.iter().all(|byte| *byte == 0) {
        MediaIdentitySnapshot::plain(hardware, derived, observation)
    } else {
        MediaIdentitySnapshot {
            hardware,
            protocol: ProtocolIdentityEvidence {
                device_id: None,
                onlyid,
                provision_kind: None,
                lba4_identity_digest,
            },
            derived,
            observation,
        }
    };
    Ok(snapshot)
}

/// Observe media identity using only read/probe operations.
///
/// Raw USB serial text exists only long enough to normalize/hash it. It is not retained in the
/// returned snapshot or protocol image.
pub fn observe_media_identity_readonly(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<ReadonlyMediaObservation> {
    let protocol_image = read_protocol_image_readonly(dev)?;
    let snapshot = media_identity_from_protocol_image(runner, disk, &protocol_image)?;
    Ok(ReadonlyMediaObservation {
        snapshot,
        protocol_image,
    })
}
