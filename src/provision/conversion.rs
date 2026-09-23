//! Pure existing-media conversion into the verified passwordless type2+type4 layout.
//!
//! This module never opens or writes a device. It preserves the source type4
//! geometry and key material, rebuilds only the front plaintext filesystem, and
//! returns the exact metadata sectors that a later application-layer writer may
//! commit after backup and target revalidation.

use crate::{
    common::SECTOR,
    crypto::{a6b0_full, a7f0_full, crc32_bare, xor_rolling},
    protocol::edpf::{EdpfEntry64, EdpfEntry96},
};

use super::{build_empty_exfat, ProvisionImage, SparseFilesystemImage};

const SHARE_START: u64 = 63;
const E7: usize = 0x40;
const E12: usize = 0x60;
const MAX_ENTRIES: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordlessConversionPlan {
    pub front_start_lba: u64,
    pub front_sectors: u64,
    pub encrypt_start_lba: u64,
    pub encrypt_size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasswordlessConversionImage {
    pub plan: PasswordlessConversionPlan,
    pub lba0: [u8; SECTOR],
    pub lba7: [u8; SECTOR],
    pub lba12: [u8; SECTOR],
    pub front_filesystem: SparseFilesystemImage,
}

fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn entry_count(raw: &[u8], stride: usize) -> Result<usize, String> {
    if raw.len() < stride || &raw[..4] != b"EDPF" {
        return Err("source EDPF table has no valid first entry".into());
    }
    let count = u32::from_le_bytes(raw[0x08..0x0c].try_into().unwrap()) as usize;
    if !(1..=MAX_ENTRIES).contains(&count) {
        return Err(format!("source EDPF partition count is invalid: {count}"));
    }
    Ok(count)
}

fn find_type4_96(raw: &[u8]) -> Result<(usize, [u8; E12], EdpfEntry96), String> {
    let count = entry_count(raw, E12)?;
    for index in 0..count {
        let base = index * E12;
        let bytes: [u8; E12] = raw[base..base + E12].try_into().unwrap();
        let entry = EdpfEntry96::parse(&bytes).map_err(|e| e.to_string())?;
        if entry.partition_type == 4 {
            return Ok((index, bytes, entry));
        }
    }
    Err("source LBA12 does not contain a type4 partition".into())
}

fn find_type4_64(raw: &[u8]) -> Result<(usize, [u8; E7], EdpfEntry64), String> {
    let count = entry_count(raw, E7)?;
    for index in 0..count {
        let base = index * E7;
        let bytes: [u8; E7] = raw[base..base + E7].try_into().unwrap();
        let entry = EdpfEntry64::parse(&bytes).map_err(|e| e.to_string())?;
        if entry.partition_type == 4 {
            return Ok((index, bytes, entry));
        }
    }
    Err("source LBA7 does not contain a type4 partition".into())
}

fn already_passwordless(lba0: &[u8], lba12: &[u8]) -> bool {
    if lba0.len() != SECTOR || lba12.len() != SECTOR || &lba12[..4] != b"EDPF" {
        return false;
    }
    let count = u32::from_le_bytes(lba12[0x08..0x0c].try_into().unwrap());
    if count != 2 {
        return false;
    }
    let type0 = u32::from_le_bytes(lba12[0x0c..0x10].try_into().unwrap());
    let type1 = u32::from_le_bytes(lba12[E12 + 0x0c..E12 + 0x10].try_into().unwrap());
    let start0 = u64::from_le_bytes(lba12[0x18..0x20].try_into().unwrap());
    lba0[0x1be + 4] == 0x07 && type0 == 2 && type1 == 4 && start0 == SHARE_START
}

fn validate_type4(entry: &EdpfEntry96) -> Result<PasswordlessConversionPlan, String> {
    if entry.need_encrypt == 0 {
        return Err("source type4 is not marked encrypted".into());
    }
    if entry.sector_size != SECTOR as u64 {
        return Err(format!(
            "source type4 sector size is {}, expected {SECTOR}",
            entry.sector_size
        ));
    }
    if entry.start_sector <= SHARE_START {
        return Err("source type4 starts before the passwordless front region can exist".into());
    }
    if entry.partition_size < SECTOR as u64 || entry.partition_size % SECTOR as u64 != 0 {
        return Err("source type4 size is empty or not 512-byte aligned".into());
    }
    let front_sectors = entry.start_sector - SHARE_START;
    if front_sectors > u32::MAX as u64 {
        return Err("passwordless front region exceeds the MBR 32-bit sector-count field".into());
    }
    Ok(PasswordlessConversionPlan {
        front_start_lba: SHARE_START,
        front_sectors,
        encrypt_start_lba: entry.start_sector,
        encrypt_size_bytes: entry.partition_size,
    })
}

fn build_lba0(source: &[u8; SECTOR], plan: PasswordlessConversionPlan) -> [u8; SECTOR] {
    let mut out = *source;
    out[0x1be..0x1fe].fill(0);
    out[0x1be + 4] = 0x07;
    put_u32(&mut out, 0x1be + 8, plan.front_start_lba as u32);
    put_u32(&mut out, 0x1be + 12, plan.front_sectors as u32);
    out[0x1fe..0x200].copy_from_slice(&[0x55, 0xaa]);
    out
}

fn as_front_entry<const N: usize>(
    source_type4: &[u8; N],
    plan: PasswordlessConversionPlan,
) -> [u8; N] {
    let mut out = *source_type4;
    put_u32(&mut out, 0x08, 2);
    put_u32(&mut out, 0x0c, 2);
    put_u32(&mut out, 0x10, 1);
    put_u32(&mut out, 0x14, 1);
    put_u64(&mut out, 0x18, plan.front_start_lba);
    put_u64(&mut out, 0x20, SECTOR as u64);
    put_u64(&mut out, 0x28, plan.front_sectors * SECTOR as u64);
    out
}

fn as_preserved_type4<const N: usize>(source_type4: &[u8; N]) -> [u8; N] {
    let mut out = *source_type4;
    put_u32(&mut out, 0x08, 2);
    put_u32(&mut out, 0x10, 1);
    out
}

/// Build a strict conversion of an existing official disk into the verified
/// passwordless type2+type4 layout.
///
/// The source type4 geometry and key material are preserved byte-for-byte.
/// The front region is intentionally rebuilt as an empty plaintext exFAT
/// filesystem; migrating existing front-region user files is a separate step.
pub fn build_passwordless_conversion(
    source: &ProvisionImage,
    device_id: &str,
    volume_serial: u32,
    volume_label: &str,
) -> Result<PasswordlessConversionImage, String> {
    let bytes = source.as_bytes();
    let lba0: [u8; SECTOR] = bytes[..SECTOR].try_into().unwrap();
    let raw7: [u8; SECTOR] = bytes[7 * SECTOR..8 * SECTOR].try_into().unwrap();
    let raw12: [u8; SECTOR] = bytes[12 * SECTOR..13 * SECTOR].try_into().unwrap();

    let crc = crc32_bare(device_id.as_bytes());
    let plain12 = a6b0_full(&raw12, &crc.to_le_bytes(), 0);
    if already_passwordless(&lba0, &plain12) {
        return Err("source is already a passwordless type2+type4 layout".into());
    }
    let (_, source_type4_12, type4_12) = find_type4_96(&plain12)?;
    let plan = validate_type4(&type4_12)?;

    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let plain7 = xor_rolling(&raw7, k0);
    let (_, source_type4_7, type4_7) = find_type4_64(&plain7)?;
    if type4_7.need_encrypt == 0 || type4_7.sector_size != SECTOR as u64 {
        return Err("source LBA7 type4 flags or sector size are invalid".into());
    }

    let mut target12 = plain12.clone();
    target12[..E12].copy_from_slice(&as_front_entry(&source_type4_12, plan));
    target12[E12..2 * E12].copy_from_slice(&as_preserved_type4(&source_type4_12));
    target12[2 * E12..3 * E12].fill(0);
    let lba12: [u8; SECTOR] = a7f0_full(&target12, &crc.to_le_bytes(), 0)
        .try_into()
        .map_err(|_| "converted LBA12 length mismatch")?;

    let mut target7 = plain7.clone();
    target7[..E7].copy_from_slice(&as_front_entry(&source_type4_7, plan));
    target7[E7..2 * E7].copy_from_slice(&as_preserved_type4(&source_type4_7));
    target7[2 * E7..3 * E7].fill(0);
    let lba7: [u8; SECTOR] = xor_rolling(&target7, k0)
        .try_into()
        .map_err(|_| "converted LBA7 length mismatch")?;

    let front_filesystem = build_empty_exfat(
        plan.front_start_lba,
        plan.front_sectors,
        volume_serial,
        volume_label,
    )?;

    Ok(PasswordlessConversionImage {
        plan,
        lba0: build_lba0(&lba0, plan),
        lba7,
        lba12,
        front_filesystem,
    })
}
