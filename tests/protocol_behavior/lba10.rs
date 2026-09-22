use edpcli::{
    crypto::crc32_bare,
    protocol::{
        lba10::{parse_lba10, Lba10View},
        profile::Lba10Eesi,
    },
};

#[test]
pub fn lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail() {
    let mut absent = 0usize;
    for row in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = row.split('\t').collect();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(c[4])).unwrap();
        let raw: &[u8; 512] = image[10 * 512..11 * 512].try_into().unwrap();
        let view = parse_lba10(raw, 0, Lba10Eesi::AbsentZero).unwrap();
        assert_eq!(view.reconstruct(), *raw);
        assert_eq!(view.reencode(), *raw);
        absent += 1;
    }
    assert!(absent > 0);

    let image = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin"),
    )
    .unwrap();
    let raw: &[u8; 512] = image[10 * 512..11 * 512].try_into().unwrap();
    let crc = crc32_bare(b"disk&ven_netac&prod_onlydisk&rev_0000");
    assert_eq!(crc, 0x5088_ee37);
    let view = parse_lba10(raw, crc, Lba10Eesi::EesiEnabled).unwrap();
    let Lba10View::Enabled {
        suspension_flag,
        share_label,
        encrypt_label,
        extension,
        tail,
        ..
    } = &view
    else {
        panic!("expected enabled EESI");
    };
    assert_eq!(*suspension_flag, 1);
    assert!(!share_label.value().is_empty());
    assert!(!encrypt_label.value().is_empty());
    assert!(extension.bytes().iter().all(|byte| *byte == 0));
    assert!(tail.bytes().iter().all(|byte| *byte == 0));
    assert_eq!(view.reconstruct(), *raw);
    assert_eq!(view.reencode(), *raw);

    // The EESI writer owns only the first 0x80 bytes. A nonzero historical
    // physical tail remains byte-for-byte outside the encrypted payload.
    let mut with_tail = *raw;
    with_tail[0x80] = 0x5a;
    with_tail[0x1ff] = 0xa5;
    let preserved = parse_lba10(&with_tail, crc, Lba10Eesi::EesiEnabled).unwrap();
    let Lba10View::Enabled { tail, .. } = &preserved else {
        unreachable!()
    };
    assert_eq!(tail.bytes()[0], 0x5a);
    assert_eq!(tail.bytes()[383], 0xa5);
    assert_eq!(preserved.reencode(), with_tail);
}

#[test]
pub fn lba10_profiles_fail_closed() {
    let zero = [0u8; 512];
    assert!(parse_lba10(&zero, 0, Lba10Eesi::Unknown).is_err());
    let mut nonzero = zero;
    nonzero[0] = 1;
    assert!(parse_lba10(&nonzero, 0, Lba10Eesi::AbsentZero).is_err());
}
