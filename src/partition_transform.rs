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

/// Data-layer cipher from the EDPF EncryptMode byte, NOT the FileKey
/// password wrapping algorithm, nor the four official partition layouts.
/// No raw FileKey is retained by this policy value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePartitionDataCipher {
    /// Manufacturer's physical-byte-offset-dependent AES variant.
    AesOffset,
    /// Standard SM4-128-ECB, Windows driver EncryptMode=2.
    Sm4Ecb,
    /// Standard AES-128-ECB, Windows driver EncryptMode=3.
    AesCrossEcb,
}

impl NativePartitionDataCipher {
    pub fn from_encrypt_mode(mode: u8) -> Result<Self, String> {
        match mode {
            1 => Ok(Self::AesOffset),
            2 => Ok(Self::Sm4Ecb),
            3 => Ok(Self::AesCrossEcb),
            _ => Err(format!("EncryptMode={mode}无已认证原生分区数据算法")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCipherDirection {
    Encrypt,
    Decrypt,
}

/// Offline memory-only native-sector cipher. Caller supplies the *absolute*
/// logical LBA and its native byte width, never a partition-relative 512B LBA.
/// Never opens a disk or grants physical-write capability. These primitives
/// have independent official Windows-driver ciphertext regression goldens.
pub fn transform_native_sector_offline(
    algorithm: NativePartitionDataCipher,
    direction: NativeCipherDirection,
    source: &[u8],
    key: &[u8; 16],
    absolute_lba: u64,
    logical_sector_bytes: u32,
) -> Result<Vec<u8>, String> {
    if !crate::domain::hardware::valid_native_sector_bytes(logical_sector_bytes)
        || source.len() != logical_sector_bytes as usize
    {
        return Err("加密输入必须是完整的512B正整数倍原生逻辑扇区".into());
    }
    let initial_byte_offset = absolute_lba
        .checked_mul(u64::from(logical_sector_bytes))
        .and_then(|offset| {
            offset
                .checked_add(u64::from(logical_sector_bytes))
                .map(|_| offset)
        })
        .ok_or("原生物理字节偏移或完整扇区范围溢出")?;
    use NativeCipherDirection::{Decrypt, Encrypt};
    let result = match algorithm {
        NativePartitionDataCipher::AesOffset => {
            use crate::protocol::crypto::{a6b0_full, a7f0_full};
            match direction {
                Encrypt => a7f0_full(source, key, initial_byte_offset),
                Decrypt => a6b0_full(source, key, initial_byte_offset),
            }
        }
        NativePartitionDataCipher::Sm4Ecb => {
            let cipher = Sm4Cipher::new(key);
            source
                .as_chunks::<16>()
                .0
                .iter()
                .flat_map(|block| match direction {
                    Encrypt => cipher.encrypt_block(block),
                    Decrypt => cipher.decrypt_block(block),
                })
                .collect()
        }
        NativePartitionDataCipher::AesCrossEcb => {
            use crate::protocol::crypto::{aes128_ecb_decrypt_block, aes128_ecb_encrypt_block};
            source
                .as_chunks::<16>()
                .0
                .iter()
                .flat_map(|block| match direction {
                    Encrypt => aes128_ecb_encrypt_block(block, key),
                    Decrypt => aes128_ecb_decrypt_block(block, key),
                })
                .collect()
        }
    };
    Ok(result)
}

/// Read-only AES_CROSS data-sector decoder, independently pinned to the
/// official Windows driver mode3 (AES-128-ECB). No physical disk I/O here.
/// Accept exactly one full 512B or 4096B native logical sector.
pub fn decrypt_mode3_native(data: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, String> {
    if u32::try_from(data.len())
        .ok()
        .is_none_or(|n| !crate::domain::hardware::valid_native_sector_bytes(n))
    {
        return Err("AES_CROSS 只读解密要求完整512B正整数倍原生逻辑扇区".into());
    }
    transform_native_sector_offline(
        NativePartitionDataCipher::AesCrossEcb,
        NativeCipherDirection::Decrypt,
        data,
        key,
        0, // AES_CROSS is offset-independent; source LBA must not affect ECB.
        data.len() as u32,
    )
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
    fn unified_native_cipher_matches_official_driver_independent_goldens() {
        use super::{
            transform_native_sector_offline as transform, NativeCipherDirection as Direction,
            NativePartitionDataCipher as Algorithm,
        };
        let key = std::array::from_fn(|index| index as u8);
        let original_block = hex_bytes_from_public_aes_vector_for_test();
        let original = original_block.repeat(32);
        assert_eq!(original.len(), 512);
        for (lba, expected_sha) in [
            (
                0,
                "b290b2e6e598e027e78453d2db0a91fe5ebb9676b6e05712f986affae34094c9",
            ),
            (
                8,
                "163d9a5c52dbdde281042fe08272fec8f058c65ebc7eb792f0dded1df9bec66a",
            ),
        ] {
            let encrypted = transform(
                Algorithm::AesOffset,
                Direction::Encrypt,
                &original,
                &key,
                lba,
                512,
            )
            .unwrap();
            assert_eq!(
                crate::sha256::sha256_hex(&encrypted),
                expected_sha,
                "Windows driver mode1 byte offset {}",
                lba * 512
            );
            assert_eq!(
                transform(
                    Algorithm::AesOffset,
                    Direction::Decrypt,
                    &encrypted,
                    &key,
                    lba,
                    512
                )
                .unwrap(),
                original
            );
        }
        let sm4_key = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        let sm4_plain = (0..512).map(|i| (i * 7 + 89) as u8).collect::<Vec<u8>>();
        let sm4_cipher = transform(
            Algorithm::Sm4Ecb,
            Direction::Encrypt,
            &sm4_plain,
            &sm4_key,
            7,
            512,
        )
        .unwrap();
        assert_eq!(
            crate::sha256::sha256_hex(&sm4_cipher),
            "12afec563ab2f58e81690eab87fc85fd99080662fd96da905ac6310cb15e88a6"
        );
        assert_eq!(
            transform(
                Algorithm::Sm4Ecb,
                Direction::Decrypt,
                &sm4_cipher,
                &sm4_key,
                0,
                512
            )
            .unwrap(),
            sm4_plain
        );
        let data = (0..4096).map(|i| (i * 37 + 13) as u8).collect::<Vec<u8>>();
        for lba in [0, 1, 49_979_648] {
            let cipher = transform(
                Algorithm::AesCrossEcb,
                Direction::Encrypt,
                &data,
                &key,
                lba,
                4096,
            )
            .unwrap();
            assert_eq!(
                crate::sha256::sha256_hex(&cipher),
                "ddb4a6b19a1ccb322887daf997e776e98e90e158141182df47ca741e2d1cef5c"
            );
            assert_eq!(
                transform(
                    Algorithm::AesCrossEcb,
                    Direction::Decrypt,
                    &cipher,
                    &key,
                    lba,
                    4096
                )
                .unwrap(),
                data
            );
        }
        for (raw, expected) in [
            (1, Algorithm::AesOffset),
            (2, Algorithm::Sm4Ecb),
            (3, Algorithm::AesCrossEcb),
        ] {
            assert_eq!(Algorithm::from_encrypt_mode(raw).unwrap(), expected);
        }
        for raw in [0, 4, 255] {
            assert!(Algorithm::from_encrypt_mode(raw).is_err());
        }
        for size in [0, 16, 511, 513, 1023, 1537, 4095, 8193] {
            assert!(transform(
                Algorithm::AesCrossEcb,
                Direction::Encrypt,
                &vec![0; size],
                &key,
                0,
                size as u32
            )
            .is_err());
        }
        for lba in [u64::MAX, u64::MAX / 4096, u64::MAX / 4096 + 1] {
            assert!(transform(
                Algorithm::AesOffset,
                Direction::Encrypt,
                &data,
                &key,
                lba,
                4096
            )
            .is_err());
        }
    }

    fn hex_bytes_from_public_aes_vector_for_test() -> Vec<u8> {
        vec![
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ]
    }

    #[test]
    fn mode3_native_read_only_matches_official_driver_4096_sha_golden() {
        use crate::protocol::crypto::aes128_ecb_encrypt_block;
        let key = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let source = (0..4096)
            .map(|index| (index * 37 + 13) as u8)
            .collect::<Vec<_>>();
        let mut encrypted = Vec::with_capacity(4096);
        for block in source.as_chunks::<16>().0 {
            encrypted.extend_from_slice(&aes128_ecb_encrypt_block(block, &key));
        }
        // Produced independently by Unicorn executing official EdpEDisk64.sys
        // mode3 encrypt @0x160E0, SHA-pinned to driver 724544a96f89... .
        assert_eq!(
            crate::sha256::sha256_hex(&encrypted),
            "ddb4a6b19a1ccb322887daf997e776e98e90e158141182df47ca741e2d1cef5c"
        );
        assert_eq!(
            super::decrypt_mode3_native(&encrypted, &key).unwrap(),
            source
        );
        assert_eq!(
            super::decrypt_mode3_native(&encrypted[..512], &key).unwrap(),
            source[..512]
        );
        for len in [0, 1, 16, 511, 513, 1023, 4095, 4097] {
            assert!(super::decrypt_mode3_native(&vec![0; len], &key).is_err());
        }
    }

    #[test]
    fn all_native_512_multiple_sizes_roundtrip_all_three_ciphers() {
        use super::{NativeCipherDirection as Direction, NativePartitionDataCipher as Algorithm};
        let key = [0x42; 16];
        for size in [512u32, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let source = (0..size as usize)
                .map(|index| (index.wrapping_mul(31).wrapping_add(13)) as u8)
                .collect::<Vec<_>>();
            for cipher in [
                Algorithm::AesOffset,
                Algorithm::Sm4Ecb,
                Algorithm::AesCrossEcb,
            ] {
                let encrypted = super::transform_native_sector_offline(
                    cipher,
                    Direction::Encrypt,
                    &source,
                    &key,
                    331,
                    size,
                )
                .unwrap();
                assert_eq!(encrypted.len(), size as usize);
                let decrypted = super::transform_native_sector_offline(
                    cipher,
                    Direction::Decrypt,
                    &encrypted,
                    &key,
                    331,
                    size,
                )
                .unwrap();
                assert_eq!(decrypted, source);
            }
        }
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
