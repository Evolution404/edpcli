//! Provision Phase 0 protocol audit.
//!
//! These tests intentionally use curated real-device LBA0-12 protocol fixtures as evidence.
//! They do not open or mutate a physical disk.

mod common;

#[path = "support/gold_name.rs"]
mod gold_name;

use std::{collections::HashSet, fs};

use common::FIXTURE_DIR;
use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::crypto::{
    a6b0_decrypt, a6b0_full, a7f0_full, crc32_bare, lba6_checksum, lba6_decode, xor_rolling, RCON,
    SBOX, SBOX2,
};
use edpcli::diskio::BackupMeta;
use edpcli::inspect::InspectMeta;
use edpcli::metainfo::ownership_from_lba8;
use edpcli::sha256::sha256_hex;
use encoding_rs::GBK;

fn load(name: &str) -> Vec<u8> {
    fs::read(std::path::Path::new(FIXTURE_DIR).join(name)).expect("committed protocol fixture")
}

const MIN_PROTOCOL_FIXTURES: usize = 7;
const SANDISK_LBA10: &[u8; 512] =
    include_bytes!("fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba10.bin");
const SANDISK_LBA11_HEX: &str =
    include_str!("fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba11.hex");
const SANDISK_LBA6_HEX: &str =
    include_str!("fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba6.hex");
const SANDISK_LBA12_HEX: &str =
    include_str!("fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba12.hex");
const AIGO_REV_PMAP_EXACT_SIZE_LBA11_HEX: &str =
    include_str!("fixtures/protocol_evidence/aigo_u335_rev_pmap_exact_size_lba11.hex");
const SANDISK_DEVICE_ID: &str = "disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00";
const NETAC_ONLYDISK_DEVICE_ID: &str = "disk&ven_netac&prod_onlydisk&rev_0000";
const NETAC_ONLYDISK_LBA10_HEAD_HEX: &str =
    include_str!("fixtures/protocol_evidence/netac_onlydisk_20260804_lba10_head.hex");
const NETAC_EESI_CAPTURE: &[u8; 6656] =
    include_bytes!("../audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin");
const NETAC_EESI_META: &str = include_str!(
    "../audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.meta.json"
);
const NETAC_EESI_PROVENANCE: &str =
    include_str!("../audit/protocol/physical-evidence/eesi/README.md");
const SANDISK_AUTHENTIC_NOPASS_LBA7_HEX: &str =
    include_str!("fixtures/protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex");
const SANDISK_AUTHENTIC_NOPASS_DEVICE_ID: &str = "disk&ven_sandisk&prod_ultra&rev_1.00";
const LEXAR_JOIN59_LBA6_HEX: &str =
    include_str!("fixtures/protocol_evidence/lexar_join59_lba6.hex");
const LEXAR_JOIN59_LBA9_HEX: &str =
    include_str!("fixtures/protocol_evidence/lexar_join59_lba9.hex");
const KINGSTON_20260803_MP_LBA3_HEX: &str =
    include_str!("fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex");
const OFFICIAL_VIRTUAL_WRITER_MODE1_LBA12_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode1_lba12.hex");
const OFFICIAL_VIRTUAL_WRITER_MODE2_LBA12_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode2_lba12.hex");
const OFFICIAL_VIRTUAL_WRITER_MODE3_LBA12_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode3_lba12.hex");
const OFFICIAL_VIRTUAL_GPT_LBA1_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_gpt_lba1.hex");
const OFFICIAL_VIRTUAL_GPT_LBA2_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_gpt_lba2.hex");
const OFFICIAL_VIRTUAL_LONG_USER_LBA6_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_long_user_lba6.hex");
const OFFICIAL_VIRTUAL_LONG_USER_LBA9_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_long_user_lba9.hex");
const OFFICIAL_VIRTUAL_SLOT_BACKING_LBA6_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_slot_backing_lba6.hex");
const OFFICIAL_VIRTUAL_LBA4_FULL_NONZERO_BACKING_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_lba4_full_nonzero_backing.hex");
const OFFICIAL_VIRTUAL_LBA4_NULL_NONZERO_BACKING_HEX: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_lba4_null_nonzero_backing.hex");
const AIGO_L8302_NETAC_LBA0_PREFIX_HEX: &str =
    include_str!("fixtures/protocol_evidence/aigo_l8302_netac_lba0_prefix.hex");

fn parse_reference_backup_name(name: &str) -> Option<BackupMeta> {
    let meta = gold_name::parse_gold_name(name)?;
    if meta.tagged_nopwd {
        return None;
    }
    Some(meta)
}

fn sector(image: &[u8], lba: usize) -> &[u8] {
    &image[lba * SECTOR..(lba + 1) * SECTOR]
}

fn u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn u64_le(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

fn decode_hex_fixture(text: &str) -> Vec<u8> {
    let hex: String = text
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    assert_eq!(hex.len() % 2, 0, "hex fixture must contain whole bytes");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid fixture hex"))
        .collect()
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut out = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            out ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    out
}

fn aes128_expand_key(key: &[u8; 16]) -> [u8; 176] {
    let mut out = [0u8; 176];
    out[..16].copy_from_slice(key);
    let mut generated = 16usize;
    let mut rcon_index = 1usize;

    while generated < out.len() {
        let mut temp = [
            out[generated - 4],
            out[generated - 3],
            out[generated - 2],
            out[generated - 1],
        ];
        if generated.is_multiple_of(16) {
            temp.rotate_left(1);
            for byte in &mut temp {
                *byte = SBOX[*byte as usize];
            }
            temp[0] ^= RCON[rcon_index];
            rcon_index += 1;
        }
        for byte in temp {
            out[generated] = out[generated - 16] ^ byte;
            generated += 1;
        }
    }
    out
}

fn aes128_ecb_decrypt_block(block: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    fn add_round_key(state: &mut [u8; 16], expanded: &[u8; 176], round: usize) {
        let base = round * 16;
        for (index, byte) in state.iter_mut().enumerate() {
            *byte ^= expanded[base + index];
        }
    }

    fn inv_shift_rows(state: &mut [u8; 16]) {
        let original = *state;
        state[1] = original[13];
        state[5] = original[1];
        state[9] = original[5];
        state[13] = original[9];

        state[2] = original[10];
        state[6] = original[14];
        state[10] = original[2];
        state[14] = original[6];

        state[3] = original[7];
        state[7] = original[11];
        state[11] = original[15];
        state[15] = original[3];
    }

    fn inv_sub_bytes(state: &mut [u8; 16]) {
        for byte in state {
            *byte = SBOX2[*byte as usize];
        }
    }

    fn inv_mix_columns(state: &mut [u8; 16]) {
        for column in 0..4 {
            let i = column * 4;
            let a0 = state[i];
            let a1 = state[i + 1];
            let a2 = state[i + 2];
            let a3 = state[i + 3];
            state[i] = gf_mul(a0, 14) ^ gf_mul(a1, 11) ^ gf_mul(a2, 13) ^ gf_mul(a3, 9);
            state[i + 1] = gf_mul(a0, 9) ^ gf_mul(a1, 14) ^ gf_mul(a2, 11) ^ gf_mul(a3, 13);
            state[i + 2] = gf_mul(a0, 13) ^ gf_mul(a1, 9) ^ gf_mul(a2, 14) ^ gf_mul(a3, 11);
            state[i + 3] = gf_mul(a0, 11) ^ gf_mul(a1, 13) ^ gf_mul(a2, 9) ^ gf_mul(a3, 14);
        }
    }

    let expanded = aes128_expand_key(key);
    let mut state = *block;
    add_round_key(&mut state, &expanded, 10);
    for round in (1..10).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        add_round_key(&mut state, &expanded, round);
        inv_mix_columns(&mut state);
    }
    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    add_round_key(&mut state, &expanded, 0);
    state
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

fn onlyid_bits(text: &str) -> u32 {
    if text.starts_with('-') {
        text.parse::<i32>().unwrap() as u32
    } else {
        text.parse::<u32>().unwrap()
    }
}

fn round_up_16(value: usize) -> usize {
    (value + 15) & !15
}

fn padded4_ascii(text: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    let bytes = text.as_bytes();
    let len = bytes.len().min(4);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}

fn chs_capacity(size: u64) -> u64 {
    const UNIT: u64 = 255 * 63 * 512;
    size / UNIT * UNIT
}

fn decode_edpf_tail(stored: &[u8]) -> [u8; 14] {
    assert_eq!(stored.len(), 14);
    let mut tail: [u8; 14] = stored.try_into().unwrap();
    tail[0] ^= 0x88;
    tail[3] ^= 0x88;
    tail[6] ^= 0x88;
    tail
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

fn sm4_decrypt_block(ciphertext: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
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

    let mut x = [
        u32::from_be_bytes(ciphertext[0..4].try_into().unwrap()),
        u32::from_be_bytes(ciphertext[4..8].try_into().unwrap()),
        u32::from_be_bytes(ciphertext[8..12].try_into().unwrap()),
        u32::from_be_bytes(ciphertext[12..16].try_into().unwrap()),
    ];
    for round_key in round_keys.iter().rev() {
        let b = sm4_tau(x[1] ^ x[2] ^ x[3] ^ round_key);
        let next =
            x[0] ^ b ^ b.rotate_left(2) ^ b.rotate_left(10) ^ b.rotate_left(18) ^ b.rotate_left(24);
        x = [x[1], x[2], x[3], next];
    }
    let words = [x[3], x[2], x[1], x[0]];
    let mut out = [0u8; 16];
    for (index, word) in words.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn v206_hidden_default_password() -> [u8; 10] {
    const SEED: [u8; 32] = [
        0x46, 0x8b, 0x46, 0x08, 0x8b, 0x4e, 0x04, 0x8b, 0xd0, 0x2b, 0xd1, 0x3b, 0xd3, 0x7f, 0x21,
        0x83, 0xc1, 0x09, 0x8d, 0x3c, 0x00, 0x3b, 0xf9, 0x7f, 0x02, 0x8b, 0xf9, 0x8b, 0x06, 0x57,
        0x50, 0xe8,
    ];
    const TABLE: [u8; 96] = [
        0x8a, 0xf7, 0x11, 0x65, 0x41, 0x32, 0xda, 0x09, 0x20, 0x6c, 0x57, 0xd1, 0xe4, 0x29, 0xa8,
        0x3d, 0x86, 0x9b, 0xc3, 0xec, 0x17, 0x9b, 0x26, 0xc4, 0xcc, 0xf3, 0x86, 0x53, 0x80, 0x68,
        0xc3, 0x6d, 0x8d, 0xb6, 0x54, 0x6e, 0x7c, 0xc7, 0x5d, 0x46, 0xd1, 0x89, 0x42, 0x32, 0x21,
        0xf1, 0x0a, 0xc5, 0xd0, 0x72, 0xf5, 0x57, 0x37, 0xd5, 0x55, 0xb8, 0xac, 0x8d, 0x7d, 0x3a,
        0x2b, 0x9e, 0x7d, 0xfc, 0xfc, 0x74, 0xb3, 0x18, 0x0b, 0xc4, 0xac, 0x0b, 0xb6, 0xaf, 0x79,
        0xaa, 0xd1, 0x4d, 0xe8, 0x5d, 0xad, 0x4e, 0x4f, 0x39, 0x3f, 0x96, 0x9d, 0xaa, 0xfa, 0xc8,
        0xf9, 0xf2, 0xf8, 0x14, 0x57, 0x65,
    ];
    const INDEX: [u8; 16] = [
        0xb0, 0xa5, 0x98, 0xe5, 0x96, 0x11, 0x10, 0x39, 0x2b, 0xf2, 0x3f, 0x4d, 0x6f, 0x9e, 0xc6,
        0xb6,
    ];
    let xor_key: [u8; 16] = std::array::from_fn(|i| SEED[i] ^ SEED[i + 16]);
    let add_key: [u8; 16] = std::array::from_fn(|i| SEED[i].wrapping_add(SEED[i + 16]));
    let decoded_table = a6b0_full(&TABLE, &xor_key, 0);
    let decoded_index = a6b0_full(&INDEX, &add_key, 0);
    std::array::from_fn(|i| decoded_table[decoded_index[i] as usize])
}

const NETAC_A: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin";
const NETAC_B: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid3069787975_20260910_172525.bin";
const LEXAR: &str =
    "disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin";

#[test]
fn authentic_no_password_lba7_is_a_real_two_entry_profile_not_a_generated_reference() {
    let raw = decode_hex_fixture(SANDISK_AUTHENTIC_NOPASS_LBA7_HEX);
    assert_eq!(raw.len(), SECTOR);

    let crc = crc32_bare(SANDISK_AUTHENTIC_NOPASS_DEVICE_ID.as_bytes());
    let plain = xor_rolling(&raw, (crc & 0xffff) ^ (crc >> 16));

    assert_eq!(&plain[0x00..0x04], b"EDPF");
    assert_eq!(
        u32_le(&plain, 0x04),
        0,
        "entry-local Version is not the two-entry count"
    );
    assert_eq!(u32_le(&plain, 0x08), 2, "PartionCount is stored at +0x08");

    assert_eq!(u32_le(&plain, 0x0c), 2);
    assert_eq!(u32_le(&plain, 0x10), 1);
    assert_eq!(u32_le(&plain, 0x14), 1);

    assert_eq!(&plain[0x40..0x44], b"EDPF");
    assert_eq!(u32_le(&plain, 0x44), 0);
    assert_eq!(u32_le(&plain, 0x48), 2);
    assert_eq!(u32_le(&plain, 0x4c), 4);
    assert_eq!(u32_le(&plain, 0x50), 1);
    assert_eq!(u32_le(&plain, 0x54), 1);

    let tail = decode_edpf_tail(&plain[0xc0..0xce]);
    assert_eq!(u16::from_le_bytes([tail[0], tail[1]]), 0x0064);
    assert_eq!(tail[0x0a], 0);
    assert_eq!(tail[0x0c], 0);
    assert_eq!(tail[0x0d], 0);
}

#[test]
fn lba7_entry_version_is_not_partition_count_across_real_profiles() {
    let mut checked_entries = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = xor_rolling(sector(&image, 7), (crc & 0xffff) ^ (crc >> 16));
        let count = u32_le(&plain, 0x08) as usize;

        assert!((2..=3).contains(&count), "unexpected LBA7 count: {name}");
        for index in 0..count {
            let base = index * 0x40;
            assert_eq!(&plain[base..base + 4], b"EDPF", "{name} entry {index}");
            assert_eq!(
                u32_le(&plain, base + 0x04),
                0,
                "entry Version must stay distinct from PartionCount: {name} entry {index}"
            );
            let expected_need_disturb = if index < 2 { 1 } else { 0 };
            assert_eq!(
                u32_le(&plain, base + 0x10),
                expected_need_disturb,
                "LBA7 positional NeedDisturb compatibility profile changed: {name} entry {index}"
            );
            assert_eq!(
                u32_le(&plain, base + 0x08),
                count as u32,
                "PartionCount must be stored at +0x08: {name} entry {index}"
            );
            checked_entries += 1;
        }
    }

    let raw = decode_hex_fixture(SANDISK_AUTHENTIC_NOPASS_LBA7_HEX);
    let crc = crc32_bare(SANDISK_AUTHENTIC_NOPASS_DEVICE_ID.as_bytes());
    let plain = xor_rolling(&raw, (crc & 0xffff) ^ (crc >> 16));
    assert_eq!(u32_le(&plain, 0x08), 2);
    for base in [0x00, 0x40] {
        assert_eq!(u32_le(&plain, base + 0x04), 0);
        assert_eq!(u32_le(&plain, base + 0x08), 2);
        assert_eq!(u32_le(&plain, base + 0x10), 1);
        checked_entries += 1;
    }

    assert!(
        checked_entries >= MIN_PROTOCOL_FIXTURES * 3,
        "protocol audit unexpectedly lost LBA7 Version evidence"
    );
}

#[test]
fn lba7_need_disturb_is_not_a_partition_type_invariant() {
    let standard = load(LEXAR);
    let standard_meta = parse_reference_backup_name(LEXAR).expect("Lexar fixture metadata");
    let standard_crc = crc32_bare(standard_meta.device_id.as_bytes());
    let standard_lba7 = xor_rolling(
        sector(&standard, 7),
        (standard_crc & 0xffff) ^ (standard_crc >> 16),
    );
    assert_eq!(u32_le(&standard_lba7, 0x80 + 0x0c), 4);
    assert_eq!(
        u32_le(&standard_lba7, 0x80 + 0x10),
        0,
        "standard three-entry type4 profile changed"
    );

    let raw = decode_hex_fixture(SANDISK_AUTHENTIC_NOPASS_LBA7_HEX);
    let crc = crc32_bare(SANDISK_AUTHENTIC_NOPASS_DEVICE_ID.as_bytes());
    let no_password_lba7 = xor_rolling(&raw, (crc & 0xffff) ^ (crc >> 16));
    assert_eq!(u32_le(&no_password_lba7, 0x40 + 0x0c), 4);
    assert_eq!(
        u32_le(&no_password_lba7, 0x40 + 0x10),
        1,
        "authentic two-entry type4 profile must remain positive"
    );
}

#[test]
fn lba12_v206_hidden_default_password_wraps_real_mode2_file_keys() {
    const DEFAULT_USER_KEY_CRC: u32 = 0x0429_735d;
    const HIDDEN_PASSWORD_MD5: [u8; 16] = [
        0x54, 0x8b, 0x07, 0x2c, 0xba, 0x7f, 0x10, 0x4d, 0x88, 0xa4, 0x46, 0x55, 0x6c, 0xc3, 0xc4,
        0x32,
    ];
    const NONDEFAULT_NETAC: &str =
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid3274129259_20260910_172709.bin";

    assert_eq!(&v206_hidden_default_password(), b"LtSWi[2f)j");
    assert_eq!(crc32_bare(b"0000aaaa"), DEFAULT_USER_KEY_CRC);

    let mut checked_default_entries = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let key_crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &key_crc.to_le_bytes(), 0);

        for index in 0..3 {
            let base = index * 0x60;
            if &plain[base..base + 4] != b"EDPF"
                || plain[base + 0x58] != 2
                || u32_le(&plain, base + 0x30) != DEFAULT_USER_KEY_CRC
            {
                continue;
            }
            let wrapped: [u8; 16] = plain[base + 0x38..base + 0x48].try_into().unwrap();
            let file_key = sm4_decrypt_block(&wrapped, &HIDDEN_PASSWORD_MD5);
            assert_eq!(
                crc32_bare(&file_key),
                u32_le(&plain, base + 0x34),
                "v0x0206 default-password wrapped key failed CRC: {name} entry {index}"
            );
            checked_default_entries += 1;
        }
    }
    assert!(
        checked_default_entries >= 12,
        "protocol fixture set lost the default-password mode2 wrapped-key evidence"
    );

    let image = load(NONDEFAULT_NETAC);
    let meta = parse_reference_backup_name(NONDEFAULT_NETAC).expect("Netac fixture metadata");
    let key_crc = crc32_bare(meta.device_id.as_bytes());
    let plain = a6b0_full(sector(&image, 12), &key_crc.to_le_bytes(), 0);
    let type2 = &plain[0x60..0xc0];
    let type4 = &plain[0xc0..0x120];
    assert_eq!(u32_le(type2, 0x0c), 2);
    assert_eq!(u32_le(type4, 0x0c), 4);
    assert_ne!(u32_le(type2, 0x30), DEFAULT_USER_KEY_CRC);
    assert_eq!(u32_le(type4, 0x30), DEFAULT_USER_KEY_CRC);

    let wrapped_type4: [u8; 16] = type4[0x38..0x48].try_into().unwrap();
    let shared_file_key = sm4_decrypt_block(&wrapped_type4, &HIDDEN_PASSWORD_MD5);
    let shared_crc = crc32_bare(&shared_file_key);
    assert_eq!(shared_crc, u32_le(type4, 0x34));
    assert_eq!(
        shared_crc,
        u32_le(type2, 0x34),
        "the non-default Share entry must reference the same generated file key as Encrypt"
    );
    assert_ne!(
        &type2[0x38..0x48],
        &type4[0x38..0x48],
        "different effective passwords must not be mistaken for identical wrapping material"
    );
}

