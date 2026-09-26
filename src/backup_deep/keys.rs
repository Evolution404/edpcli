//! SM4 primitive reused from the independently verified protocol audit reader.
const SM4_SBOX: [u8; 256] = [
    0xd6, 0x90, 0xe9, 0xfe, 0xcc, 0xe1, 0x3d, 0xb7, 0x16, 0xb6, 0x14, 0xc2, 0x28, 0xfb, 0x2c, 0x05,
    0x2b, 0x67, 0x9a, 0x76, 0x2a, 0xbe, 0x04, 0xc3, 0xaa, 0x44, 0x13, 0x26, 0x49, 0x86, 0x06, 0x99,
    0x9c, 0x42, 0x50, 0xf4, 0x91, 0xef, 0x98, 0x7a, 0x33, 0x54, 0x0b, 0x43, 0xed, 0xcf, 0xac, 0x62,
    0xe4, 0xb3, 0x1c, 0xa9, 0xc9, 0x08, 0xe8, 0x95, 0x80, 0xdf, 0x94, 0xfa, 0x75, 0x8f, 0x3f, 0xa6,
    0x47, 0x07, 0xa7, 0xfc, 0xf3, 0x73, 0x17, 0xba, 0x83, 0x59, 0x3c, 0x19, 0xe6, 0x85, 0x4f, 0xa8,
    0x68, 0x6b, 0x81, 0xb2, 0x71, 0x64, 0xda, 0x8b, 0xf8, 0xeb, 0x0f, 0x4b, 0x70, 0x56, 0x9d, 0x35,
    0x1e, 0x24, 0x0e, 0x5e, 0x63, 0x58, 0xd1, 0xa2, 0x25, 0x22, 0x7c, 0x3b, 0x01, 0x21, 0x78, 0x87,
    0xd4, 0x00, 0x46, 0x57, 0x9f, 0xd3, 0x27, 0x52, 0x4c, 0x36, 0x02, 0xe7, 0xa0, 0xc4, 0xc8, 0x9e,
    0xea, 0xbf, 0x8a, 0xd2, 0x40, 0xc7, 0x38, 0xb5, 0xa3, 0xf7, 0xf2, 0xce, 0xf9, 0x61, 0x15, 0xa1,
    0xe0, 0xae, 0x5d, 0xa4, 0x9b, 0x34, 0x1a, 0x55, 0xad, 0x93, 0x32, 0x30, 0xf5, 0x8c, 0xb1, 0xe3,
    0x1d, 0xf6, 0xe2, 0x2e, 0x82, 0x66, 0xca, 0x60, 0xc0, 0x29, 0x23, 0xab, 0x0d, 0x53, 0x4e, 0x6f,
    0xd5, 0xdb, 0x37, 0x45, 0xde, 0xfd, 0x8e, 0x2f, 0x03, 0xff, 0x6a, 0x72, 0x6d, 0x6c, 0x5b, 0x51,
    0x8d, 0x1b, 0xaf, 0x92, 0xbb, 0xdd, 0xbc, 0x7f, 0x11, 0xd9, 0x5c, 0x41, 0x1f, 0x10, 0x5a, 0xd8,
    0x0a, 0xc1, 0x31, 0x88, 0xa5, 0xcd, 0x7b, 0xbd, 0x2d, 0x74, 0xd0, 0x12, 0xb8, 0xe5, 0xb4, 0xb0,
    0x89, 0x69, 0x97, 0x4a, 0x0c, 0x96, 0x77, 0x7e, 0x65, 0xb9, 0xf1, 0x09, 0xc5, 0x6e, 0xc6, 0x84,
    0x18, 0xf0, 0x7d, 0xec, 0x3a, 0xdc, 0x4d, 0x20, 0x79, 0xee, 0x5f, 0x3e, 0xd7, 0xcb, 0x39, 0x48,
];

fn sm4_tau(value: u32) -> u32 {
    let bytes = value.to_be_bytes();
    u32::from_be_bytes([
        SM4_SBOX[bytes[0] as usize],
        SM4_SBOX[bytes[1] as usize],
        SM4_SBOX[bytes[2] as usize],
        SM4_SBOX[bytes[3] as usize],
    ])
}

fn sm4_round_keys(key: &[u8; 16]) -> [u32; 32] {
    const FK: [u32; 4] = [0xa3b1bac6, 0x56aa3350, 0x677d9197, 0xb27022dc];
    let mut rk_state = [
        u32::from_be_bytes(key[0..4].try_into().unwrap()) ^ FK[0],
        u32::from_be_bytes(key[4..8].try_into().unwrap()) ^ FK[1],
        u32::from_be_bytes(key[8..12].try_into().unwrap()) ^ FK[2],
        u32::from_be_bytes(key[12..16].try_into().unwrap()) ^ FK[3],
    ];
    let mut round_keys = [0u32; 32];
    for (i, round_key) in round_keys.iter_mut().enumerate() {
        let ck = u32::from_be_bytes([
            ((4 * i * 7) & 0xff) as u8,
            (((4 * i + 1) * 7) & 0xff) as u8,
            (((4 * i + 2) * 7) & 0xff) as u8,
            (((4 * i + 3) * 7) & 0xff) as u8,
        ]);
        let b = sm4_tau(rk_state[1] ^ rk_state[2] ^ rk_state[3] ^ ck);
        let next = rk_state[0] ^ b ^ b.rotate_left(13) ^ b.rotate_left(23);
        *round_key = next;
        rk_state = [rk_state[1], rk_state[2], rk_state[3], next];
    }
    round_keys
}

