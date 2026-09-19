//! Provision Phase 0 protocol audit.
//!
//! These tests intentionally use the committed real-device LBA0-13 snapshots as evidence.
//! They do not open or mutate a physical disk.

mod common;

use std::fs;

use common::BAK_DIR;
use edpcli::common::SECTOR;
use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare, xor_rolling};
use edpcli::diskio::parse_backup_name;
use edpcli::inspect::InspectMeta;
use edpcli::metainfo::ownership_from_lba8;

fn load(name: &str) -> Vec<u8> {
    fs::read(std::path::Path::new(BAK_DIR).join(name)).expect("committed protocol fixture")
}

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

const NETAC_A: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin";
const NETAC_B: &str =
    "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid3069787975_20260910_172525.bin";
const LEXAR: &str =
    "disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin";

#[test]
fn canonical_reserved_sectors_are_zero_across_committed_real_images() {
    let mut checked = 0usize;
    let mut manufacturing_marks = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let image = fs::read(&path).expect("fixture bytes");
        assert_eq!(image.len(), 14 * SECTOR, "{}", path.display());
        for lba in [1usize, 2, 5, 10, 13] {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
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
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
        let path = entry.expect("backup entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = parse_backup_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        assert_eq!(image.len(), 14 * SECTOR, "{name}");
        let crc = crc32_bare(meta.device_id.as_bytes());
        let expected = a7f0_full(&[0u8; 144], &crc.to_le_bytes(), 0x170);
        assert_eq!(&sector(&image, 12)[0x170..], expected.as_slice(), "{name}");
        checked += 1;
    }
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn canonical_glab_is_stable_across_decodable_real_images() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn lba8_encrypted_prefix_length_is_llgb_length_rounded_to_aes_block() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn lba11_is_drkb_random252_and_uses_ascii_vid_pid_in_crc_input() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn lba12_is_a_single_512_byte_ciphertext_with_zero_plaintext_tail() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}

#[test]
fn edpf_offset_08_is_partition_count_in_both_tables() {
    let mut checked = 0usize;
    for entry in fs::read_dir(BAK_DIR).expect("backup fixtures") {
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
    assert!(checked >= 20, "protocol audit unexpectedly lost fixtures");
}
