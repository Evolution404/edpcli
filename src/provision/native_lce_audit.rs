//! Pure source-vs-first-party-legacy LCE producer compatibility audit.
//! A successful comparison is *not* evidence of physical 4Kn writer support.
//! In particular, 1024 opaque bytes in a 4096B native source block MUST NOT
//! be assumed to be empty, overwritten, or synthesized by this legacy producer.
use super::lce_plaintext;
use crate::protocol::crypto::a7f0_full;
use crate::protocol::lba7_compat::LBA7_COMPAT_EXTENT_TOTAL_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLceProducerAudit {
    pub logical_sector_bytes: u32,
    pub source_native_bytes: usize,
    pub legacy_payload_bytes: usize,
    pub opaque_tail_bytes: usize,
    pub opaque_tail_nonzero: usize,
    pub mismatching_legacy_payload_bytes: usize,
    /// The legacy 512B producer is certified only for its legacy geometry.
    /// A 4Kn native source always requires an independent OEM producer proof.
    pub certified_legacy_512b_reproduction: bool,
}

/// Independently generate the legacy zero8 LCE payload using the true native
/// physical byte offset and compare it byte-for-byte with complete source
/// native blocks. Returns only mismatch statistics; never logs source bytes.
pub fn audit_native_lce_against_legacy_producer(
    logical_sector_bytes: u32,
    lce_start_lba: u64,
    source_blocks: &[Vec<u8>],
) -> Result<NativeLceProducerAudit, String> {
    if !matches!(logical_sector_bytes, 512 | 4096) {
        return Err("unsupported native LCE sector geometry".into());
    }
    let block_bytes = logical_sector_bytes as usize;
    let count = LBA7_COMPAT_EXTENT_TOTAL_SIZE.div_ceil(block_bytes);
    if source_blocks.len() != count || source_blocks.iter().any(|block| block.len() != block_bytes)
    {
        return Err("incomplete native LCE source blocks".into());
    }
    let offset = lce_start_lba
        .checked_mul(u64::from(logical_sector_bytes))
        .ok_or("native LCE physical byte offset overflow")?;
    let regenerated = a7f0_full(lce_plaintext(), &[0u8; 8], offset);
    let mismatch = source_blocks
        .iter()
        .flat_map(|block| block.iter())
        .take(LBA7_COMPAT_EXTENT_TOTAL_SIZE)
        .zip(regenerated.iter())
        .filter(|(actual, reference)| *actual != *reference)
        .count();
    let tail_nonzero = source_blocks
        .iter()
        .flat_map(|block| block.iter())
        .skip(LBA7_COMPAT_EXTENT_TOTAL_SIZE)
        .filter(|byte| **byte != 0)
        .count();
    Ok(NativeLceProducerAudit {
        logical_sector_bytes,
        source_native_bytes: count * block_bytes,
        legacy_payload_bytes: LBA7_COMPAT_EXTENT_TOTAL_SIZE,
        opaque_tail_bytes: count * block_bytes - LBA7_COMPAT_EXTENT_TOTAL_SIZE,
        opaque_tail_nonzero: tail_nonzero,
        mismatching_legacy_payload_bytes: mismatch,
        certified_legacy_512b_reproduction: logical_sector_bytes == 512 && mismatch == 0,
    })
}
