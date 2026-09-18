//! cems 盘内置加密原语(逆向 cemsusbregsiter.dll / sectormanage64.dll 得到)。
//!
//!   device_id → CRC32(bare: init=0, poly 0xEDB88320, 无final-xor)
//!   LBA7  K0 = low16(CRC) ^ high16(CRC), 整扇 16位字滚动 XOR(逐字递减1)
//!   LBA12 key = CRC的4字节×4 ^ "EDPSECDISK200709" → AES-128 变体(A6B0/a7f0,
//!          counter=块号×16), 仅加密前 368B, 尾部 144B 为原始数据
//!   LBA6  固定 K0=0x4DAA 滚动 XOR; 0x1FC 校验和 = 密文CRC32(bare)×10轮
//!          ((v>>15)+(v<<1)) 变换, 小端写入

use crate::common::SECTOR;

pub const LBA6_K0: u32 = 0x4DAA; // LBA6 SAFE6 固定滚动 XOR key(跨盘通用)
pub const KEY_TABLE: &[u8; 16] = b"EDPSECDISK200709";

// ══════════════════════════════════════════════════════════════════
// 1. CRC32 (bare: init=0, poly 0xEDB88320, 无 final-xor) — sub_10001130
// ══════════════════════════════════════════════════════════════════
const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut b = 0;
        while b < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB88320
            } else {
                c >> 1
            };
            b += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

pub const CRC_TABLE: [u32; 256] = build_crc_table();

pub fn crc32_bare(data: &[u8]) -> u32 {
    let mut c: u32 = 0;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c
}

// ══════════════════════════════════════════════════════════════════
// 2. AES-128 变体 (A6B0 解密 / a7f0 加密) — sub_1003a6b0 / sub_1003a7f0
//    key = CRC32(device_id) 4字节重复4遍 ^ "EDPSECDISK200709"
//    counter XOR 进 round keys (counter = 块号*16)
// ══════════════════════════════════════════════════════════════════
pub const SBOX: [u8; 256] = [
    99, 124, 119, 123, 242, 107, 111, 197, 48, 1, 103, 43, 254, 215, 171, 118, 202, 130, 201, 125,
    250, 89, 71, 240, 173, 212, 162, 175, 156, 164, 114, 192, 183, 253, 147, 38, 54, 63, 247, 204,
    52, 165, 229, 241, 113, 216, 49, 21, 4, 199, 35, 195, 24, 150, 5, 154, 7, 18, 128, 226, 235,
    39, 178, 117, 9, 131, 44, 26, 27, 110, 90, 160, 82, 59, 214, 179, 41, 227, 47, 132, 83, 209, 0,
    237, 32, 252, 177, 91, 106, 203, 190, 57, 74, 76, 88, 207, 208, 239, 170, 251, 67, 77, 51, 133,
    69, 249, 2, 127, 80, 60, 159, 168, 81, 163, 64, 143, 146, 157, 56, 245, 188, 182, 218, 33, 16,
    255, 243, 210, 205, 12, 19, 236, 95, 151, 68, 23, 196, 167, 126, 61, 100, 93, 25, 115, 96, 129,
    79, 220, 34, 42, 144, 136, 70, 238, 184, 20, 222, 94, 11, 219, 224, 50, 58, 10, 73, 6, 36, 92,
    194, 211, 172, 98, 145, 149, 228, 121, 231, 200, 55, 109, 141, 213, 78, 169, 108, 86, 244, 234,
    101, 122, 174, 8, 186, 120, 37, 46, 28, 166, 180, 198, 232, 221, 116, 31, 75, 189, 139, 138,
    112, 62, 181, 102, 72, 3, 246, 14, 97, 53, 87, 185, 134, 193, 29, 158, 225, 248, 152, 17, 105,
    217, 142, 148, 155, 30, 135, 233, 206, 85, 40, 223, 140, 161, 137, 13, 191, 230, 66, 104, 65,
    153, 45, 15, 176, 84, 187, 22,
];
pub const SBOX2: [u8; 256] = [
    82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
    155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
    238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73, 109,
    139, 209, 37, 114, 248, 246, 100, 134, 104, 152, 22, 212, 164, 92, 204, 93, 101, 182, 146, 108,
    112, 72, 80, 253, 237, 185, 218, 94, 21, 70, 87, 167, 141, 157, 132, 144, 216, 171, 0, 140,
    188, 211, 10, 247, 228, 88, 5, 184, 179, 69, 6, 208, 44, 30, 143, 202, 63, 15, 2, 193, 175,
    189, 3, 1, 19, 138, 107, 58, 145, 17, 65, 79, 103, 220, 234, 151, 242, 207, 206, 240, 180, 230,
    115, 150, 172, 116, 34, 231, 173, 53, 133, 226, 249, 55, 232, 28, 117, 223, 110, 71, 241, 26,
    113, 29, 41, 197, 137, 111, 183, 98, 14, 170, 24, 190, 27, 252, 86, 62, 75, 198, 210, 121, 32,
    154, 219, 192, 254, 120, 205, 90, 244, 31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39,
    128, 236, 95, 96, 81, 127, 169, 25, 181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160,
    224, 59, 77, 174, 42, 245, 176, 200, 235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119,
    214, 38, 225, 105, 20, 99, 85, 33, 12, 125,
];
pub const RCON: [u8; 11] = [0, 1, 2, 4, 8, 16, 32, 64, 128, 27, 54];

