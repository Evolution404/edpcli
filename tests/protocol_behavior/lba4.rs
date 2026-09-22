use edpcli::{
    crypto::xor_rolling,
    protocol::{lba4::*, profile::*},
};
fn sample(encoding: Lba4Encoding, short: bool) -> [u8; 512] {
    let id = 794661040u32;
    let mut plain = [0xa5; 512];
    plain[..24].fill(0);
    plain[..15].copy_from_slice(b"$$$794661040$$$");
    plain[24..28].copy_from_slice(&(id ^ 0x88888888).to_le_bytes());
    plain[28..32].copy_from_slice(&0x4a32ba39u32.to_le_bytes());
    for i in 0..5 {
        plain[32 + i * 4..36 + i * 4].copy_from_slice(&(10 + i as u32).to_le_bytes());
    }
    plain[52] = 1;
    plain[53..57].copy_from_slice(&0x12345678u32.to_le_bytes());
    plain[57..61].copy_from_slice(b"LLGB");
    plain[61..65].copy_from_slice(&1u32.to_le_bytes());
    plain[65..69].copy_from_slice(&[8, 4, 12, 1]);
    plain[69] = 11;
    plain[70] = 7;
    plain[508..].copy_from_slice(b"LLGB");
    let mut wire = plain;
    wire[24..].copy_from_slice(&xor_rolling(&plain[24..], (id & 0xffff) ^ (id >> 16)));
    if encoding == Lba4Encoding::PostXor {
        wire[69] = plain[69];
        wire[70] = plain[70];
    }
    if short {
        wire[71..508].fill(0);
    }
    wire
}
#[test]
pub fn lba4_separates_wire_reader_and_producer_for_both_encodings() {
    for encoding in [Lba4Encoding::PostXor, Lba4Encoding::OrdinaryRolling] {
        for short in [false, true] {
            let raw = sample(encoding, short);
            let view = parse_lba4(
                &raw,
                Lba4Context {
                    encoding,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(view.onlyid, 794661040);
            assert_eq!(view.node.second_key, 0x4a32ba39);
            assert_eq!(view.node.hserial, [10, 11, 12, 13, 14]);
            assert_eq!(view.node.single_usb, 1);
            assert_eq!(view.node.host_hardinfo, 0x12345678);
            assert_eq!(view.node.new_lab_flag, *b"LLGB");
            assert_eq!(view.node.version, 1);
            assert_eq!(view.node.sector_tuple, [8, 4, 12, 1]);
            assert_eq!(view.node.onlyid_guard, view.onlyid ^ 0x88888888);
            assert_eq!(
                view.producer_flags(),
                Some(ServerFlags {
                    data_to_server: 11,
                    connect_server: 7
                })
            );
            if encoding == Lba4Encoding::PostXor {
                assert_ne!(view.reader_flags(), view.producer_flags().unwrap());
            } else {
                assert_eq!(view.reader_flags(), view.producer_flags().unwrap());
            }
            assert_eq!(
                view.wire_flags(),
                ServerFlags {
                    data_to_server: raw[69],
                    connect_server: raw[70]
                }
            );
            assert_eq!(view.backing_is_raw_zero(), short);
            assert_eq!(
                view.representation_backing.bytes(),
                if short { &[0; 437] } else { &[0xa5; 437] }
            );
            assert_eq!(view.trailing_magic, *b"LLGB");
            assert_eq!(view.reconstruct(), raw);
            assert_eq!(view.encode().unwrap(), raw);
            let unknown = parse_lba4(&raw, Lba4Context::default()).unwrap();
            assert_eq!(unknown.context.encoding, Lba4Encoding::Unknown);
            assert_eq!(unknown.producer_flags(), None);
            assert!(unknown.encode().is_err());
        }
    }
}
#[test]
pub fn lba4_overlays_are_explicit_and_do_not_select_encoding() {
    let raw = sample(Lba4Encoding::PostXor, false);
    for second_key_source in [
        Lba4SecondKeySource::CurrentMainOnlyid,
        Lba4SecondKeySource::LegacyGuidCrc,
    ] {
        for hserial_source in [
            Lba4HserialSource::CurrentZero,
            Lba4HserialSource::LegacyCallerVector,
        ] {
            for host_hardinfo_source in [
                HostHardinfoSource::CurrentZero,
                HostHardinfoSource::LegacyHostIdentity,
            ] {
                let context = Lba4Context {
                    second_key_source,
                    hserial_source,
                    host_hardinfo_source,
                    ..Default::default()
                };
                let mut adjusted = raw;
                let id = 794661040u32;
                let key = (id & 0xffff) ^ (id >> 16);
                let mut node = xor_rolling(&raw[24..], key);
                if second_key_source == Lba4SecondKeySource::CurrentMainOnlyid {
                    node[4..8].copy_from_slice(&id.to_le_bytes());
                }
                if hserial_source == Lba4HserialSource::CurrentZero {
                    node[8..28].fill(0);
                }
                if host_hardinfo_source == HostHardinfoSource::CurrentZero {
                    node[29..33].fill(0);
                }
                adjusted[24..].copy_from_slice(&xor_rolling(&node, key));
                let view = parse_lba4(&adjusted, context).unwrap();
                assert_eq!(view.context, context);
                assert_eq!(view.producer_flags(), None);
                assert_eq!(
                    view.node.hserial,
                    if hserial_source == Lba4HserialSource::CurrentZero {
                        [0; 5]
                    } else {
                        [10, 11, 12, 13, 14]
                    }
                );
            }
        }
    }
}
#[test]
pub fn lba4_authentic_capture_keeps_reader_flags_and_nonzero_backing() {
    let image = include_bytes!(
        "../../audit/protocol/gold/authentic-nopwd/sandisk_ultra_20260823_lba0_12.bin"
    );
    let raw: &[u8; 512] = image[2048..2560].try_into().unwrap();
    let view = parse_lba4(raw, Lba4Context::default()).unwrap();
    assert_eq!(
        view.reader_flags(),
        ServerFlags {
            data_to_server: 0xd4,
            connect_server: 0xd9
        }
    );
    assert_eq!(
        view.wire_flags(),
        ServerFlags {
            data_to_server: 0,
            connect_server: 0
        }
    );
    assert_eq!(view.producer_flags(), None);
    assert_eq!(view.reconstruct(), *raw);
    assert_eq!(view.node.second_key, 0x4a32ba39);
    assert!(!view.backing_is_raw_zero());
    let mut corrupt = *raw;
    corrupt[24] ^= 1;
    assert!(parse_lba4(&corrupt, Lba4Context::default()).is_err());
    let mut corrupt = *raw;
    corrupt[..24].fill(0);
    assert!(parse_lba4(&corrupt, Lba4Context::default()).is_err());
}
