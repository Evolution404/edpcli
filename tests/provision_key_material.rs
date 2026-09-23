use edpcli::{
    crypto::{a6b0_full, crc32_bare},
    provision::{wrap_file_key, FileKeyWrapMode},
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
