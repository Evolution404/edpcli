//! Geometry for the fixed LBA7 legacy compatibility extent.
//!
//! The legacy Sector7 format stores `tagEdpPartionInfo` / `EDP_PARTION_INFO` entries.
//! First-party `CUsbRegsiter::CreatePartitions` preserves each logical
//! `PartionType`, but rewrites legacy entries after entry0 to the same fixed
//! 0xC00-byte physical extent. Therefore this extent is not a "type4 region":
//! its pointer entry may carry type2 (share) or type4 (encrypt).

pub const LBA7_COMPAT_EXTENT_TOTAL_SIZE: usize = 0xC00;
pub const LBA7_COMPAT_CHS_TAIL_DISTANCE_BYTES: u64 = 0xE0000;
pub const VERIFIED_USB_TRACKS_PER_CYLINDER: u32 = 255;
pub const VERIFIED_USB_SECTORS_PER_TRACK: u32 = 63;

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

/// Cross-platform equivalent of the current first-party USB geometry profile.
///
/// Physical Lexar/aigo/SanDisk fixtures all use the Windows removable-media
/// translation 255 tracks/cylinder x 63 sectors/track with 512-byte sectors.
/// We deliberately floor to a complete translated cylinder, matching the
/// DISK_GEOMETRY Cylinders product consumed by the first-party writer.
pub fn locate_lba7_compatibility_extent_from_verified_usb_capacity(
    total_sectors: u64,
    bytes_per_sector: u32,
) -> Option<Lba7CompatibilityExtentLayout> {
    if bytes_per_sector != 512 {
        return None;
    }
    let sectors_per_cylinder = u64::from(VERIFIED_USB_TRACKS_PER_CYLINDER)
        .checked_mul(u64::from(VERIFIED_USB_SECTORS_PER_TRACK))?;
    let cylinders = total_sectors.checked_div(sectors_per_cylinder)?;
    if cylinders == 0 {
        return None;
    }
    locate_lba7_compatibility_extent_from_geometry(
        cylinders,
        VERIFIED_USB_TRACKS_PER_CYLINDER,
        VERIFIED_USB_SECTORS_PER_TRACK,
        bytes_per_sector,
    )
}