#[test]
fn official_virtual_gpt_builder_emits_valid_lba1_and_entry0() {
    // First-party Linux libcemsfilesyscheck BuildSector0/1/2_Gpt output from an
    // isolated Unicorn run. The harness pre-zeroed the 34-sector staging area,
    // then the official builders emitted the protective entry, entry0 and the
    // complete primary header. Harness-owned unused-entry zeros are not treated
    // as official producer evidence.
    const BASIC_DATA_GUID: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ];
    const PARTITION_GUID: [u8; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    const TOTAL_LBA: u64 = 4_194_304;

    let lba1 = decode_hex_fixture(OFFICIAL_VIRTUAL_GPT_LBA1_HEX);
    let lba2 = decode_hex_fixture(OFFICIAL_VIRTUAL_GPT_LBA2_HEX);
    assert_eq!(lba1.len(), SECTOR);
    assert_eq!(lba2.len(), SECTOR);

    assert_eq!(&lba1[0x00..0x08], b"EFI PART");
    assert_eq!(u32_le(&lba1, 0x08), 0x0001_0000);
    assert_eq!(u32_le(&lba1, 0x0c), 0x5c);
    assert_eq!(u32_le(&lba1, 0x14), 0);
    assert_eq!(u64_le(&lba1, 0x18), 1);
    assert_eq!(u64_le(&lba1, 0x20), TOTAL_LBA - 1);
    assert_eq!(u64_le(&lba1, 0x28), 34);
    assert_eq!(u64_le(&lba1, 0x30), TOTAL_LBA - 34);
    assert_eq!(&lba1[0x38..0x48], &BASIC_DATA_GUID);
    assert_eq!(u64_le(&lba1, 0x48), 2);
    assert_eq!(u32_le(&lba1, 0x50), 128);
    assert_eq!(u32_le(&lba1, 0x54), 128);
    assert!(lba1[0x5c..].iter().all(|byte| *byte == 0));

    let mut header_for_crc = lba1[..0x5c].to_vec();
    header_for_crc[0x10..0x14].fill(0);
    assert_eq!(
        u32_le(&lba1, 0x10),
        crc32_ieee(&header_for_crc),
        "official BuildSector1_Gpt header CRC"
    );

    // BuildSector1_Gpt CRCs the full 128-entry / 16 KiB array. In this
    // deterministic run only entry0 is official-builder populated; all later
    // zeros belong to the harness staging buffer and remain PARTIAL evidence.
    let mut partition_array = vec![0u8; 32 * SECTOR];
    partition_array[..SECTOR].copy_from_slice(&lba2);
    assert_eq!(
        u32_le(&lba1, 0x58),
        crc32_ieee(&partition_array),
        "official BuildSector1_Gpt partition-array CRC"
    );

    assert_eq!(&lba2[0x00..0x10], &BASIC_DATA_GUID);
    assert_eq!(&lba2[0x10..0x20], &PARTITION_GUID);
    assert_eq!(u64_le(&lba2, 0x20), 63);
    assert_eq!(u64_le(&lba2, 0x28), TOTAL_LBA - 34);
    assert_eq!(u64_le(&lba2, 0x30), 0);
    assert!(lba2[0x38..0x80].iter().all(|byte| *byte == 0));
    assert!(lba2[0x80..].iter().all(|byte| *byte == 0));
}

#[test]
fn one_partition_gpt_keeps_entries1_to3_partition_type_guids_unused() {
    // The virtual builder fixture's residual bytes were pre-zeroed by the
    // harness, so only assert the three 16-byte PartitionTypeGUID fields here.
    // Those fields are the standards-defined discriminator for an unused GPT
    // entry; the remaining 112 bytes per entry deliberately stay PARTIAL.
    let lba2 = decode_hex_fixture(OFFICIAL_VIRTUAL_GPT_LBA2_HEX);
    assert_eq!(lba2.len(), SECTOR);
    for offset in [0x80usize, 0x100, 0x180] {
        assert!(
            lba2[offset..offset + 0x10].iter().all(|byte| *byte == 0),
            "unused GPT PartitionTypeGUID at {offset:#x} must be zero"
        );
    }
}

#[test]
fn lba12_official_virtual_writer_executes_mode1_mode2_mode3_wrapping_paths() {
    // These are not real-device captures.  They are deterministic 512-byte LBA12
    // outputs emitted by the official CEMSUsbRegsiter.dll CreatePartitions path
    // under an isolated Unicorn virtual-disk harness.  Keep them separate from
    // FIXTURE_DIR's real-device reference population.
    const DEVICE_ID: &[u8] = b"disk&ven_virtual&prod_writerproof&rev_0001";
    const PASSWORD: &[u8] = b"ProofPass1!";
    const PASSWORD_MD5: [u8; 16] = [
        0xe9, 0xc7, 0x0f, 0xce, 0xb0, 0x7e, 0x32, 0x63, 0x70, 0xb8, 0xcd, 0xc0, 0x85, 0x3a, 0xa3,
        0x9f,
    ];
    const EXPECTED_USER_KEY_CRC: u32 = 0xe5a0_95a1;
    const EXPECTED_FILE_KEY_CRC: u32 = 0xff4c_1d36;
    const EXPECTED_FILE_KEY: [u8; 16] = [
        0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb,
        0x42,
    ];
    let fixtures = [
        (1u8, OFFICIAL_VIRTUAL_WRITER_MODE1_LBA12_HEX),
        (2u8, OFFICIAL_VIRTUAL_WRITER_MODE2_LBA12_HEX),
        (3u8, OFFICIAL_VIRTUAL_WRITER_MODE3_LBA12_HEX),
    ];

    assert_eq!(crc32_bare(PASSWORD), EXPECTED_USER_KEY_CRC);
    let outer_crc = crc32_bare(DEVICE_ID);
    let mut unwrapped = Vec::new();

    for (mode, fixture) in fixtures {
        let raw = decode_hex_fixture(fixture);
        assert_eq!(raw.len(), SECTOR, "mode{mode} official writer fixture");
        let plain = a6b0_full(&raw, &outer_crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"EDPF", "mode{mode}");
        assert_eq!(u32_le(&plain, 0x08), 1, "mode{mode} entry count");
        assert_eq!(u32_le(&plain, 0x0c), 2, "mode{mode} Share type");
        assert_eq!(u32_le(&plain, 0x10), 1, "mode{mode} NeedDisturb");
        assert_eq!(u32_le(&plain, 0x14), 1, "mode{mode} NeedEncrypt");
        assert_eq!(u64_le(&plain, 0x20), 512, "mode{mode} sector size");
        assert_eq!(u32_le(&plain, 0x30), EXPECTED_USER_KEY_CRC, "mode{mode}");
        assert_eq!(u32_le(&plain, 0x34), EXPECTED_FILE_KEY_CRC, "mode{mode}");
        assert_eq!(plain[0x58], mode, "mode{mode} EncryptMode");
        assert!(plain[0x59..0x60].iter().all(|byte| *byte == 0));

        let wrapped: [u8; 16] = plain[0x38..0x48].try_into().unwrap();
        let file_key = match mode {
            1 => a6b0_decrypt(&wrapped, &PASSWORD_MD5, 0),
            2 => sm4_decrypt_block(&wrapped, &PASSWORD_MD5),
            3 => aes128_ecb_decrypt_block(&wrapped, &PASSWORD_MD5),
            _ => unreachable!(),
        };
        assert_eq!(file_key, EXPECTED_FILE_KEY, "mode{mode} unwrapped file key");
        assert_eq!(
            crc32_bare(&file_key),
            EXPECTED_FILE_KEY_CRC,
            "mode{mode} unwrapped FileKeyCRC"
        );
        unwrapped.push(file_key);
    }

    assert_eq!(unwrapped[0], unwrapped[1]);
    assert_eq!(unwrapped[1], unwrapped[2]);
}

#[test]
fn lba12_reference_fixtures_do_not_invent_unobserved_mode1_or_mode3_profiles() {
    let mut mode_counts = [0usize; 4];
    let mut encrypted_entries = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);

        for index in 0..3 {
            let base = index * 0x60;
            if &plain[base..base + 4] != b"EDPF" {
                continue;
            }
            let mode = plain[base + 0x58] as usize;
            assert!(
                mode < mode_counts.len(),
                "unexpected EncryptMode {mode}: {name}"
            );
            mode_counts[mode] += 1;
            if mode != 0 {
                encrypted_entries += 1;
            }
        }
    }

    assert!(
        encrypted_entries >= 12,
        "protocol fixture set lost encrypted-entry coverage"
    );
    assert_eq!(
        mode_counts[1], 0,
        "committed original fixtures must not be presented as positive mode1 evidence"
    );
    assert_eq!(
        mode_counts[3], 0,
        "committed original fixtures must not be presented as positive mode3 evidence"
    );
    assert_eq!(
        mode_counts[2], encrypted_entries,
        "all encrypted entries in the committed original fixture subset are mode2"
    );
}

#[test]
fn lba12_packed_reserved7_is_zero_and_separate_from_key_extension_material() {
    let mut checked_entries = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);

        for index in 0..3 {
            let base = index * 0x60;
            if &plain[base..base + 4] != b"EDPF" {
                continue;
            }

            assert!(
                plain[base + 0x59..base + 0x60]
                    .iter()
                    .all(|byte| *byte == 0),
                "packed Reserved[7] must stay zero in original fixture: {name} entry {index}"
            );

            // Keep the adjacent 16-byte slot separate from Reserved[7].  Another
            // official ABI formally names the corresponding material
            // EncryptFileKey32[16].  Its COMPLETE status comes from the external
            // producer/ABI/negative-consumer chain; this test contributes only
            // the original-device all-zero evidence.
            assert_eq!(plain[base + 0x48..base + 0x58].len(), 16);
            checked_entries += 1;
        }
    }

    assert!(
        checked_entries >= 18,
        "protocol fixture set lost packed Reserved[7] coverage"
    );
}

#[test]
fn lba6_offset_1ca_is_inside_gserial_slot_not_a_standalone_state_field() {
    let mut values = std::collections::BTreeSet::new();
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let plain = lba6_decode(sector(&image, 6));
        assert_eq!(
            &plain[0x1c0..0x1c8],
            b"322CA28A",
            "unexpected GSerial prefix: {name}"
        );
        values.insert(u32_le(&plain, 0x1ca));
    }

    assert!(values.contains(&20_417));
    assert!(values.contains(&128_480));
    assert!(
        values
            .iter()
            .any(|value| ![20_417, 128_480].contains(value)),
        "real fixtures must retain an ASCII-overlap value at +0x1CA"
    );
}

#[test]
fn lba6_autoid_matches_lba8_autonum_but_fixed_slot_tail_is_not_semantic_padding() {
    let mut checked = 0usize;
    let mut saw_nonzero_after_nul = false;
    let mut empty_autoid_backing = std::collections::BTreeSet::new();

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let lba6 = lba6_decode(sector(&image, 6));
        let slot = &lba6[0x70..0x80];
        let nul = slot
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(slot.len());
        let autoid = &slot[..nul];
        if nul < slot.len() && slot[nul + 1..].iter().any(|byte| *byte != 0) {
            saw_nonzero_after_nul = true;
        }
        if autoid.is_empty() && nul < slot.len() {
            empty_autoid_backing.insert(slot[nul + 1..].to_vec());
        }

        let inspect_meta = InspectMeta {
            device_id: Some(meta.device_id.clone()),
            ..InspectMeta::default()
        };
        let ownership =
            ownership_from_lba8(sector(&image, 8), &inspect_meta).expect("LBA8 ownership");
        let autonum = ownership.autonum.unwrap_or_default();
        assert_eq!(
            autoid,
            autonum.as_bytes(),
            "LBA6 m_autoid and LBA8 Autonum diverged: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        saw_nonzero_after_nul,
        "real fixtures must preserve evidence that bytes after the m_autoid NUL are not semantic zero padding"
    );
    assert!(
        empty_autoid_backing.len() >= 2,
        "the same empty m_autoid string must retain multiple distinct post-NUL backing profiles"
    );
}

