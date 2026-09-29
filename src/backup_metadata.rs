//! Metadata-level EDPB capture planning and read-only acquisition.
//!
//! This module never writes the source device. It derives partition geometry
//! from the current LBA12 EDPF table, captures bounded filesystem metadata
//! extents, and preserves identified historical tail backup structures.

use serde::Serialize;

use crate::common::SECTOR;
use crate::crypto::{a6b0_full, crc32_bare, xor_rolling};
use crate::diskio::SectorDev;
use crate::edpb::{
    ArtifactCompleteness, ArtifactInput, Derivation, Extent, ManifestPartition, Region,
    RestorePolicy, SemanticStatus,
};
use crate::partition_table::{PartitionSource, PartitionTableKind};
use crate::protocol::{
    edpf::{EdpPartitionType, EdpfEntry64, EdpfEntry96},
    lba7::Lba7PartitionMode,
};

pub const PARTITION_PREFIX_SECTORS: u64 = 64;
pub const PARTITION_SUFFIX_SECTORS: u64 = 8;
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
    pub partitions: Vec<ManifestPartition>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlainGptHeader {
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    disk_guid: [u8; 16],
    partition_entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
    partition_array_crc32: u32,
}

fn parse_plain_gpt_header(raw: &[u8], expected_current_lba: u64) -> Result<PlainGptHeader, String> {
    if raw.len() != SECTOR || raw.get(..8) != Some(b"EFI PART") {
        return Err(format!(
            "GPT header LBA{expected_current_lba} signature/length invalid"
        ));
    }
    let header_size = u32le(raw, 12).ok_or_else(|| "GPT header_size missing".to_string())? as usize;
    if !(92..=SECTOR).contains(&header_size) {
        return Err(format!(
            "GPT header LBA{expected_current_lba} header_size invalid"
        ));
    }
    let stored_crc = u32le(raw, 16).ok_or_else(|| "GPT header_crc32 missing".to_string())?;
    let mut crc_bytes = raw[..header_size].to_vec();
    crc_bytes[16..20].fill(0);
    if crate::protocol::lba1::crc32_ieee(&crc_bytes) != stored_crc {
        return Err(format!("GPT header LBA{expected_current_lba} CRC mismatch"));
    }
    let current_lba = u64le(raw, 24).ok_or_else(|| "GPT current_lba missing".to_string())?;
    if current_lba != expected_current_lba {
        return Err(format!(
            "GPT header current_lba={current_lba}, expected {expected_current_lba}"
        ));
    }
    Ok(PlainGptHeader {
        current_lba,
        backup_lba: u64le(raw, 32).ok_or_else(|| "GPT backup_lba missing".to_string())?,
        first_usable_lba: u64le(raw, 40)
            .ok_or_else(|| "GPT first_usable_lba missing".to_string())?,
        last_usable_lba: u64le(raw, 48).ok_or_else(|| "GPT last_usable_lba missing".to_string())?,
        disk_guid: raw[56..72]
            .try_into()
            .map_err(|_| "GPT disk_guid missing".to_string())?,
        partition_entries_lba: u64le(raw, 72)
            .ok_or_else(|| "GPT partition_entries_lba missing".to_string())?,
        entry_count: u32le(raw, 80).ok_or_else(|| "GPT entry_count missing".to_string())?,
        entry_size: u32le(raw, 84).ok_or_else(|| "GPT entry_size missing".to_string())?,
        partition_array_crc32: u32le(raw, 88)
            .ok_or_else(|| "GPT partition_array_crc32 missing".to_string())?,
    })
}