fn aes_expand(key16: &[u8; 16]) -> [[u8; 4]; 44] {
    let mut w = [[0u8; 4]; 44];
    for i in 0..4 {
        w[i] = [
            key16[i * 4],
            key16[i * 4 + 1],
            key16[i * 4 + 2],
            key16[i * 4 + 3],
        ];
    }
    for i in 4..44 {
        let mut t = w[i - 1];
        if i % 4 == 0 {
            t = [t[1], t[2], t[3], t[0]];
            for b in t.iter_mut() {
                *b = SBOX[*b as usize];
            }
            t[0] ^= RCON[i / 4];
        }
        for j in 0..4 {
            w[i][j] = w[i - 4][j] ^ t[j];
        }
    }
    w
}

fn shift_rows(s: &[u8; 16]) -> [u8; 16] {
    [
        s[0], s[5], s[10], s[15], s[4], s[9], s[14], s[3], s[8], s[13], s[2], s[7], s[12], s[1],
        s[6], s[11],
    ]
}
fn inv_shift(s: &[u8; 16]) -> [u8; 16] {
    [
        s[0], s[13], s[10], s[7], s[4], s[1], s[14], s[11], s[8], s[5], s[2], s[15], s[12], s[9],
        s[6], s[3],
    ]
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let h = a & 0x80;
        a = a.wrapping_shl(1); // (a << 1) & 0xFF
        if h != 0 {
            a ^= 0x1B;
        }
        b >>= 1;
    }
    p
}

fn mix_column(col: &[u8; 4]) -> [u8; 4] {
    let (a, b, c, d) = (col[0], col[1], col[2], col[3]);
    [
        gf_mul(a, 2) ^ gf_mul(b, 3) ^ c ^ d,
        a ^ gf_mul(b, 2) ^ gf_mul(c, 3) ^ d,
        a ^ b ^ gf_mul(c, 2) ^ gf_mul(d, 3),
        gf_mul(a, 3) ^ b ^ c ^ gf_mul(d, 2),
    ]
}

fn imix(col: &[u8; 4]) -> [u8; 4] {
    let (a, b, c, d) = (col[0], col[1], col[2], col[3]);
    [
        gf_mul(a, 14) ^ gf_mul(b, 11) ^ gf_mul(c, 13) ^ gf_mul(d, 9),
        gf_mul(a, 9) ^ gf_mul(b, 14) ^ gf_mul(c, 11) ^ gf_mul(d, 13),
        gf_mul(a, 13) ^ gf_mul(b, 9) ^ gf_mul(c, 14) ^ gf_mul(d, 11),
        gf_mul(a, 11) ^ gf_mul(b, 13) ^ gf_mul(c, 9) ^ gf_mul(d, 14),
    ]
}

/// 轮密钥展开 + counter 混入(cb 的 8 字节循环 XOR 进全部 176 个轮密钥字节)。
fn expand_with_counter(key_raw: &[u8], counter: u32) -> [[u8; 4]; 44] {
    let mut expanded = [0u8; 16];
    for i in 0..16 {
        expanded[i] = key_raw[i % key_raw.len()] ^ KEY_TABLE[i];
    }
    let mut rk = aes_expand(&expanded);
    let cb: [u8; 8] = {
        let mut b = [0u8; 8];
        b[0..4].copy_from_slice(&counter.to_le_bytes());
        b
    };
    for wi in 0..44 {
        for bi in 0..4 {
            rk[wi][bi] ^= cb[(wi * 4 + bi) % 8];
        }
    }
    rk
}

