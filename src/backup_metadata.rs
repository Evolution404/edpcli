//! Metadata-level EDPB capture planning and read-only acquisition.
//!
//! This module never writes the source device. It derives partition geometry
//! from the current LBA12 EDPF table, captures bounded filesystem metadata
//! extents, and preserves the known/unknown tail areas as evidence-only
//! artifacts.

use serde::Serialize;

use crate::common::SECTOR;
use crate::crypto::{a6b0_full, crc32_bare, xor_rolling};
use crate::diskio::SectorDev;
use crate::edpb::{
    ArtifactCompleteness, ArtifactInput, Derivation, Extent, Region, RestorePolicy, SemanticStatus,
};
use crate::protocol::{
    edpf::{EdpPartitionType, EdpfEntry64, EdpfEntry96},
    lba7::Lba7PartitionMode,
};

pub const PARTITION_PREFIX_SECTORS: u64 = 64;
pub const PARTITION_SUFFIX_SECTORS: u64 = 8;
pub const DEVICE_TAIL_WINDOW_SECTORS: u64 = 2048;
pub const LBA7_COMPAT_EXTENT_SECTORS: u64 = 6;
pub const LBA7_COMPAT_EXTENT_BYTES: u64 = LBA7_COMPAT_EXTENT_SECTORS * SECTOR as u64;
pub const LBA7_COMPAT_CHS_TRACK_SECTORS: u64 = 16_065;
pub const LBA7_COMPAT_CHS_BACKOFF_SECTORS: u64 = 1_792;
pub const TAIL_METADATA_MIRROR_OFFSET_SECTORS: u64 = 1024;
pub const TAIL_METADATA_MIRROR_SECTORS: u64 = 9;
pub const TAIL_END4_MIRROR_OFFSET_SECTORS: u64 = 4;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PartitionGeometry {
    pub index: usize,
    pub partition_type: u32,
    pub partition_count: u32,
    pub need_disturb: u32,
    pub need_encrypt: u32,
    pub start_sector: u64,
    pub sector_size: u64,
    pub partition_size: u64,
    pub sector_count: u64,
    pub user_key_crc: u32,
    pub file_key_crc: u32,
    pub encrypt_mode: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lba7CompatibilityPointer {
    pub entry_index: usize,
    pub partition_type: u32,
    pub partition_role: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lba7CompatibilityGeometry {
    pub start_lba: u64,
    pub sector_count: u64,
    pub lba7_pointer_entries: Vec<Lba7CompatibilityPointer>,
    pub official_partition_mode: Option<String>,
    pub chs_expected_start_lba: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemKind {
    Ntfs,
    Exfat,
    Fat32,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FilesystemProbe {
    pub partition_index: usize,
    pub partition_type: u32,
    pub kind: FilesystemKind,
    pub bytes_per_sector: Option<u32>,
    pub sectors_per_cluster: Option<u32>,
    pub key_lbas: Vec<u64>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct MetadataAcquisition {
    pub regions: Vec<Region>,
    pub extents: Vec<Extent>,
    pub artifacts: Vec<ArtifactInput>,
    pub notes: Vec<String>,
    pub issues: Vec<CaptureIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureIssue {
    pub region_id: String,
    pub start_lba: u64,
    pub sector_count: u64,
    pub error: String,
}

fn u16le(raw: &[u8], offset: usize) -> Option<u16> {
    raw.get(offset..offset + 2)
        .and_then(|v| v.try_into().ok())
        .map(u16::from_le_bytes)
}

fn u32le(raw: &[u8], offset: usize) -> Option<u32> {
    raw.get(offset..offset + 4)
        .and_then(|v| v.try_into().ok())
        .map(u32::from_le_bytes)
}

fn u64le(raw: &[u8], offset: usize) -> Option<u64> {
    raw.get(offset..offset + 8)
        .and_then(|v| v.try_into().ok())
        .map(u64::from_le_bytes)
}

fn read_extent(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
) -> Result<Vec<u8>, String> {
    let byte_len = sector_count
        .checked_mul(SECTOR as u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| "metadata extent byte length overflow".to_string())?;
    let mut out = Vec::with_capacity(byte_len);
    for offset in 0..sector_count {
        let lba = start_lba
            .checked_add(offset)
            .ok_or_else(|| "metadata extent LBA overflow".to_string())?;
        let lba32 = u32::try_from(lba)
            .map_err(|_| format!("metadata extent LBA {lba} exceeds SectorDev u32 range"))?;
        let sector = dev
            .read_sector(lba32)
            .map_err(|e| format!("read metadata LBA{lba} failed: {e}"))?;
        if sector.len() != SECTOR {
            return Err(format!(
                "read metadata LBA{lba} returned {}B, expected {SECTOR}B",
                sector.len()
            ));
        }
        out.extend_from_slice(&sector);
    }
    Ok(out)
}

fn extent_id(partition_index: usize, name: &str) -> String {
    format!("extent.partition.{partition_index}.{name}")
}

fn artifact_id(partition_index: usize, name: &str) -> String {
    format!("raw.partition.{partition_index}.{name}")
}

// Mirrors one raw-evidence record into the extent/artifact model; keeping the
// fields explicit makes call sites auditable against the on-disk capture schema.
#[allow(clippy::too_many_arguments)]
fn add_raw_extent(
    out: &mut MetadataAcquisition,
    dev: &mut dyn SectorDev,
    region_id: &str,
    extent_id: String,
    artifact_id: String,
    start_lba: u64,
    sector_count: u64,
    purpose: &str,
) -> Result<Option<Vec<u8>>, String> {
    if sector_count == 0 {
        return Ok(Some(Vec::new()));
    }
    let data = match read_extent(dev, start_lba, sector_count) {
        Ok(data) => data,
        Err(error) => {
            out.issues.push(CaptureIssue {
                region_id: region_id.to_string(),
                start_lba,
                sector_count,
                error,
            });
            return Ok(None);
        }
    };
    out.extents.push(Extent {
        id: extent_id.clone(),
        region_id: region_id.to_string(),
        start_lba,
        sector_count,
        purpose: purpose.to_string(),
    });
    out.artifacts.push(ArtifactInput {
        id: artifact_id,
        kind: "raw_sectors".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec![extent_id],
        derivation: None,
        restore_policy: RestorePolicy::EvidenceOnly,
        completeness: ArtifactCompleteness::Complete,
        data: data.clone(),
    });
    Ok(Some(data))
}

pub fn parse_partition_geometry(
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
) -> Result<Vec<PartitionGeometry>, String> {
    if lba0_12.len() != 13 * SECTOR {
        return Err(format!(
            "metadata planning requires 6656B LBA0-12, got {}B",
            lba0_12.len()
        ));
    }
    let lba12: &[u8; SECTOR] = lba0_12[12 * SECTOR..13 * SECTOR]
        .try_into()
        .expect("slice length checked");
    let device_crc = crc32_bare(device_id.as_bytes());
    let plain = a6b0_full(lba12, &device_crc.to_le_bytes(), 0);
    if plain.get(..4) != Some(b"EDPF") {
        return Err("LBA12 does not decode to EDPF with current device_id".into());
    }
    let count = u32le(&plain, 0x08).ok_or("LBA12 partition_count missing")? as usize;
    if !(1..=3).contains(&count) {
        return Err(format!("unsupported LBA12 partition_count {count}"));
    }

    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let base = index * 0x60;
        let raw: &[u8; 0x60] = plain[base..base + 0x60]
            .try_into()
            .expect("EDPF entry bounds");
        let entry =
            EdpfEntry96::parse(raw).map_err(|e| format!("parse LBA12 entry{index}: {e}"))?;
        if entry.sector_size != SECTOR as u64 {
            return Err(format!(
                "LBA12 entry{index} sector_size={} is unsupported",
                entry.sector_size
            ));
        }
        if entry.partition_size % entry.sector_size != 0 {
            return Err(format!(
                "LBA12 entry{index} partition_size is not sector aligned"
            ));
        }
        let sector_count = entry.partition_size / entry.sector_size;
        if sector_count == 0 {
            return Err(format!("LBA12 entry{index} has zero partition size"));
        }
        let end = entry
            .start_sector
            .checked_add(sector_count)
            .ok_or_else(|| format!("LBA12 entry{index} geometry overflow"))?;
        if end > total_sectors {
            return Err(format!(
                "LBA12 entry{index} exceeds source disk: end={end}, total={total_sectors}"
            ));
        }
        out.push(PartitionGeometry {
            index,
            partition_type: entry.partition_type,
            partition_count: entry.partition_count,
            need_disturb: entry.need_disturb,
            need_encrypt: entry.need_encrypt,
            start_sector: entry.start_sector,
            sector_size: entry.sector_size,
            partition_size: entry.partition_size,
            sector_count,
            user_key_crc: entry.user_key_crc,
            file_key_crc: entry.file_key_crc,
            encrypt_mode: entry.encrypt_mode,
        });
    }
    Ok(out)
}

fn parse_lba7_legacy_entries(
    lba0_12: &[u8],
    device_id: &str,
) -> Result<Vec<(usize, EdpfEntry64)>, String> {
    if lba0_12.len() != 13 * SECTOR {
        return Err(format!(
            "LBA7 compatibility extent planning requires 6656B LBA0-12, got {}B",
            lba0_12.len()
        ));
    }
    let lba7: &[u8; SECTOR] = lba0_12[7 * SECTOR..8 * SECTOR]
        .try_into()
        .expect("slice length checked");
    let device_crc = crc32_bare(device_id.as_bytes());
    let k0 = (device_crc & 0xffff) ^ (device_crc >> 16);
    let plain = xor_rolling(lba7, k0);

    let mut entries = Vec::new();
    for index in 0..3usize {
        let base = index * 0x40;
        let raw: &[u8; 0x40] = plain[base..base + 0x40]
            .try_into()
            .expect("LBA7 entry bounds");
        if &raw[..4] != b"EDPF" {
            if index < 2 {
                return Err(format!("LBA7 entry{index} missing EDPF magic"));
            }
            break;
        }
        let entry = EdpfEntry64::parse(raw).map_err(|e| format!("parse LBA7 entry{index}: {e}"))?;
        entries.push((index, entry));
    }
    Ok(entries)
}

pub fn parse_lba7_compatibility_geometry(
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
) -> Result<Lba7CompatibilityGeometry, String> {
    let entries = parse_lba7_legacy_entries(lba0_12, device_id)?;
    let partition_types = entries
        .iter()
        .map(|(_, entry)| entry.partition_type)
        .collect::<Vec<_>>();
    let official_partition_mode = Lba7PartitionMode::from_partition_types(&partition_types)
        .map(|mode| format!("{} ({})", mode as u8, mode.ui_name_zh()));
    let candidates: Vec<(usize, EdpfEntry64)> = entries
        .into_iter()
        .filter(|(index, entry)| {
            *index > 0
                && entry.sector_size == SECTOR as u64
                && entry.partition_size == LBA7_COMPAT_EXTENT_BYTES
        })
        .collect();
    if candidates.is_empty() {
        return Err("LBA7 has no 3072-byte legacy compatibility extent pointer entry".into());
    }

    let start_lba = candidates[0].1.start_sector;
    if candidates
        .iter()
        .any(|(_, entry)| entry.start_sector != start_lba)
    {
        return Err(format!(
            "LBA7 3072-byte compatibility extent pointers disagree: {}",
            candidates
                .iter()
                .map(|(index, entry)| format!(
                    "entry{index}={} type={}",
                    entry.start_sector, entry.partition_type
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let end = start_lba
        .checked_add(LBA7_COMPAT_EXTENT_SECTORS)
        .ok_or_else(|| "LBA7 compatibility extent geometry overflow".to_string())?;
    if end > total_sectors {
        return Err(format!(
            "LBA7 compatibility extent exceeds source disk: start={start_lba}, end={end}, total={total_sectors}"
        ));
    }

    let chs_aligned = (total_sectors / LBA7_COMPAT_CHS_TRACK_SECTORS)
        .checked_mul(LBA7_COMPAT_CHS_TRACK_SECTORS)
        .ok_or_else(|| "LBA7 compatibility extent CHS geometry overflow".to_string())?;
    let chs_expected_start_lba = chs_aligned.checked_sub(LBA7_COMPAT_CHS_BACKOFF_SECTORS);

    Ok(Lba7CompatibilityGeometry {
        start_lba,
        sector_count: LBA7_COMPAT_EXTENT_SECTORS,
        lba7_pointer_entries: candidates
            .iter()
            .map(|(entry_index, entry)| Lba7CompatibilityPointer {
                entry_index: *entry_index,
                partition_type: entry.partition_type,
                partition_role: EdpPartitionType::from_raw(entry.partition_type)
                    .map(|partition_type| partition_type.role().to_string()),
            })
            .collect(),
        official_partition_mode,
        chs_expected_start_lba,
    })
}

pub(crate) fn probe_filesystem(partition: &PartitionGeometry, prefix: &[u8]) -> FilesystemProbe {
    let mut probe = FilesystemProbe {
        partition_index: partition.index,
        partition_type: partition.partition_type,
        kind: FilesystemKind::Unknown,
        bytes_per_sector: None,
        sectors_per_cluster: None,
        key_lbas: Vec::new(),
        notes: Vec::new(),
    };
    if prefix.len() < SECTOR {
        probe
            .notes
            .push("partition prefix is shorter than one sector".into());
        return probe;
    }
    let boot = &prefix[..SECTOR];

    if boot.get(3..11) == Some(b"NTFS    ") {
        let Some(bps) = u16le(boot, 11).map(u32::from) else {
            return probe;
        };
        let spc = boot[13] as u32;
        if bps == SECTOR as u32 && spc != 0 {
            probe.kind = FilesystemKind::Ntfs;
            probe.bytes_per_sector = Some(bps);
            probe.sectors_per_cluster = Some(spc);
            if let Some(mft_lcn) = u64le(boot, 48) {
                if let Some(lba) = mft_lcn
                    .checked_mul(spc as u64)
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                    probe.notes.push(format!("NTFS $MFT LCN={mft_lcn}"));
                }
            }
            if let Some(mftmirr_lcn) = u64le(boot, 56) {
                if let Some(lba) = mftmirr_lcn
                    .checked_mul(spc as u64)
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                    probe.notes.push(format!("NTFS $MFTMirr LCN={mftmirr_lcn}"));
                }
            }
            probe.key_lbas.push(
                partition
                    .start_sector
                    .saturating_add(partition.sector_count.saturating_sub(1)),
            );
        }
        return probe;
    }

    if boot.get(3..11) == Some(b"EXFAT   ") {
        let bps_shift = boot[108];
        let spc_shift = boot[109];
        if bps_shift < 32 && spc_shift < 32 {
            let bps = 1u32.checked_shl(bps_shift as u32).unwrap_or(0);
            let spc = 1u32.checked_shl(spc_shift as u32).unwrap_or(0);
            if bps == SECTOR as u32 && spc != 0 {
                probe.kind = FilesystemKind::Exfat;
                probe.bytes_per_sector = Some(bps);
                probe.sectors_per_cluster = Some(spc);
                let fat_offset = u32le(boot, 80).unwrap_or(0) as u64;
                let heap_offset = u32le(boot, 88).unwrap_or(0) as u64;
                let root_cluster = u32le(boot, 96).unwrap_or(0) as u64;
                if fat_offset != 0 {
                    if let Some(lba) = partition.start_sector.checked_add(fat_offset) {
                        probe.key_lbas.push(lba);
                    }
                    probe.notes.push(format!("exFAT FAT offset={fat_offset}"));
                }
                if root_cluster >= 2 {
                    if let Some(lba) = (root_cluster - 2)
                        .checked_mul(spc as u64)
                        .and_then(|v| heap_offset.checked_add(v))
                        .and_then(|rel| partition.start_sector.checked_add(rel))
                    {
                        probe.key_lbas.push(lba);
                        probe
                            .notes
                            .push(format!("exFAT root cluster={root_cluster}"));
                    }
                }
                if let Some(lba) = partition.start_sector.checked_add(12) {
                    probe.key_lbas.push(lba);
                }
            }
        }
        return probe;
    }

    if boot.get(82..90) == Some(b"FAT32   ") {
        let bps = u16le(boot, 11).unwrap_or(0) as u32;
        let spc = boot[13] as u32;
        let reserved = u16le(boot, 14).unwrap_or(0) as u64;
        let fats = boot[16] as u64;
        let fat_size = u32le(boot, 36).unwrap_or(0) as u64;
        let root_cluster = u32le(boot, 44).unwrap_or(0) as u64;
        if bps == SECTOR as u32 && spc != 0 && fat_size != 0 {
            probe.kind = FilesystemKind::Fat32;
            probe.bytes_per_sector = Some(bps);
            probe.sectors_per_cluster = Some(spc);
            if let Some(lba) = partition.start_sector.checked_add(reserved) {
                probe.key_lbas.push(lba);
            }
            let data_start = reserved.saturating_add(fats.saturating_mul(fat_size));
            if root_cluster >= 2 {
                if let Some(lba) = (root_cluster - 2)
                    .checked_mul(spc as u64)
                    .and_then(|v| data_start.checked_add(v))
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                }
            }
            if let Some(fsinfo) = u16le(boot, 48) {
                if let Some(lba) = partition.start_sector.checked_add(fsinfo as u64) {
                    probe.key_lbas.push(lba);
                }
            }
            if let Some(backup_boot) = u16le(boot, 50) {
                if backup_boot != 0 {
                    if let Some(lba) = partition.start_sector.checked_add(backup_boot as u64) {
                        probe.key_lbas.push(lba);
                    }
                }
            }
        }
    }
    probe
}

fn add_key_sector(
    out: &mut MetadataAcquisition,
    dev: &mut dyn SectorDev,
    partition: &PartitionGeometry,
    region_id: &str,
    ordinal: usize,
    lba: u64,
) -> Result<(), String> {
    let partition_end = partition
        .start_sector
        .checked_add(partition.sector_count)
        .ok_or_else(|| format!("partition {} end overflow", partition.index))?;
    if lba < partition.start_sector || lba >= partition_end {
        out.notes.push(format!(
            "filesystem key LBA {lba} falls outside partition {} and was not captured",
            partition.index
        ));
        return Ok(());
    }
    let _ = add_raw_extent(
        out,
        dev,
        region_id,
        extent_id(partition.index, &format!("fskey{ordinal}")),
        artifact_id(partition.index, &format!("fskey{ordinal}")),
        lba,
        1,
        "filesystem_key_sector",
    )?;
    Ok(())
}

pub fn acquire_metadata(
    dev: &mut dyn SectorDev,
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
) -> Result<MetadataAcquisition, String> {
    if total_sectors < 13 {
        return Err("source disk is smaller than protocol region".into());
    }
    let partitions = parse_partition_geometry(lba0_12, device_id, total_sectors)?;
    let mut out = MetadataAcquisition::default();

    let topology_json = serde_json::to_vec_pretty(&partitions)
        .map_err(|e| format!("serialize partition topology failed: {e}"))?;
    out.artifacts.push(ArtifactInput {
        id: "derived.partition_topology".into(),
        kind: "partition_topology".into(),
        media_type: "application/json".into(),
        source_extent_ids: vec!["extent.protocol.lba0_12".into()],
        derivation: Some(Derivation {
            method: "lba12_edpf_decode_v1".into(),
            source_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        }),
        restore_policy: RestorePolicy::DerivedOnly,
        completeness: ArtifactCompleteness::Complete,
        data: topology_json,
    });

    for partition in &partitions {
        let region_id = format!(
            "region.partition.{}.type{}",
            partition.index, partition.partition_type
        );
        out.regions.push(Region {
            id: region_id.clone(),
            role: format!("partition.type{}", partition.partition_type),
            start_lba: Some(partition.start_sector),
            sector_count: Some(partition.sector_count),
            semantic_status: SemanticStatus::Identified,
        });

        let prefix_count = partition.sector_count.min(PARTITION_PREFIX_SECTORS);
        let prefix_start = partition.start_sector;
        let prefix_extent_id = extent_id(partition.index, "prefix");
        let prefix_artifact_id = artifact_id(partition.index, "prefix");
        let prefix = add_raw_extent(
            &mut out,
            dev,
            &region_id,
            prefix_extent_id.clone(),
            prefix_artifact_id.clone(),
            prefix_start,
            prefix_count,
            "partition_metadata_prefix",
        )?;

        if partition.sector_count > prefix_count {
            let suffix_count = PARTITION_SUFFIX_SECTORS.min(partition.sector_count - prefix_count);
            if suffix_count > 0 {
                let suffix_start = partition.start_sector + partition.sector_count - suffix_count;
                let _ = add_raw_extent(
                    &mut out,
                    dev,
                    &region_id,
                    extent_id(partition.index, "suffix"),
                    artifact_id(partition.index, "suffix"),
                    suffix_start,
                    suffix_count,
                    "partition_metadata_suffix",
                )?;
            }
        }

        let probe = probe_filesystem(partition, prefix.as_deref().unwrap_or(&[]));
        let mut seen = std::collections::BTreeSet::new();
        for (ordinal, lba) in probe
            .key_lbas
            .iter()
            .copied()
            .filter(|lba| seen.insert(*lba))
            .enumerate()
        {
            add_key_sector(&mut out, dev, partition, &region_id, ordinal, lba)?;
        }
        if prefix.is_some() {
            let probe_json = serde_json::to_vec_pretty(&probe)
                .map_err(|e| format!("serialize filesystem probe failed: {e}"))?;
            out.artifacts.push(ArtifactInput {
                id: format!("derived.partition.{}.filesystem_probe", partition.index),
                kind: "filesystem_probe".into(),
                media_type: "application/json".into(),
                source_extent_ids: vec![prefix_extent_id],
                derivation: Some(Derivation {
                    method: "filesystem_boot_probe_v1".into(),
                    source_artifact_ids: vec![prefix_artifact_id],
                }),
                restore_policy: RestorePolicy::DerivedOnly,
                completeness: ArtifactCompleteness::Complete,
                data: probe_json,
            });
        }
    }

    match parse_lba7_compatibility_geometry(lba0_12, device_id, total_sectors) {
        Ok(compat) => {
            let region_id = "region.lba7_compatibility_extent";
            out.regions.push(Region {
                id: region_id.into(),
                role: "lba7_legacy_partition_compatibility_extent".into(),
                start_lba: Some(compat.start_lba),
                sector_count: Some(compat.sector_count),
                semantic_status: SemanticStatus::Identified,
            });
            let raw = add_raw_extent(
                &mut out,
                dev,
                region_id,
                "extent.lba7_compatibility".into(),
                "raw.lba7_compatibility".into(),
                compat.start_lba,
                compat.sector_count,
                "lba7_compatibility_extent_ciphertext",
            )?;
            if let Some(expected) = compat.chs_expected_start_lba {
                if expected != compat.start_lba {
                    out.issues.push(CaptureIssue {
                        region_id: region_id.into(),
                        start_lba: compat.start_lba,
                        sector_count: compat.sector_count,
                        error: format!(
                            "LBA7 compatibility extent pointer {} differs from CHS-1792 expected {}; LBA7 pointer preserved as authoritative",
                            compat.start_lba, expected
                        ),
                    });
                }
            }
            if raw.is_some() {
                let layout = serde_json::json!({
                    "total_size": LBA7_COMPAT_EXTENT_BYTES,
                    "source": "lba7_edp_partion_info",
                    "pointer_entries": compat.lba7_pointer_entries,
                    "official_partition_mode": compat.official_partition_mode,
                    "producer_rule": "CreatePartitions preserves PartionType but rewrites legacy entries after entry0 to the same aligned 0xC00 compatibility extent",
                    "wire_semantics": {
                        "offset": 0,
                        "length": LBA7_COMPAT_EXTENT_BYTES,
                        "classification": "fixed_fat16_compatibility_image_encrypted",
                        "plaintext_sha256": "386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c",
                        "crypto": "EDPSECDISK zero8 + physical backing byte-offset tweak"
                    }
                });
                out.artifacts.push(ArtifactInput {
                    id: "derived.lba7_compatibility.layout".into(),
                    kind: "lba7_compatibility_layout".into(),
                    media_type: "application/json".into(),
                    source_extent_ids: vec!["extent.lba7_compatibility".into()],
                    derivation: Some(Derivation {
                        method: "lba7_compatibility_layout_v1".into(),
                        source_artifact_ids: vec!["raw.lba7_compatibility".into()],
                    }),
                    restore_policy: RestorePolicy::DerivedOnly,
                    completeness: ArtifactCompleteness::Complete,
                    data: serde_json::to_vec_pretty(&layout).map_err(|e| {
                        format!("serialize LBA7 compatibility extent layout failed: {e}")
                    })?,
                });
            }
        }
        Err(error) => {
            let fallback_start = (total_sectors / LBA7_COMPAT_CHS_TRACK_SECTORS)
                .checked_mul(LBA7_COMPAT_CHS_TRACK_SECTORS)
                .and_then(|aligned| aligned.checked_sub(LBA7_COMPAT_CHS_BACKOFF_SECTORS))
                .unwrap_or(0);
            out.issues.push(CaptureIssue {
                region_id: "region.lba7_compatibility_extent".into(),
                start_lba: fallback_start,
                sector_count: LBA7_COMPAT_EXTENT_SECTORS,
                error: format!(
                    "LBA7 compatibility extent was not captured because LBA7 pointer validation failed: {error}"
                ),
            });
        }
    }

    let tail_count = total_sectors.min(DEVICE_TAIL_WINDOW_SECTORS);
    let tail_start = total_sectors - tail_count;
    let tail_region = "region.device_tail_window";
    out.regions.push(Region {
        id: tail_region.into(),
        role: "forensic_tail_window".into(),
        start_lba: Some(tail_start),
        sector_count: Some(tail_count),
        semantic_status: SemanticStatus::Unknown,
    });
    let _ = add_raw_extent(
        &mut out,
        dev,
        tail_region,
        "extent.device_tail_window".into(),
        "raw.device_tail_window".into(),
        tail_start,
        tail_count,
        "forensic_tail_evidence_window",
    )?;

    if total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS + TAIL_METADATA_MIRROR_SECTORS {
        let mirror_start = total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS;
        let region_id = "region.tail.metadata_mirror_512k";
        out.regions.push(Region {
            id: region_id.into(),
            role: "lba4_lba12_backup_mirror".into(),
            start_lba: Some(mirror_start),
            sector_count: Some(TAIL_METADATA_MIRROR_SECTORS),
            semantic_status: SemanticStatus::Identified,
        });
        let _ = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.metadata_mirror_512k".into(),
            "raw.tail.metadata_mirror_512k".into(),
            mirror_start,
            TAIL_METADATA_MIRROR_SECTORS,
            "historical_lba4_lba12_mirror",
        )?;
    }

    if total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS {
        let mirror_start = total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS;
        let region_id = "region.tail.restore_node_end4";
        out.regions.push(Region {
            id: region_id.into(),
            role: "historical_restore_node_mirror".into(),
            start_lba: Some(mirror_start),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        });
        let _ = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.restore_node_end4".into(),
            "raw.tail.restore_node_end4".into(),
            mirror_start,
            1,
            "historical_restore_node_mirror",
        )?;
    }

    out.notes.push(format!(
        "metadata capture policy: partition prefix={} sectors, suffix={} sectors, device tail window={} sectors",
        PARTITION_PREFIX_SECTORS, PARTITION_SUFFIX_SECTORS, tail_count
    ));
    out.notes.push(
        "LBA7 compatibility extent is the LBA7-pointed six-sector compatibility block; device tail window is separate forensic evidence"
            .into(),
    );
    if !out.issues.is_empty() {
        let issue_json = serde_json::to_vec_pretty(&out.issues)
            .map_err(|e| format!("serialize metadata capture issues failed: {e}"))?;
        out.artifacts.push(ArtifactInput {
            id: "derived.capture_issues".into(),
            kind: "capture_issues".into(),
            media_type: "application/json".into(),
            source_extent_ids: Vec::new(),
            derivation: None,
            restore_policy: RestorePolicy::DerivedOnly,
            completeness: ArtifactCompleteness::Complete,
            data: issue_json,
        });
        out.notes.push(format!(
            "metadata capture completed with {} unreadable extent(s); no missing bytes were zero-filled",
            out.issues.len()
        ));
    }
    Ok(out)
}