fn validate_plain_gpt_mirror(dev: &mut dyn SectorDev, total_sectors: u64) -> Result<(), String> {
    if total_sectors < 4 {
        return Err("GPT source disk is too small".into());
    }
    let primary_raw = read_extent(dev, 1, 1)?;
    let primary = parse_plain_gpt_header(&primary_raw, 1)?;
    if primary.backup_lba != total_sectors - 1 {
        return Err(format!(
            "GPT primary backup_lba={} conflicts with disk geometry {}",
            primary.backup_lba,
            total_sectors - 1
        ));
    }
    let entry_bytes = usize::try_from(primary.entry_count)
        .ok()
        .and_then(|count| count.checked_mul(primary.entry_size as usize))
        .ok_or_else(|| "GPT entry array length overflow".to_string())?;
    if entry_bytes == 0 || primary.entry_size != 128 {
        return Err("GPT entry array geometry unsupported".into());
    }
    let entry_sectors = entry_bytes.div_ceil(SECTOR) as u64;
    let mut primary_entries = read_extent(dev, primary.partition_entries_lba, entry_sectors)?;
    primary_entries.truncate(entry_bytes);
    if crate::protocol::lba1::crc32_ieee(&primary_entries) != primary.partition_array_crc32 {
        return Err("GPT primary partition array CRC mismatch".into());
    }

    let backup_raw = read_extent(dev, primary.backup_lba, 1)?;
    let backup = parse_plain_gpt_header(&backup_raw, primary.backup_lba)?;
    if backup.backup_lba != primary.current_lba
        || backup.first_usable_lba != primary.first_usable_lba
        || backup.last_usable_lba != primary.last_usable_lba
        || backup.disk_guid != primary.disk_guid
        || backup.entry_count != primary.entry_count
        || backup.entry_size != primary.entry_size
        || backup.partition_array_crc32 != primary.partition_array_crc32
    {
        return Err("GPT backup header geometry/CRC contract conflicts with primary".into());
    }
    if backup.partition_entries_lba + entry_sectors != backup.current_lba {
        return Err("GPT backup partition array is not adjacent to backup header".into());
    }
    let mut backup_entries = read_extent(dev, backup.partition_entries_lba, entry_sectors)?;
    backup_entries.truncate(entry_bytes);
    if crate::protocol::lba1::crc32_ieee(&backup_entries) != backup.partition_array_crc32 {
        return Err("GPT backup partition array CRC mismatch".into());
    }
    if backup_entries != primary_entries {
        return Err("GPT primary and backup partition arrays differ".into());
    }
    Ok(())
}

fn manifest_partition_from_plain(
    partition: &crate::partition_table::PhysicalPartition,
) -> ManifestPartition {
    let (role, partition_type, volume_label_hint) = match &partition.source {
        PartitionSource::Mbr {
            partition_type,
            primary_slot,
        } => (
            Some(if primary_slot.is_some() {
                "mbr_primary".to_string()
            } else {
                "mbr_logical".to_string()
            }),
            Some(format!("mbr:0x{partition_type:02X}")),
            None,
        ),
        PartitionSource::Gpt { type_guid, .. } => (
            Some("gpt_partition".to_string()),
            Some(format!(
                "gpt:{}",
                type_guid
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )),
            None,
        ),
    };
    ManifestPartition {
        index: partition.index as u32,
        role,
        partition_type,
        start_lba: partition.start_lba,
        sector_count: partition.sector_count,
        filesystem_hint: partition.filesystem.clone(),
        volume_label_hint,
    }
}

