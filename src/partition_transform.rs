//! Partition-relative sector transforms.
//!
//! This layer is intentionally independent from filesystem semantics and EDP
//! protocol/key-domain policy. Callers must resolve and validate any FileKey
//! before constructing a transform.

use crate::protocol::crypto::Sm4Cipher;

pub const TRANSFORM_SECTOR_SIZE: usize = 512;

pub trait PartitionTransform: Send + Sync {
    fn transform_sector(
        &self,
        relative_lba: u64,
        source: &[u8; TRANSFORM_SECTOR_SIZE],
    ) -> [u8; TRANSFORM_SECTOR_SIZE];
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub struct IdentityTransform;

#[cfg(test)]
impl PartitionTransform for IdentityTransform {
    fn transform_sector(
        &self,
        _relative_lba: u64,
        source: &[u8; TRANSFORM_SECTOR_SIZE],
    ) -> [u8; TRANSFORM_SECTOR_SIZE] {
        *source
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct EdpSm4Transform {
    // The raw FileKey is not retained in addition to the expanded schedule.
    cipher: Sm4Cipher,
}

// Never disclose the FileKey-derived round schedule in logs or error output.
impl std::fmt::Debug for EdpSm4Transform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EdpSm4Transform").finish_non_exhaustive()
    }
}

impl EdpSm4Transform {
    pub fn new(file_key: [u8; 16]) -> Self {
        Self {
            cipher: Sm4Cipher::new(&file_key),
        }
    }

    #[cfg(test)]
    pub fn decrypt_sector(
        &self,
        source: &[u8; TRANSFORM_SECTOR_SIZE],
    ) -> [u8; TRANSFORM_SECTOR_SIZE] {
        let mut out = [0u8; TRANSFORM_SECTOR_SIZE];
        for (source, target) in source
            .as_chunks::<16>()
            .0
            .iter()
            .zip(out.as_chunks_mut::<16>().0.iter_mut())
        {
            target.copy_from_slice(&self.cipher.decrypt_block(source));
        }
        out
    }
}

impl PartitionTransform for EdpSm4Transform {
    fn transform_sector(
        &self,
        _relative_lba: u64,
        source: &[u8; TRANSFORM_SECTOR_SIZE],
    ) -> [u8; TRANSFORM_SECTOR_SIZE] {
        let mut out = [0u8; TRANSFORM_SECTOR_SIZE];
        for (source, target) in source
            .as_chunks::<16>()
            .0
            .iter()
            .zip(out.as_chunks_mut::<16>().0.iter_mut())
        {
            target.copy_from_slice(&self.cipher.encrypt_block(source));
        }
        out
    }
}

pub fn decrypt_mode2(data: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, String> {
    let (blocks, remainder) = data.as_chunks::<16>();
    if !remainder.is_empty() {
        return Err("truncated SM4 block".into());
    }
    // Inspect and source-profile read paths share one schedule per buffer,
    // preserving each existing 16-byte SM4 block without changing its mode.
    let cipher = Sm4Cipher::new(key);
    let mut plain = Vec::with_capacity(data.len());
    for block in blocks {
        plain.extend_from_slice(&cipher.decrypt_block(block));
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::{decrypt_mode2, EdpSm4Transform, IdentityTransform, PartitionTransform};

    #[test]
    fn identity_transform_is_bit_exact() {
        let source = [0x5au8; 512];
        assert_eq!(IdentityTransform.transform_sector(123, &source), source);
    }

    #[test]
    fn cached_sm4_matches_preoptimization_bit_exact_golden_vectors() {
        // Golden SHA-256 values were produced by the original pre-P0
        // sm4_encrypt_block implementation at main 378d95c (not by this
        // cached implementation). Four keys, 1024 full 512B sectors each.
        let keys: [[u8; 16]; 4] = [
            [0; 16],
            [0xff; 16],
            [
                0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66,
                0xcb, 0x42,
            ],
            [
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
                0x32, 0x10,
            ],
        ];
        let historical_sha256 = [
            "3fdd389fade459539988da3f96a37967b6cb946e3b25e42250b009595b27b26f",
            "7e7f61adb94c4f05ad61c23fba84eea86f3df8f682a631ff0938b67ac1b4d8bc",
            "f3d9cb13c2ef4db0b16905d6220ebfb335c437eefa85a213dc63435c5871cc88",
            "c4f0e8605f7e9481028409ceec61edbc34f635b5b131b0e34232c328ca71db01",
        ];
        for (key, expected_sha256) in keys.into_iter().zip(historical_sha256) {
            let transform = EdpSm4Transform::new(key);
            let mut encrypted = Vec::with_capacity(1024 * 512);
            for sector in 0..1024_usize {
                let mut plain = [0u8; 512];
                for (index, byte) in plain.iter_mut().enumerate() {
                    *byte = (sector.wrapping_mul(41)
                        ^ index.wrapping_mul(29)
                        ^ ((sector >> 5).wrapping_add(index >> 2)))
                        as u8;
                }
                let raw = transform.transform_sector(sector as u64, &plain);
                if sector % 128 == 0 {
                    assert_eq!(transform.decrypt_sector(&raw), plain);
                    assert_eq!(decrypt_mode2(&raw, &key).unwrap(), plain);
                    // Preserve the public single-block key-wrap helper API.
                    for (before, after) in plain
                        .as_chunks::<16>()
                        .0
                        .iter()
                        .zip(raw.as_chunks::<16>().0.iter())
                    {
                        assert_eq!(
                            crate::protocol::crypto::sm4_encrypt_block(before, &key).as_slice(),
                            after,
                        );
                    }
                }
                encrypted.extend_from_slice(&raw);
            }
            assert_eq!(crate::sha256::sha256_hex(&encrypted), expected_sha256);
        }
    }

    #[test]
    fn sm4_debug_does_not_print_file_key_or_derived_round_keys() {
        let key = [0x42; 16];
        let debug = format!("{:?}", EdpSm4Transform::new(key));
        assert_eq!(debug, "EdpSm4Transform { .. }");
        assert_eq!(debug, format!("{:?}", EdpSm4Transform::new([0x18; 16])),);
    }

    #[test]
    fn sm4_decrypt_mode2_rejects_truncated_blocks() {
        let key = [0x11; 16];
        assert_eq!(decrypt_mode2(&[], &key).unwrap(), Vec::<u8>::new());
        for len in [1, 15, 17, 511, 513] {
            assert!(decrypt_mode2(&vec![0; len], &key).is_err());
        }
    }

    #[test]
    fn edp_sm4_sector_transform_round_trips() {
        let key = [
            0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66,
            0xcb, 0x42,
        ];
        let mut source = [0u8; 512];
        for (index, byte) in source.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
        let transform = EdpSm4Transform::new(key);
        let encrypted = transform.transform_sector(7, &source);
        assert_ne!(encrypted, source);
        assert_eq!(transform.decrypt_sector(&encrypted), source);
        assert_eq!(decrypt_mode2(&encrypted, &key).unwrap(), source);
    }
}
