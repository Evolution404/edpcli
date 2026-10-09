use edpcli::{
    protocol::crypto::a6b0_full,
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
        a6b0_full(&generated, &[0u8; 8], layout.start_byte_offset),
        lce_plaintext().as_slice()
    );
}

#[test]
fn lce_builder_uses_the_full_64_bit_physical_byte_offset() {
    let layout = locate_lba7_compatibility_extent_from_geometry(7_481, 255, 63, 512).unwrap();
    assert!(layout.start_byte_offset > u32::MAX as u64);
    let generated = build_lce_ciphertext(layout).unwrap();
    assert_eq!(
        a6b0_full(&generated, &[0u8; 8], layout.start_byte_offset),
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

#[test]
fn native_lce_producer_audit_certifies_exact_legacy_512_source_only() {
    use edpcli::provision::audit_native_lce_against_legacy_producer;
    let blocks = LEXAR_LCE_CIPHER
        .chunks_exact(512)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>();
    let result = audit_native_lce_against_legacy_producer(512, 243_623_933, &blocks).unwrap();
    assert!(result.certified_legacy_512b_reproduction);
    assert_eq!(result.mismatching_legacy_payload_bytes, 0);
    assert_eq!(result.opaque_tail_bytes, 0);
    assert_eq!(result.source_native_bytes, 3072);
}

#[test]
fn native_lce_producer_audit_preserves_4kn_unowned_tail_and_never_certifies_writer() {
    use edpcli::protocol::crypto::a7f0_full;
    use edpcli::provision::audit_native_lce_against_legacy_producer;
    let start = 62_476_561u64;
    let mut sector = a7f0_full(lce_plaintext(), &[0u8; 8], start * 4096);
    sector.extend((0..1024).map(|n| if n % 5 == 0 { 0u8 } else { 0x9b }));
    let blocks = vec![sector.clone()];
    let matched = audit_native_lce_against_legacy_producer(4096, start, &blocks).unwrap();
    assert_eq!(matched.mismatching_legacy_payload_bytes, 0);
    assert_eq!(matched.opaque_tail_bytes, 1024);
    assert_eq!(matched.opaque_tail_nonzero, 819);
    assert!(!matched.certified_legacy_512b_reproduction);
    sector[3072] = 0x42;
    assert_eq!(
        audit_native_lce_against_legacy_producer(4096, start, &[sector.clone()])
            .unwrap()
            .mismatching_legacy_payload_bytes,
        0
    );
    sector[3071] ^= 0xff;
    assert_eq!(
        audit_native_lce_against_legacy_producer(4096, start, &[sector])
            .unwrap()
            .mismatching_legacy_payload_bytes,
        1
    );
    assert!(audit_native_lce_against_legacy_producer(4096, start, &vec![vec![0; 512]; 6]).is_err());
    assert!(audit_native_lce_against_legacy_producer(512, start, &[vec![0; 4096]]).is_err());
    assert!(audit_native_lce_against_legacy_producer(1024, start, &[vec![0; 1024]]).is_err());
    assert!(audit_native_lce_against_legacy_producer(4096, u64::MAX, &blocks).is_err());
}