fn c_string_bytes(slot: &[u8]) -> &[u8] {
    let end = slot
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(slot.len());
    &slot[..end]
}

fn gbk_string(slot: &[u8]) -> String {
    let (text, _, errors) = GBK.decode(c_string_bytes(slot));
    assert!(
        !errors,
        "fixture contains invalid GBK in a protocol string slot"
    );
    text.into_owned()
}

#[test]
fn lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries() {
    let mut checked = 0usize;
    let mut owner_tail = false;
    let mut office_tail = false;
    let mut label_tail = false;
    let mut empty_office_backing = std::collections::BTreeSet::new();
    let mut safe6_label_backing = std::collections::BTreeSet::new();

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let plain = lba6_decode(sector(&image, 6));

        let owner = &plain[0x50..0x70];
        let office = &plain[0x80..0xc0];
        let label = &plain[0x188..0x1c0];
        for (slot, seen_tail) in [
            (owner, &mut owner_tail),
            (office, &mut office_tail),
            (label, &mut label_tail),
        ] {
            if let Some(nul) = slot.iter().position(|byte| *byte == 0) {
                *seen_tail |= slot[nul + 1..].iter().any(|byte| *byte != 0);
            }
        }
        let office_nul = office
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(office.len());
        if office_nul == 0 {
            empty_office_backing.insert(office[1..].to_vec());
        }
        let label_nul = label
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(label.len());
        if gbk_string(label) == "江苏电力!SAFE6" && label_nul < label.len() {
            safe6_label_backing.insert(label[label_nul + 1..].to_vec());
        }

        let inspect_meta = InspectMeta {
            device_id: Some(meta.device_id.clone()),
            ..InspectMeta::default()
        };
        let ownership =
            ownership_from_lba8(sector(&image, 8), &inspect_meta).expect("LBA8 ownership");
        assert_eq!(
            ownership.user.unwrap_or_default(),
            gbk_string(owner),
            "LBA6 32B owner slot and LBA8 User diverged: {name}"
        );
        assert_eq!(
            ownership.label.unwrap_or_default(),
            gbk_string(label),
            "LBA6 56B label slot and LBA8 Label diverged: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        owner_tail && office_tail && label_tail,
        "real fixtures must preserve post-NUL backing evidence in all three fixed storage slots"
    );
    assert!(
        empty_office_backing.len() >= 3,
        "the same empty m_UsbOffice string must retain multiple distinct post-NUL backing profiles"
    );
    assert!(
        safe6_label_backing.len() >= 3
            && safe6_label_backing
                .iter()
                .all(|tail| tail.iter().any(|byte| *byte != 0)),
        "the same 江苏电力!SAFE6 label must retain at least three distinct non-zero post-NUL backing profiles"
    );
}

#[test]
fn lba6_gserial_and_beizhu_semantic_prefixes_stop_before_profile_underlay() {
    let mut checked = 0usize;
    let mut saw_short_gserial = false;
    let mut saw_long_gserial = false;
    let mut saw_empty_beizhu = false;
    let mut saw_normal_beizhu = false;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let plain = lba6_decode(sector(&image, 6));
        let gserial = &plain[0x1c0..0x1d0];
        let beizhu = &plain[0x1d0..0x1e0];

        assert_eq!(
            &plain[0x1ee..0x1f0],
            &[0, 0],
            "legacy/current MBR entry4 prefix must remain zero: {name}"
        );

        assert_eq!(
            gserial[15], 0,
            "BuildSector6 must keep the dedicated GSerial slot terminator byte zero: {name}"
        );
        assert_eq!(
            beizhu[15], 0,
            "BuildSector6 must keep the dedicated BeiZhu slot terminator byte zero: {name}"
        );

        assert_eq!(
            &gserial[..8],
            b"322CA28A",
            "all committed official profiles retain the common GSerial string prefix: {name}"
        );
        match gserial[8] {
            0 => saw_short_gserial = true,
            b'-' => saw_long_gserial = true,
            other => panic!("unexpected byte8 in GSerial semantic region: {other:#x} ({name})"),
        }

        match beizhu[0] {
            0 => saw_empty_beizhu = true,
            0xc6 => {
                assert_eq!(
                    gbk_string(beizhu),
                    "普通",
                    "legacy BeiZhu value changed: {name}"
                );
                saw_normal_beizhu = true;
            }
            other => panic!("unexpected BeiZhu first semantic byte: {other:#x} ({name})"),
        }
        checked += 1;
    }

    assert!(checked >= MIN_PROTOCOL_FIXTURES);
    assert!(saw_short_gserial && saw_long_gserial);
    assert!(saw_empty_beizhu && saw_normal_beizhu);
}

#[test]
fn official_virtual_lba6_fixed_string_slots_preserve_nonsemantic_source_backing() {
    // First-party Windows CEMSUsbRegsiter.dll::BuildSector6 output.  The source
    // UsbWriteParam slots were deliberately seeded after their first NUL:
    // GSerial backing = A5, BeiZhu backing = 5A.  BuildSector6 must preserve
    // those first 15 source bytes while owning only the dedicated byte15 NUL.
    let raw = decode_hex_fixture(OFFICIAL_VIRTUAL_SLOT_BACKING_LBA6_HEX);
    assert_eq!(raw.len(), SECTOR);
    assert_eq!(
        lba6_checksum(&raw[..0x1fc]),
        u32_le(&raw, 0x1fc),
        "official virtual writer fixture must retain a valid SAFE6 checksum"
    );

    let plain = lba6_decode(&raw);
    let gserial = &plain[0x1c0..0x1d0];
    let beizhu = &plain[0x1d0..0x1e0];

    assert_eq!(&gserial[..9], b"322CA28A\0");
    assert!(gserial[9..15].iter().all(|byte| *byte == 0xa5));
    assert_eq!(gserial[15], 0);
    assert_eq!(gbk_string(gserial), "322CA28A");

    assert_eq!(beizhu[0], 0);
    assert!(beizhu[1..15].iter().all(|byte| *byte == 0x5a));
    assert_eq!(beizhu[15], 0);
    assert_eq!(gbk_string(beizhu), "");
}

#[test]
fn official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot() {
    // First-party Windows CEMSUsbRegsiter.dll::BuildSector6 output.  The
    // surrounding LBA9 staging bytes were prefilled with 0xCC so this fixture
    // proves the exact write boundary instead of accidentally treating a
    // zeroed harness buffer as producer ownership.
    let raw6 = decode_hex_fixture(OFFICIAL_VIRTUAL_LONG_USER_LBA6_HEX);
    let lba9 = decode_hex_fixture(OFFICIAL_VIRTUAL_LONG_USER_LBA9_HEX);
    assert_eq!(raw6.len(), SECTOR);
    assert_eq!(lba9.len(), SECTOR);

    let mut user = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".repeat(3);
    user.truncate(155);
    assert_eq!(user.len(), 155);

    let plain6 = lba6_decode(&raw6);
    assert_eq!(u32_le(&plain6, 0x50), 0x4024_5e2a);
    assert_eq!(&plain6[0x54..0x70], &user[..28]);

    let continuation = &lba9[0x100..0x180];
    assert_eq!(&continuation[..127], &user[28..]);
    assert_eq!(
        continuation[127], 0,
        "155-byte User must end exactly at +0x17f"
    );

    let mut reconstructed = Vec::with_capacity(156);
    reconstructed.extend_from_slice(&plain6[0x54..0x70]);
    reconstructed.extend_from_slice(continuation);
    let nul = reconstructed
        .iter()
        .position(|byte| *byte == 0)
        .expect("official writer must terminate long User");
    assert_eq!(&reconstructed[..nul], user.as_slice());

    assert!(
        lba9[..0x100].iter().all(|byte| *byte == 0xcc),
        "BuildSector6 long-User path must not own pre-continuation LBA9 backing"
    );
    assert!(
        lba9[0x180..].iter().all(|byte| *byte == 0xcc),
        "BuildSector6 must stop exactly after its 0x80-byte maximum continuation"
    );
}

fn assert_lba6_static_template_holes(raw: &[u8], label: &str) {
    let plain = lba6_decode(raw);
    let expected_040 = decode_hex_fixture("f0ac3c0074fcbb0700b40ecd10ebf288");
    let expected_0c0 = decode_hex_fixture(concat!(
        "0a77237205394608731cb80102bb007c8b4e028b5600cd1373514f744e32e48a",
        "5600cd13ebe48a560060bbaa55b441cd13723681fb55aa7530f6c101742b6160"
    ));
    let expected_108 = decode_hex_fixture(concat!(
        "76086a0068007c6a016a10b4428bf4cd136161730e4f740b32e48a5600cd13eb",
        "d661f9c3496e76616c696420706172746974696f6e207461626c65004572726f",
        "72206c6f6164696e67206f7065726174696e672073797374656d004d69737369",
        "6e67206f7065726174696e672073797374656d00000000000000000000000000"
    ));
    assert_eq!(&plain[0x040..0x050], expected_040.as_slice(), "{label}");
    assert_eq!(&plain[0x0c0..0x100], expected_0c0.as_slice(), "{label}");
    assert_eq!(&plain[0x108..0x188], expected_108.as_slice(), "{label}");
    assert!(
        plain[0x1f4..0x1fc].iter().all(|byte| *byte == 0),
        "LBA6 static template zero tail changed: {label}"
    );

    let stored = u32_le(raw, 0x1fc);
    let checksum = lba6_checksum(&raw[..0x1fc]);
    assert!(
        stored == checksum || stored == checksum.wrapping_shl(1),
        "LBA6 static template material is no longer covered by the accepted checksum profile: {label}"
    );
}

#[test]
fn lba6_static_usb_main_bsec_holes_are_exact_and_checksum_protected() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        assert_lba6_static_template_holes(sector(&image, 6), name);
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );

    let sandisk = decode_hex_fixture(SANDISK_LBA6_HEX);
    assert_lba6_static_template_holes(&sandisk, "independent SanDisk original");
}

fn assert_lba6_crc_usb_id_pair(lba6_raw: &[u8], device_id: &str, label: &str) {
    let lba6 = lba6_decode(lba6_raw);
    let crc = crc32_bare(device_id.as_bytes());
    assert_ne!(
        crc, 0,
        "fixture unexpectedly has a zero device-id CRC: {label}"
    );
    assert_eq!(
        u32_le(&lba6, 0x100),
        crc,
        "LBA6 m_crcUsbID[0] must be CRC32(device_id): {label}"
    );
    assert_eq!(
        u32_le(&lba6, 0x104),
        crc.wrapping_mul(2),
        "LBA6 m_crcUsbID[1] must be the doubled CRC guard: {label}"
    );
}

#[test]
fn lba6_crc_usb_id_pair_is_device_id_crc_and_doubled_guard() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        assert_lba6_crc_usb_id_pair(sector(&image, 6), &meta.device_id, name);
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost LBA6 crcUsbID fixtures"
    );

    let sandisk_lba6 = decode_hex_fixture(SANDISK_LBA6_HEX);
    assert_eq!(sandisk_lba6.len(), SECTOR);
    assert_lba6_crc_usb_id_pair(
        &sandisk_lba6,
        SANDISK_DEVICE_ID,
        "independent SanDisk original",
    );
}

#[test]
fn lba6_m_encrypt_is_the_observed_write_only_safe_label_metadata() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let lba6 = lba6_decode(sector(&image, 6));
        assert_eq!(
            u32_le(&lba6, 0x1f0),
            1,
            "strict original must retain the observed !SAFE m_encrypt profile: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost LBA6 m_encrypt fixtures"
    );

    let sandisk = lba6_decode(&decode_hex_fixture(SANDISK_LBA6_HEX));
    assert_eq!(
        u32_le(&sandisk, 0x1f0),
        1,
        "independent SanDisk original must retain the same m_encrypt profile"
    );
}

fn assert_lba6_legacy_mbr_type4_fragment_matches_lba12(
    lba6_raw: &[u8],
    lba12_raw: &[u8],
    device_id: &str,
) {
    let lba6 = lba6_decode(lba6_raw);
    let crc = crc32_bare(device_id.as_bytes());
    let lba12 = a6b0_full(lba12_raw, &crc.to_le_bytes(), 0);

    assert_eq!(u32_le(&lba12, 8), 3, "expected three packed LBA12 entries");
    let type4 = 2 * 0x60;
    assert_eq!(u32_le(&lba12, type4 + 0x0c), 4);

    let start_lba = u64_le(&lba12, type4 + 0x18);
    let partition_bytes = u64_le(&lba12, type4 + 0x28);
    assert_eq!(partition_bytes % SECTOR as u64, 0);
    let sector_count = partition_bytes / SECTOR as u64;
    assert!(start_lba <= u32::MAX as u64);
    assert!(sector_count <= u32::MAX as u64);

    let mut expected_fragment = [0u8; 14];
    expected_fragment[..6].copy_from_slice(&[0xc1, 0xff, 0x07, 0xef, 0xff, 0xff]);
    expected_fragment[6..10].copy_from_slice(&(start_lba as u32).to_le_bytes());
    expected_fragment[10..14].copy_from_slice(&(sector_count as u32).to_le_bytes());
    assert_eq!(
        &lba6[0x1e0..0x1ee],
        &expected_fragment,
        "legacy MBR entry3 surviving bytes must be the deterministic type4 snapshot fragment"
    );

    // LBA6 +0x1DE is the third 16-byte MBR partition entry. The first two
    // bytes of that entry were overwritten by the preceding BeiZhu slot,
    // but +0x1E0 onward still preserves the rest of the entry.
    let start_chs = &lba6[0x1df..0x1e2];
    let end_chs = &lba6[0x1e3..0x1e6];
    let decode_chs = |chs: &[u8]| {
        let head = chs[0];
        let sector = chs[1] & 0x3f;
        let cylinder = (((chs[1] as u16) & 0xc0) << 2) | chs[2] as u16;
        (cylinder, head, sector)
    };
    assert_eq!(
        decode_chs(start_chs),
        (1023, 0, 1),
        "legacy MBR start CHS must retain the saturated 240/63 geometry"
    );
    assert_eq!(lba6[0x1e2], 0x07, "legacy MBR partition type");
    assert_eq!(
        decode_chs(end_chs),
        (1023, 239, 63),
        "legacy MBR end CHS must retain the saturated 240/63 geometry"
    );
    assert_eq!(
        u32_le(&lba6, 0x1e6) as u64,
        u64_le(&lba12, type4 + 0x18),
        "legacy MBR start LBA must match the LBA12 type4 partition"
    );
    assert_eq!(
        u32_le(&lba6, 0x1ea) as u64,
        u64_le(&lba12, type4 + 0x28) / SECTOR as u64,
        "legacy MBR sector count must match the LBA12 type4 partition size"
    );
    assert_eq!(
        &lba6[0x1ee..0x1f0],
        &[0, 0],
        "the final two bytes have already crossed into MBR entry4"
    );
}

