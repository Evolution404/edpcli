//! Current LBA12 file-key wrapping used by the first-party writer.

use crate::{
    backup_deep::keys::sm4_encrypt_block,
    crypto::{a7f0_encrypt, aes128_ecb_encrypt_block, crc32_bare},
};

const DEFAULT_PASSWORD: &[u8] = b"0000aaaa";
const DEFAULT_EFFECTIVE_PASSWORD: &[u8] = b"LtSWi[2f)j";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FileKeyWrapMode {
    A7f0 = 1,
    Sm4 = 2,
    Aes128Ecb = 3,
}

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
    let effective_password = if original_password == DEFAULT_PASSWORD {
        DEFAULT_EFFECTIVE_PASSWORD
    } else {
        original_password
    };
    let digest = md5_digest(effective_password);
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
    for chunk in padded.chunks_exact(64) {
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
}
