//! Cross-check historical fingerprints against the full, deduplicated gold set.
//! These observations do not promote any protocol field to COMPLETE.

use edpcli::crypto::{a6b0_full, crc32_bare, lba6_checksum, lba6_decode, xor_rolling};
use edpcli::diskio::parse_backup_name;
use edpcli::inspect::{analyze_sector, InspectMeta};
use std::{fs, path::Path};

const GOLD: &str = include_str!("../audit/protocol/gold_samples.tsv");
const AUTHENTIC: &[u8; 6656] =
    include_bytes!("../audit/protocol/gold/authentic-nopwd/sandisk_ultra_20260823_lba0_12.bin");

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn strict_gold_legacy_fingerprints_are_not_a_single_required_conjunction() {
    let mut total = 0;
    let mut hserial_counts = [0; 3]; // zero, common fixed tuple, other nonzero
    let mut join_counts = [0; 3]; // short, join59, join60
    let mut nonzero_fragments = 0;
    let mut bootstrap_counts = [0; 3]; // zero, UsbMainBSec, Netac

    for row in GOLD.lines().skip(1) {
        let columns: Vec<_> = row.split('\t').collect();
        if columns[0] != "strict-encrypted" {
            continue;
        }
        let name = columns[3];
        let meta = parse_backup_name(name).expect("gold backup metadata");
        let onlyid = meta.onlyid.as_ref().unwrap().parse::<i64>().unwrap() as u32;
        let image = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(columns[4])).unwrap();
        let node = xor_rolling(
            &image[4 * 512 + 0x18..5 * 512],
            (onlyid & 0xffff) ^ (onlyid >> 16),
        );
        assert_eq!(word(&node, 0), onlyid ^ 0x8888_8888, "{name}");
        let hserial: Vec<_> = (0..5).map(|i| word(&node, 8 + 4 * i)).collect();
        let hserial_class = if hserial.iter().all(|value| *value == 0) {
            0
        } else if hserial == [0x1d29, 0x7b, 0x4dd, 0x79, 0x7c] {
            1
        } else {
            2
        };
        hserial_counts[hserial_class] += 1;

        let raw6 = &image[6 * 512..7 * 512];
        assert_eq!(lba6_checksum(&raw6[..508]), word(raw6, 508), "{name}");
        let plain6 = lba6_decode(raw6);
        let join_class = if word(&plain6, 0) != 0x4024_5e2a {
            0
        } else if plain6[0x3f] == 0 {
            1
        } else {
            2
        };
        join_counts[join_class] += 1;
        let fragment_nonzero = plain6[0x1e0..0x1ee].iter().any(|byte| *byte != 0);
        nonzero_fragments += usize::from(fragment_nonzero);
        if join_class == 1 {
            assert_eq!(hserial_class, 1, "{name}");
            assert!(
                !fragment_nonzero,
                "current join59 examples have zero fragments: {name}"
            );
        }
        if fragment_nonzero {
            assert_eq!(
                join_class, 0,
                "current nonzero-fragment example has short Dept: {name}"
            );
            assert_eq!(hserial_class, 2, "{name}");
        }

        let head8 = a6b0_full(
            &image[8 * 512..8 * 512 + 0x80],
            &crc32_bare(meta.device_id.as_bytes()).to_le_bytes(),
            0,
        );
        assert_eq!(&head8[..4], b"LLGB", "{name}");
        assert_eq!(word(&node, 0x1d), word(&head8, 0x14), "{name}");
        assert_eq!(
            head8[0x1e..0x3e].iter().all(|byte| *byte == 0),
            hserial_class != 0,
            "observed legacy UsbOnlyInfo shape: {name}"
        );

        let prefix_hash = edpcli::sha256::sha256_hex(&image[..400]);
        let bootstrap_class = if image[..400].iter().all(|byte| *byte == 0) {
            0
        } else if prefix_hash == "4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed"
        {
            1
        } else {
            assert_eq!(
                prefix_hash,
                "00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec"
            );
            2
        };
        bootstrap_counts[bootstrap_class] += 1;
        total += 1;
    }
    assert_eq!(total, 19);
    assert_eq!(hserial_counts, [6, 12, 1]);
    assert_eq!(join_counts, [12, 3, 4]);
    assert_eq!(nonzero_fragments, 1);
    assert_eq!(bootstrap_counts, [8, 10, 1]);
}

#[test]
fn authentic_nopwd_lba4_flags_require_a_separate_representation_audit() {
    let raw = &AUTHENTIC[4 * 512..5 * 512];
    let onlyid = 794_661_040u32;
    let node = xor_rolling(&raw[0x18..], (onlyid & 0xffff) ^ (onlyid >> 16));
    assert_eq!(word(&node, 0), onlyid ^ 0x8888_8888);
    assert_eq!(word(&node, 4), 0x4a32_ba39);
    assert!(node[8..0x1c].iter().any(|byte| *byte != 0));
    assert_eq!(&node[0x21..0x25], b"LLGB");
    assert_eq!(word(&node, 0x25), 1);
    assert_eq!(&node[0x29..0x2d], &[8, 4, 12, 1]);
    assert_eq!(&raw[0x45..0x47], &[0, 0]);
    assert_eq!(&node[0x2d..0x2f], &[0xd4, 0xd9]);
    assert!(raw[0x47..0x1fc].iter().any(|byte| *byte != 0));

    // Current inspect exposes the rolling-reader view for this nonzero-HSerial
    // profile. Neither it nor the wire zeros prove producer-side flag values.
    let view = analyze_sector(4, raw, &InspectMeta::default());
    assert_eq!(&view.decoded[0x45..0x47], &[0xd4, 0xd9]);
    assert!(view.fields.iter().any(|field| {
        field.label == "bDataToServer"
            && field.value.contains("reader=0xD4")
            && field.value.contains("wire=0x00")
    }));
    assert!(view.fields.iter().any(|field| {
        field.label == "bConnetServer"
            && field.value.contains("reader=0xD9")
            && field.value.contains("wire=0x00")
    }));
    // An older decoder zeroed every raw-zero byte independently; that even
    // erased the B of LLGB, so its derived dec/ files are not counterevidence.
    assert_eq!(raw[0x3c], 0);
    assert_eq!(node[0x24], b'B');
}