#[test]
fn lba6_legacy_beizhu_post_nul_bytes_continue_into_mbr_type4_fragment() {
    const LEGACY_AIGO: &str =
        "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin";
    let image = load(LEGACY_AIGO);
    let plain = lba6_decode(sector(&image, 6));

    assert_eq!(&plain[0x1d0..0x1d4], &[0xc6, 0xd5, 0xcd, 0xa8]);
    assert_eq!(plain[0x1d4], 0);
    assert!(
        plain[0x1d5..0x1e0].iter().any(|byte| *byte != 0),
        "legacy BeiZhu slot must retain the observed nonzero backing bytes after its NUL"
    );
    assert_eq!(
        &plain[0x1e0..0x1f0],
        &[
            0xc1, 0xff, 0x07, 0xef, 0xff, 0xff, 0x1c, 0xa8, 0x7d, 0x0e, 0xe3, 0xf4, 0x27, 0x00,
            0x00, 0x00,
        ]
    );
    assert_lba6_legacy_mbr_type4_fragment_matches_lba12(
        sector(&image, 6),
        sector(&image, 12),
        "disk&ven_aigo&prod_u335&rev_pmap",
    );
}

#[test]
fn lba6_authentic_sandisk_legacy_mbr_type4_fragment_matches_lba12() {
    let lba6 = decode_hex_fixture(SANDISK_LBA6_HEX);
    let lba12 = decode_hex_fixture(SANDISK_LBA12_HEX);
    assert_eq!(lba6.len(), SECTOR);
    assert_eq!(lba12.len(), SECTOR);
    assert_lba6_legacy_mbr_type4_fragment_matches_lba12(&lba6, &lba12, SANDISK_DEVICE_ID);
}

#[test]
fn lba6_netac_legacy_mbr_type4_fragment_matches_lba12() {
    assert_lba6_legacy_mbr_type4_fragment_matches_lba12(
        sector(NETAC_EESI_CAPTURE, 6),
        sector(NETAC_EESI_CAPTURE, 12),
        NETAC_ONLYDISK_DEVICE_ID,
    );
}

#[test]
fn lba6_legacy_mbr_fragment_is_profile_specific_even_when_lba12_type4_exists() {
    let mut zero_fragment = 0usize;
    let mut legacy_fragment = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        if image.len() < 13 * SECTOR {
            continue;
        }

        let crc = crc32_bare(meta.device_id.as_bytes());
        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let type4 = 2 * 0x60;
        assert_eq!(
            u32_le(&lba12, type4 + 0x0c),
            4,
            "committed original fixture lost its type4 partition: {name}"
        );

        let lba6 = lba6_decode(sector(&image, 6));
        if lba6[0x1e0..0x1f0].iter().all(|byte| *byte == 0) {
            zero_fragment += 1;
        } else {
            legacy_fragment += 1;
            assert_lba6_legacy_mbr_type4_fragment_matches_lba12(
                sector(&image, 6),
                sector(&image, 12),
                &meta.device_id,
            );
        }
    }

    assert!(
        zero_fragment >= 6,
        "current-style fixtures must prove that LBA12 type4 does not require an LBA6 MBR fragment"
    );
    assert!(
        legacy_fragment >= 1,
        "committed fixture subset lost the legacy LBA6 MBR fragment profile"
    );
}

#[test]
fn committed_blank_sector_evidence_matches_real_images() {
    let mut checked = 0usize;
    let mut manufacturing_marks = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        assert_eq!(image.len(), METADATA_IMAGE_LEN, "{}", path.display());
        for lba in [1usize, 2, 5, 10] {
            assert!(
                sector(&image, lba).iter().all(|byte| *byte == 0),
                "{} LBA{lba} is not zero",
                path.display()
            );
        }
        let lba3 = sector(&image, 3);
        if lba3.iter().any(|byte| *byte != 0) {
            assert!(
                lba3.windows(b"this is mp mark".len())
                    .any(|window| window == b"this is mp mark"),
                "unexpected non-zero LBA3 payload in {}",
                path.display()
            );
            manufacturing_marks += 1;
        }
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert_eq!(manufacturing_marks, 1, "LBA3 evidence set changed");
}

#[test]
fn lba0_legacy_mbr_message_pointer_bytes_have_only_template_or_cleared_profiles() {
    let mut template_profile = 0usize;
    let mut cleared_profile = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let bytes = &sector(&image, 0)[0x1b5..0x1b8];

        match bytes {
            [0x2c, 0x44, 0x63] => template_profile += 1,
            [0x00, 0x00, 0x00] => cleared_profile += 1,
            _ => panic!(
                "unexpected LBA0 legacy MBR message-pointer profile in {name}: {:02x?}",
                bytes
            ),
        }
    }

    assert!(
        template_profile > 0,
        "protocol fixtures lost the legacy MBR message-pointer profile"
    );
    assert!(
        cleared_profile > 0,
        "protocol fixtures must retain the cleared/preserve-existing profile"
    );
}

#[test]
fn lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix() {
    const USB_MAIN_BSEC_PREFIX_SHA256: &str =
        "4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed";

    let mut template_profile = 0usize;
    let mut zero_profile = 0usize;
    let mut checked = 0usize;
    let mut disk_signatures = HashSet::new();
    let mut sector_size_profiles = HashSet::new();

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }

        let image = fs::read(&path).expect("fixture bytes");
        let lba0 = sector(&image, 0);
        let prefix = &lba0[..0x190];

        if prefix.iter().all(|byte| *byte == 0) {
            zero_profile += 1;
        } else {
            assert_eq!(
                sha256_hex(prefix),
                USB_MAIN_BSEC_PREFIX_SHA256,
                "unexpected non-zero LBA0 bootstrap profile in {name}"
            );
            template_profile += 1;
        }

        assert!(
            lba0[0x190..0x1a0].iter().all(|byte| *byte == 0),
            "unexpected LBA0 +0x190..+0x19f material in {name}"
        );
        assert!(
            matches!(u32_le(lba0, 0x1a0), 0 | 512),
            "unexpected LBA0 SectorSize profile in {name}: {}",
            u32_le(lba0, 0x1a0)
        );
        sector_size_profiles.insert(u32_le(lba0, 0x1a0));
        assert!(
            lba0[0x1a4..0x1b5].iter().all(|byte| *byte == 0),
            "unexpected LBA0 +0x1a4..+0x1b4 material in {name}"
        );
        assert_eq!(
            &lba0[0x1bc..0x1be],
            &[0, 0],
            "standard MBR reserved word changed in {name}"
        );
        let disk_signature = u32_le(lba0, 0x1b8);
        assert_ne!(
            disk_signature, 0,
            "standard MBR disk signature unexpectedly zero in {name}"
        );
        disk_signatures.insert(disk_signature);
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost LBA0 fixtures"
    );
    assert!(
        template_profile > 0,
        "protocol fixtures lost the official UsbMainBSec bootstrap profile"
    );
    assert!(
        zero_profile > 0,
        "protocol fixtures lost the current zero-bootstrap profile"
    );
    assert!(
        disk_signatures.len() > 1,
        "protocol fixtures must retain multiple real MBR disk signatures"
    );
    assert_eq!(
        sector_size_profiles,
        HashSet::from([0, 512]),
        "protocol fixtures must retain both absent and populated SectorSize overlay profiles"
    );
}

#[test]
fn strict_progress_has_no_partial_detail_rows_for_fully_complete_lbas() {
    let trace = include_str!("../docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md");
    let complete_lbas = ["LBA1", "LBA2", "LBA5", "LBA7", "LBA11", "LBA12"];

    for lba in complete_lbas {
        let stale = trace
            .lines()
            .filter(|line| line.starts_with(&format!("| {lba} |")))
            .filter(|line| line.contains("| PARTIAL |"))
            .collect::<Vec<_>>();
        assert!(
            stale.is_empty(),
            "{lba} is 100% COMPLETE in STRICT_PROGRESS but still has PARTIAL detail rows: {stale:?}"
        );
    }
}

#[test]
fn lba4_strict_progress_matches_non_overlapping_detail_ranges() {
    let trace = include_str!("../docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md");
    let mut owner = vec![None::<&str>; SECTOR];
    let mut complete = 0usize;
    let mut partial = 0usize;

    for line in trace.lines().filter(|line| line.starts_with("| LBA4 | 0x")) {
        let columns = line.split('|').map(str::trim).collect::<Vec<_>>();
        let range = columns[2];
        let status = columns[3];
        let parse_hex = |value: &str| {
            usize::from_str_radix(value.trim_start_matches("0x"), 16)
                .expect("LBA4 detail range must be hexadecimal")
        };
        let (start, end) = if let Some((a, b)) = range.split_once('–') {
            (parse_hex(a), parse_hex(b))
        } else {
            let offset = parse_hex(range);
            (offset, offset)
        };
        assert!(end < SECTOR && start <= end, "invalid LBA4 range: {range}");

        for (offset, slot) in owner.iter_mut().enumerate().take(end + 1).skip(start) {
            assert!(
                slot.replace(status).is_none(),
                "overlapping LBA4 detail row at +0x{offset:03X}: {line}"
            );
            match status {
                "COMPLETE" => complete += 1,
                "PARTIAL" => partial += 1,
                other => panic!("unexpected LBA4 detail status {other}: {line}"),
            }
        }
    }

    assert!(
        owner.iter().all(Option::is_some),
        "LBA4 detail rows must cover all 512 bytes"
    );
    assert_eq!((complete, partial), (512, 0));
    assert!(
        trace.contains("| LBA4 | 512 | 0 | 0 | 100.0% |"),
        "STRICT_PROGRESS LBA4 summary drifted from byte-detail accounting"
    );
}

#[test]
fn lba0_bootstrap_body_closes_all_three_profile_invariant_zero_bytes() {
    const LEGACY: &str =
        "disk26_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_nopwd_20260916_233626.bin";
    const ISOLATED_INVARIANT_ZERO: [usize; 9] = [
        0x0e1, 0x0e8, 0x101, 0x103, 0x10b, 0x10d, 0x124, 0x143, 0x162,
    ];

    let legacy = load(LEGACY);
    let legacy_lba0 = sector(&legacy, 0);
    let netac_prefix = decode_hex_fixture(AIGO_L8302_NETAC_LBA0_PREFIX_HEX);

    assert_eq!(netac_prefix.len(), 0x190);

    // Seven isolated zero bytes are operands in the executable legacy bootstrap.
    assert_eq!(&legacy_lba0[0x0df..0x0e2], &[0x8a, 0x56, 0x00]); // mov dl,[bp+0]
    assert_eq!(&legacy_lba0[0x0e6..0x0e9], &[0x8a, 0x56, 0x00]); // mov dl,[bp+0]
    assert_eq!(&legacy_lba0[0x100..0x102], &[0x6a, 0x00]); // push 0
    assert_eq!(&legacy_lba0[0x102..0x104], &[0x6a, 0x00]); // push 0
    assert_eq!(&legacy_lba0[0x10a..0x10c], &[0x6a, 0x00]); // push 0
    assert_eq!(&legacy_lba0[0x10c..0x10f], &[0x68, 0x00, 0x7c]); // push 0x7c00
    assert_eq!(&legacy_lba0[0x122..0x125], &[0x8a, 0x56, 0x00]); // mov dl,[bp+0]

    // The other three message terminators are invariant zeros too.
    assert_eq!(&legacy_lba0[0x12c..0x143], b"Invalid partition table");
    assert_eq!(legacy_lba0[0x143], 0);
    assert_eq!(
        &legacy_lba0[0x144..0x162],
        b"Error loading operating system"
    );
    assert_eq!(legacy_lba0[0x162], 0);
    assert_eq!(&legacy_lba0[0x163..0x17b], b"Missing operating system");
    assert_eq!(legacy_lba0[0x17b], 0);

    assert!(
        legacy_lba0[0x17c..0x190].iter().all(|byte| *byte == 0),
        "legacy UsbMainBSec tail padding changed"
    );
    assert!(
        ISOLATED_INVARIANT_ZERO
            .iter()
            .all(|offset| netac_prefix[*offset] == 0)
            && netac_prefix[0x17b..0x190].iter().all(|byte| *byte == 0),
        "Aigo/Netac MBR template lost an invariant zero byte"
    );

    let mut current_zero = 0usize;
    let mut legacy_template = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let lba0 = sector(&image, 0);
        assert!(
            ISOLATED_INVARIANT_ZERO
                .iter()
                .all(|offset| lba0[*offset] == 0)
                && lba0[0x17b..0x190].iter().all(|byte| *byte == 0),
            "known LBA0 profile changed one of the 30 invariant zero bytes: {name}"
        );
        if lba0[..0x190].iter().all(|byte| *byte == 0) {
            current_zero += 1;
        } else {
            legacy_template += 1;
        }
    }
    assert!(current_zero > 0, "lost current zero-bootstrap evidence");
    assert!(legacy_template > 0, "lost legacy UsbMainBSec evidence");
}

#[test]
fn lba3_manufacturing_payload_is_an_opaque_whole_sector_not_just_a_marker_string() {
    const MARKED: &str =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    let image = load(MARKED);
    let lba3 = sector(&image, 3);

    assert_eq!(&lba3[0x000..0x004], &[0x00, 0x01, 0x00, 0x00]);
    assert!(
        !lba3
            .windows(4)
            .any(|window| window == [0x12, 0x01, 0x00, 0x02]),
        "real LBA3 must not be conflated with the independently observed MPALL F2 INFO header"
    );
    assert_eq!(lba3[0x001], 0x01);
    assert_eq!(
        &lba3[0x020..0x028],
        &[0xb5, 0x7e, 0x9c, 0x45, 0x00, 0x80, 0x00, 0x14]
    );
    assert_eq!(&lba3[0x1f0..0x200], b"this is mp mark\0");

    for (offset, byte) in lba3.iter().copied().enumerate() {
        let belongs_to_observed_payload =
            offset == 0x001 || (0x020..0x028).contains(&offset) || (0x1f0..0x200).contains(&offset);
        if !belongs_to_observed_payload {
            assert_eq!(
                byte, 0,
                "unexpected byte outside the observed Kingston MP payload at +0x{offset:03X}"
            );
        }
    }
}

#[test]
fn lba3_mp_marker_has_multiple_real_historical_payload_profiles() {
    const STRICT_MARKED: &str =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    let strict_image = load(STRICT_MARKED);
    let strict_lba3 = sector(&strict_image, 3);
    let historical_lba3 = decode_hex_fixture(KINGSTON_20260803_MP_LBA3_HEX);

    assert_eq!(historical_lba3.len(), SECTOR);
    assert_eq!(&historical_lba3[0x000..0x004], &[0x00, 0x01, 0x00, 0x00]);
    assert!(
        !historical_lba3
            .windows(4)
            .any(|window| window == [0x12, 0x01, 0x00, 0x02]),
        "historical LBA3 must remain distinct from the MPALL F2 INFO page layout"
    );
    assert_eq!(strict_lba3[0x001], 0x01);
    assert_eq!(historical_lba3[0x001], 0x01);
    assert_eq!(&strict_lba3[0x1f0..], b"this is mp mark\0");
    assert_eq!(&historical_lba3[0x1f0..], b"this is mp mark\0");

    assert_eq!(
        &historical_lba3[0x020..0x028],
        &[0xa8, 0x82, 0xa4, 0x22, 0x00, 0x20, 0x02, 0x16]
    );
    assert_eq!(
        &strict_lba3[0x020..0x028],
        &[0xb5, 0x7e, 0x9c, 0x45, 0x00, 0x80, 0x00, 0x14]
    );
    assert_ne!(
        &historical_lba3[0x020..0x028],
        &strict_lba3[0x020..0x028],
        "MP marker must not collapse distinct manufacturer payload profiles into one fixed template"
    );
}

#[test]
fn lba3_independent_mp_profiles_share_one_exact_472_byte_marker_tail() {
    const STRICT_MARKED: &str =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    const COMMON_TAIL_SHA256: &str =
        "5f88797f7273191052e7a9300316e1a4f0f31563db07110a86fa4e648379198f";

    let strict_image = load(STRICT_MARKED);
    let strict_lba3 = sector(&strict_image, 3);
    let historical_lba3 = decode_hex_fixture(KINGSTON_20260803_MP_LBA3_HEX);

    assert_ne!(&strict_lba3[..0x28], &historical_lba3[..0x28]);
    assert_eq!(&strict_lba3[0x28..], &historical_lba3[0x28..]);
    assert_eq!(
        sha256_hex(&strict_lba3[0x28..]),
        COMMON_TAIL_SHA256,
        "the 472-byte MP marker tail must stay stable across independent physical profiles"
    );
    assert!(strict_lba3[0x28..0x1f0].iter().all(|byte| *byte == 0));
    assert_eq!(&strict_lba3[0x1f0..], b"this is mp mark\0");
}

