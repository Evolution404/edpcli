use edpcli::{
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    provision::{
        unwrap_legacy_lba7_file_key, wrap_file_key, wrap_legacy_lba7_file_key, FileKeyWrapMode,
    },
};

const DEVICE_ID: &[u8] = b"disk&ven_virtual&prod_writerproof&rev_0001";
const PASSWORD: &[u8] = b"ProofPass1!";
const FILE_KEY: [u8; 16] = [
    0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb, 0x42,
];

const MODE1: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode1_lba12.hex");
const MODE2: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode2_lba12.hex");
const MODE3: &str =
    include_str!("fixtures/protocol_evidence/official_virtual_writer_mode3_lba12.hex");

fn decode_hex(text: &str) -> Vec<u8> {
    let hex: String = text
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn legacy_lba7_key_material_matches_real_current_netac_bytes() {
    let image = std::fs::read(
        "tests/fixtures/protocol/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin",
    )
    .unwrap();
    let device_id = b"disk&ven_netac&prod_onlydisk";
    let crc = crc32_bare(device_id);
    let plain = xor_rolling(&image[7 * 512..8 * 512], (crc & 0xffff) ^ (crc >> 16));
    assert_eq!(u32::from_le_bytes(plain[0x08..0x0c].try_into().unwrap()), 3);

    let file_key = [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38];
    let material = wrap_legacy_lba7_file_key(b"0000aaaa", file_key);
    assert_eq!(material.user_key_crc, 0x0429_735d);
    assert_eq!(material.file_key_crc, 0xf169_bc97);

    assert!(plain[0x30..0x40].iter().all(|byte| *byte == 0));
    assert_eq!(material.packed16().as_slice(), &plain[0x70..0x80]);
    assert_eq!(material.packed16().as_slice(), &plain[0xb0..0xc0]);
}

#[test]
fn key_material_matches_official_mode1_mode2_mode3_writer_bytes() {
    let outer_crc = crc32_bare(DEVICE_ID);
    for (mode, fixture) in [
        (FileKeyWrapMode::A7f0, MODE1),
        (FileKeyWrapMode::Sm4, MODE2),
        (FileKeyWrapMode::Aes128Ecb, MODE3),
    ] {
        let raw = decode_hex(fixture);
        let plain = a6b0_full(&raw, &outer_crc.to_le_bytes(), 0);
        assert_eq!(&plain[..4], b"EDPF");

        let expected = wrap_file_key(PASSWORD, FILE_KEY, mode);
        assert_eq!(
            expected.packed24().as_slice(),
            &plain[0x30..0x48],
            "mode{} key material differs from first-party output",
            mode.raw()
        );
        assert_eq!(plain[0x58], mode.raw());
    }
}

#[test]
fn key_material_crc_fields_are_over_original_password_and_plain_file_key() {
    let material = wrap_file_key(PASSWORD, FILE_KEY, FileKeyWrapMode::Sm4);
    assert_eq!(material.user_key_crc, 0xe5a0_95a1);
    assert_eq!(material.file_key_crc, 0xff4c_1d36);
}

#[test]
fn legacy_lba7_rewrap_changes_only_password_wrapper_not_raw_file_key() {
    let raw = [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38];
    let old_password = b"OldPass1!";
    let new_password = b"NewPass2!";

    let old = wrap_legacy_lba7_file_key(old_password, raw);
    assert_eq!(unwrap_legacy_lba7_file_key(old_password, old).unwrap(), raw);
    assert!(unwrap_legacy_lba7_file_key(new_password, old).is_err());

    let recovered = unwrap_legacy_lba7_file_key(old_password, old).unwrap();
    let new = wrap_legacy_lba7_file_key(new_password, recovered);

    assert_eq!(new.file_key_crc, old.file_key_crc);
    assert_ne!(new.user_key_crc, old.user_key_crc);
    assert_ne!(new.wrapped_file_key, old.wrapped_file_key);
    assert_eq!(unwrap_legacy_lba7_file_key(new_password, new).unwrap(), raw);
    assert!(unwrap_legacy_lba7_file_key(old_password, new).is_err());
}
