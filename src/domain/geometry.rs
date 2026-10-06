//! Pure protocol geometry and filesystem observations; no acquisition or storage.
use crate::common::SECTOR;
use crate::protocol::crypto::{a6b0_full, crc32_bare, xor_rolling};
use crate::protocol::{
    edpf::{EdpPartitionType, EdpfEntry64, EdpfEntry96},
    lba7::Lba7PartitionMode,
};
use serde::Serialize;
fn u32le(raw: &[u8], offset: usize) -> Option<u32> {
    raw.get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}
pub const LBA7_COMPAT_EXTENT_SECTORS: u64 = 6;
pub const LBA7_COMPAT_EXTENT_BYTES: u64 = LBA7_COMPAT_EXTENT_SECTORS * SECTOR as u64;
pub const LBA7_COMPAT_CHS_TRACK_SECTORS: u64 = 16_065;
pub const LBA7_COMPAT_CHS_BACKOFF_SECTORS: u64 = 1_792;
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