#[test]
fn original_fixtures_keep_lba5_zero_while_the_protocol_treats_it_as_opaque_scratch() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        assert!(
            sector(&image, 5).iter().all(|byte| *byte == 0),
            "original fixture has unexpected LBA5 bytes: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost original LBA5 fixtures: {checked}"
    );
}

#[test]
fn onlyid_is_not_a_function_of_device_id_vid_pid_or_capacity() {
    let a = load(NETAC_A);
    let b = load(NETAC_B);
    assert_ne!(&sector(&a, 4)[..16], &sector(&b, 4)[..16]);
    assert_eq!(&sector(&a, 12)[0x170..], &sector(&b, 12)[0x170..]);
}

#[test]
fn lba12_tail_is_profile_material_not_a_global_constant() {
    let netac_a = load(NETAC_A);
    let netac_b = load(NETAC_B);
    let lexar = load(LEXAR);
    assert_eq!(
        &sector(&netac_a, 12)[0x170..],
        &sector(&netac_b, 12)[0x170..]
    );
    assert_ne!(&sector(&netac_a, 12)[0x170..], &sector(&lexar, 12)[0x170..]);
}

#[test]
fn every_committed_lba12_tail_is_encrypted_zeroes_from_device_id() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        assert_eq!(image.len(), METADATA_IMAGE_LEN, "{name}");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let expected = a7f0_full(&[0u8; 144], &crc.to_le_bytes(), 0x170);
        assert_eq!(&sector(&image, 12)[0x170..], expected.as_slice(), "{name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn canonical_glab_is_stable_across_decodable_real_images() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let inspect_meta = InspectMeta {
            device_id: Some(meta.device_id.clone()),
            vid: Some(meta.vid),
            pid: Some(meta.pid),
            size_bytes: meta.secs.map(|v| v * SECTOR as u64),
            onlyid: meta.onlyid.clone(),
        };
        let own = ownership_from_lba8(sector(&image, 8), &inspect_meta)
            .expect("committed LBA8 must decode");
        assert_eq!(
            own.glab.as_deref(),
            Some("322CA28A-D7D1448B-DCE2CED9"),
            "{name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes() {
    let mut checked = 0usize;
    let mut raw_zero_gap = 0usize;
    let mut rolling_zero_gap = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(onlyid) = meta.onlyid.as_deref() else {
            continue;
        };
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 4);
        let mut decoded = raw.to_vec();
        decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));

        // Front LBA4 has a short form whose middle extension region was never written.
        // Preserve that *region* as zero; do not treat individual zero ciphertext bytes as gaps.
        if raw[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            decoded[0x47..0x1fc].fill(0);
            raw_zero_gap += 1;
        } else {
            rolling_zero_gap += 1;
        }

        assert_eq!(&decoded[0x39..0x3d], b"LLGB", "{name}");
        assert_eq!(&decoded[0x1fc..0x200], b"LLGB", "{name}");
        assert_eq!(u32_le(&decoded, 0x18), bits ^ 0x8888_8888, "{name}");
        assert!(
            decoded[0x47..0x1fc].iter().all(|byte| *byte == 0),
            "LBA4 extension must be semantic zero in both observed physical representations: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        raw_zero_gap >= 4,
        "committed fixtures lost the raw-zero LBA4 representation"
    );
    assert!(
        rolling_zero_gap >= 2,
        "committed fixtures lost the rolling-encrypted-zero LBA4 representation"
    );
}

#[test]
fn official_virtual_lba4_backing_is_unowned_and_representation_only() {
    const ONLYID: u32 = 1_625_940_067;
    let full = decode_hex_fixture(OFFICIAL_VIRTUAL_LBA4_FULL_NONZERO_BACKING_HEX);
    let short = decode_hex_fixture(OFFICIAL_VIRTUAL_LBA4_NULL_NONZERO_BACKING_HEX);
    assert_eq!(full.len(), SECTOR);
    assert_eq!(short.len(), SECTOR);

    let k0 = (ONLYID & 0xffff) ^ (ONLYID >> 16);
    let mut decoded = full.clone();
    decoded[0x18..].copy_from_slice(&xor_rolling(&full[0x18..], k0));

    assert!(
        full[0x47..0x1fc].iter().any(|byte| *byte != 0xa5),
        "full BuildSector4 branch must transform arbitrary backing"
    );
    assert!(
        decoded[0x47..0x1fc].iter().all(|byte| *byte == 0xa5),
        "rolling decode must recover arbitrary backing exactly"
    );
    assert_eq!(&decoded[0x1fc..0x200], b"LLGB");

    assert!(
        short[0x47..0x1fc].iter().all(|byte| *byte == 0xa5),
        "NULL-node BuildSector4 branch must preserve backing byte-for-byte"
    );
    assert!(
        short[0x1fc..0x200].iter().all(|byte| *byte == 0xa5),
        "NULL-node branch must not invent the trailing LLGB anchor"
    );
}

#[test]
fn lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile() {
    let raw = include_bytes!(
        "fixtures/protocol_evidence/kingston_20260827_current_identity_raw_zero_lba4.bin"
    );
    assert_eq!(raw.len(), SECTOR);

    let onlyid = 1_625_940_067u32;
    let k0 = (onlyid & 0xffff) ^ (onlyid >> 16);
    let mut decoded = raw.to_vec();
    decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));

    // This strict-original Kingston sample has the current identity node shape
    // (second ID == main ID and HSerialCRC[5] == zero), yet its physical
    // extension is the historical raw-zero representation rather than full rolling.
    // Therefore representation choice must not be inferred from identity generation.
    assert_eq!(u32_le(&decoded, 0x1c), onlyid);
    assert!(decoded[0x20..0x34].iter().all(|byte| *byte == 0));
    assert!(raw[0x47..0x1fc].iter().all(|byte| *byte == 0));
    assert_eq!(&decoded[0x39..0x3d], b"LLGB");
}

#[test]
fn lba4_common_hserial_profile_is_shared_across_different_target_usb_devices() {
    let decode = |name: &str| {
        let image = load(name);
        let meta = parse_reference_backup_name(name).expect("fixture metadata");
        let onlyid = meta.onlyid.as_deref().expect("fixture onlyid");
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let raw = sector(&image, 4);
        let mut decoded = raw.to_vec();
        decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));
        if raw[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            decoded[0x47..0x1fc].fill(0);
        }
        (meta, decoded)
    };

    let (lexar_meta, lexar) = decode(LEXAR);
    let (netac_meta, netac) = decode(NETAC_A);
    assert_ne!(lexar_meta.device_id, netac_meta.device_id);
    assert_ne!(lexar_meta.vid, netac_meta.vid);
    assert_eq!(&lexar[0x20..0x34], &netac[0x20..0x34]);
    assert!(lexar[0x20..0x34].iter().any(|byte| *byte != 0));
    assert_eq!(
        &lexar[0x20..0x34],
        &[
            0x29, 0x1d, 0x00, 0x00, 0x7b, 0x00, 0x00, 0x00, 0xdd, 0x04, 0x00, 0x00, 0x79, 0x00,
            0x00, 0x00, 0x7c, 0x00, 0x00, 0x00,
        ]
    );
}

#[test]
fn lba4_fixed_hserial_is_independent_from_hardinfo_and_sapf_backing() {
    const NETAC_C: &str =
        "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid3274129259_20260910_172709.bin";

    let decode_profile = |name: &str| {
        let image = load(name);
        let meta = parse_reference_backup_name(name).expect("fixture metadata");
        let onlyid = meta.onlyid.as_deref().expect("fixture onlyid");
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);

        let raw4 = sector(&image, 4);
        let mut lba4 = raw4.to_vec();
        lba4[0x18..].copy_from_slice(&xor_rolling(&raw4[0x18..], k0));
        if raw4[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            lba4[0x47..0x1fc].fill(0);
        }

        let hserial: [u8; 20] = lba4[0x20..0x34].try_into().unwrap();
        let hardinfo = u32_le(&lba4, 0x35);

        let raw9 = sector(&image, 9);
        let sapf_head: Vec<u8> = raw9[0x100..0x104].iter().map(|byte| byte ^ 0x88).collect();
        assert_eq!(sapf_head, b"SAPF", "expected SAPF profile: {name}");
        let sapf_tail: [u8; 12] = std::array::from_fn(|index| raw9[0x114 + index] ^ 0x88);

        (hserial, hardinfo, sapf_tail)
    };

    let lexar = decode_profile(LEXAR);
    let netac_a = decode_profile(NETAC_A);
    let netac_b = decode_profile(NETAC_B);
    let netac_c = decode_profile(NETAC_C);

    for profile in [&netac_a, &netac_b, &netac_c] {
        assert_eq!(
            lexar.0, profile.0,
            "fixed legacy HSerial profile must stay identical across target USB devices"
        );
    }

    assert_ne!(
        lexar.1, netac_a.1,
        "identical HSerial material must not be treated as an expansion of MyHardinfo/HDSerialInfo"
    );

    assert!(
        lexar.2.iter().all(|byte| *byte == 0),
        "Lexar supplies the zero SAPF-tail counterexample"
    );
    assert!(
        netac_a.2.iter().any(|byte| *byte != 0),
        "Netac supplies a non-zero SAPF-tail counterexample"
    );
    assert_eq!(
        netac_a.2, netac_b.2,
        "two Netac captures keep one stable SAPF backing shape"
    );
    assert_ne!(
        netac_a.2, netac_c.2,
        "the same Netac fixed-HSerial profile also exhibits a changed SAPF backing shape"
    );
}

#[test]
fn lba4_restore_node_profiles_keep_current_and_legacy_fields_separate() {
    const KINGSTON_CURRENT: &str =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    const AIGO_CURRENT: &str =
        "disk4_1953525168_vid174c_pid55aa_disk&ven_aigo&prod_hd806_onlyid-1833210541_20260903_121552.bin";

    let decode = |name: &str| {
        let image = load(name);
        let meta = parse_reference_backup_name(name).expect("fixture metadata");
        let onlyid = meta.onlyid.as_deref().expect("fixture onlyid");
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let raw = sector(&image, 4);
        let mut decoded = raw.to_vec();
        decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));
        if raw[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            decoded[0x47..0x1fc].fill(0);
        }
        (bits, decoded)
    };

    for name in [KINGSTON_CURRENT, AIGO_CURRENT] {
        let (main_onlyid, decoded) = decode(name);
        assert_eq!(
            u32_le(&decoded, 0x1c),
            main_onlyid,
            "current-style OnllyID2Nd must mirror main onlyid: {name}"
        );
        assert!(
            decoded[0x20..0x34].iter().all(|byte| *byte == 0),
            "current-style HSerialCRC[5] must remain zero: {name}"
        );
    }

    for name in [LEXAR, NETAC_A] {
        let (main_onlyid, decoded) = decode(name);
        assert_ne!(
            u32_le(&decoded, 0x1c),
            main_onlyid,
            "legacy fixed-HSerial profile must not be collapsed into current writer: {name}"
        );
        assert_eq!(
            &decoded[0x20..0x34],
            &[
                0x29, 0x1d, 0x00, 0x00, 0x7b, 0x00, 0x00, 0x00, 0xdd, 0x04, 0x00, 0x00, 0x79, 0x00,
                0x00, 0x00, 0x7c, 0x00, 0x00, 0x00,
            ],
            "legacy HSerialCRC profile changed: {name}"
        );
    }

    for name in [KINGSTON_CURRENT, AIGO_CURRENT, LEXAR, NETAC_A] {
        let (_, decoded) = decode(name);
        assert_eq!(decoded[0x34], 0, "SingleUsbFlg profile changed: {name}");
        assert_eq!(&decoded[0x39..0x3d], b"LLGB", "NewLabFlag changed: {name}");
        assert_eq!(u32_le(&decoded, 0x3d), 1, "restore Version changed: {name}");
        assert_eq!(
            &decoded[0x41..0x45],
            &[0x08, 0x04, 0x0c, 0x01],
            "restore sector tuple changed: {name}"
        );
    }

    let mut checked_fixed_metadata = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let (_, decoded) = decode(name);
        assert_eq!(decoded[0x34], 0, "SingleUsbFlg profile changed: {name}");
        assert_eq!(&decoded[0x39..0x3d], b"LLGB", "NewLabFlag changed: {name}");
        assert_eq!(u32_le(&decoded, 0x3d), 1, "restore Version changed: {name}");
        assert_eq!(
            &decoded[0x41..0x45],
            &[0x08, 0x04, 0x0c, 0x01],
            "restore sector tuple changed: {name}"
        );
        checked_fixed_metadata += 1;
    }
    assert!(
        checked_fixed_metadata >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost LBA4 fixed restore metadata fixtures"
    );
}

#[test]
fn lba4_legacy_second_onlyid_is_a_stable_distinct_backup_key_seed() {
    let expected = [
        (NETAC_A, 0x44d9_ce02u32),
        (NETAC_B, 0x028e_ffd3u32),
        (LEXAR, 0x7647_b1efu32),
    ];

    let main_ids = expected
        .iter()
        .map(|(name, _)| {
            let meta = parse_reference_backup_name(name).expect("fixture metadata");
            onlyid_bits(meta.onlyid.as_deref().expect("fixture onlyid"))
        })
        .collect::<HashSet<_>>();

    for (name, expected_second) in expected {
        let image = load(name);
        let meta = parse_reference_backup_name(name).expect("fixture metadata");
        let main = onlyid_bits(meta.onlyid.as_deref().expect("fixture onlyid"));
        let k0 = (main & 0xffff) ^ (main >> 16);
        let raw = sector(&image, 4);
        let mut decoded = raw.to_vec();
        decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));
        let second = u32_le(&decoded, 0x1c);

        assert_eq!(second, expected_second, "legacy second-key drift: {name}");
        assert_ne!(
            second, main,
            "legacy backup key must not collapse to main onlyid: {name}"
        );
        assert!(
            !main_ids.contains(&second),
            "legacy second-key unexpectedly aliases another committed main onlyid: {name}"
        );
        assert_ne!(
            second,
            crc32_bare(meta.device_id.as_bytes()),
            "legacy backup key must not collapse to target device-id CRC: {name}"
        );
        assert_ne!(
            second,
            u32_le(sector(&image, 0), 0x1b8),
            "legacy backup key must not collapse to the target MBR disk signature: {name}"
        );
        assert_ne!(
            second,
            u32_le(&decoded, 0x35),
            "legacy backup key must remain independent from host-hardinfo identity: {name}"
        );
    }
}

#[test]
fn lba4_myhardinfo_mirrors_lba8_hdserialinfo_in_original_profiles() {
    let mut checked = 0usize;
    let mut nonzero = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(onlyid) = meta.onlyid.as_deref() else {
            continue;
        };

        let image = fs::read(&path).expect("fixture bytes");
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let raw4 = sector(&image, 4);
        let mut lba4 = raw4.to_vec();
        lba4[0x18..].copy_from_slice(&xor_rolling(&raw4[0x18..], k0));

        let crc = crc32_bare(meta.device_id.as_bytes());
        let lba8 = a6b0_full(sector(&image, 8), &crc.to_le_bytes(), 0);
        assert_eq!(&lba8[..4], b"LLGB", "LBA8 decode failed: {name}");

        let my_hardinfo = u32_le(&lba4, 0x35);
        let hd_serial_info = u32_le(&lba8, 0x14);
        assert_eq!(
            my_hardinfo, hd_serial_info,
            "LBA4 MyHardinfo must mirror LBA8 HDSerialInfo in observed original profiles: {name}"
        );
        nonzero += usize::from(my_hardinfo != 0);
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost MyHardinfo/HDSerialInfo fixtures"
    );
    assert!(
        nonzero > 0 && nonzero < checked,
        "mirror evidence must retain both zero current and nonzero legacy profiles"
    );
}

