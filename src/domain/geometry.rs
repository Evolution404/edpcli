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
    parse_partition_geometry_with_sector_bytes(lba0_12, device_id, total_sectors, SECTOR as u32)
}

/// Parse the fixed 512B LBA12 EDP payload against a separately observed
/// native logical sector size; never infer device geometry from ciphertext.
pub fn parse_partition_geometry_with_sector_bytes(
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
    logical_sector_bytes: u32,
) -> Result<Vec<PartitionGeometry>, String> {
    if !(512..=65_536).contains(&logical_sector_bytes) || !logical_sector_bytes.is_power_of_two() {
        return Err(format!("设备逻辑扇区大小 {logical_sector_bytes}B 无效"));
    }
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
        if entry.sector_size != u64::from(logical_sector_bytes) {
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
    parse_lba7_compatibility_geometry_with_sector_bytes(
        lba0_12,
        device_id,
        total_sectors,
        SECTOR as u32,
    )
}

/// The LCE pointer is in native LBAs, not fixed 512B units.
/// Round the 3072B legacy minimum up to whole native sectors, then require
/// LBA7 metadata to independently confirm the resulting size.
/// 512B and 4096B have real evidence; other sizes are read-only hypotheses.
pub fn parse_lba7_compatibility_geometry_with_sector_bytes(
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
    logical_sector_bytes: u32,
) -> Result<Lba7CompatibilityGeometry, String> {
    if !(512..=65_536).contains(&logical_sector_bytes) || !logical_sector_bytes.is_power_of_two() {
        return Err(format!("LCE 原生扇区大小 {logical_sector_bytes}B 非法"));
    }
    let native_bytes = u64::from(logical_sector_bytes);
    let extent_blocks = LBA7_COMPAT_EXTENT_BYTES.div_ceil(native_bytes);
    let extent_bytes = extent_blocks
        .checked_mul(native_bytes)
        .ok_or("LCE 原生长度溢出")?;
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
                && entry.sector_size == u64::from(logical_sector_bytes)
                && entry.partition_size == extent_bytes
        })
        .collect();
    if candidates.is_empty() {
        return Err(format!(
            "LBA7 中未发现与 {logical_sector_bytes}B 几何一致的 LCE 指针"
        ));
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
        .checked_add(extent_blocks)
        .ok_or_else(|| "LBA7 compatibility extent geometry overflow".to_string())?;
    if end > total_sectors {
        return Err(format!(
            "LBA7 compatibility extent exceeds source disk: start={start_lba}, end={end}, total={total_sectors}"
        ));
    }

    let chs_expected_start_lba = if logical_sector_bytes == 512 {
        let chs_aligned = (total_sectors / LBA7_COMPAT_CHS_TRACK_SECTORS)
            .checked_mul(LBA7_COMPAT_CHS_TRACK_SECTORS)
            .ok_or_else(|| "LBA7 compatibility extent CHS geometry overflow".to_string())?;
        chs_aligned.checked_sub(LBA7_COMPAT_CHS_BACKOFF_SECTORS)
    } else {
        None // Historical 512B CHS fallback is not valid for 4Kn blocks.
    };

    Ok(Lba7CompatibilityGeometry {
        start_lba,
        sector_count: extent_blocks,
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

#[cfg(test)]
mod logical_sector_compatibility_tests {
    use super::*;

    #[test]
    fn native_extent_alignment_adapts_to_simulated_logical_sector_sizes() {
        let did = "disk&ven_test&prod_native_size";
        for logical in [512u32, 1024, 2048, 4096, 8192] {
            let native_bytes = u64::from(logical);
            let count = LBA7_COMPAT_EXTENT_BYTES.div_ceil(native_bytes);
            let bytes = count * native_bytes;
            let mut plain = [0u8; SECTOR];
            for (i, kind) in [1u32, 2, 4].into_iter().enumerate() {
                let p = i * 64;
                plain[p..p + 4].copy_from_slice(b"EDPF");
                plain[p + 8..p + 12].copy_from_slice(&3u32.to_le_bytes());
                plain[p + 12..p + 16].copy_from_slice(&kind.to_le_bytes());
                plain[p + 24..p + 32]
                    .copy_from_slice(&(if i == 0 { 63u64 } else { 50_000u64 }).to_le_bytes());
                plain[p + 32..p + 40].copy_from_slice(&native_bytes.to_le_bytes());
                plain[p + 40..p + 48].copy_from_slice(
                    &(if i == 0 { 100 * native_bytes } else { bytes }).to_le_bytes(),
                );
            }
            let crc = crc32_bare(did.as_bytes());
            let cipher = xor_rolling(&plain, (crc & 0xffff) ^ (crc >> 16));
            let mut image = vec![0u8; 13 * SECTOR];
            image[7 * SECTOR..8 * SECTOR].copy_from_slice(&cipher);
            let geometry =
                parse_lba7_compatibility_geometry_with_sector_bytes(&image, did, 60_000, logical)
                    .expect("read-only native geometry");
            assert_eq!(geometry.start_lba, 50_000);
            assert_eq!(geometry.sector_count, count);
            assert_eq!(geometry.chs_expected_start_lba.is_some(), logical == 512);
            assert!(parse_lba7_compatibility_geometry_with_sector_bytes(
                &image,
                did,
                60_000,
                logical * 2
            )
            .is_err());
        }
    }

    #[test]
    fn four_kn_lce_pointer_is_one_full_native_sector_not_six_or_eight() {
        let did = "disk&ven_test&prod_4kn";
        let mut decoded = [0u8; SECTOR];
        for (idx, kind) in [1u32, 2, 4].into_iter().enumerate() {
            let at = idx * 64;
            decoded[at..at + 4].copy_from_slice(b"EDPF");
            decoded[at + 8..at + 12].copy_from_slice(&3u32.to_le_bytes());
            decoded[at + 12..at + 16].copy_from_slice(&kind.to_le_bytes());
            let lba = if idx == 0 { 63u64 } else { 62_476_561u64 };
            decoded[at + 24..at + 32].copy_from_slice(&lba.to_le_bytes());
            decoded[at + 32..at + 40].copy_from_slice(&4096u64.to_le_bytes());
            let bytes = if idx == 0 { 2497u64 * 4096 } else { 4096u64 };
            decoded[at + 40..at + 48].copy_from_slice(&bytes.to_le_bytes());
        }
        let crc = crc32_bare(did.as_bytes());
        let encrypted = xor_rolling(&decoded, (crc & 0xffff) ^ (crc >> 16));
        let mut image = vec![0u8; 13 * SECTOR];
        image[7 * SECTOR..8 * SECTOR].copy_from_slice(&encrypted);
        let parsed =
            parse_lba7_compatibility_geometry_with_sector_bytes(&image, did, 62_486_528, 4096)
                .expect("verified 4Kn LCE pointer");
        assert_eq!(parsed.start_lba, 62_476_561);
        assert_eq!(parsed.sector_count, 1);
        assert_eq!(parsed.lba7_pointer_entries.len(), 2);
        assert_eq!(parsed.chs_expected_start_lba, None);
        assert!(parse_lba7_compatibility_geometry(&image, did, 62_486_528).is_err());
        assert!(
            parse_lba7_compatibility_geometry_with_sector_bytes(&image, did, 62_486_528, 2048)
                .is_err()
        );
    }
}
