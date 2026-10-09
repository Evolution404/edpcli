//! Read-only collection of canonical media identity evidence.
//!
//! This module may probe hardware and read protocol sectors, but it deliberately has no path to
//! prepare/unmount a disk, reopen a handle read-write, or write a sector. Destructive callers must
//! separately apply their authorization policy and the existing write safety state machine.

use crate::common::{
    EdpCliError, EdpCliResult, EXIT_IO, METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR,
};
use crate::identify::{generate_candidates, identify};
use crate::platform::system;
use crate::platform::{HardwareProbe, NativeTransport};
use crate::ports::{CmdRunner, SectorDev};
use crate::provision::DiskProvisionKind;

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
                .is_none_or(|inquiry| !inquiry.has_model_identity())
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
                .is_none_or(|inquiry| !inquiry.has_model_identity())
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
        .then(|| crate::sha256::sha256_hex(lba4))
}

pub(crate) fn apply_runtime_plain_override(
    mut snapshot: MediaIdentitySnapshot,
    protocol_image: &[u8],
    total_sectors: u64,
    mut read_sector: impl FnMut(u64) -> Result<Vec<u8>, String>,
) -> MediaIdentitySnapshot {
    if snapshot
        .protocol
        .provision_kind
        .and_then(DiskProvisionKind::official_mode)
        .is_none()
        || protocol_image.len() != METADATA_IMAGE_LEN
        || total_sectors == 0
    {
        return snapshot;
    }

    let Ok(raw0) = <&[u8; SECTOR]>::try_from(&protocol_image[..SECTOR]) else {
        return snapshot;
    };
    let Ok(mbr) = crate::protocol::lba0::parse_lba0(raw0) else {
        return snapshot;
    };
    let partitions = mbr
        .partitions
        .iter()
        .filter(|partition| partition.partition_type != 0 && partition.sector_count != 0)
        .collect::<Vec<_>>();
    if partitions.is_empty()
        || partitions.iter().any(|partition| {
            matches!(partition.partition_type, 0x05 | 0x0f | 0x85 | 0xee)
                || u64::from(partition.start_lba) < 2_048
                || u64::from(partition.start_lba)
                    .checked_add(u64::from(partition.sector_count))
                    .is_none_or(|end| end > total_sectors)
        })
    {
        return snapshot;
    }

    for partition in partitions {
        let start = u64::from(partition.start_lba);
        let count = u64::from(partition.sector_count);
        let Ok(boot) = read_sector(start) else {
            return snapshot;
        };
        if crate::filesystem::detect_boot_sector(count, &boot)
            .ok()
            .flatten()
            .is_none()
        {
            return snapshot;
        }
    }

    snapshot.protocol = ProtocolIdentityEvidence {
        device_id: None,
        onlyid: None,
        provision_kind: Some(DiskProvisionKind::Plain),
        lba4_identity_digest: None,
    };
    snapshot
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
    let raw_serial = runner.hardware_serial(disk);
    let serial = serial_digest_evidence(raw_serial.as_deref());
    let retained_raw_serial = (serial.quality != super::media_identity::SerialQuality::Missing)
        .then_some(raw_serial)
        .flatten();
    let total_sectors = system::device_geometry(runner, disk)
        .and_then(|geometry| geometry.native_read_geometry().ok())
        .map(|geometry| geometry.native_sector_count)
        .or_else(|| system::disk_total_sectors(runner, disk));
    let hardware = HardwareIdentityEvidence {
        vid: probe.as_ref().and_then(|value| value.vid),
        pid: probe.as_ref().and_then(|value| value.pid),
        serial: retained_raw_serial,
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
        logical_sector_size: system::device_geometry(runner, disk)
            .and_then(|geometry| geometry.logical_sector_bytes),
    };

    let candidates = generate_candidates(runner, disk);
    let identified = identify(runner, disk, lba7);
    let onlyid = crate::infrastructure::backup_store::catalog::lba4_label_id_from(lba4);
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
        let native_size = system::device_geometry(runner, disk)
            .and_then(|geometry| geometry.native_read_geometry().ok())
            .map_or(SECTOR as u32, |geometry| geometry.logical_sector_bytes);
        let provision_kind = DiskProvisionKind::from_sectors_with_logical_size(
            &protocol_image[7 * SECTOR..8 * SECTOR],
            &protocol_image[12 * SECTOR..13 * SECTOR],
            &device_id,
            native_size,
        );
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
    } else if onlyid.is_none()
        && total_sectors.is_some_and(|total| {
            crate::partition_table::confirmed_plain_protocol_prefix(protocol_image, total)
        })
    {
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
/// Raw USB serial text is retained only in the in-memory snapshot so manifest v3 can persist the
/// reviewed identity field. Snapshot serialization and Debug output redact/omit the raw value.
pub fn observe_media_identity_readonly(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<ReadonlyMediaObservation> {
    let protocol_image = read_protocol_image_readonly(dev)?;
    let mut snapshot = media_identity_from_protocol_image(runner, disk, &protocol_image)?;
    if let Some(total_sectors) = snapshot.hardware.total_sectors {
        snapshot = apply_runtime_plain_override(snapshot, &protocol_image, total_sectors, |lba| {
            let lba = u32::try_from(lba)
                .map_err(|_| format!("Plain runtime evidence LBA{lba} exceeds u32"))?;
            dev.read_sector(lba)
                .map_err(|error| format!("read Plain runtime evidence LBA{lba}: {error}"))
        });
    }
    Ok(ReadonlyMediaObservation {
        snapshot,
        protocol_image,
    })
}
