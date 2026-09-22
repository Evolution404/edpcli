use edpcli::{
    crypto::{a6b0_full, a7f0_full, crc32_bare, lba6_decode},
    protocol::{lba6::*, lba9::*, profile::*},
};
fn hex(text: &str) -> [u8; 512] {
    let h: String = text.split_whitespace().collect();
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
#[test]
pub fn dept_and_safe6_fields_replay_physical_profiles_without_losing_backing() {
    let zero9 = parse_lba9(&[0; 512], 0, DeptLayout::Short, false).unwrap();
    assert!(matches!(zero9.eetu, EetuState::Absent));
    assert!(matches!(zero9.upper, UpperPayload::Zero));
    assert_eq!(zero9.reencode(), [0; 512]);
    let mut counts = [0; 3];
    let mut snapshots = 0;
    for row in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = row.split('\t').collect();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(c[4])).unwrap();
        let raw6: &[u8; 512] = image[3072..3584].try_into().unwrap();
        let raw9: &[u8; 512] = image[4608..5120].try_into().unwrap();
        let v = parse_lba6(raw6).unwrap();
        let d = lba6_decode(raw6);
        assert_eq!(v.reencode(), *raw6);
        assert_eq!(v.reconstruct(), *raw6);
        assert_eq!(v.user.bytes(), &d[0x50..0x70]);
        assert_eq!(v.autonum.bytes(), &d[0x70..0x80]);
        assert_eq!(v.office.bytes(), &d[0x80..0xc0]);
        assert_eq!(v.label.bytes(), &d[0x188..0x1c0]);
        assert_eq!(v.gserial.bytes(), &d[0x1c0..0x1d0]);
        assert_eq!(v.beizhu.bytes(), &d[0x1d0..0x1e0]);
        assert_eq!(v.template_040_04f, &d[0x40..0x50]);
        assert_eq!(v.template_0c0_0ff, &d[0xc0..0x100]);
        assert_eq!(v.template_108_187, &d[0x108..0x188]);
        assert_eq!(v.compatibility_tail, &d[0x1ee..0x1f0]);
        assert_eq!(v.zero_tail, &d[0x1f4..0x1fc]);
        assert_eq!(v.device_crc_guard, v.device_crc.wrapping_mul(2));
        assert_eq!(
            v.encrypt_generation_flag,
            u32::from_le_bytes(d[0x1f0..0x1f4].try_into().unwrap())
        );
        let crc = if c[0] == "authentic-nopwd" {
            crc32_bare(b"disk&ven_sandisk&prod_ultra&rev_1.00")
        } else {
            crc32_bare(
                crate::gold_name::parse_gold_name(c[3])
                    .unwrap()
                    .device_id
                    .as_bytes(),
            )
        };
        assert_eq!(v.device_crc, crc);
        let nine = parse_lba9(raw9, crc, v.dept_profile(), v.has_long_user()).unwrap();
        assert_eq!(nine.reconstruct(), *raw9);
        assert_eq!(nine.reencode(), *raw9);
        let dept = reconstruct_dept(&v, &nine).unwrap();
        match v.dept_profile() {
            DeptLayout::Short => {
                counts[0] += 1;
                assert_eq!(dept, v.dept_inline_value());
            }
            DeptLayout::Join59 => {
                counts[1] += 1;
                assert_eq!(dept.len(), 76);
            }
            DeptLayout::Join60 => {
                counts[2] += 1;
                assert_eq!(dept.len(), 76);
            }
            _ => panic!(),
        }
        if let MbrUnderlay::LegacySnapshot(s) = v.mbr_underlay {
            snapshots += 1;
            assert!(s.start_lba > 0);
            let twelve = a6b0_full(&image[6144..6656], &crc.to_le_bytes(), 0);
            let entry = twelve
                .chunks_exact(96)
                .take(3)
                .find(|e| u32::from_le_bytes(e[12..16].try_into().unwrap()) == 4)
                .unwrap();
            assert_eq!(
                s.start_lba as u64,
                u64::from_le_bytes(entry[24..32].try_into().unwrap())
            );
            assert_eq!(
                s.sector_count as u64,
                u64::from_le_bytes(entry[40..48].try_into().unwrap()) / 512
            );
        }
        let mut broken = *raw6;
        broken[123] ^= 1;
        assert!(parse_lba6(&broken).is_err());
    }
    assert_eq!(counts, [13, 3, 4]);
    assert_eq!(snapshots, 1);
}
#[test]
pub fn dept_join_seams_and_post_nul_backing_are_independent() {
    let raw6 = hex(include_str!(
        "../fixtures/protocol_evidence/lexar_join59_lba6.hex"
    ));
    let mut raw9 = hex(include_str!(
        "../fixtures/protocol_evidence/lexar_join59_lba9.hex"
    ));
    let six = parse_lba6(&raw6).unwrap();
    let expected = reconstruct_dept(
        &six,
        &parse_lba9(&raw9, six.device_crc, six.dept_profile(), false).unwrap(),
    )
    .unwrap();
    let nul = raw9[128..256].iter().position(|b| *b == 0).unwrap() + 128;
    raw9[nul + 1..256].fill(0xa5);
    let nine = parse_lba9(&raw9, six.device_crc, six.dept_profile(), false).unwrap();
    assert_eq!(reconstruct_dept(&six, &nine).unwrap(), expected);
    assert_eq!(nine.reencode(), raw9);
    let mut plain = *six.decoded();
    plain[0x3f] = expected[59]; // join60 consumes 60 inline bytes.
    raw9[128..256].fill(0xa5);
    raw9[128..128 + expected.len() - 60].copy_from_slice(&expected[60..]);
    raw9[128 + expected.len() - 60] = 0;
    let sixty = parse_lba6(&encode_safe6(&plain)).unwrap();
    assert_eq!(sixty.dept_profile(), DeptLayout::Join60);
    let nine = parse_lba9(&raw9, sixty.device_crc, sixty.dept_profile(), false).unwrap();
    assert_eq!(reconstruct_dept(&sixty, &nine).unwrap(), expected);
    plain[..64].fill(0xa5);
    plain[..4].copy_from_slice(b"ABC\0");
    let short = parse_lba6(&encode_safe6(&plain)).unwrap();
    assert_eq!(short.dept_profile(), DeptLayout::Short);
    let nine = parse_lba9(&raw9, short.device_crc, short.dept_profile(), false).unwrap();
    assert_eq!(reconstruct_dept(&short, &nine).unwrap(), b"ABC");
}
#[test]
pub fn lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership() {
    let crc = 0x12345678u32;
    let key = crc.to_le_bytes();
    let mut raw = [0u8; 512];
    let mut eetu = [0xa5; 128];
    eetu[..4].copy_from_slice(b"EETU");
    eetu[4..12].copy_from_slice(&100u64.to_le_bytes());
    eetu[12..20].copy_from_slice(&200u64.to_le_bytes());
    eetu[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    eetu[126..].fill(0);
    raw[..128].copy_from_slice(&a7f0_full(&eetu, &key, 0));
    let mut sapf = [0u8; 32];
    sapf[..4].copy_from_slice(b"SAPF");
    sapf[8] = 7;
    sapf[12..16].copy_from_slice(&63u32.to_le_bytes());
    sapf[16..20].copy_from_slice(&99u32.to_le_bytes());
    raw[256..288].copy_from_slice(&sapf.map(|b| b ^ 0x88));
    raw[288..].fill(0xa5);
    let nine = parse_lba9(&raw, crc, DeptLayout::Short, false).unwrap();
    let EetuState::Present(e) = &nine.eetu else {
        panic!()
    };
    assert_eq!(
        (e.begin_time, e.end_time, e.use_count),
        (100, 200, u32::MAX)
    );
    assert_eq!(e.reverse.bytes(), &[0xa5; 102]);
    let UpperPayload::Sapf(s) = &nine.upper else {
        panic!()
    };
    assert_eq!((s.partition.start_lba, s.partition.sector_count), (63, 99));
    assert_eq!(s.tail.bytes(), &[0xa5; 224]);
    assert_eq!(nine.reencode(), raw);
    raw[256..384].fill(0x77);
    let mut eppe = [0u8; 128];
    eppe[..4].copy_from_slice(b"EPPE");
    eppe[4..8].copy_from_slice(&12u32.to_le_bytes());
    raw[384..].copy_from_slice(&a7f0_full(&eppe, &key, 0));
    let nine = parse_lba9(&raw, crc, DeptLayout::Short, false).unwrap();
    let UpperPayload::Eppe(e) = &nine.upper else {
        panic!()
    };
    assert_eq!(e.min_length, 12);
    assert_eq!(e.prefix.bytes(), &[0x77; 128]);
    assert_eq!(nine.reencode(), raw);
    let raw6 = hex(include_str!(
        "../fixtures/protocol_evidence/official_virtual_long_user_lba6.hex"
    ));
    let raw9 = hex(include_str!(
        "../fixtures/protocol_evidence/official_virtual_long_user_lba9.hex"
    ));
    let six = parse_lba6(&raw6).unwrap();
    assert!(six.has_long_user());
    let nine = parse_lba9(&raw9, six.device_crc, six.dept_profile(), true).unwrap();
    assert!(matches!(nine.upper, UpperPayload::LongUser { .. }));
    let user = reconstruct_user(&six, &nine).unwrap();
    assert!(user.len() > 28);
    assert_eq!(nine.reencode(), raw9);
}
