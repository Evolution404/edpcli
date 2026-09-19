//! Provision Phase 0 protocol audit.
//!
//! These tests intentionally use curated real-device LBA0-12 protocol fixtures as evidence.
//! They do not open or mutate a physical disk.

mod common;

use std::fs;

use common::FIXTURE_DIR;
use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare, lba6_decode, xor_rolling};
use edpcli::diskio::{parse_backup_name, BackupMeta};
use edpcli::inspect::InspectMeta;
use edpcli::metainfo::ownership_from_lba8;

fn load(name: &str) -> Vec<u8> {
    fs::read(std::path::Path::new(FIXTURE_DIR).join(name)).expect("committed protocol fixture")
}

const MIN_PROTOCOL_FIXTURES: usize = 7;
const SANDISK_LBA10: &[u8; 512] =
    include_bytes!("fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba10.bin");
const SANDISK_DEVICE_ID: &str = "disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00";

fn parse_reference_backup_name(name: &str) -> Option<BackupMeta> {
    let meta = parse_backup_name(name)?;
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

            // Keep the adjacent 16-byte extension slot as a separately tracked
            // field. Its current all-zero profile is not evidence that it is
            // padding: another official ABI names the corresponding material
            // EncryptFileKey32[16].
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
}

#[test]
fn lba6_legacy_beizhu_and_extension_keep_opaque_bytes_after_the_c_string() {
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
fn lba3_manufacturing_payload_is_an_opaque_whole_sector_not_just_a_marker_string() {
    const MARKED: &str =
        "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin";
    let image = load(MARKED);
    let lba3 = sector(&image, 3);

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
        }

        assert_eq!(&decoded[0x39..0x3d], b"LLGB", "{name}");
        assert_eq!(&decoded[0x1fc..0x200], b"LLGB", "{name}");
        assert_eq!(u32_le(&decoded, 0x18), bits ^ 0x8888_8888, "{name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
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
fn lba12_is_a_single_512_byte_ciphertext_with_zero_plaintext_tail() {
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
        assert!(plain[0x170..].iter().all(|byte| *byte == 0), "{name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
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
            let partition_type = u32_le(&plain, base + 0x0c);
            let need_disturb = u32_le(&plain, base + 0x10);
            assert!(
                need_disturb <= 1,
                "unexpected NeedDisturb={need_disturb}: {name}"
            );
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
    assert_eq!(type1_mask, 0b10, "type1 NeedDisturb sample set changed");
    assert_eq!(type2_mask, 0b10, "type2 NeedDisturb sample set changed");
    assert_eq!(type4_mask, 0b01, "type4 NeedDisturb sample set changed");
}

#[test]
fn lba12_packed_entry_unresolved_extension_bytes_are_observationally_zero() {
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
fn lba12_pass_info_reset_key_and_backup_prompt_bytes_are_observationally_zero() {
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
        let lba12 = a6b0_full(sector(&image, 12), &crc.to_le_bytes(), 0);
        let tail = decode_edpf_tail(&lba12[0x120..0x12e]);

        // This locks only the committed real-sample observation. The consumer
        // evidence for bResetFileKey comes from the reverse audit; zero here
        // must never be generalized into "reserved".
        assert_eq!(tail[0x0b], 0, "bResetFileKey sample changed: {name}");
        assert_eq!(
            tail[0x0c], 0,
            "ShareBackuppromptPeriod sample changed: {name}"
        );
        assert_eq!(
            tail[0x0d], 0,
            "EncryptBackuppromptPeriod sample changed: {name}"
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