fn sm4_crypt_block(input: &[u8; 16], key: &[u8; 16], decrypt: bool) -> [u8; 16] {
    let round_keys = sm4_round_keys(key);
    let mut x = [
        u32::from_be_bytes(input[0..4].try_into().unwrap()),
        u32::from_be_bytes(input[4..8].try_into().unwrap()),
        u32::from_be_bytes(input[8..12].try_into().unwrap()),
        u32::from_be_bytes(input[12..16].try_into().unwrap()),
    ];
    let mut apply_round = |round_key: u32| {
        let b = sm4_tau(x[1] ^ x[2] ^ x[3] ^ round_key);
        let next =
            x[0] ^ b ^ b.rotate_left(2) ^ b.rotate_left(10) ^ b.rotate_left(18) ^ b.rotate_left(24);
        x = [x[1], x[2], x[3], next];
    };
    if decrypt {
        for &round_key in round_keys.iter().rev() {
            apply_round(round_key);
        }
    } else {
        for &round_key in &round_keys {
            apply_round(round_key);
        }
    }
    let words = [x[3], x[2], x[1], x[0]];
    let mut out = [0u8; 16];
    for (index, word) in words.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn sm4_decrypt_block(ciphertext: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    sm4_crypt_block(ciphertext, key, true)
}

/// Test/provisioning helper for producing mode2 ciphertext from known plaintext.
/// Inspect itself never calls this: its data path remains strictly read-only.
pub fn sm4_encrypt_block(plaintext: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    sm4_crypt_block(plaintext, key, false)
}

/// Recover a mode2 file key from an LBA12 v0x0206 default-password entry.
/// The first-party default-password substitution and the entry's FileKeyCRC
/// must both agree before the key can be used for partition-sector reads.
pub fn default_file_key(image: &[u8], device_id: &str, index: usize) -> Result<[u8; 16], String> {
    default_file_key_checked(image, device_id, index).map_err(|error| error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultFileKeyError {
    InvalidImageOrIndex,
    DeviceIdMismatch,
    UnsupportedPassInfo,
    InvalidEntry(crate::protocol::types::ProtocolError),
    NotEncryptedMode2,
    NotDefaultPassword,
    FileKeyCrcMismatch,
}

impl std::fmt::Display for DefaultFileKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidImageOrIndex => {
                formatter.write_str("invalid protocol image or partition index")
            }
            Self::DeviceIdMismatch => {
                formatter.write_str("LBA12 does not decode to EDPF with current device_id")
            }
            Self::UnsupportedPassInfo => {
                formatter.write_str("default key unwrap supports PassInfo v0x0206 only")
            }
            Self::InvalidEntry(error) => error.fmt(formatter),
            Self::NotEncryptedMode2 => {
                formatter.write_str("default key unwrap requires an encrypted mode2 entry")
            }
            Self::NotDefaultPassword => {
                formatter.write_str("partition does not advertise the default password")
            }
            Self::FileKeyCrcMismatch => formatter.write_str("default password FileKeyCRC mismatch"),
        }
    }
}

impl std::error::Error for DefaultFileKeyError {}

pub fn default_file_key_checked(
    image: &[u8],
    device_id: &str,
    index: usize,
) -> Result<[u8; 16], DefaultFileKeyError> {
    use crate::crypto::{a6b0_full, crc32_bare};
    use crate::protocol::edpf::{EdpfEntry96, PassInfo};

    if image.len() != 13 * 512 || index >= 3 {
        return Err(DefaultFileKeyError::InvalidImageOrIndex);
    }
    let device_crc = crc32_bare(device_id.as_bytes());
    let plain = a6b0_full(&image[12 * 512..13 * 512], &device_crc.to_le_bytes(), 0);
    if &plain[..4] != b"EDPF" {
        return Err(DefaultFileKeyError::DeviceIdMismatch);
    }
    let pass_bytes = plain
        .get(0x120..0x12e)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(DefaultFileKeyError::InvalidImageOrIndex)?;
    let pass = PassInfo::decode_stored(pass_bytes);
    if pass.version != 0x0206 {
        return Err(DefaultFileKeyError::UnsupportedPassInfo);
    }
    let entry_bytes = plain
        .get(index * 96..(index + 1) * 96)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(DefaultFileKeyError::InvalidImageOrIndex)?;
    let entry = EdpfEntry96::parse(entry_bytes).map_err(DefaultFileKeyError::InvalidEntry)?;
    if index >= entry.partition_count as usize || entry.need_encrypt == 0 || entry.encrypt_mode != 2
    {
        return Err(DefaultFileKeyError::NotEncryptedMode2);
    }
    if entry.user_key_crc != crc32_bare(b"0000aaaa") {
        return Err(DefaultFileKeyError::NotDefaultPassword);
    }
    // v0x0206 substitutes LtSWi[2f)j before MD5. This digest is pinned by
    // the first-party producer and consumer evidence in the protocol audit.
    const EFFECTIVE_MD5: [u8; 16] = [
        0x54, 0x8b, 0x07, 0x2c, 0xba, 0x7f, 0x10, 0x4d, 0x88, 0xa4, 0x46, 0x55, 0x6c, 0xc3, 0xc4,
        0x32,
    ];
    let key = sm4_decrypt_block(&entry.encrypted_file_key, &EFFECTIVE_MD5);
    if crc32_bare(&key) != entry.file_key_crc {
        return Err(DefaultFileKeyError::FileKeyCrcMismatch);
    }
    Ok(key)
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
