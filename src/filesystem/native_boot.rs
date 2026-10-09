//! Geometry-aware native boot-sector classification, without enabling 4Kn formatting.
//! Filesystem *metadata* fields are at fixed byte offsets; BPB byte/sector and
//! volume extents must agree with the actual device-native logical geometry.
use super::{FilesystemError, FilesystemErrorKind, FilesystemKind};

fn le16(raw: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}
fn le32(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}
fn le64(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

/// Conservative 4Kn-only read-side probe, plus an unchanged 512B delegation.
/// Does not implement new filesystem support, formatting or write permissions.
pub fn detect_native_boot_sector(
    native: &[u8],
    volume_native_sectors: u64,
    logical_sector_bytes: u32,
) -> Result<Option<FilesystemKind>, FilesystemError> {
    if logical_sector_bytes == 512 {
        return super::detect_boot_sector(volume_native_sectors, native);
    }
    if logical_sector_bytes != 4096 || native.len() != 4096 {
        return Err(FilesystemError::new(
            FilesystemErrorKind::ReadFailure,
            "尚未验证的原生引导扇区几何",
        ));
    }
    if volume_native_sectors == 0
        || native[510..512] != [0x55, 0xaa]
        || !((native[0] == 0xeb && native[2] == 0x90) || native[0] == 0xe9)
    {
        return Ok(None);
    }
    if &native[3..11] == b"EXFAT   " {
        let bps = native[108];
        let spc = native[109];
        let fats = native[110] as u64;
        let total = le64(native, 72);
        let fat_start = le32(native, 80) as u64;
        let fat_size = le32(native, 84) as u64;
        let heap_start = le32(native, 88) as u64;
        let clusters = le32(native, 92) as u64;
        let root = le32(native, 96) as u64;
        let cluster_sectors = 1u64.checked_shl(spc as u32);
        if bps == 12
            && spc < 26
            && matches!(fats, 1 | 2)
            && native[11..64].iter().all(|b| *b == 0)
            && total > 0
            && total <= volume_native_sectors
            && fat_start >= 24
            && fat_size > 0
            && clusters > 0
            && root >= 2
            && root < clusters.saturating_add(2)
            && cluster_sectors.is_some_and(|per| {
                fat_start
                    .checked_add(fat_size.saturating_mul(fats))
                    .is_some_and(|end| end <= heap_start)
                    && heap_start
                        .checked_add(clusters.saturating_mul(per))
                        .is_some_and(|end| end <= total)
            })
        {
            return Ok(Some(FilesystemKind::ExFat));
        }
        return Ok(None);
    }
    if &native[3..11] == b"NTFS    " {
        // NTFS does not have FAT's BPB layout. Do not run it through the FAT
        // reserved-sector/FAT-count checks, which reject valid 4Kn NTFS boots.
        // Match the existing 512B NTFS driver's structural checks, using the
        // observed native 4096B sector geometry instead of assuming 512B.
        let bps = le16(native, 11) as u32;
        let spc = native[13] as u64;
        let total = le64(native, 40);
        let clusters = total.checked_div(spc);
        return Ok((bps == logical_sector_bytes
            && spc > 0
            && spc.is_power_of_two()
            && spc <= 128
            && total > 0
            && total <= volume_native_sectors
            && native[14..21].iter().all(|byte| *byte == 0)
            && native[21] >= 0xf0
            && clusters.is_some_and(|count| {
                count > 0 && le64(native, 48) < count && le64(native, 56) < count
            }))
        .then_some(FilesystemKind::Ntfs));
    }
    let bps = le16(native, 11) as u32;
    let spc = native[13] as u64;
    let reserved = le16(native, 14) as u64;
    let fats = native[16] as u64;
    let root_entries = le16(native, 17) as u64;
    let total16 = le16(native, 19) as u64;
    let total = if total16 != 0 {
        total16
    } else {
        le32(native, 32) as u64
    };
    let fat16 = le16(native, 22) as u64;
    let fat32 = le32(native, 36) as u64;
    if bps != logical_sector_bytes
        || spc == 0
        || !spc.is_power_of_two()
        || spc > 128
        || reserved == 0
        || !matches!(fats, 1 | 2)
        || native[21] < 0xf0
        || total == 0
        || total > volume_native_sectors
    {
        return Ok(None);
    }
    let is_fat32 = fat16 == 0 && root_entries == 0 && fat32 != 0;
    let fat_sectors = if is_fat32 { fat32 } else { fat16 };
    if fat_sectors == 0 {
        return Ok(None);
    }
    let root_sectors = root_entries.saturating_mul(32).div_ceil(u64::from(bps));
    let overhead = reserved
        .checked_add(fats.saturating_mul(fat_sectors))
        .and_then(|v| v.checked_add(root_sectors));
    let Some(data_sectors) = overhead.and_then(|count| total.checked_sub(count)) else {
        return Ok(None);
    };
    let clusters = data_sectors / spc;
    if is_fat32 {
        let enough_fat_entries = fat_sectors.saturating_mul(u64::from(bps)) / 4 >= clusters + 2;
        return Ok(((65_525..=0x0fff_ffef).contains(&clusters)
            && le16(native, 42) == 0
            && le32(native, 44) >= 2
            && enough_fat_entries)
            .then_some(FilesystemKind::Fat32));
    }
    if root_entries == 0 {
        return Ok(None);
    }
    if clusters < 4_085 {
        let enough_fat_entries = fat_sectors.saturating_mul(u64::from(bps)) * 2 / 3 >= clusters + 2;
        return Ok(enough_fat_entries.then_some(FilesystemKind::Fat12));
    }
    if clusters < 65_525 {
        let enough_fat_entries = fat_sectors.saturating_mul(u64::from(bps)) / 2 >= clusters + 2;
        return Ok(enough_fat_entries.then_some(FilesystemKind::Fat16));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boot() -> Vec<u8> {
        let mut b = vec![0u8; 4096];
        b[..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
        b[11..13].copy_from_slice(&4096u16.to_le_bytes());
        b[13] = 1;
        b[14..16].copy_from_slice(&1u16.to_le_bytes());
        b[16] = 2;
        b[17..19].copy_from_slice(&512u16.to_le_bytes());
        b[21] = 0xf8;
        b[510..512].copy_from_slice(&[0x55, 0xaa]);
        b
    }
    #[test]
    fn four_kn_bpb_detects_fat12_fat16_fat32_without_changing_512_parser() {
        let mut fat12 = boot();
        fat12[19..21].copy_from_slice(&2497u16.to_le_bytes());
        fat12[22..24].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&fat12, 2497, 4096).unwrap(),
            Some(FilesystemKind::Fat12)
        );
        assert!(detect_native_boot_sector(&fat12, 2497, 512).is_err());
        let mut fat16 = boot();
        fat16[19..21].copy_from_slice(&5000u16.to_le_bytes());
        fat16[22..24].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&fat16, 5000, 4096).unwrap(),
            Some(FilesystemKind::Fat16)
        );
        let mut fat32 = boot();
        fat32[14..16].copy_from_slice(&32u16.to_le_bytes());
        fat32[17..19].copy_from_slice(&0u16.to_le_bytes());
        fat32[19..21].copy_from_slice(&0u16.to_le_bytes());
        fat32[32..36].copy_from_slice(&120000u32.to_le_bytes());
        fat32[36..40].copy_from_slice(&118u32.to_le_bytes());
        fat32[44..48].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&fat32, 120000, 4096).unwrap(),
            Some(FilesystemKind::Fat32)
        );
        fat32[11..13].copy_from_slice(&512u16.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&fat32, 120000, 4096).unwrap(),
            None
        );
    }
    #[test]
    fn four_kn_ntfs_accepts_real_u391_geometry_and_rejects_invalid_boots() {
        let mut ntfs = boot();
        ntfs[3..11].copy_from_slice(b"NTFS    ");
        ntfs[13] = 1;
        ntfs[14..21].fill(0);
        ntfs[21] = 0xf8;
        let volume_sectors = 12_494_112u64;
        ntfs[40..48].copy_from_slice(&(volume_sectors - 1).to_le_bytes());
        ntfs[48..56].copy_from_slice(&4u64.to_le_bytes());
        ntfs[56..64].copy_from_slice(&8u64.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            Some(FilesystemKind::Ntfs)
        );
        // Regression: prior FAT-only path returned None for the valid NTFS boot.
        let valid = ntfs.clone();
        ntfs[11..13].copy_from_slice(&512u16.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[13] = 3;
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[40..48].copy_from_slice(&(volume_sectors + 1).to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[48..56].copy_from_slice(&volume_sectors.to_le_bytes());
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[14] = 1;
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[3..11].copy_from_slice(b"BADFS   ");
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
        ntfs = valid.clone();
        ntfs[510] = 0;
        assert_eq!(
            detect_native_boot_sector(&ntfs, volume_sectors, 4096).unwrap(),
            None
        );
    }

    #[test]
    fn four_kn_exfat_requires_native_geometry_and_consistent_extents() {
        let mut boot = boot();
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[11..64].fill(0);
        boot[72..80].copy_from_slice(&8192u64.to_le_bytes());
        boot[80..84].copy_from_slice(&24u32.to_le_bytes());
        boot[84..88].copy_from_slice(&256u32.to_le_bytes());
        boot[88..92].copy_from_slice(&512u32.to_le_bytes());
        boot[92..96].copy_from_slice(&4096u32.to_le_bytes());
        boot[96..100].copy_from_slice(&2u32.to_le_bytes());
        boot[108] = 12;
        boot[109] = 0;
        boot[110] = 1;
        assert_eq!(
            detect_native_boot_sector(&boot, 8192, 4096).unwrap(),
            Some(FilesystemKind::ExFat)
        );
        boot[108] = 9;
        assert_eq!(detect_native_boot_sector(&boot, 8192, 4096).unwrap(), None);
    }
}