#[test]
fn lba4_server_flag_wire_representation_is_not_inferred_from_identity_shape() {
    let mut current_style = 0usize;
    let mut legacy_style = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(onlyid) = meta.onlyid.as_deref() else {
            continue;
        };
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 4);
        let generic = xor_rolling(&raw[0x18..], k0);
        let second_id = u32_le(&generic, 0x04);
        let hserial = &generic[0x08..0x1c];
        let current_profile = second_id == bits && hserial.iter().all(|byte| *byte == 0);
        let physical_flags = &raw[0x45..0x47];
        let generic_flags = &generic[0x2d..0x2f];

        let inspect_meta = InspectMeta::from_backup_meta(&meta);
        let view = edpcli::inspect::analyze_sector(4, raw, &inspect_meta);
        assert_eq!(
            &view.decoded[0x45..0x47],
            generic_flags,
            "inspect must preserve the official rolling-reader view: {name}"
        );

        if current_profile {
            current_style += 1;
            assert_eq!(physical_flags, &[0, 0], "{name}");
            assert_ne!(generic_flags, &[0, 0], "{name}");
        } else {
            legacy_style += 1;
            assert_ne!(physical_flags, generic_flags, "{name}");
            assert!(
                generic_flags == [0, 0] || generic_flags == [0x0b, 0x00],
                "unexpected historical reader-transformed flag profile in {name}: {generic_flags:02x?}"
            );
        }
    }

    assert!(current_style >= 2, "lost current-style zero wire flags");
    assert!(legacy_style >= 5, "lost legacy nonzero wire flag profiles");
}

#[test]
fn lba4_nonzero_bdatatoserver_reader_profile_is_the_high_entropy_zero_lba9_generation() {
    const AIGO_REV_PMAP: &str =
        "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin";

    let image = load(AIGO_REV_PMAP);
    let meta = parse_reference_backup_name(AIGO_REV_PMAP).expect("Aigo fixture metadata");
    let bits = onlyid_bits(meta.onlyid.as_deref().expect("Aigo onlyid"));
    let k0 = (bits & 0xffff) ^ (bits >> 16);
    let raw4 = sector(&image, 4);
    let mut decoded4 = raw4.to_vec();
    decoded4[0x18..].copy_from_slice(&xor_rolling(&raw4[0x18..], k0));

    assert_eq!(&raw4[0x45..0x47], &[0x64, 0x7a]);
    assert_eq!(&decoded4[0x45..0x47], &[0x0b, 0x00]);
    assert_eq!(
        &decoded4[0x20..0x34],
        &[
            0x55, 0x9c, 0xff, 0xb5, 0x28, 0xab, 0x9d, 0xb3, 0x4a, 0x5d, 0x8f, 0x7e, 0x5a, 0x60,
            0xc9, 0x9a, 0x37, 0xd6, 0xc6, 0x7a,
        ],
        "the reader=0B profile must retain the high-entropy HSerial generation"
    );
    assert_eq!(u32_le(&decoded4, 0x35), 0x8b46_13f5);
    assert!(
        sector(&image, 9).iter().all(|byte| *byte == 0),
        "the same historical generation must retain the observed all-zero LBA9 profile"
    );
}

#[test]
fn lba4_old_server_flag_profile_survives_nopwd_conversion_bit_exact() {
    const ORIGINAL: &str =
        "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin";
    const NOPWD: &str =
        "disk26_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_nopwd_20260916_233626.bin";

    let original = load(ORIGINAL);
    let nopwd = load(NOPWD);

    assert_eq!(
        sector(&original, 4),
        sector(&nopwd, 4),
        "no-password conversion must preserve the old LBA4 restore node bit-for-bit"
    );
    assert_ne!(
        sector(&original, 0),
        sector(&nopwd, 0),
        "fixtures must remain distinct pre/post-conversion captures"
    );
    assert_ne!(
        sector(&original, 12),
        sector(&nopwd, 12),
        "the no-password conversion must still show its expected metadata rewrite elsewhere"
    );
}

#[test]
fn lba4_current_writer_profile_never_carries_legacy_hserial_material() {
    let mut current = 0usize;
    let mut legacy = 0usize;
    let mut legacy_nonzero = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(onlyid) = meta.onlyid.as_deref() else {
            continue;
        };
        let bits = onlyid_bits(onlyid);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let raw = sector(&fs::read(&path).expect("fixture bytes"), 4).to_vec();
        let mut decoded = raw.clone();
        decoded[0x18..].copy_from_slice(&xor_rolling(&raw[0x18..], k0));
        if raw[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            decoded[0x47..0x1fc].fill(0);
        }

        let second = u32_le(&decoded, 0x1c);
        let hserial = &decoded[0x20..0x34];
        if second == bits {
            current += 1;
            assert!(
                hserial.iter().all(|byte| *byte == 0),
                "current-style restore node unexpectedly carries HSerialCRC material: {name}"
            );
        } else {
            legacy += 1;
            legacy_nonzero += usize::from(hserial.iter().any(|byte| *byte != 0));
        }
    }

    assert!(
        current >= 2,
        "committed fixture subset lost current-style LBA4 profiles"
    );
    assert!(
        legacy >= 5,
        "committed fixture subset lost legacy LBA4 profiles"
    );
    assert_eq!(
        legacy_nonzero, legacy,
        "every committed legacy LBA4 profile should retain non-zero HSerialCRC evidence"
    );
}

#[test]
fn lba8_encrypted_prefix_covers_the_elabel_terminating_nul() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 8);
        let last_nonzero = raw
            .iter()
            .rposition(|byte| *byte != 0)
            .expect("real LBA8 has encrypted LLGB data");
        let encrypted_len = round_up_16(last_nonzero + 1);
        assert!(raw[encrypted_len..].iter().all(|byte| *byte == 0), "{name}");

        let crc = crc32_bare(meta.device_id.as_bytes());
        let decoded = a6b0_full(&raw[..encrypted_len], &crc.to_le_bytes(), 0);
        assert_eq!(&decoded[..4], b"LLGB", "{name}");
        let llgb_len = u32_le(&decoded, 4) as usize;
        assert_eq!(
            (llgb_len / 16 + 1) * 16,
            encrypted_len,
            "writer must encrypt the block containing the ELABEL NUL: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba8_static_version_write_time_and_reserved_header_profile_match_real_fixtures() {
    let mut checked = 0usize;
    let mut write_times = std::collections::BTreeSet::new();

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 8);
        let last_nonzero = raw
            .iter()
            .rposition(|byte| *byte != 0)
            .expect("real LBA8 has encrypted LLGB data");
        let encrypted_len = round_up_16(last_nonzero + 1);
        let crc = crc32_bare(meta.device_id.as_bytes());
        let decoded = a6b0_full(&raw[..encrypted_len], &crc.to_le_bytes(), 0);

        assert_eq!(&decoded[0x08..0x0c], &[0x01, 0x00, 0x00, 0x01], "{name}");
        assert_eq!(u32_le(&decoded, 0x0c), 0x222, "{name}");
        let write_time = u32_le(&decoded, 0x10);
        assert_ne!(write_time, 0, "{name}");
        write_times.insert(write_time);
        assert!(
            decoded[0x40..0x80].iter().all(|byte| *byte == 0),
            "LBA8 reserved header range changed: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        write_times.len() >= 4,
        "LBA8 writeTime evidence lost expected per-label variability"
    );
}

#[test]
fn lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty() {
    let mut checked = 0usize;
    let mut current = 0usize;
    let mut legacy = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(onlyid) = meta.onlyid.as_deref() else {
            continue;
        };
        let bits = onlyid_bits(onlyid);
        let image = fs::read(&path).expect("fixture bytes");

        let raw4 = sector(&image, 4);
        let k0 = (bits & 0xffff) ^ (bits >> 16);
        let mut lba4 = raw4.to_vec();
        lba4[0x18..].copy_from_slice(&xor_rolling(&raw4[0x18..], k0));
        if raw4[0x47..0x1fc].iter().all(|byte| *byte == 0) {
            lba4[0x47..0x1fc].fill(0);
        }
        let current_identity =
            u32_le(&lba4, 0x1c) == bits && lba4[0x20..0x34].iter().all(|byte| *byte == 0);

        let raw8 = sector(&image, 8);
        let crc = crc32_bare(meta.device_id.as_bytes());
        let head = a6b0_full(&raw8[..0x80], &crc.to_le_bytes(), 0);
        assert_eq!(&head[..4], b"LLGB", "{name}");
        let logical_len = u32_le(&head, 4) as usize;
        let encrypted_len = ((logical_len / 16 + 1) * 16).min(SECTOR);
        let lba8 = a6b0_full(&raw8[..encrypted_len], &crc.to_le_bytes(), 0);
        let usb_only = &lba8[0x1e..0x3e];
        assert!(
            usb_only[16..].iter().all(|byte| *byte == 0),
            "UsbOnlyInfo fixed terminator/zero suffix changed: {name}"
        );
        assert!(
            lba8[0x18..0x1e].iter().all(|byte| *byte == 0),
            "MacInfo[6] must stay zero across current and legacy original profiles: {name}"
        );

        if current_identity {
            current += 1;
            assert_eq!(u32_le(&lba8, 0x14), 0, "{name}");
            let expected = format!("{bits:08x}00000000");
            assert_eq!(
                &usb_only[..expected.len()],
                expected.as_bytes(),
                "current UsbOnlyInfo must encode main onlyid followed by zero DWORD: {name}"
            );
            assert!(
                usb_only[expected.len()..].iter().all(|byte| *byte == 0),
                "current UsbOnlyInfo slot tail must stay zero: {name}"
            );
        } else {
            legacy += 1;
            assert_ne!(
                u32_le(&lba8, 0x14),
                0,
                "committed legacy profile unexpectedly lost nonzero HDSerialInfo: {name}"
            );
            assert!(
                usb_only.iter().all(|byte| *byte == 0),
                "committed legacy profile unexpectedly gained current UsbOnlyInfo: {name}"
            );
        }
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        current >= 2,
        "committed fixtures lost current LBA8 identity profiles"
    );
    assert!(
        legacy >= 2,
        "committed fixtures lost legacy LBA8 identity profiles"
    );
}

#[test]
fn lba11_is_drkb_random252_and_uses_ascii_vid_pid_in_crc_input() {
    let mut checked = 0usize;
    let mut saw_disk_size = false;
    let mut saw_chs = false;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(sectors) = meta.secs else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 11);
        assert_eq!(&raw[..4], b"DRKB", "{name}");
        assert!(
            raw[4..0x100].iter().all(|byte| *byte != 0xff),
            "RandBuffer256 uses rand()%255, so 0xFF is impossible: {name}"
        );

        let rand = &raw[..0x100];
        let cipher = &raw[0x100..];
        let disk_size = sectors * SECTOR as u64;
        let chs_size = chs_capacity(disk_size);
        let mut decoded = None;
        for (source, size) in [("DiskSize", disk_size), ("CHS", chs_size)] {
            let mut input = Vec::with_capacity(0x110);
            input.extend_from_slice(rand);
            input.extend_from_slice(&padded4_ascii(&meta.vid));
            input.extend_from_slice(&padded4_ascii(&meta.pid));
            input.extend_from_slice(&size.to_le_bytes());
            let crc = crc32_bare(&input);
            let plain = a6b0_full(cipher, &crc.to_le_bytes(), 0);
            if plain.starts_with(b"PDKB") {
                match source {
                    "DiskSize" => saw_disk_size = true,
                    "CHS" if chs_size != disk_size => saw_chs = true,
                    _ => {}
                }
                decoded = Some((source, plain));
                break;
            }
        }
        let (source, plain) =
            decoded.unwrap_or_else(|| panic!("ASCII VID/PID did not decode PDKB: {name}"));
        assert!(plain[4..].starts_with(meta.device_id.as_bytes()), "{name}");
        let end = 4 + meta.device_id.len();
        assert_eq!(plain[end], 0, "{name}");
        assert!(plain[end + 1..].iter().all(|byte| *byte == 0), "{name}");
        if name.contains("rev_pmap") {
            assert_eq!(
                source, "CHS",
                "legacy rev_pmap fixture must keep the historical CHS key profile"
            );
        }

        // Numeric little-endian VID/PID is a tempting but incorrect interpretation.
        let vid = u16::from_str_radix(&meta.vid, 16).unwrap();
        let pid = u16::from_str_radix(&meta.pid, 16).unwrap();
        for size in [disk_size, chs_capacity(disk_size)] {
            let mut input = Vec::with_capacity(0x110);
            input.extend_from_slice(rand);
            input.extend_from_slice(&(vid as u32).to_le_bytes());
            input.extend_from_slice(&(pid as u32).to_le_bytes());
            input.extend_from_slice(&size.to_le_bytes());
            let crc = crc32_bare(&input);
            let wrong = a6b0_full(cipher, &crc.to_le_bytes(), 0);
            assert!(
                !wrong.starts_with(b"PDKB"),
                "numeric VID/PID unexpectedly matched: {name}"
            );
        }
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        saw_disk_size,
        "protocol fixtures lost the normal DiskSize LBA11 profile"
    );
    assert!(
        saw_chs,
        "protocol fixtures lost the legacy CHS LBA11 profile"
    );
}

#[test]
fn lba11_authentic_sandisk_legacy_profile_still_uses_exact_disk_size() {
    const VID: &str = "0781";
    const PID: &str = "5591";
    const PHYSICAL_SECTORS: u64 = 240_254_976;

    let raw = decode_hex_fixture(SANDISK_LBA11_HEX);
    assert_eq!(raw.len(), SECTOR);
    assert_eq!(&raw[..4], b"DRKB");

    let disk_size = PHYSICAL_SECTORS * SECTOR as u64;
    let chs_size = chs_capacity(disk_size);
    assert_ne!(disk_size, chs_size);

    let decode = |size: u64| {
        let mut input = Vec::with_capacity(0x110);
        input.extend_from_slice(&raw[..0x100]);
        input.extend_from_slice(&padded4_ascii(VID));
        input.extend_from_slice(&padded4_ascii(PID));
        input.extend_from_slice(&size.to_le_bytes());
        let crc = crc32_bare(&input);
        a6b0_full(&raw[0x100..], &crc.to_le_bytes(), 0)
    };

    let exact = decode(disk_size);
    assert_eq!(&exact[..4], b"PDKB");
    assert!(exact[4..].starts_with(SANDISK_DEVICE_ID.as_bytes()));

    let legacy_chs = decode(chs_size);
    assert_ne!(
        &legacy_chs[..4],
        b"PDKB",
        "legacy/high-entropy profile must not be generalized into CHS sizing"
    );
}

#[test]
fn lba11_same_rev_pmap_device_has_both_chs_and_exact_size_writer_profiles() {
    const AIGO_REV_PMAP: &str =
        "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin";

    let original = load(AIGO_REV_PMAP);
    let meta = parse_reference_backup_name(AIGO_REV_PMAP).expect("Aigo fixture metadata");
    let exact_capture = decode_hex_fixture(AIGO_REV_PMAP_EXACT_SIZE_LBA11_HEX);
    assert_eq!(exact_capture.len(), SECTOR);

    let disk_size = meta.secs.expect("Aigo physical sectors") * SECTOR as u64;
    let chs_size = chs_capacity(disk_size);
    assert_ne!(disk_size, chs_size);

    let decode = |raw: &[u8], size: u64| {
        let mut input = Vec::with_capacity(0x110);
        input.extend_from_slice(&raw[..0x100]);
        input.extend_from_slice(&padded4_ascii(&meta.vid));
        input.extend_from_slice(&padded4_ascii(&meta.pid));
        input.extend_from_slice(&size.to_le_bytes());
        let crc = crc32_bare(&input);
        a6b0_full(&raw[0x100..], &crc.to_le_bytes(), 0)
    };

    let original_lba11 = sector(&original, 11);
    let original_chs = decode(original_lba11, chs_size);
    assert_eq!(&original_chs[..4], b"PDKB");
    assert!(original_chs[4..].starts_with(meta.device_id.as_bytes()));
    assert_ne!(&decode(original_lba11, disk_size)[..4], b"PDKB");

    let exact = decode(&exact_capture, disk_size);
    assert_eq!(&exact[..4], b"PDKB");
    assert!(exact[4..].starts_with(meta.device_id.as_bytes()));
    assert_ne!(&decode(&exact_capture, chs_size)[..4], b"PDKB");
}

