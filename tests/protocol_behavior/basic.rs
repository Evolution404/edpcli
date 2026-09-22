use edpcli::protocol::{lba0::*, lba3::*, lba5::*, profile::*};

fn hex(text: &str) -> Vec<u8> {
    let s: String = text.split_whitespace().collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
pub fn lba0_profiles_decode_mbr_and_preserve_backing() {
    let prefixes = [
        (vec![0; 400], Lba0Bootstrap::Zero),
        (
            hex(include_str!(
                "../fixtures/protocol_evidence/official_usb_main_bsec_lba0_prefix.hex"
            )),
            Lba0Bootstrap::UsbMainBsec,
        ),
        (
            hex(include_str!(
                "../fixtures/protocol_evidence/official_netac_mbr_lba0_prefix.hex"
            )),
            Lba0Bootstrap::NetacMbr,
        ),
    ];
    for (prefix, expected) in prefixes {
        for sector_size in [0u32, 512] {
            let mut raw = [0xa5; 512];
            raw[..400].copy_from_slice(&prefix);
            raw[0x1a0..0x1a4].copy_from_slice(&sector_size.to_le_bytes());
            raw[0x1b8..0x1bc].copy_from_slice(&0x12345678u32.to_le_bytes());
            for i in 0..4 {
                let off = 0x1be + 16 * i;
                raw[off] = 0x80;
                raw[off + 4] = 7;
                raw[off + 8..off + 12].copy_from_slice(&(63 + i as u32).to_le_bytes());
                raw[off + 12..off + 16].copy_from_slice(&(4096 + i as u32).to_le_bytes());
            }
            raw[510..].copy_from_slice(&[0x55, 0xaa]);
            let view = parse_lba0(&raw).unwrap();
            assert_eq!(view.bootstrap, expected);
            assert_eq!(
                view.sector_size,
                if sector_size == 0 {
                    Lba0SectorSizeOverlay::Absent
                } else {
                    Lba0SectorSizeOverlay::SectorSize512
                }
            );
            assert_eq!(view.disk_signature, 0x12345678);
            for (i, part) in view.partitions.iter().enumerate() {
                assert_eq!(
                    (
                        part.boot_indicator,
                        part.partition_type,
                        part.start_lba,
                        part.sector_count
                    ),
                    (0x80, 7, 63 + i as u32, 4096 + i as u32)
                );
                assert_eq!(part.start_chs, [0xa5; 3]);
                assert_eq!(part.end_chs, [0xa5; 3]);
            }
            assert_eq!(view.compat_190_19f.bytes(), &[0xa5; 16]);
            assert_eq!(view.compat_1a4_1b4.bytes(), &[0xa5; 17]);
            assert_eq!(view.message_pointers, [0xa5; 3]);
            assert_eq!(view.mbr_reserved.bytes(), &[0xa5; 2]);
            assert_eq!(view.reconstruct(), raw);
            raw[0] ^= 1;
            assert_eq!(parse_lba0(&raw).unwrap().bootstrap, Lba0Bootstrap::Unknown);
            raw[510] = 0;
            assert!(parse_lba0(&raw).is_err());
        }
    }
}

#[test]
pub fn opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics() {
    let mut raw = [0; 512];
    for seed in [0, 7, 255] {
        for (i, b) in raw.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(seed);
        }
        let lba3 = parse_lba3(&raw);
        let lba5 = parse_lba5(&raw);
        assert_eq!(lba3.payload.bytes(), &raw);
        assert_eq!(lba5.payload.bytes(), &raw);
        assert_eq!(lba3.reconstruct(), raw);
        assert_eq!(lba5.reconstruct(), raw);
        assert_eq!(
            lba3.profile,
            if seed == 0 {
                Lba3Metadata::Zero
            } else {
                Lba3Metadata::Unknown
            }
        );
    }
    let historical: [u8; 512] = hex(include_str!(
        "../fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex"
    ))
    .try_into()
    .unwrap();
    assert_eq!(parse_lba3(&historical).profile, Lba3Metadata::HistoricalMpB);
    assert_eq!(parse_lba3(&historical).reconstruct(), historical);
}

#[test]
pub fn basic_parsers_replay_all_committed_physical_gold() {
    let mut saw_mp_a = false;
    for line in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let columns: Vec<_> = line.split('\t').collect();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(columns[4]))
                .unwrap();
        let raw0 = image[..512].try_into().unwrap();
        assert_ne!(parse_lba0(raw0).unwrap().bootstrap, Lba0Bootstrap::Unknown);
        assert_eq!(parse_lba0(raw0).unwrap().reconstruct(), *raw0);
        let raw3 = image[1536..2048].try_into().unwrap();
        let view = parse_lba3(raw3);
        saw_mp_a |= view.profile == Lba3Metadata::KingstonMpA;
        assert_ne!(view.profile, Lba3Metadata::Unknown);
        assert_eq!(view.reconstruct(), *raw3);
        assert_eq!(
            parse_lba5(image[2560..3072].try_into().unwrap())
                .reconstruct()
                .as_slice(),
            &image[2560..3072]
        );
    }
    assert!(saw_mp_a);
}