pub fn acquire_plain_metadata(
    dev: &mut dyn SectorDev,
    total_sectors: u64,
) -> Result<MetadataAcquisition, String> {
    let table = crate::partition_table::read_partition_table(total_sectors, |lba| {
        let lba = u32::try_from(lba)
            .map_err(|_| format!("partition-table LBA{lba} exceeds SectorDev u32 range"))?;
        dev.read_sector(lba)
            .map_err(|error| format!("read partition-table LBA{lba} failed: {error}"))
    })?;
    if table.kind == PartitionTableKind::Gpt {
        validate_plain_gpt_mirror(dev, total_sectors)?;
    }

    let mut out = MetadataAcquisition {
        partitions: table
            .partitions
            .iter()
            .map(manifest_partition_from_plain)
            .collect(),
        ..MetadataAcquisition::default()
    };
    let region_id = "region.plain.partition_table";
    out.regions.push(Region {
        id: region_id.into(),
        role: "plain_partition_table".into(),
        start_lba: None,
        sector_count: None,
        semantic_status: SemanticStatus::Identified,
    });
    for (index, extent) in table.table_extents.iter().enumerate() {
        let data = read_extent(dev, extent.start_lba, extent.sector_count)?;
        let extent_id = format!("extent.plain.partition_table.{index}");
        out.extents.push(Extent {
            id: extent_id.clone(),
            region_id: region_id.into(),
            start_lba: extent.start_lba,
            sector_count: extent.sector_count,
            purpose: extent.label.clone(),
        });
        out.artifacts.push(ArtifactInput {
            id: format!("raw.plain.partition_table.{index}"),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            data,
        });
    }
    out.notes.push(match table.kind {
        PartitionTableKind::Mbr => "Plain MBR metadata only; filesystem/user data excluded".into(),
        PartitionTableKind::Gpt => {
            "Plain GPT primary/backup metadata only; filesystem/user data excluded".into()
        }
    });
    Ok(out)
}

fn manifest_partition_from_edp(partition: &PartitionGeometry) -> ManifestPartition {
    let role = match partition.partition_type {
        1 => "boot",
        2 => "share",
        4 => "encrypt",
        _ => "unknown",
    };
    ManifestPartition {
        index: (partition.index + 1) as u32,
        role: Some(role.into()),
        partition_type: Some(format!("edp:{}", partition.partition_type)),
        start_lba: partition.start_sector,
        sector_count: partition.sector_count,
        filesystem_hint: None,
        volume_label_hint: None,
    }
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
            id: region_id,
            role: format!("partition.type{}", partition.partition_type),
            start_lba: Some(partition.start_sector),
            sector_count: Some(partition.sector_count),
            semantic_status: SemanticStatus::Identified,
        });
        out.partitions.push(manifest_partition_from_edp(partition));
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
            // This extent is not merely forensic: LBA7 points to it as active protocol
            // state, and official provisioning may rewrite it. Once the pointer geometry has
            // passed parse_lba7_compatibility_geometry(), keep the exact ciphertext restorable
            // so a metadata restore can roll the protocol back coherently with LBA0-12.
            if raw.is_some() {
                let artifact = out
                    .artifacts
                    .iter_mut()
                    .find(|artifact| artifact.id == "raw.lba7_compatibility")
                    .expect("captured compatibility artifact must exist");
                artifact.restore_policy = RestorePolicy::Restorable;
            }
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
        let raw = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.metadata_mirror_512k".into(),
            "raw.tail.metadata_mirror_512k".into(),
            mirror_start,
            TAIL_METADATA_MIRROR_SECTORS,
            "historical_lba4_lba12_mirror",
        )?;
        if raw.is_some() {
            out.artifacts
                .iter_mut()
                .find(|artifact| artifact.id == "raw.tail.metadata_mirror_512k")
                .expect("captured tail metadata mirror artifact")
                .restore_policy = RestorePolicy::Restorable;
        }
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
        let raw = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.restore_node_end4".into(),
            "raw.tail.restore_node_end4".into(),
            mirror_start,
            1,
            "historical_restore_node_mirror",
        )?;
        if raw.is_some() {
            out.artifacts
                .iter_mut()
                .find(|artifact| artifact.id == "raw.tail.restore_node_end4")
                .expect("captured tail restore-node artifact")
                .restore_policy = RestorePolicy::Restorable;
        }
    }

    out.notes.push(
        "EDP metadata-only capture: filesystem boot/FAT/directory/user payload excluded".into(),
    );
    out.notes.push(
        "LBA7 compatibility extent and identified historical tail recovery structures are captured as independent restorable protocol extents"
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