#[test]
fn lba12_post_table_plaintext_is_zero_through_sector_end() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 12);
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(raw, &crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"EDPF", "{name}");
        assert!(
            plain[0x12e..].iter().all(|byte| *byte == 0),
            "post-table plaintext must remain zero through the sector end: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );

    let sandisk = decode_hex_fixture(SANDISK_LBA12_HEX);
    let crc = crc32_bare(SANDISK_DEVICE_ID.as_bytes());
    let plain = a6b0_full(&sandisk, &crc.to_le_bytes(), 0);
    assert_eq!(&plain[..4], b"EDPF");
    assert!(
        plain[0x12e..].iter().all(|byte| *byte == 0),
        "independent SanDisk original lost zero post-table plaintext"
    );
}

#[test]
fn edpf_offset_08_is_partition_count_in_both_tables() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());

        let k0 = (crc & 0xffff) ^ (crc >> 16);
        let lba7 = xor_rolling(sector(&image, 7), k0);
        let count7 = (0..8)
            .take_while(|index| {
                let base = index * 0x40;
                base + 4 <= lba7.len() && &lba7[base..base + 4] == b"EDPF"
            })
            .count();
        assert_eq!(u32_le(&lba7, 8) as usize, count7, "LBA7 {name}");

        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let count12 = (0..5)
            .take_while(|index| {
                let base = index * 0x60;
                base + 4 <= 0x170 && &lba12[base..base + 4] == b"EDPF"
            })
            .count();
        assert_eq!(u32_le(&lba12, 8) as usize, count12, "LBA12 {name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba7_physical_entries_are_packed_64_not_linux_natural_72() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let k0 = (crc & 0xffff) ^ (crc >> 16);
        let lba7 = xor_rolling(sector(&image, 7), k0);

        assert_eq!(&lba7[0x00..0x04], b"EDPF", "LBA7 entry0: {name}");
        assert_eq!(&lba7[0x40..0x44], b"EDPF", "LBA7 entry1: {name}");
        assert_eq!(&lba7[0x80..0x84], b"EDPF", "LBA7 entry2: {name}");
        assert_ne!(
            &lba7[0x48..0x4c],
            b"EDPF",
            "Linux natural 72-byte ABI must not be used for physical LBA7: {name}"
        );

        let tail = decode_edpf_tail(&lba7[0xc0..0xce]);
        assert!(
            matches!(u16::from_le_bytes([tail[0], tail[1]]), 0x0064 | 0x0206),
            "packed LBA7 pass-info must start at +0xC0: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost packed LBA7 fixtures: {checked}"
    );
}

#[test]
fn lba7_post_table_plaintext_is_zero_through_sector_end() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let k0 = (crc & 0xffff) ^ (crc >> 16);
        let lba7 = xor_rolling(sector(&image, 7), k0);

        assert!(
            lba7[0x0ce..0x200].iter().all(|byte| *byte == 0),
            "physical packed LBA7 post-table plaintext must remain writer-zero: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost LBA7 post-table evidence: {checked}"
    );
}

#[test]
fn lba7_v64_packed_legacy_file_key_wrap_matches_real_fixtures() {
    const DEFAULT_PASSWORD: &[u8] = b"0000aaaa";
    const DEFAULT_USER_KEY_CRC: u32 = 0x0429_735d;
    const LEGACY_PASSWORD_FOLD: u32 = 0x9191_9191;

    assert_eq!(crc32_bare(DEFAULT_PASSWORD), DEFAULT_USER_KEY_CRC);
    assert_eq!(
        legacy_password_fold32(DEFAULT_PASSWORD),
        LEGACY_PASSWORD_FOLD
    );

    let mut checked_entries = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let k0 = (crc & 0xffff) ^ (crc >> 16);
        let lba7 = xor_rolling(sector(&image, 7), k0);

        for index in 0..3 {
            let base = index * 0x40;
            if &lba7[base..base + 4] != b"EDPF" {
                continue;
            }
            let partition_type = u32_le(&lba7, base + 0x0c);
            if !matches!(partition_type, 2 | 4)
                || u32_le(&lba7, base + 0x30) != DEFAULT_USER_KEY_CRC
            {
                continue;
            }
            let file_key_crc = u32_le(&lba7, base + 0x34);
            let wrapped = &lba7[base + 0x38..base + 0x40];
            if file_key_crc == 0 && wrapped.iter().all(|byte| *byte == 0) {
                continue;
            }

            let mut file_key = [0u8; 8];
            let lo = u32::from_le_bytes(wrapped[..4].try_into().unwrap()) ^ LEGACY_PASSWORD_FOLD;
            let hi = u32::from_le_bytes(wrapped[4..].try_into().unwrap()) ^ LEGACY_PASSWORD_FOLD;
            file_key[..4].copy_from_slice(&lo.to_le_bytes());
            file_key[4..].copy_from_slice(&hi.to_le_bytes());
            assert_eq!(
                crc32_bare(&file_key),
                file_key_crc,
                "v0x0064 packed legacy file-key CRC mismatch: {name} entry {index}"
            );
            checked_entries += 1;
        }
    }
    assert!(
        checked_entries >= 8,
        "protocol fixtures lost positive v0x0064 legacy wrapped-key evidence: {checked_entries}"
    );
}

#[test]
fn lba8_elabel_offset_is_0x80_and_points_to_the_elabel_payload() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 8), &crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"LLGB", "{name}");

        let elabel_offset = u16::from_le_bytes(plain[0x3e..0x40].try_into().unwrap()) as usize;
        assert_eq!(elabel_offset, 0x80, "{name}");
        assert!(
            plain[elabel_offset..].starts_with(b"<ELABEL>"),
            "ElabOffset does not point to ELABEL: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba8_real_elabel_keeps_all_wire_keys_and_current_compat_slots_empty() {
    const WIRE_KEYS: [&str; 17] = [
        "GLab", "Indus", "Orgcd", "Org", "Unit", "Dept", "User", "Alarm", "Autonum", "Label",
        "Rmark", "VOL0", "VOL1", "VOL2", "VOLC0", "VOLC1", "VOLC2",
    ];
    const CURRENT_COMPAT_KEYS: [&str; 10] = [
        "Indus", "Orgcd", "Org", "Alarm", "VOL0", "VOL1", "VOL2", "VOLC0", "VOLC1", "VOLC2",
    ];

    let mut checked = 0usize;
    let mut logical_lengths = std::collections::BTreeSet::new();
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 8);
        let crc = crc32_bare(meta.device_id.as_bytes());
        let head = a6b0_full(&raw[..0x80], &crc.to_le_bytes(), 0);
        assert_eq!(&head[..4], b"LLGB", "{name}");
        let logical_len = u32_le(&head, 4) as usize;
        assert!((0x80..SECTOR).contains(&logical_len), "{name}");
        let encrypted_len = (logical_len / 16 + 1) * 16;
        assert!(encrypted_len <= SECTOR, "{name}");
        let plain = a6b0_full(&raw[..encrypted_len], &crc.to_le_bytes(), 0);
        assert_eq!(plain[logical_len], 0, "ELABEL trailing NUL moved: {name}");

        let body = &plain[0x80..logical_len];
        assert!(body.starts_with(b"<ELABEL>"), "{name}");
        let mut actual_keys = Vec::new();
        let mut ignored = std::collections::BTreeMap::<String, Vec<u8>>::new();
        for part in body[b"<ELABEL>".len()..].split(|byte| *byte == b'|') {
            if part.is_empty() {
                continue;
            }
            let Some(eq) = part.iter().position(|byte| *byte == b'=') else {
                panic!("ELABEL segment lost '=' in {name}: {part:02x?}");
            };
            let key = std::str::from_utf8(&part[..eq]).expect("ASCII ELABEL key");
            actual_keys.push(key.to_string());
            if CURRENT_COMPAT_KEYS.contains(&key) {
                ignored.insert(key.to_string(), part[eq + 1..].to_vec());
            }
        }
        assert_eq!(
            actual_keys,
            WIRE_KEYS.map(str::to_string),
            "17-key ELABEL wire order changed: {name}"
        );
        for key in CURRENT_COMPAT_KEYS {
            assert_eq!(
                ignored.get(key).map(Vec::as_slice),
                Some(&[][..]),
                "current compatibility key became non-empty: {name} {key}"
            );
        }

        // Current real profiles happen to carry zero backing between the ELABEL NUL
        // and the end of the encrypted block.  This is observational evidence only;
        // official writers preserve whatever bytes were already in that part of LBA8.
        assert!(
            plain[logical_len + 1..encrypted_len]
                .iter()
                .all(|byte| *byte == 0),
            "committed real profile gained non-zero in-block backing: {name}"
        );
        logical_lengths.insert(logical_len);
        checked += 1;
    }

    assert!(checked >= MIN_PROTOCOL_FIXTURES, "lost LBA8 real fixtures");
    assert!(
        logical_lengths.len() >= 2,
        "real LBA8 evidence must retain multiple dynamic ELABEL lengths"
    );
}

#[test]
fn real_eetu_temp_use_limits_match_the_official_unlimited_profile() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = &sector(&image, 9)[..0x80];
        if raw.iter().all(|byte| *byte == 0) {
            continue;
        }

        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(raw, &crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"EETU", "{name}");
        assert_eq!(u64_le(&plain, 0x04), 0, "ullBTime changed: {name}");
        assert_eq!(u64_le(&plain, 0x0c), 0, "ullETime changed: {name}");
        assert_eq!(
            u32_le(&plain, 0x14),
            u32::MAX,
            "useCount unlimited sentinel changed: {name}"
        );
        assert!(
            plain[0x18..0x80].iter().all(|byte| *byte == 0),
            "current real-reference EETU reverse[104] is observationally zero: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= 5,
        "protocol fixtures unexpectedly lost EETU real-device evidence: {checked}"
    );
}

#[test]
fn lba9_eetu_final_two_reverse_bytes_are_writer_zero_padding() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = &sector(&image, 9)[..0x80];
        if raw.iter().all(|byte| *byte == 0) {
            continue;
        }

        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(raw, &crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"EETU", "{name}");
        assert_eq!(
            &plain[0x7e..0x80],
            &[0, 0],
            "SetTempUse zero-initialized reverse[102..103] changed: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= 5,
        "protocol fixtures unexpectedly lost EETU zero-tail evidence: {checked}"
    );
}

#[test]
fn real_eppe_samples_keep_the_current_writer_zero_tail() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = &sector(&image, 9)[0x180..0x200];
        if raw.iter().all(|byte| *byte == 0) {
            continue;
        }

        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(raw, &crc.to_le_bytes(), 0);
        if &plain[..4] != b"EPPE" {
            continue;
        }

        let min_pass_len = u32_le(&plain, 0x04);
        assert!(
            (6..=19).contains(&min_pass_len),
            "EPPE minimum password length escaped the official writer range: {name}"
        );
        assert!(
            plain[0x08..].iter().all(|byte| *byte == 0),
            "current EPPE writer-zero tail changed: {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= 2,
        "committed protocol fixtures unexpectedly lost EPPE coverage: {checked}"
    );
}

#[test]
fn lba9_middle_profile_material_must_not_be_canonicalized_to_zero() {
    let mut sapf = 0usize;
    let mut sapf_zero_tail = 0usize;
    let mut sapf_nonzero_tail = 0usize;
    let mut nonzero_middle_gap = 0usize;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }

        let image = fs::read(&path).expect("fixture bytes");
        let lba9 = sector(&image, 9);

        if lba9[0x80..0x100].iter().any(|byte| *byte != 0) {
            nonzero_middle_gap += 1;
        }

        let mut sapf_plain = [0u8; 0x20];
        for (dst, src) in sapf_plain.iter_mut().zip(&lba9[0x100..0x120]) {
            *dst = *src ^ 0x88;
        }
        if &sapf_plain[..4] != b"SAPF" {
            continue;
        }

        sapf += 1;
        if sapf_plain[0x14..].iter().all(|byte| *byte == 0) {
            sapf_zero_tail += 1;
        } else {
            sapf_nonzero_tail += 1;
        }

        assert!(
            lba9[0x120..0x180].iter().all(|byte| *byte == 0),
            "committed SAPF fixture gained post-SAPF bytes: {name}"
        );
    }

    assert!(
        nonzero_middle_gap >= 2,
        "committed real fixtures lost the nonzero LBA9 +0x80 legacy/profile counterexample"
    );
    assert!(sapf >= 3, "committed real fixtures lost SAPF coverage");
    assert!(
        sapf_zero_tail >= 1 && sapf_nonzero_tail >= 1,
        "SAPF decoded +0x14..+0x1f must retain both zero and nonzero real profiles"
    );
}

#[test]
fn lba9_sapf_trailing_bytes_are_profile_overlap_not_reserved_zero() {
    let mut saw_zero_sapf_tail = false;
    let mut saw_nonzero_sapf_tail = false;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if parse_reference_backup_name(name).is_none() {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        let lba9 = sector(&image, 9);
        let mut sapf = [0u8; 0x20];
        for (dst, src) in sapf.iter_mut().zip(&lba9[0x100..0x120]) {
            *dst = *src ^ 0x88;
        }
        if &sapf[..4] != b"SAPF" {
            continue;
        }
        if sapf[0x14..].iter().all(|byte| *byte == 0) {
            saw_zero_sapf_tail = true;
        } else {
            saw_nonzero_sapf_tail = true;
        }
    }
    assert!(saw_zero_sapf_tail && saw_nonzero_sapf_tail);

    let lba9 = decode_hex_fixture(OFFICIAL_VIRTUAL_LONG_USER_LBA9_HEX);
    let mut user = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".repeat(3);
    user.truncate(155);
    // LBA9+0x100 corresponds to User[28].  Therefore the physical SAPF trailing
    // window +0x114..+0x11f is active User[48..60] in the long-User profile.
    assert_eq!(&lba9[0x114..0x120], &user[48..60]);
    assert!(lba9[0x114..0x120].iter().any(|byte| *byte != 0));
}

fn reconstruct_long_dept_from_lba6_lba9(raw6: &[u8], raw9: &[u8]) -> (usize, Vec<u8>) {
    let plain6 = lba6_decode(raw6);
    assert_eq!(u32_le(&plain6, 0), 0x4024_5e2a);
    let inline = &plain6[4..0x40];
    let join = if inline[59] == 0 { 59 } else { 60 };
    let continuation = &raw9[0x80..0x100];
    let nul = continuation
        .iter()
        .position(|byte| *byte == 0)
        .expect("long Dept continuation must contain a terminating NUL");
    let mut rebuilt = inline[..join].to_vec();
    rebuilt.extend_from_slice(&continuation[..nul]);
    (join, rebuilt)
}