/// A6B0 单块解密(data 恰 16B)。
pub fn a6b0_decrypt(data: &[u8], key_raw: &[u8], counter: u32) -> [u8; 16] {
    assert!(data.len() == 16, "A6B0 块长须 16");
    let rk = expand_with_counter(key_raw, counter);
    let mut s = [0u8; 16];
    s.copy_from_slice(data);
    for i in 0..16 {
        s[i] ^= rk[40 + i / 4][i % 4]; // flatten(rk[40:44])
    }
    for rnd in (1..=9).rev() {
        s = inv_shift(&s);
        for b in s.iter_mut() {
            *b = SBOX2[*b as usize];
        }
        for i in 0..16 {
            s[i] ^= rk[rnd * 4 + i / 4][i % 4];
        }
        for c in (0..16).step_by(4) {
            let col: [u8; 4] = s[c..c + 4].try_into().unwrap();
            s[c..c + 4].copy_from_slice(&imix(&col));
        }
    }
    s = inv_shift(&s);
    for b in s.iter_mut() {
        *b = SBOX2[*b as usize];
    }
    for i in 0..16 {
        s[i] ^= rk[i / 4][i % 4]; // flatten(rk[0:4])
    }
    s
}

/// a7f0 单块加密(data 恰 16B)。
pub fn a7f0_encrypt(data: &[u8], key_raw: &[u8], counter: u32) -> [u8; 16] {
    assert!(data.len() == 16, "a7f0 块长须 16");
    let rk = expand_with_counter(key_raw, counter);
    let mut s = [0u8; 16];
    s.copy_from_slice(data);
    for i in 0..16 {
        s[i] ^= rk[i / 4][i % 4];
    }
    for rnd in 1..=9 {
        for b in s.iter_mut() {
            *b = SBOX[*b as usize];
        }
        s = shift_rows(&s);
        for c in (0..16).step_by(4) {
            let col: [u8; 4] = s[c..c + 4].try_into().unwrap();
            s[c..c + 4].copy_from_slice(&mix_column(&col));
        }
        for i in 0..16 {
            s[i] ^= rk[rnd * 4 + i / 4][i % 4];
        }
    }
    for b in s.iter_mut() {
        *b = SBOX[*b as usize];
    }
    s = shift_rows(&s);
    for i in 0..16 {
        s[i] ^= rk[40 + i / 4][i % 4];
    }
    s
}

/// A6B0 连续块解密(长度须 16 的倍数; counter 每块 +16)。
pub fn a6b0_full(data: &[u8], key: &[u8], initial_counter: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut ctr = initial_counter;
    let (blocks, remainder) = data.as_chunks::<16>();
    debug_assert!(remainder.is_empty(), "A6B0 输入长度必须是 16 的倍数");
    for blk in blocks {
        out.extend_from_slice(&a6b0_decrypt(blk, key, ctr));
        ctr = ctr.wrapping_add(16);
    }
    out
}

/// a7f0 连续块加密(长度须 16 的倍数; counter 每块 +16)。
pub fn a7f0_full(data: &[u8], key: &[u8], initial_counter: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut ctr = initial_counter;
    let (blocks, remainder) = data.as_chunks::<16>();
    debug_assert!(remainder.is_empty(), "a7f0 输入长度必须是 16 的倍数");
    for blk in blocks {
        out.extend_from_slice(&a7f0_encrypt(blk, key, ctr));
        ctr = ctr.wrapping_add(16);
    }
    out
}

// ══════════════════════════════════════════════════════════════════
// 3. 滚动 XOR (16位字, key 逐字递减1) — sub_10013de0 / sub_10013fd0
// ══════════════════════════════════════════════════════════════════
pub fn xor_rolling(data: &[u8], k0: u32) -> Vec<u8> {
    let mut r = data.to_vec();
    let mut key: u32 = k0 & 0xFFFF;
    for i in 0..r.len() / 2 {
        let off = i * 2;
        let w = (u16::from_le_bytes([r[off], r[off + 1]]) as u32) ^ key;
        r[off..off + 2].copy_from_slice(&(w as u16).to_le_bytes());
        // key_{i+1} = key_i + 0x100 - i - 1; i ≤ 255 时无下溢(u32 足够)
        key = (key.wrapping_add(0x100).wrapping_sub(i as u32 + 1)) & 0xFFFF;
    }
    r
}

