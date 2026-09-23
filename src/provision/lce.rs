//! First-party LBA7 compatibility extent (LCE) payload builder.
//!
//! Current first-party media use one fixed 3072-byte FAT16 compatibility
//! plaintext.  The physical representation is the zero8 EDPSECDISK transform
//! with the physical byte offset as the 64-bit tweak seed.

use crate::{
    crypto::a7f0_full_offset,
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
    a7f0_full_offset(LCE_PLAINTEXT, &ZERO8, layout.start_byte_offset)
        .try_into()
        .map_err(|_| "LCE ciphertext length mismatch".into())
}
