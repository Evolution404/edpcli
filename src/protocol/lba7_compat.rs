//! Geometry for the fixed LBA7 legacy compatibility extent.
//!
//! The legacy Sector7 format stores `tagEdpPartionInfo` / `EDP_PARTION_INFO` entries.
//! First-party `CUsbRegsiter::CreatePartitions` preserves each logical
//! `PartionType`, but rewrites legacy entries after entry0 to the same fixed
//! 0xC00-byte physical extent. Therefore this extent is not a "type4 region":
//! its pointer entry may carry type2 (share) or type4 (encrypt).

pub const LBA7_COMPAT_EXTENT_TOTAL_SIZE: usize = 0xC00;
pub const LBA7_COMPAT_CHS_TAIL_DISTANCE_BYTES: u64 = 0xE0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lba7CompatibilityExtentLayout {
    pub chs_bytes: u64,
    pub start_byte_offset: u64,
    pub start_lba: u64,
    pub size_bytes: u64,
    pub size_sectors: u64,
}

/// Reproduces the fixed compatibility-extent locator used by
/// `cemsusbregsiter.dll`.
///
/// The official registration path obtains a classic DISK_GEOMETRY, computes
/// Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector, subtracts
/// 0xE0000 bytes, then divides by the sector size for the EDPF StartSector.
/// The 0xC00-byte compatibility extent is rounded up to a whole sector.
pub fn locate_lba7_compatibility_extent_from_geometry(
    cylinders: u64,
    tracks_per_cylinder: u32,
    sectors_per_track: u32,
    bytes_per_sector: u32,
) -> Option<Lba7CompatibilityExtentLayout> {
    let sector_size = u64::from(bytes_per_sector);
    if sector_size == 0 {
        return None;
    }

    let chs_bytes = cylinders
        .checked_mul(u64::from(tracks_per_cylinder))?
        .checked_mul(u64::from(sectors_per_track))?
        .checked_mul(sector_size)?;
    let start_byte_offset = chs_bytes.checked_sub(LBA7_COMPAT_CHS_TAIL_DISTANCE_BYTES)?;
    let size_bytes = (LBA7_COMPAT_EXTENT_TOTAL_SIZE as u64)
        .checked_add(sector_size - 1)?
        .checked_div(sector_size)?
        .checked_mul(sector_size)?;

    Some(Lba7CompatibilityExtentLayout {
        chs_bytes,
        start_byte_offset,
        start_lba: start_byte_offset / sector_size,
        size_bytes,
        size_sectors: size_bytes / sector_size,
    })
}
