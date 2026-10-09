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
        .as_chunks::<512>()
        .0
        .iter()
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

#[test]
fn u391_native_4kn_lce_locator_matches_official_chs_geometry_not_payload_provenance() {
    // Real U391 read-only 2026-10-09 evidence: two LBA7 records point here.
    // Independently prove that the current OEM producer's CHS-minus-0xE0000
    // formula results in the same native LBA with a single 4096B block.
    let layout = locate_lba7_compatibility_extent_from_geometry(3889, 255, 63, 4096)
        .expect("official native USB geometry must be representable");
    assert_eq!(layout.start_lba, 62_476_561);
    assert_eq!(layout.size_sectors, 1);
    assert_eq!(layout.size_bytes, 4096);
    assert_eq!(layout.start_byte_offset, 62_476_561 * 4096);
    assert_eq!(layout.chs_bytes, 62_476_785 * 4096);
    // A correct address and allocation size do not certify the 4Kn payload.
    assert!(build_lce_ciphertext(layout).is_err());
}

#[test]
fn native_lce_512_retains_real_lexar_gold_without_trailing_bytes() {
    use edpcli::provision::build_native_lce_ciphertext;
    let layout = locate_lba7_compatibility_extent_from_geometry(15_165, 255, 63, 512).unwrap();
    let generated = build_native_lce_ciphertext(layout, 512).unwrap();
    assert_eq!(generated.len(), 3072);
    assert_eq!(generated.as_slice(), LEXAR_LCE_CIPHER.as_slice());
}

#[test]
fn native_lce_4kn_encrypts_original_3072_and_a_zero_plaintext_tail_as_one_stream() {
    use edpcli::protocol::crypto::a7f0_full;
    use edpcli::provision::build_native_lce_ciphertext;

    let layout = locate_lba7_compatibility_extent_from_geometry(3889, 255, 63, 4096).unwrap();
    assert_eq!(layout.start_byte_offset, 62_476_561u64 * 4096);
    let generated = build_native_lce_ciphertext(layout, 4096).unwrap();
    assert_eq!(generated.len(), 4096);
    let mut expected_plain = vec![0u8; 4096];
    expected_plain[..3072].copy_from_slice(lce_plaintext());
    assert_eq!(
        a6b0_full(&generated, &[0u8; 8], layout.start_byte_offset),
        expected_plain,
    );
    assert_eq!(
        &generated[..3072],
        a7f0_full(lce_plaintext(), &[0u8; 8], layout.start_byte_offset)
    );
    assert!(
        generated[3072..].iter().any(|b| *b != 0),
        "the last 1024B MUST be encrypted zeros, never raw zeros"
    );
    assert_eq!(
        &generated[3072..],
        a7f0_full(&[0u8; 1024], &[0u8; 8], layout.start_byte_offset + 3072),
    );
    assert_ne!(
        &generated[3072..],
        a7f0_full(&[0u8; 1024], &[0u8; 8], 0),
        "the tail may not restart its tweak at zero"
    );
    let mut modified = generated.clone();
    modified[3072] ^= 0x01;
    assert_ne!(
        &a6b0_full(&modified, &[0u8; 8], layout.start_byte_offset)[3072..],
        &[0u8; 1024],
    );
}

#[test]
fn native_lce_4kn_rejects_mismatched_geometry_and_offsets() {
    use edpcli::provision::build_native_lce_ciphertext;
    let layout = locate_lba7_compatibility_extent_from_geometry(3889, 255, 63, 4096).unwrap();
    assert!(build_native_lce_ciphertext(layout, 512).is_err());
    assert!(build_native_lce_ciphertext(layout, 1024).is_err());
    let mut bad = layout;
    bad.size_bytes = 3072;
    assert!(build_native_lce_ciphertext(bad, 4096).is_err());
    let mut bad = layout;
    bad.size_sectors = 2;
    assert!(build_native_lce_ciphertext(bad, 4096).is_err());
    let mut bad = layout;
    bad.start_byte_offset += 512;
    assert!(build_native_lce_ciphertext(bad, 4096).is_err());
    let mut bad = layout;
    bad.start_lba = u64::MAX;
    assert!(build_native_lce_ciphertext(bad, 4096).is_err());
}
#[test]
fn legacy_lce_gold_does_not_have_u391_neighborhood_512b_zero_frame_signature() {
    // For the U391 source and adjacent blocks, every 512B frame was observed
    // to have these five zero offsets. Neither genuine legacy 512B LCE
    // ciphertext fixture has this signature; it is not an OEM payload gold.
    let zero_slots = [0usize, 5, 6, 7, 256];
    let frames = LEXAR_LCE_CIPHER
        .as_chunks::<512>()
        .0
        .iter()
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 6);
    assert!(!zero_slots
        .iter()
        .all(|offset| { frames.iter().all(|frame| frame[*offset] == 0) }));
    let mut synthetic_4kn = vec![0x8Au8; 4096];
    for subframe in synthetic_4kn.as_chunks_mut::<512>().0 {
        for offset in zero_slots {
            subframe[offset] = 0;
        }
    }
    assert!(synthetic_4kn[3072..].iter().any(|byte| *byte != 0));
    let audit = edpcli::provision::audit_native_lce_against_legacy_producer(
        4096,
        62_476_561,
        &[synthetic_4kn],
    )
    .unwrap();
    assert_eq!(audit.opaque_tail_bytes, 1024);
    assert!(audit.opaque_tail_nonzero > 0);
    assert!(!audit.certified_legacy_512b_reproduction);
}
