//! Partition-relative sector transforms.
//!
//! This layer is intentionally independent from filesystem semantics and EDP
//! protocol/key-domain policy. Callers must resolve and validate any FileKey
//! before constructing a transform.

use crate::protocol::crypto::{sm4_decrypt_block, sm4_encrypt_block};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdpSm4Transform {
    file_key: [u8; 16],
}

impl EdpSm4Transform {
    pub const fn new(file_key: [u8; 16]) -> Self {
        Self { file_key }
    }

    #[cfg(test)]
    pub const fn file_key(&self) -> &[u8; 16] {
        &self.file_key
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
            target.copy_from_slice(&sm4_decrypt_block(source, &self.file_key));
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
            target.copy_from_slice(&sm4_encrypt_block(source, &self.file_key));
        }
        out
    }
}

pub fn decrypt_mode2(data: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, String> {
    let (blocks, remainder) = data.as_chunks::<16>();
    if !remainder.is_empty() {
        return Err("truncated SM4 block".into());
    }
    Ok(blocks
        .iter()
        .flat_map(|block| sm4_decrypt_block(block, key))
        .collect())
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
