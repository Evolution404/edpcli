//! First-party LBA7 compatibility extent (LCE) payload builder.
//!
//! Current first-party media use one fixed 3072-byte FAT16 compatibility
//! plaintext.  The physical representation is the zero8 EDPSECDISK transform
//! with the physical byte offset as the 64-bit tweak seed.

use crate::{
    protocol::crypto::a7f0_full,
    protocol::lba7_compat::{Lba7CompatibilityExtentLayout, LBA7_COMPAT_EXTENT_TOTAL_SIZE},
};

const LCE_PLAINTEXT: &[u8; LBA7_COMPAT_EXTENT_TOTAL_SIZE] =
    include_bytes!("../../audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin");
const ZERO8: [u8; 8] = [0; 8];

pub fn lce_plaintext() -> &'static [u8; LBA7_COMPAT_EXTENT_TOTAL_SIZE] {
    LCE_PLAINTEXT
}

pub fn build_lce_ciphertext(
    layout: Lba7CompatibilityExtentLayout,
) -> Result<[u8; LBA7_COMPAT_EXTENT_TOTAL_SIZE], String> {
    if layout.size_bytes != LBA7_COMPAT_EXTENT_TOTAL_SIZE as u64 {
        return Err(format!(
            "LCE size mismatch: got {}, expected {}",
            layout.size_bytes, LBA7_COMPAT_EXTENT_TOTAL_SIZE
        ));
    }
    if layout.start_byte_offset != layout.start_lba.saturating_mul(512) {
        return Err("LCE layout is not a 512-byte-sector physical byte-offset mapping".into());
    }
    a7f0_full(LCE_PLAINTEXT, &ZERO8, layout.start_byte_offset)
        .try_into()
        .map_err(|_| "LCE ciphertext length mismatch".into())
}
/// Generate a complete native LCE using the existing zero8+A7F0 cipher.
///
/// 512B delegates to the independently gold-verified 3072B producer. On 4Kn,
/// the **candidate new-provision policy** is the 3072B FAT16 plaintext plus
/// 1024 plaintext zero bytes, encrypted continuously as one full 4096B block
/// with the real physical byte-offset tweak (not plaintext zero padding).
///
/// The 4Kn OEM writer has NOT been independently authenticated. This offline
/// constructor does not permit physical writes and must not replace an existing
/// source LCE's unowned 1024B tail during exact byte-for-byte source replay.
pub fn build_native_lce_ciphertext(
    layout: Lba7CompatibilityExtentLayout,
    logical_sector_bytes: u32,
) -> Result<Vec<u8>, String> {
    if !crate::domain::hardware::valid_native_sector_bytes(logical_sector_bytes) {
        return Err("native LCE sector size must be a positive multiple of 512B".into());
    }
    let logical = u64::from(logical_sector_bytes);
    let expected_sectors = (LBA7_COMPAT_EXTENT_TOTAL_SIZE as u64).div_ceil(logical);
    let expected_bytes = expected_sectors
        .checked_mul(logical)
        .ok_or("native LCE length overflow")?;
    if layout.size_bytes != expected_bytes || layout.size_sectors != expected_sectors {
        return Err("native LCE extent length does not match sector geometry".into());
    }
    if layout.start_lba.checked_mul(logical) != Some(layout.start_byte_offset)
        || layout
            .start_byte_offset
            .checked_add(expected_bytes)
            .is_none()
    {
        return Err("native LCE start address or byte range is invalid".into());
    }
    let mut plaintext = vec![0u8; expected_bytes as usize];
    plaintext[..LBA7_COMPAT_EXTENT_TOTAL_SIZE].copy_from_slice(LCE_PLAINTEXT);
    Ok(a7f0_full(&plaintext, &ZERO8, layout.start_byte_offset))
}
