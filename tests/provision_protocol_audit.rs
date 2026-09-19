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
