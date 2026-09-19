//! Provision Phase 0 protocol audit.
//!
//! These tests intentionally use curated real-device LBA0-12 protocol fixtures as evidence.
//! They do not open or mutate a physical disk.

mod common;

use std::fs;

use common::FIXTURE_DIR;
use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare, xor_rolling};
use edpcli::diskio::parse_backup_name;
use edpcli::inspect::InspectMeta;
use edpcli::metainfo::ownership_from_lba8;

fn load(name: &str) -> Vec<u8> {
    fs::read(std::path::Path::new(FIXTURE_DIR).join(name)).expect("committed protocol fixture")
}

const MIN_PROTOCOL_FIXTURES: usize = 7;

fn sector(image: &[u8], lba: usize) -> &[u8] {
    &image[lba * SECTOR..(lba + 1) * SECTOR]
}

fn u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
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

const NETAC_A: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin";
const NETAC_B: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid3069787975_20260910_172525.bin";
const LEXAR: &str =
    "disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin";

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
        let Some(meta) = parse_backup_name(name) else {
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
        let Some(meta) = parse_backup_name(name) else {
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
        let Some(meta) = parse_backup_name(name) else {
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
fn lba8_encrypted_prefix_length_is_llgb_length_rounded_to_aes_block() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_backup_name(name) else {
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
        assert_eq!(round_up_16(llgb_len), encrypted_len, "{name}");
        checked += 1;
    }
    assert!(
        checked >= MIN_PROTOCOL_FIXTURES,
        "protocol audit unexpectedly lost fixtures"
    );
}

#[test]
fn lba11_is_drkb_random252_and_uses_ascii_vid_pid_in_crc_input() {
    let mut checked = 0usize;
    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_backup_name(name) else {
            continue;
        };
        let Some(sectors) = meta.secs else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        let raw = sector(&image, 11);
        assert_eq!(&raw[..4], b"DRKB", "{name}");

        let rand = &raw[..0x100];
        let cipher = &raw[0x100..];
        let disk_size = sectors * SECTOR as u64;
        let mut decoded = None;
        for size in [disk_size, chs_capacity(disk_size)] {
            let mut input = Vec::with_capacity(0x110);
            input.extend_from_slice(rand);
            input.extend_from_slice(&padded4_ascii(&meta.vid));
            input.extend_from_slice(&padded4_ascii(&meta.pid));
            input.extend_from_slice(&size.to_le_bytes());
            let crc = crc32_bare(&input);
            let plain = a6b0_full(cipher, &crc.to_le_bytes(), 0);
            if plain.starts_with(b"PDKB") {
                decoded = Some(plain);
                break;
            }
        }
        let plain = decoded.unwrap_or_else(|| panic!("ASCII VID/PID did not decode PDKB: {name}"));
        assert!(plain[4..].starts_with(meta.device_id.as_bytes()), "{name}");
        let end = 4 + meta.device_id.len();
        assert_eq!(plain[end], 0, "{name}");
        assert!(plain[end + 1..].iter().all(|byte| *byte == 0), "{name}");

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
        let Some(meta) = parse_backup_name(name) else {
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
        let Some(meta) = parse_backup_name(name) else {
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
fn edpf_tail_has_version_and_password_retry_fields_not_a_terminator() {
    let mut checked = 0usize;
    let mut saw_lba7_v64 = false;
    let mut saw_lba7_v206 = false;
    let mut saw_nonzero_retry = false;
    let mut saw_unknown_state_bit = false;

    for entry in fs::read_dir(FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_backup_name(name) else {
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
        saw_unknown_state_bit |= tail7[2] != 0
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
    assert!(
        saw_unknown_state_bit,
        "audit lost unknown tail state-bit evidence"
    );
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
        let Some(meta) = parse_backup_name(name) else {
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