// ══════════════════════════════════════════════════════════════════
// 4. LBA6 (SAFE6): 解密 + 校验和
// ══════════════════════════════════════════════════════════════════
/// 对 XOR 加密后的密文: CRC32(bare) 后 10 轮 ((v>>15)+(v<<1)) 变换。
pub fn lba6_checksum(cipher_508b: &[u8]) -> u32 {
    let mut c: u32 = 0;
    for &b in cipher_508b {
        c = (c >> 8) ^ CRC_TABLE[((c & 0xFF) ^ b as u32) as usize];
    }
    for _ in 0..10 {
        c = (c >> 15).wrapping_add(c.wrapping_shl(1));
    }
    c
}

/// 前 508B 滚动 XOR 解密, 后 4B 校验和(写入在加密之后, 不参与)。
pub fn lba6_decode(raw: &[u8]) -> Vec<u8> {
    let mut out = xor_rolling(&raw[..0x1FC], LBA6_K0);
    out.extend_from_slice(&raw[0x1FC..SECTOR]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed
    }

    #[test]
    fn crc32_empty_is_zero() {
        assert_eq!(crc32_bare(b""), 0);
    }

    #[test]
    fn crc32_golden_device_ids() {
        // 拆包前单文件版实测值
        for (did, crc, k0) in [
            ("disk&ven_netac&prod_onlydisk", 0xF1A78819u32, 0x79BEu32),
            ("disk&ven_lexar&prod_usb_flash_drive", 0x6BBAEEFB, 0x8541),
            ("disk&ven_aigo&prod_u335&rev_pmap", 0x2EEB4CE1, 0x620A),
        ] {
            let c = crc32_bare(did.as_bytes());
            assert_eq!(c, crc, "{}", did);
            assert_eq!((c & 0xFFFF) ^ (c >> 16), k0, "{}", did);
        }
    }

    #[test]
    fn xor_rolling_roundtrip() {
        let mut seed = 42u64;
        for _ in 0..20 {
            let len = [2usize, 64, 512][(lcg(&mut seed) % 3) as usize];
            let d: Vec<u8> = (0..len).map(|_| (lcg(&mut seed) & 0xFF) as u8).collect();
            let k = (lcg(&mut seed) % 0x10000) as u32;
            assert_eq!(xor_rolling(&xor_rolling(&d, k), k), d);
        }
    }

    #[test]
    fn xor_rolling_key_schedule() {
        // 密钥演进 key_{i+1} = key_i + 0x100 - i - 1: 两个字全零明文 → 密文即各位置密钥
        let out = xor_rolling(&[0u8; 4], 0x1234);
        let w0 = u16::from_le_bytes([out[0], out[1]]);
        let w1 = u16::from_le_bytes([out[2], out[3]]);
        assert_eq!(w0, 0x1234);
        assert_eq!(w1, (0x1234u32.wrapping_add(0x100).wrapping_sub(1)) as u16);
    }

    #[test]
    fn aes_variant_roundtrip() {
        let mut seed = 7u64;
        for _ in 0..5 {
            let d: Vec<u8> = (0..368).map(|_| (lcg(&mut seed) & 0xFF) as u8).collect();
            let key: Vec<u8> = (0..4).map(|_| (lcg(&mut seed) & 0xFF) as u8).collect();
            assert_eq!(a6b0_full(&a7f0_full(&d, &key, 0), &key, 0), d);
        }
    }

    #[test]
    fn aes_counter_is_block_number_times_16() {
        let mut seed = 8u64;
        let d: Vec<u8> = (0..48).map(|_| (lcg(&mut seed) & 0xFF) as u8).collect();
        let key: Vec<u8> = vec![0, 1, 2, 3];
        assert_eq!(
            a6b0_full(&a7f0_full(&d, &key, 0), &key, 0),
            a6b0_full(&a7f0_full(&d, &key, 16), &key, 16)
        );
    }

    #[test]
    fn lba6_decode_splits_checksum() {
        let raw: Vec<u8> = (0..256)
            .map(|i| i as u8)
            .chain((0..256).map(|i| i as u8))
            .collect();
        let dec = lba6_decode(&raw);
        assert_eq!(dec.len(), SECTOR);
        assert_eq!(&dec[0x1FC..], &raw[0x1FC..]); // 尾 4B 校验和原样透传
    }

    #[test]
    fn lba6_checksum_is_rot10_of_crc() {
        fn rot10(mut v: u32) -> u32 {
            for _ in 0..10 {
                v = (v >> 15).wrapping_add(v.wrapping_shl(1));
            }
            v
        }
        let d = vec![1u8; 508];
        assert_eq!(lba6_checksum(&d), rot10(crc32_bare(&d)));
        assert_ne!(lba6_checksum(&d), crc32_bare(&d)); // 旋转变换改变值
    }
}