#[test]
fn lba9_dept_continuation_preserves_both_official_reader_join_profiles() {
    let current_name =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    let current = load(current_name);
    let current6 = lba6_decode(sector(&current, 6));
    let (current_join, current_dept) =
        reconstruct_long_dept_from_lba6_lba9(sector(&current, 6), sector(&current, 9));

    let legacy6 = decode_hex_fixture(LEXAR_JOIN59_LBA6_HEX);
    let legacy9 = decode_hex_fixture(LEXAR_JOIN59_LBA9_HEX);
    let legacy6_plain = lba6_decode(&legacy6);
    let (legacy_join, legacy_dept) = reconstruct_long_dept_from_lba6_lba9(&legacy6, &legacy9);

    assert_eq!(
        current_join, 60,
        "current writer profile must join at Dept[60]"
    );
    assert_eq!(
        legacy_join, 59,
        "CEMS2.0 legacy profile must join at Dept[59] when inline[59] is NUL"
    );
    assert_eq!(
        usize::from(current6[0x3f] != 0) + 59,
        current_join,
        "current reader join must be self-described by inline Dept[59]"
    );
    assert_eq!(
        usize::from(legacy6_plain[0x3f] != 0) + 59,
        legacy_join,
        "legacy reader join must be self-described by inline Dept[59]"
    );
    assert_eq!(
        current_dept, legacy_dept,
        "both join profiles must reconstruct the same full Dept bytes"
    );
    assert_eq!(
        &current6[..0x3f],
        &legacy6_plain[..0x3f],
        "join60 and join59 profiles must keep the marker plus Dept[0..59) byte-identical"
    );
    assert_ne!(
        current6[0x3f], legacy6_plain[0x3f],
        "the historical profile split must remain isolated to the final byte of the 64B LBA6 Dept slot"
    );
    assert_eq!(
        legacy6_plain[0x3f], 0,
        "legacy join59 terminates the inline Dept at LBA6+0x3f"
    );
    assert_eq!(
        current6[0x3f], current_dept[59],
        "current join60 stores Dept[59] in the final inline byte"
    );
    assert_eq!(current_dept.len(), 76);
    let (_, _, errors) = GBK.decode(&current_dept);
    assert!(!errors, "reconstructed Dept must be valid GBK");
}

#[test]
fn lba6_short_dept_slot_is_c_string_plus_uninitialized_backing() {
    let mut checked = 0usize;
    let mut saw_nonzero_backing = false;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let plain6 = lba6_decode(sector(&image, 6));
        if u32_le(&plain6, 0) == 0x4024_5e2a {
            continue;
        }

        let dept = &plain6[..0x40];
        let inspect_meta = InspectMeta {
            device_id: Some(meta.device_id.clone()),
            ..InspectMeta::default()
        };
        let ownership =
            ownership_from_lba8(sector(&image, 8), &inspect_meta).expect("LBA8 ownership");
        assert_eq!(
            ownership.dept.unwrap_or_default(),
            gbk_string(dept),
            "short LBA6 Dept slot and LBA8 Dept diverged: {name}"
        );
        if let Some(nul) = dept.iter().position(|byte| *byte == 0) {
            saw_nonzero_backing |= dept[nul + 1..0x3f].iter().any(|byte| *byte != 0);
        }
        checked += 1;
    }

    assert!(checked >= 3, "lost short-Dept original fixture coverage");
    assert!(
        saw_nonzero_backing,
        "short Dept originals must retain non-zero post-NUL backing evidence inside +0x00..+0x3e"
    );
}

#[test]
fn real_sandisk_lba10_contains_share_and_encrypt_volume_labels() {
    let crc = crc32_bare(SANDISK_DEVICE_ID.as_bytes());
    let plain = a6b0_full(&SANDISK_LBA10[..0x80], &crc.to_le_bytes(), 0);
    assert_eq!(&plain[..4], b"EESI");
    assert_eq!(u32_le(&plain, 0x04), 1);

    let share = &plain[0x08..0x18];
    let encrypt = &plain[0x18..0x28];
    assert_eq!(
        share,
        &[0xbd, 0xbb, 0xbb, 0xbb, 0xc7, 0xf8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        encrypt,
        &[0xb1, 0xa3, 0xc3, 0xdc, 0xc7, 0xf8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert!(
        plain[0x28..0x80].iter().all(|byte| *byte == 0),
        "the one enabled real EESI sample has a zero remainder after the two label slots"
    );
    assert!(
        SANDISK_LBA10[0x80..].iter().all(|byte| *byte == 0),
        "the independent enabled EESI sample currently has a zero preserved physical tail"
    );
}

#[test]
fn provenance_audited_netac_physical_capture_closes_the_enabled_eesi_profile() {
    assert_eq!(NETAC_EESI_CAPTURE.len(), 13 * SECTOR);
    assert_eq!(
        sha256_hex(NETAC_EESI_CAPTURE),
        "3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39"
    );
    assert!(NETAC_EESI_META.contains("disk&ven_netac&prod_onlydisk&rev_0000"));
    assert!(NETAC_EESI_META.contains("\"crc32\": \"5088ee37\""));
    assert!(NETAC_EESI_META.contains("\"backup\": \"LBA0-12 (13 sectors)\""));
    assert!(NETAC_EESI_META.contains("\"md5\": \"db17edf8246ad55e9800b36701afd8e4\""));
    assert!(NETAC_EESI_PROVENANCE.contains("O_RDONLY"));
    assert!(NETAC_EESI_PROVENANCE.contains("before the second `YES` confirmation"));
    assert!(NETAC_EESI_PROVENANCE
        .contains("d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd"));

    let fixture_head = decode_hex_fixture(NETAC_ONLYDISK_LBA10_HEAD_HEX);
    let physical_lba10 = sector(NETAC_EESI_CAPTURE, 10);
    assert_eq!(&physical_lba10[..0x80], fixture_head.as_slice());

    let crc = crc32_bare(NETAC_ONLYDISK_DEVICE_ID.as_bytes());
    assert_eq!(crc, 0x5088_ee37);
    let plain = a6b0_full(&physical_lba10[..0x80], &crc.to_le_bytes(), 0);

    assert_eq!(&plain[..4], b"EESI");
    assert_eq!(u32_le(&plain, 0x04), 1);
    assert_eq!(
        &plain[0x08..0x18],
        &[0xbd, 0xbb, 0xbb, 0xbb, 0xc7, 0xf8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        &plain[0x18..0x28],
        &[0xb1, 0xa3, 0xc3, 0xdc, 0xc7, 0xf8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert!(
        plain[0x28..0x80].iter().all(|byte| *byte == 0),
        "the physical Netac EESI capture must retain the observed zero +0x28..+0x7f extension"
    );
    assert!(
        physical_lba10[0x80..].iter().all(|byte| *byte == 0),
        "the physical Netac EESI capture currently has a zero preserved tail"
    );
}

#[test]
fn lba12_main_runtime_layout_is_three_packed_96_byte_entries_plus_tail_at_0x120() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);

        let count = u32_le(&plain, 8) as usize;
        assert!((2..=3).contains(&count), "{name}");
        for index in 0..count {
            let base = index * 0x60;
            assert_eq!(&plain[base..base + 4], b"EDPF", "entry {index} {name}");
        }
        if count < 3 {
            let base = count * 0x60;
            assert!(
                plain[base..0x120].iter().all(|byte| *byte == 0),
                "unused packed entries are not zero: {name}"
            );
        }

        // The packed Windows/Linux mount format ends after 3 * 0x60 bytes.
        // The 14-byte table tail begins at 0x120; do not reinterpret 0x120 as
        // another 104-byte-entry payload from the filesystem-check component.
        let tail = decode_edpf_tail(&plain[0x120..0x12e]);
        assert_eq!(u16::from_le_bytes([tail[0], tail[1]]), 0x0206, "{name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba12_need_disturb_values_match_all_real_reference_backups() {
    let mut checked = 0usize;
    let mut checked_versions = 0usize;
    let mut type1_mask = 0u8;
    let mut type2_mask = 0u8;
    let mut type4_mask = 0u8;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let count = u32_le(&plain, 8) as usize;
        assert_eq!(
            u32_le(&plain, 0x10),
            1,
            "entry0 NeedDisturb compatibility gate changed: {name}"
        );

        for index in 0..count {
            let base = index * 0x60;
            assert_eq!(
                u32_le(&plain, base + 0x04),
                0,
                "LBA12 entry Version compatibility metadata changed: {name} entry {index}"
            );
            let partition_type = u32_le(&plain, base + 0x0c);
            let need_disturb = u32_le(&plain, base + 0x10);
            let expected_need_disturb = if index < 2 { 1 } else { 0 };
            assert_eq!(
                need_disturb, expected_need_disturb,
                "LBA12 positional NeedDisturb profile changed: {name} entry {index}"
            );
            assert!(
                need_disturb <= 1,
                "unexpected NeedDisturb={need_disturb}: {name}"
            );
            checked_versions += 1;
            match partition_type {
                1 => type1_mask |= 1 << need_disturb,
                2 => type2_mask |= 1 << need_disturb,
                4 => type4_mask |= 1 << need_disturb,
                other => panic!("unexpected partition type {other}: {name}"),
            }
        }
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        checked_versions >= MIN_PROTOCOL_FIXTURES * 3,
        "protocol audit unexpectedly lost LBA12 Version evidence"
    );
    assert_eq!(type1_mask, 0b10, "type1 NeedDisturb sample set changed");
    assert_eq!(type2_mask, 0b10, "type2 NeedDisturb sample set changed");
    assert_eq!(type4_mask, 0b01, "type4 NeedDisturb sample set changed");
}

#[test]
fn lba12_encrypt_file_key32_compatibility_slots_are_zero_in_original_entries() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let plain = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let count = u32_le(&plain, 8) as usize;

        for index in 0..count {
            let base = index * 0x60;
            assert!(
                plain[base + 0x48..base + 0x58]
                    .iter()
                    .all(|byte| *byte == 0),
                "entry {index} +0x48..+0x57 changed: {name}"
            );
            assert!(
                plain[base + 0x59..base + 0x60]
                    .iter()
                    .all(|byte| *byte == 0),
                "entry {index} +0x59..+0x5f changed: {name}"
            );
        }
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn edpf_tail_has_version_and_password_retry_fields_not_a_terminator() {
    let mut checked = 0usize;
    let mut saw_lba7_v64 = false;
    let mut saw_lba7_v206 = false;
    let mut saw_nonzero_retry = false;
    let mut saw_policy_flag = false;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());

        let k0 = (crc & 0xffff) ^ (crc >> 16);
        let lba7 = xor_rolling(sector(&image, 7), k0);
        let tail7 = decode_edpf_tail(&lba7[0xc0..0xce]);
        let version7 = u16::from_le_bytes([tail7[0], tail7[1]]);
        assert!(matches!(version7, 0x0064 | 0x0206), "LBA7 {name}");
        saw_lba7_v64 |= version7 == 0x0064;
        saw_lba7_v206 |= version7 == 0x0206;
        assert_eq!(tail7[3], 0xff, "LBA7 Share retry max {name}");
        assert!(tail7[4] <= tail7[3], "LBA7 Share retry count {name}");
        assert_eq!(tail7[6], 0xff, "LBA7 Encrypt retry max {name}");
        assert!(tail7[7] <= tail7[6], "LBA7 Encrypt retry count {name}");
        assert!(tail7[8..10].iter().all(|byte| *byte == 0), "LBA7 {name}");
        assert!(tail7[11..].iter().all(|byte| *byte == 0), "LBA7 {name}");

        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let tail12 = decode_edpf_tail(&lba12[0x120..0x12e]);
        assert_eq!(
            u16::from_le_bytes([tail12[0], tail12[1]]),
            0x0206,
            "LBA12 {name}"
        );
        assert_eq!(tail12[3], 0xff, "LBA12 Share retry max {name}");
        assert!(tail12[4] <= tail12[3], "LBA12 Share retry count {name}");
        assert_eq!(tail12[6], 0xff, "LBA12 Encrypt retry max {name}");
        assert!(tail12[7] <= tail12[6], "LBA12 Encrypt retry count {name}");
        assert!(tail12[8..10].iter().all(|byte| *byte == 0), "LBA12 {name}");
        assert!(tail12[11..].iter().all(|byte| *byte == 0), "LBA12 {name}");

        saw_nonzero_retry |= tail7[4] != 0 || tail7[7] != 0 || tail12[4] != 0 || tail12[7] != 0;
        saw_policy_flag |= tail7[2] != 0
            || tail7[5] != 0
            || tail7[10] != 0
            || tail12[2] != 0
            || tail12[5] != 0
            || tail12[10] != 0;
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(
        saw_lba7_v64 && saw_lba7_v206,
        "audit lost LBA7 version diversity"
    );
    assert!(
        saw_nonzero_retry,
        "audit lost a non-zero password retry sample"
    );
    assert!(saw_policy_flag, "audit lost EDPF tail policy-flag evidence");
}

#[test]
fn pass_info_backup_prompt_compatibility_bytes_are_zero_and_synced() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let lba7 = xor_rolling(sector(&image, 7), (crc & 0xffff) ^ (crc >> 16));
        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let tail7 = decode_edpf_tail(&lba7[0xc0..0xce]);
        let tail12 = decode_edpf_tail(&lba12[0x120..0x12e]);

        // This locks only the committed real-sample observation. The consumer
        // evidence for bResetFileKey comes from the reverse audit; zero here
        // must never be generalized into "reserved".
        assert_eq!(tail12[0x0b], 0, "bResetFileKey sample changed: {name}");
        assert_eq!(
            tail7[0x0c], 0,
            "ShareBackuppromptPeriod sample changed: {name}"
        );
        assert_eq!(
            tail7[0x0d], 0,
            "EncryptBackuppromptPeriod sample changed: {name}"
        );
        assert_eq!(
            tail7[0x0c], tail12[0x0c],
            "ShareBackuppromptPeriod diverged between LBA7/LBA12: {name}"
        );
        assert_eq!(
            tail7[0x0d], tail12[0x0d],
            "EncryptBackuppromptPeriod diverged between LBA7/LBA12: {name}"
        );
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn pass_info_no_usb_safe_flag_varies_and_matches_between_lba7_and_lba12() {
    let mut checked = 0usize;
    let mut saw_zero = false;
    let mut saw_one = false;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let lba7 = xor_rolling(sector(&image, 7), (crc & 0xffff) ^ (crc >> 16));
        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let tail7 = decode_edpf_tail(&lba7[0xc0..0xce]);
        let tail12 = decode_edpf_tail(&lba12[0x120..0x12e]);

        assert_eq!(
            tail7[0x0a], tail12[0x0a],
            "bNoUsbChkPasSafe diverged between LBA7/LBA12: {name}"
        );
        match tail12[0x0a] {
            0 => saw_zero = true,
            1 => saw_one = true,
            value => panic!("unexpected bNoUsbChkPasSafe={value}: {name}"),
        }
        assert_eq!(tail7[0x0c], 0, "LBA7 ShareBackuppromptPeriod: {name}");
        assert_eq!(tail7[0x0d], 0, "LBA7 EncryptBackuppromptPeriod: {name}");
        assert_eq!(tail12[0x0c], 0, "LBA12 ShareBackuppromptPeriod: {name}");
        assert_eq!(tail12[0x0d], 0, "LBA12 EncryptBackuppromptPeriod: {name}");
        checked += 1;
    }

    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(saw_zero && saw_one, "audit lost +0x0A value diversity");
}

#[test]
fn onlyid_text_is_a_signed_or_unsigned_view_of_one_u32_bit_pattern() {
    let mut checked = 0usize;
    let mut saw_negative = false;
    let mut saw_above_i32_max_as_unsigned = false;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_reference_backup_name(name) else {
            continue;
        };
        let Some(text) = meta.onlyid.as_deref() else {
            continue;
        };
        let bits = onlyid_bits(text);
        if text.starts_with('-') {
            saw_negative = true;
            assert_eq!(text.parse::<i32>().unwrap() as u32, bits, "{name}");
        } else {
            let unsigned = text.parse::<u32>().unwrap();
            assert_eq!(unsigned, bits, "{name}");
            saw_above_i32_max_as_unsigned |= unsigned > i32::MAX as u32;
        }
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
    assert!(saw_negative, "audit lost signed-decimal onlyid evidence");
    assert!(
        saw_above_i32_max_as_unsigned,
        "audit lost unsigned-decimal onlyid evidence above i32::MAX"
    );
}
