//! Current LBA12 file-key wrapping used by the first-party writer.

use crate::crypto::{
    a6b0_decrypt, a7f0_encrypt, aes128_ecb_decrypt_block, aes128_ecb_encrypt_block, crc32_bare,
    sm4_decrypt_block, sm4_encrypt_block,
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
