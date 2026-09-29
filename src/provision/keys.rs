//! Current LBA12 file-key wrapping used by the first-party writer.

use crate::crypto::{
    a6b0_decrypt, a7f0_encrypt, aes128_ecb_decrypt_block, aes128_ecb_encrypt_block, crc32_bare,
};

const DEFAULT_PASSWORD: &[u8] = b"0000aaaa";
const DEFAULT_EFFECTIVE_PASSWORD: &[u8] = b"LtSWi[2f)j";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyLba7KeyMaterial {
    pub user_key_crc: u32,
    pub file_key_crc: u32,
    pub wrapped_file_key: [u8; 8],
}

impl LegacyLba7KeyMaterial {
    pub fn packed16(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[..4].copy_from_slice(&self.user_key_crc.to_le_bytes());
        out[4..8].copy_from_slice(&self.file_key_crc.to_le_bytes());
        out[8..].copy_from_slice(&self.wrapped_file_key);
        out
    }
}

pub fn wrap_legacy_lba7_file_key(password: &[u8], file_key: [u8; 8]) -> LegacyLba7KeyMaterial {
    let folded = legacy_password_fold32(password);
    let mut wrapped_file_key = [0u8; 8];
    for (index, chunk) in file_key.as_chunks::<4>().0.iter().enumerate() {
        let word = u32::from_le_bytes(*chunk) ^ folded;
        wrapped_file_key[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    LegacyLba7KeyMaterial {
        user_key_crc: crc32_bare(password),
        file_key_crc: crc32_bare(&file_key),
        wrapped_file_key,
    }
}

pub fn unwrap_legacy_lba7_file_key(
    password: &[u8],
    material: LegacyLba7KeyMaterial,
) -> Result<[u8; 8], String> {
    if material.user_key_crc != crc32_bare(password) {
        return Err("password does not match existing LBA7 key record".into());
    }
    let folded = legacy_password_fold32(password);
    let mut file_key = [0u8; 8];
    for (index, chunk) in material
        .wrapped_file_key
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        let word = u32::from_le_bytes(*chunk) ^ folded;
        file_key[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    if crc32_bare(&file_key) != material.file_key_crc {
        return Err("existing LBA7 FileKeyCRC does not verify".into());
    }
    Ok(file_key)
}

fn legacy_password_fold32(password: &[u8]) -> u32 {
    let mut sum = 0u32;
    let (chunks, tail) = password.as_chunks::<4>();
    for chunk in chunks {
        sum = sum.wrapping_add(u32::from_le_bytes(*chunk));
    }
    if !tail.is_empty() {
        let mut padded = [0u8; 4];
        padded[..tail.len()].copy_from_slice(tail);
        sum = sum.wrapping_add(u32::from_le_bytes(padded));
    }
    sum
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FileKeyWrapMode {
    A7f0 = 1,
    Sm4 = 2,
    Aes128Ecb = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingFileKeyError {
    PasswordRequired,
    PasswordMismatch,
    UnsupportedEncryptMode,
    FileKeyCrcMismatch,
    MalformedKeyRecord,
}

impl std::fmt::Display for ExistingFileKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PasswordRequired => "需要原密码",
            Self::PasswordMismatch => "原密码验证失败",
            Self::UnsupportedEncryptMode => "不支持的 EncryptMode",
            Self::FileKeyCrcMismatch => "FileKeyCRC 校验失败",
            Self::MalformedKeyRecord => "加密密钥记录异常",
        })
    }
}

impl std::error::Error for ExistingFileKeyError {}

impl FileKeyWrapMode {
    pub const fn raw(self) -> u8 {
        self as u8
    }

    pub const fn from_raw(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::A7f0),
            2 => Some(Self::Sm4),
            3 => Some(Self::Aes128Ecb),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvisionKeyMaterial {
    pub user_key_crc: u32,
    pub file_key_crc: u32,
    pub wrapped_file_key: [u8; 16],
    pub encrypt_mode: FileKeyWrapMode,
}

impl ProvisionKeyMaterial {
    pub fn packed24(self) -> [u8; 24] {
        let mut out = [0u8; 24];
        out[..4].copy_from_slice(&self.user_key_crc.to_le_bytes());
        out[4..8].copy_from_slice(&self.file_key_crc.to_le_bytes());
        out[8..].copy_from_slice(&self.wrapped_file_key);
        out
    }
}

pub fn wrap_file_key(
    original_password: &[u8],
    file_key: [u8; 16],
    mode: FileKeyWrapMode,
) -> ProvisionKeyMaterial {
    let digest = md5_digest(effective_password(original_password));
    let wrapped_file_key = match mode {
        FileKeyWrapMode::A7f0 => a7f0_encrypt(&file_key, &digest, 0),
        FileKeyWrapMode::Sm4 => sm4_encrypt_block(&file_key, &digest),
        FileKeyWrapMode::Aes128Ecb => aes128_ecb_encrypt_block(&file_key, &digest),
    };
    ProvisionKeyMaterial {
        user_key_crc: crc32_bare(original_password),
        file_key_crc: crc32_bare(&file_key),
        wrapped_file_key,
        encrypt_mode: mode,
    }
}

pub fn unwrap_file_key(
    password: Option<&[u8]>,
    material: ProvisionKeyMaterial,
) -> Result<[u8; 16], ExistingFileKeyError> {
    if material.file_key_crc == 0 && material.wrapped_file_key == [0; 16] {
        return Err(ExistingFileKeyError::MalformedKeyRecord);
    }
    let password = password
        .filter(|password| !password.is_empty())
        .ok_or(ExistingFileKeyError::PasswordRequired)?;
    if crc32_bare(password) != material.user_key_crc {
        return Err(ExistingFileKeyError::PasswordMismatch);
    }
    let digest = md5_digest(effective_password(password));
    let key = match material.encrypt_mode {
        FileKeyWrapMode::A7f0 => a6b0_decrypt(&material.wrapped_file_key, &digest, 0),
        FileKeyWrapMode::Sm4 => sm4_decrypt_block(&material.wrapped_file_key, &digest),
        FileKeyWrapMode::Aes128Ecb => aes128_ecb_decrypt_block(&material.wrapped_file_key, &digest),
    };
    if crc32_bare(&key) != material.file_key_crc {
        return Err(ExistingFileKeyError::FileKeyCrcMismatch);
    }
    Ok(key)
}

fn effective_password(password: &[u8]) -> &[u8] {
    if password == DEFAULT_PASSWORD {
        DEFAULT_EFFECTIVE_PASSWORD
    } else {
        password
    }
}

pub(crate) fn md5_digest(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((input.len() + 72) & !63);
    padded.extend_from_slice(input);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_le_bytes());

    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    for chunk in padded.as_chunks::<64>().0 {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = a
                .wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(m[g])
                .rotate_left(S[i])
                .wrapping_add(b);
            a = d;
            d = c;
            c = b;
            b = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    let mut out = [0u8; 16];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex16(value: &str) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn legacy_default_password_fold_matches_first_party_vector() {
        assert_eq!(legacy_password_fold32(b"0000aaaa"), 0x9191_9191);
        let material = wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        );
        assert_eq!(material.user_key_crc, 0x0429_735d);
        assert_eq!(material.file_key_crc, 0xf169_bc97);
        assert_eq!(
            material.wrapped_file_key,
            [0xec, 0x0f, 0x75, 0x79, 0xe4, 0xdb, 0x45, 0xa9]
        );
    }

    #[test]
    fn md5_matches_standard_and_first_party_default_password_vectors() {
        assert_eq!(md5_digest(b""), hex16("d41d8cd98f00b204e9800998ecf8427e"));
        assert_eq!(
            md5_digest(b"abc"),
            hex16("900150983cd24fb0d6963f7d28e17f72")
        );
        assert_eq!(
            md5_digest(DEFAULT_EFFECTIVE_PASSWORD),
            hex16("548b072cba7f104d88a446556cc3c432")
        );
    }

    #[test]
    fn existing_file_key_verification_covers_all_wrap_modes_and_typed_failures() {
        let file_key = [0x42; 16];
        for mode in [
            FileKeyWrapMode::A7f0,
            FileKeyWrapMode::Sm4,
            FileKeyWrapMode::Aes128Ecb,
        ] {
            let material = wrap_file_key(b"ProofPass1!", file_key, mode);
            assert_eq!(
                unwrap_file_key(Some(b"ProofPass1!"), material),
                Ok(file_key)
            );
            assert_eq!(
                unwrap_file_key(None, material),
                Err(ExistingFileKeyError::PasswordRequired)
            );
            assert_eq!(
                unwrap_file_key(Some(b"wrong"), material),
                Err(ExistingFileKeyError::PasswordMismatch)
            );
            let damaged = ProvisionKeyMaterial {
                file_key_crc: material.file_key_crc ^ 1,
                ..material
            };
            assert_eq!(
                unwrap_file_key(Some(b"ProofPass1!"), damaged),
                Err(ExistingFileKeyError::FileKeyCrcMismatch)
            );
        }
        let default_material = wrap_file_key(b"0000aaaa", file_key, FileKeyWrapMode::Sm4);
        assert_eq!(
            unwrap_file_key(Some(b"0000aaaa"), default_material),
            Ok(file_key)
        );
        assert_eq!(
            unwrap_file_key(
                Some(b"ProofPass1!"),
                ProvisionKeyMaterial {
                    user_key_crc: crc32_bare(b"ProofPass1!"),
                    file_key_crc: 0,
                    wrapped_file_key: [0; 16],
                    encrypt_mode: FileKeyWrapMode::Sm4,
                }
            ),
            Err(ExistingFileKeyError::MalformedKeyRecord)
        );
    }
}

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
