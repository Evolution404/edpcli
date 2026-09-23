use edpcli::{
    crypto::a6b0_full_offset,
    protocol::lba7_compat::{
        locate_lba7_compatibility_extent_from_geometry, LBA7_COMPAT_EXTENT_TOTAL_SIZE,
    },
    provision::{build_lce_ciphertext, lce_plaintext},
};

const LEXAR_LCE_CIPHER: &[u8; LBA7_COMPAT_EXTENT_TOTAL_SIZE] =
    include_bytes!("../audit/protocol/lba7_compatibility/gold/lexar_lba7_compat_lba243623933.bin");

#[test]
fn lce_builder_reproduces_real_lexar_ciphertext_byte_for_byte() {
    let layout = locate_lba7_compatibility_extent_from_geometry(15_165, 255, 63, 512).unwrap();
    assert_eq!(layout.start_lba, 243_623_933);
    assert_eq!(layout.start_byte_offset, 243_623_933 * 512);

    let generated = build_lce_ciphertext(layout).unwrap();
    assert_eq!(&generated, LEXAR_LCE_CIPHER);
    assert_eq!(
        a6b0_full_offset(&generated, &[0u8; 8], layout.start_byte_offset),
        lce_plaintext().as_slice()
    );
}

#[test]
fn lce_builder_uses_the_full_64_bit_physical_byte_offset() {
    let layout = locate_lba7_compatibility_extent_from_geometry(7_481, 255, 63, 512).unwrap();
    assert!(layout.start_byte_offset > u32::MAX as u64);
    let generated = build_lce_ciphertext(layout).unwrap();
    assert_eq!(
        a6b0_full_offset(&generated, &[0u8; 8], layout.start_byte_offset),
        lce_plaintext().as_slice()
    );
}

#[test]
fn lce_builder_rejects_noncanonical_extent_geometry() {
    let mut layout = locate_lba7_compatibility_extent_from_geometry(15_165, 255, 63, 512).unwrap();
    layout.size_bytes = 512;
    assert!(build_lce_ciphertext(layout)
        .unwrap_err()
        .contains("size mismatch"));
}
