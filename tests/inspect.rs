mod common;

use common::*;
use edpcli::crypto::{a7f0_full, crc32_bare, xor_rolling};
use edpcli::inspect::{analyze_sector, render_fields, render_hex, FieldStyle, InspectMeta};

fn crc32_ieee_test(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[test]
fn inspect_protocol_semantics_stay_routed_to_canonical_parsers() {
    let source = include_str!("../src/inspect.rs");
    for parser in [
        "lba0::parse_lba0",
        "lba1::parse_lba1",
        "lba2::parse_lba2",
        "lba3::parse_lba3",
        "lba4::parse_lba4",
        "lba5::parse_lba5",
        "lba6::parse_lba6",
        "lba7::parse_lba7",
        "lba8::parse_lba8",
        "lba9::parse_lba9",
        "lba10::parse_lba10",
        "lba11::parse_lba11",
        "lba12::parse_lba12",
    ] {
        assert!(
            source.contains(parser),
            "Inspect canonical parser link missing: {parser}"
        );
    }
    for forbidden in [
        "fn parse_lba6(",
        "fn parse_edpf(",
        "fn parse_llgb(",
        "fn parse_sapf(",
        "fn parse_eppe(",
        "fn decode_lba11(",
    ] {
        assert!(
            !source.contains(forbidden),
            "Inspect reintroduced a parallel protocol parser: {forbidden}"
        );
    }
}

fn meta_for(key: &str) -> InspectMeta {
    let (device_id, vid, pid, sectors, onlyid) = match key {
        "netac" => (
            "disk&ven_netac&prod_onlydisk",
            "0dd8",
            "2005",
            122_880_000u64,
            "1402259934",
        ),
        "lexar" => (
            "disk&ven_lexar&prod_usb_flash_drive",
            "21c4",
            "0cd1",
            243_625_984u64,
            "3164177653",
        ),
        "aigo" => (
            "disk&ven_aigo&prod_u335&rev_pmap",
            "3535",
            "6300",
            245_760_000u64,
            "1987718388",
        ),
        other => panic!("unknown fixture metadata key: {other}"),
    };
    InspectMeta {
        device_id: Some(device_id.into()),
        vid: Some(vid.into()),
        pid: Some(pid.into()),
        size_bytes: sectors.checked_mul(512),
        onlyid: Some(onlyid.into()),
    }
}

#[test]
fn lba7_and_lba12_decode_to_edpf_with_semantic_fields() {
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");

    let v7 = analyze_sector(7, &data[7 * 512..8 * 512], &meta);
    assert_eq!(&v7.decoded[..4], b"EDPF");
    assert!(v7.method.contains("XOR"));
    assert!(v7
        .fields
        .iter()
        .any(|f| f.label == "类型" && f.value.contains("Share")));
    assert!(v7
        .fields
        .iter()
        .any(|f| f.label == "起始 LBA" && f.style == FieldStyle::Address));

    let v12 = analyze_sector(12, &data[12 * 512..13 * 512], &meta);
    assert_eq!(&v12.decoded[..4], b"EDPF");
    assert!(v12.decoded[0x170..].iter().all(|byte| *byte == 0));
    assert!(v12.method.contains("整扇 512B"));
    assert!(!v12.method.contains("RAW"));
    assert!(v12
        .fields
        .iter()
        .any(|f| f.label == "大小" && f.style == FieldStyle::Size));
}

#[test]
fn lba6_reports_safe6_checksum_and_identity_fields() {
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    let v = analyze_sector(6, &data[6 * 512..7 * 512], &meta);
    assert_eq!(v.decoded.len(), 512);
    assert!(v.method.contains("SAFE6"));
    assert!(v.fields.iter().any(|f| f.label == "device_id CRC32"));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "校验和" && f.style == FieldStyle::Checksum));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "m_usbGSerial 槽" && f.start == 0x1c0 && f.end == 0x1d0));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "BeiZhu 槽" && f.start == 0x1d0 && f.end == 0x1e0));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "legacy MBR snapshot" && f.start == 0x1e0 && f.end == 0x1ee));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "m_encrypt" && f.start == 0x1f0 && f.end == 0x1f4));
    assert!(!v.fields.iter().any(|f| f.label == "模板值"));
    assert!(!v.fields.iter().any(|f| f.label == "注册标志"));
}

#[test]
fn lba11_can_decrypt_from_explicit_device_metadata() {
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");
    let v = analyze_sector(11, &data[11 * 512..12 * 512], &meta);
    assert!(v.method.contains("PDKB"), "{}", v.method);
    assert_eq!(&v.decoded[0x100..0x104], b"PDKB");
    assert!(v.fields.iter().any(|f| f.label == "PDKB device_id"));
}

#[test]
fn lba5_is_reported_as_an_opaque_write_protection_probe_sector() {
    let raw = [0u8; 512];
    let view = analyze_sector(5, &raw, &InspectMeta::default());
    assert!(view.method.contains("写保护探测"));
    assert!(view.fields.iter().any(|field| {
        field.start == 0
            && field.end == 0x200
            && field.label == "写保护探测 scratch 区"
            && field.value.contains("内容本身不解析")
    }));
    assert!(view.notes.iter().any(|note| {
        note.contains("ERROR_WRITE_PROTECT")
            && note.contains("原样保留")
            && note.contains("全零")
            && note.contains("不是协议要求")
    }));
}

#[test]
fn lba1_uses_the_canonical_gpt_header_parser() {
    let mut raw = [0u8; 512];
    raw[..8].copy_from_slice(b"EFI PART");
    raw[0x08..0x0c].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    raw[0x0c..0x10].copy_from_slice(&92u32.to_le_bytes());
    raw[0x18..0x20].copy_from_slice(&1u64.to_le_bytes());
    raw[0x20..0x28].copy_from_slice(&999u64.to_le_bytes());
    raw[0x28..0x30].copy_from_slice(&34u64.to_le_bytes());
    raw[0x30..0x38].copy_from_slice(&900u64.to_le_bytes());
    raw[0x48..0x50].copy_from_slice(&2u64.to_le_bytes());
    raw[0x50..0x54].copy_from_slice(&128u32.to_le_bytes());
    raw[0x54..0x58].copy_from_slice(&128u32.to_le_bytes());
    raw[0x58..0x5c].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    let crc = crc32_ieee_test(&raw[..92]);
    raw[0x10..0x14].copy_from_slice(&crc.to_le_bytes());

    let view = analyze_sector(1, &raw, &InspectMeta::default());
    assert!(
        view.method.contains("canonical protocol::lba1"),
        "{}",
        view.method
    );
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "GPT signature" && field.value == "EFI PART"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "分区表首 LBA" && field.value == "2"));
    assert!(view
        .notes
        .iter()
        .any(|note| note.contains("protocol::lba1::parse_lba1")));
}

#[test]
fn lba2_uses_canonical_absent_profile_without_inventing_entries() {
    let raw = [0u8; 512];
    let view = analyze_sector(2, &raw, &InspectMeta::default());
    assert!(
        view.method.contains("canonical protocol::lba2"),
        "{}",
        view.method
    );
    assert!(view
        .fields
        .iter()
        .any(|field| { field.label == "GPT partition profile" && field.value.contains("Absent") }));
    assert!(view
        .notes
        .iter()
        .any(|note| note.contains("protocol::lba2::parse_lba2")));
}

#[test]
fn lba4_zero_ciphertext_byte_is_decrypted_unless_whole_short_gap_is_unwritten() {
    let onlyid = 949_028_302u32;
    let k0 = (onlyid & 0xffff) ^ (onlyid >> 16);
    let mut plain = vec![0u8; 512];
    let header = b"$$$949028302$$$";
    plain[..header.len()].copy_from_slice(header);
    plain[0x18..0x1c].copy_from_slice(&(onlyid ^ 0x8888_8888).to_le_bytes());
    plain[0x39..0x3d].copy_from_slice(b"LLGB");
    plain[0x1fc..0x200].copy_from_slice(b"LLGB");

    // Force a valid ciphertext word to 0x0000. The old decoder incorrectly
    // treated each zero ciphertext byte as an unwritten byte.
    let keystream = xor_rolling(&vec![0u8; 512 - 0x18], k0);
    plain[0x20..0x22].copy_from_slice(&keystream[0x08..0x0a]);

    let mut raw = plain.clone();
    raw[0x18..].copy_from_slice(&xor_rolling(&plain[0x18..], k0));
    raw[0x47..0x1fc].fill(0);
    assert_eq!(&raw[0x20..0x22], &[0, 0]);

    let view = analyze_sector(4, &raw, &InspectMeta::default());
    assert_eq!(&view.decoded[0x20..0x22], &plain[0x20..0x22]);
    assert_eq!(&view.decoded[0x39..0x3d], b"LLGB");
    assert_eq!(&view.decoded[0x1fc..0x200], b"LLGB");
    assert!(view.decoded[0x47..0x1fc].iter().all(|byte| *byte == 0));
}

#[test]
fn lba4_post_xor_wire_flags_do_not_override_official_reader_view() {
    let onlyid = 1_402_259_934u32;
    let k0 = (onlyid & 0xffff) ^ (onlyid >> 16);
    let mut plain = vec![0u8; 512];
    let header = b"$$$1402259934$$$";
    plain[..header.len()].copy_from_slice(header);
    plain[0x18..0x1c].copy_from_slice(&(onlyid ^ 0x8888_8888).to_le_bytes());
    plain[0x1c..0x20].copy_from_slice(&onlyid.to_le_bytes());
    plain[0x39..0x3d].copy_from_slice(b"LLGB");
    plain[0x45] = 0xaf;
    plain[0x46] = 0x36;
    plain[0x1fc..0x200].copy_from_slice(b"LLGB");

    let mut raw = plain.clone();
    raw[0x18..].copy_from_slice(&xor_rolling(&plain[0x18..], k0));
    // Both official Windows and Linux BuildSector4 implementations perform
    // these two byte stores *after* the rolling-XOR loop.
    raw[0x45] = plain[0x45];
    raw[0x46] = plain[0x46];

    let generic = xor_rolling(&raw[0x18..], k0);
    assert_ne!(&generic[0x2d..0x2f], &[0xaf, 0x36]);

    let view = analyze_sector(4, &raw, &InspectMeta::default());
    assert_eq!(&view.decoded[0x45..0x47], &generic[0x2d..0x2f]);
    assert_eq!(&view.decoded[0x39..0x3d], b"LLGB");
    assert_eq!(&view.decoded[0x1fc..0x200], b"LLGB");
    assert!(view.fields.iter().any(|field| {
        field.label == "bDataToServer"
            && field.value.contains("wire=0xAF")
            && field.value.contains("producer=需按 writer 判定")
    }));
}

#[test]
fn lba4_current_identity_shape_does_not_reclassify_post_xor_flags() {
    let onlyid = 1_402_259_934u32;
    let k0 = (onlyid & 0xffff) ^ (onlyid >> 16);
    let mut plain = vec![0u8; 512];
    let header = b"$$$1402259934$$$";
    plain[..header.len()].copy_from_slice(header);
    plain[0x18..0x1c].copy_from_slice(&(onlyid ^ 0x8888_8888).to_le_bytes());
    plain[0x1c..0x20].copy_from_slice(&onlyid.to_le_bytes());
    // Current SAFE6 producer leaves HSerialCRC[5] zero.
    plain[0x35..0x39].copy_from_slice(&[0x08, 0x04, 0x0c, 0x01]);
    plain[0x39..0x3d].copy_from_slice(b"LLGB");
    plain[0x3d..0x41].copy_from_slice(&1u32.to_le_bytes());
    plain[0x41..0x45].copy_from_slice(&[0x08, 0x04, 0x0c, 0x01]);
    plain[0x45..0x47].copy_from_slice(&[0, 0]);
    plain[0x1fc..0x200].copy_from_slice(b"LLGB");

    let mut raw = plain.clone();
    raw[0x18..].copy_from_slice(&xor_rolling(&plain[0x18..], k0));
    // Both official Windows and Linux current builders overwrite these two
    // bytes *after* the rolling-XOR loop.
    raw[0x45..0x47].copy_from_slice(&plain[0x45..0x47]);
    let generic = xor_rolling(&raw[0x18..], k0);
    assert_ne!(&generic[0x2d..0x2f], &[0, 0]);

    let view = analyze_sector(4, &raw, &InspectMeta::default());
    assert_eq!(&view.decoded[0x45..0x47], &generic[0x2d..0x2f]);
    assert_eq!(&view.decoded[0x39..0x3d], b"LLGB");
    assert_eq!(&view.decoded[0x1fc..0x200], b"LLGB");
    assert!(raw[0x47..0x1fc].iter().any(|byte| *byte != 0));
}

#[test]
fn lba4_legacy_restore_profile_keeps_rolling_decoded_server_flags() {
    let data = load_disk_image("netac").expect("netac real-device fixture");
    let meta = meta_for("netac");
    let raw = &data[4 * 512..5 * 512];

    // This committed original fixture has a legacy-identity restore node:
    // OnllyID2Nd does not mirror the main onlyid and HSerialCRC[5] is non-zero.
    // Its physical +0x45/+0x46 bytes produce a 00 00 historical ReadSector4
    // view. The exact earlier writer remains unknown, so the fixture proves a
    // rolling-representation observation, not an identity-based classifier.
    assert_eq!(&raw[0x45..0x47], &[0xaf, 0x36]);

    let view = analyze_sector(4, raw, &meta);
    assert_ne!(
        u32::from_le_bytes(view.decoded[0x1c..0x20].try_into().unwrap()),
        1_402_259_934u32,
        "fixture unexpectedly stopped exercising the legacy-identity restore node"
    );
    assert!(view.decoded[0x20..0x34].iter().any(|byte| *byte != 0));
    assert_eq!(
        &view.decoded[0x45..0x47],
        &[0, 0],
        "inspect must remain equal to the official rolling-reader view"
    );
}

#[test]
fn lba10_decodes_only_the_eesi_head_and_preserves_tail_bytes() {
    let device_id = "disk&ven_test&prod_eesi";
    let crc = crc32_bare(device_id.as_bytes());
    let mut plain = [0u8; 0x80];
    plain[..4].copy_from_slice(b"EESI");
    plain[4..8].copy_from_slice(&1u32.to_le_bytes());
    plain[8..14].copy_from_slice(&[0xbd, 0xbb, 0xbb, 0xbb, 0xc7, 0xf8]);
    plain[0x18..0x1e].copy_from_slice(&[0xb1, 0xa3, 0xc3, 0xdc, 0xc7, 0xf8]);

    let mut raw = vec![0u8; 512];
    raw[..0x80].copy_from_slice(&a7f0_full(&plain, &crc.to_le_bytes(), 0));
    raw[0x80] = 0x5a;
    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };

    let view = analyze_sector(10, &raw, &meta);
    assert_eq!(&view.decoded[..4], b"EESI");
    assert_eq!(view.decoded[0x04..0x08], 1u32.to_le_bytes());
    assert_eq!(view.decoded[0x80], 0x5a);
    assert!(view.method.contains("前 0x80B"));
    assert!(view.fields.iter().any(|field| field.label == "EESI magic"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "EESI 交换区卷标" && field.value == "交换区"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "EESI 保密区卷标" && field.value == "保密区"));
    assert!(view
        .notes
        .iter()
        .any(|note| note.contains("SetVolumeLabelA")));
}

#[test]
fn lba9_decodes_eetu_and_sapf_without_inventing_overlapping_eppe() {
    let device_id = "disk&ven_test&prod_lba9";
    let crc = crc32_bare(device_id.as_bytes());
    let key = crc.to_le_bytes();
    let mut raw = vec![0u8; 512];

    let mut eetu = [0u8; 0x80];
    eetu[..4].copy_from_slice(b"EETU");
    eetu[0x14..0x18].copy_from_slice(&u32::MAX.to_le_bytes());
    raw[..0x80].copy_from_slice(&a7f0_full(&eetu, &key, 0));

    let mut sapf = [0u8; 0x20];
    sapf[..4].copy_from_slice(b"SAPF");
    // One 16-byte MBR partition entry immediately follows the magic.
    sapf[0x04..0x14].copy_from_slice(&[
        0x00, 0x01, 0x01, 0x00, 0x0b, 0x46, 0x05, 0x01, 0x3f, 0x00, 0x00, 0x00, 0xc1, 0x4f, 0x00,
        0x00,
    ]);
    for (dst, src) in raw[0x100..0x120].iter_mut().zip(sapf) {
        *dst = src ^ 0x88;
    }

    let mut eppe = [0u8; 0x80];
    eppe[..4].copy_from_slice(b"EPPE");
    eppe[4..8].copy_from_slice(&8u32.to_le_bytes());
    raw[0x180..0x200].copy_from_slice(&a7f0_full(&eppe, &key, 0));

    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(9, &raw, &meta);

    assert_eq!(&view.decoded[..4], b"EETU");
    assert_eq!(&view.decoded[0x100..0x104], b"SAPF");
    assert_ne!(
        &view.decoded[0x180..0x184],
        b"EPPE",
        "SAPF and EPPE are alternative LBA9 overlay profiles"
    );
    assert!(view.fields.iter().any(|field| field.label == "EETU magic"));
    assert!(view.fields.iter().any(|field| {
        field.label == "EETU 开始时间 (ullBTime)" && field.value == "0（不限制）"
    }));
    assert!(view.fields.iter().any(|field| {
        field.label == "EETU 结束时间 (ullETime)" && field.value == "0（不限制）"
    }));
    assert!(view.fields.iter().any(|field| {
        field.label == "EETU 使用次数 (useCount)" && field.value == "无限（0xFFFFFFFF）"
    }));
    assert!(view.notes.iter().any(|note| {
        note.contains("time(NULL)")
            && note.contains("0xFFFFFFFF")
            && note.contains("reverse[104]")
            && note.contains("COMPLETE")
    }));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "partition type" && field.value == "0x0B"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "起始 LBA" && field.value == "63"));
}

#[test]
fn lba9_decodes_eppe_as_its_own_canonical_overlay_profile() {
    let device_id = "disk&ven_test&prod_lba9_eppe";
    let crc = crc32_bare(device_id.as_bytes());
    let key = crc.to_le_bytes();
    let mut raw = vec![0u8; 512];

    let mut eppe = [0u8; 0x80];
    eppe[..4].copy_from_slice(b"EPPE");
    eppe[4..8].copy_from_slice(&8u32.to_le_bytes());
    raw[0x180..0x200].copy_from_slice(&a7f0_full(&eppe, &key, 0));

    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(9, &raw, &meta);

    assert_eq!(&view.decoded[0x180..0x184], b"EPPE");
    assert!(!view
        .fields
        .iter()
        .any(|field| field.label == "partition type"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "最小密码长度" && field.value == "8"));
}

#[test]
fn hex_renderer_has_offsets_and_field_legend_without_color() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");
    let v = analyze_sector(7, &data[7 * 512..8 * 512], &meta);
    let out = render_hex(&v, false);
    assert!(out.contains("+0x000:"));
    assert!(out.contains("+0x1F0:"));
    assert!(out.contains("字段图例"));
    assert!(!out.contains("\x1b["));
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn lba8_llgb_fields_render_as_vertical_key_value_rows() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    let view = analyze_sector(8, &data[8 * 512..9 * 512], &meta);
    let out = render_fields(&view);

    assert!(out.contains("[ELABEL]"), "{out}");
    assert!(out.contains("GLab"), "{out}");
    assert!(out.contains("Dept"), "{out}");
    assert!(out.contains("输电运检中心"), "{out}");
    assert!(out.contains("User"), "{out}");
    assert!(out.contains("张玉玺"), "{out}");
    assert!(out.contains("Label"), "{out}");
    assert!(out.contains("江苏电力!SAFE6"), "{out}");
    assert!(out.contains("空字段"), "空值字段应压缩成摘要: {out}");
    assert!(out.contains("Indus") && out.contains("VOLC2"), "{out}");
    assert!(
        !out.lines()
            .any(|line| line.contains("GLab=") && line.contains("Dept=")),
        "LLGB 子字段不应再拼成一行: {out}"
    );
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn lba8_splits_elabel_bytes_before_gbk_decoding_each_value() {
    let device_id = "disk&ven_test&prod_llgb_boundary";
    let crc = crc32_bare(device_id.as_bytes());
    let mut plain = vec![0u8; 368];
    plain[..4].copy_from_slice(b"LLGB");
    plain[8..12].copy_from_slice(&0x0100_0001u32.to_le_bytes());
    plain[12..16].copy_from_slice(&0x222u32.to_le_bytes());
    plain[0x3e..0x40].copy_from_slice(&0x80u16.to_le_bytes());

    // Real samples can end a truncated Dept value with a lone GBK lead byte
    // immediately before the ASCII "||User=" delimiter. Decoding the whole
    // ELABEL string first would consume the first '|' as that byte's trail
    // byte and destroy the User key.
    let mut elabel = b"<ELABEL>GLab=322CA28A-D7D1448B-DCE2CED9||Dept=".to_vec();
    elabel.extend_from_slice(&[0xbd]);
    elabel.extend_from_slice(b"||User=");
    elabel.extend_from_slice(&[0xd5, 0xc5, 0xd3, 0xf1, 0xe7, 0xf4]); // 张玉玺 (GBK)
    elabel.extend_from_slice(b"||Label=");
    elabel.extend_from_slice(&[0xbd, 0xad, 0xcb, 0xd5, 0xb5, 0xe7, 0xc1, 0xa6]); // 江苏电力
    elabel.extend_from_slice(b"!SAFE6||");
    let logical_len = 0x80 + elabel.len();
    plain[4..8].copy_from_slice(&(logical_len as u32).to_le_bytes());
    plain[0x80..0x80 + elabel.len()].copy_from_slice(&elabel);

    let mut raw = vec![0u8; 512];
    raw[..368].copy_from_slice(&a7f0_full(&plain, &crc.to_le_bytes(), 0));
    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(8, &raw, &meta);
    let out = render_fields(&view);

    assert!(out.contains("User"), "{out}");
    assert!(out.contains("张玉玺"), "{out}");
    assert!(out.contains("Label"), "{out}");
    assert!(out.contains("江苏电力!SAFE6"), "{out}");
}

#[test]
fn lba8_decrypts_the_llgb_length_instead_of_a_fixed_0x170_prefix() {
    let device_id = "disk&ven_test&prod_llgb_long";
    let crc = crc32_bare(device_id.as_bytes());
    let mut plain = vec![0u8; 0x190];
    plain[..4].copy_from_slice(b"LLGB");
    plain[8..12].copy_from_slice(&0x0100_0001u32.to_le_bytes());
    plain[12..16].copy_from_slice(&0x222u32.to_le_bytes());
    plain[0x3e..0x40].copy_from_slice(&0x80u16.to_le_bytes());

    let mut elabel = b"<ELABEL>GLab=322CA28A-D7D1448B-DCE2CED9||".to_vec();
    for key in [
        "Indus", "Orgcd", "Org", "Unit", "Dept", "User", "Alarm", "Autonum", "Label", "Rmark",
        "VOL0", "VOL1", "VOL2", "VOLC0", "VOLC1",
    ] {
        elabel.extend_from_slice(key.as_bytes());
        elabel.extend_from_slice(b"=12345||");
    }
    elabel.extend_from_slice(b"VOLC2=TAIL||");
    let logical_len = 0x80 + elabel.len();
    assert!(logical_len > 0x170 && logical_len <= 0x190);
    plain[4..8].copy_from_slice(&(logical_len as u32).to_le_bytes());
    plain[0x80..0x80 + elabel.len()].copy_from_slice(&elabel);

    let encrypted_len = (logical_len / 16 + 1) * 16;
    let mut raw = vec![0u8; 512];
    raw[..encrypted_len].copy_from_slice(&a7f0_full(
        &plain[..encrypted_len],
        &crc.to_le_bytes(),
        0,
    ));
    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(8, &raw, &meta);
    let out = render_fields(&view);

    assert!(out.contains("VOLC2"), "{out}");
    assert!(out.contains("TAIL"), "{out}");
    assert!(
        view.method.contains(&format!("前 {encrypted_len}B")),
        "{}",
        view.method
    );
}

#[test]
fn lba8_preserves_nonzero_bytes_after_the_dynamic_encrypted_prefix() {
    let device_id = "disk&ven_test&prod_llgb_preserved_tail";
    let crc = crc32_bare(device_id.as_bytes());
    let mut plain = vec![0u8; 0x160];
    plain[..4].copy_from_slice(b"LLGB");
    plain[8..12].copy_from_slice(&0x0100_0001u32.to_le_bytes());
    plain[12..16].copy_from_slice(&0x222u32.to_le_bytes());
    plain[0x3e..0x40].copy_from_slice(&0x80u16.to_le_bytes());
    let elabel = b"<ELABEL>GLab=322CA28A-D7D1448B-DCE2CED9||Label=TEST!SAFE6||";
    let logical_len = 0x80 + elabel.len();
    plain[4..8].copy_from_slice(&(logical_len as u32).to_le_bytes());
    plain[0x80..0x80 + elabel.len()].copy_from_slice(elabel);

    let encrypted_len = (logical_len / 16 + 1) * 16;
    let mut raw = vec![0u8; 512];
    raw[..encrypted_len].copy_from_slice(&a7f0_full(
        &plain[..encrypted_len],
        &crc.to_le_bytes(),
        0,
    ));
    raw[0x1f0..0x1f4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(8, &raw, &meta);

    assert_eq!(
        &view.decoded[0x1f0..0x1f4],
        &[0xde, 0xad, 0xbe, 0xef],
        "bytes after the dynamic encrypted prefix are preserved physical backing, not LLGB ciphertext"
    );
    assert!(view.method.contains(&format!("前 {encrypted_len}B")));
}

#[test]
fn lba8_preserves_nonzero_backing_inside_the_last_encrypted_block() {
    let device_id = "disk&ven_test&prod_llgb_inblock_backing";
    let crc = crc32_bare(device_id.as_bytes());
    let elabel = b"<ELABEL>GLab=SERIAL||Label=TEST!SAFE6||";
    let logical_len = 0x80 + elabel.len();
    let encrypted_len = (logical_len / 16 + 1) * 16;
    assert!(encrypted_len > logical_len + 1);

    let mut plain = vec![0u8; encrypted_len];
    plain[..4].copy_from_slice(b"LLGB");
    plain[4..8].copy_from_slice(&(logical_len as u32).to_le_bytes());
    plain[8..12].copy_from_slice(&0x0100_0001u32.to_le_bytes());
    plain[12..16].copy_from_slice(&0x222u32.to_le_bytes());
    plain[0x3e..0x40].copy_from_slice(&0x80u16.to_le_bytes());
    plain[0x80..logical_len].copy_from_slice(elabel);
    plain[logical_len] = 0;
    for (index, byte) in plain[logical_len + 1..encrypted_len].iter_mut().enumerate() {
        *byte = 0xa0u8.wrapping_add(index as u8);
    }
    let expected_backing = plain[logical_len + 1..encrypted_len].to_vec();

    let mut raw = vec![0u8; 512];
    raw[..encrypted_len].copy_from_slice(&a7f0_full(
        &plain[..encrypted_len],
        &crc.to_le_bytes(),
        0,
    ));
    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(8, &raw, &meta);

    assert_eq!(
        &view.decoded[logical_len + 1..encrypted_len],
        expected_backing.as_slice(),
        "bytes after the ELABEL NUL but inside the encrypted prefix are preserved backing, not semantic zero padding"
    );
}

#[test]
fn lba8_decrypts_one_extra_block_when_logical_length_is_16_byte_aligned() {
    let device_id = "disk&ven_test&prod_llgb_aligned";
    let crc = crc32_bare(device_id.as_bytes());
    let mut elabel = b"<ELABEL>Label=TEST!SAFE6||".to_vec();
    while !(0x80 + elabel.len()).is_multiple_of(16) {
        elabel.push(b'X');
    }
    let logical_len = 0x80 + elabel.len();
    assert_eq!(logical_len % 16, 0);
    let encrypted_len = (logical_len / 16 + 1) * 16;

    let mut plain = vec![0u8; encrypted_len];
    plain[..4].copy_from_slice(b"LLGB");
    plain[4..8].copy_from_slice(&(logical_len as u32).to_le_bytes());
    plain[8..12].copy_from_slice(&0x0100_0001u32.to_le_bytes());
    plain[12..16].copy_from_slice(&0x222u32.to_le_bytes());
    plain[0x3e..0x40].copy_from_slice(&0x80u16.to_le_bytes());
    plain[0x80..0x80 + elabel.len()].copy_from_slice(&elabel);
    // The byte exactly at logical_len is the ELABEL C-string terminator and
    // belongs to the extra encrypted block.
    assert_eq!(plain[logical_len], 0);

    let mut raw = vec![0u8; 512];
    raw[..encrypted_len].copy_from_slice(&a7f0_full(&plain, &crc.to_le_bytes(), 0));
    raw[encrypted_len] = 0x5a;

    let meta = InspectMeta {
        device_id: Some(device_id.into()),
        ..InspectMeta::default()
    };
    let view = analyze_sector(8, &raw, &meta);

    assert!(
        view.method.contains(&format!("前 {encrypted_len}B")),
        "{}",
        view.method
    );
    assert_eq!(view.decoded[logical_len], 0);
    assert_eq!(
        view.decoded[encrypted_len], 0x5a,
        "the first byte after the extra encrypted block must remain physical backing"
    );
}

#[test]
fn repeated_structures_render_as_groups_instead_of_repeating_prefixes() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");

    let edpf = render_fields(&analyze_sector(7, &data[7 * 512..8 * 512], &meta));
    assert!(edpf.contains("Entry[0]"), "{edpf}");
    assert!(edpf.contains("Entry[1]"), "{edpf}");
    assert_eq!(
        edpf.matches("Entry[0]").count(),
        1,
        "Entry 标题应只显示一次: {edpf}"
    );

    let mbr = render_fields(&analyze_sector(0, &data[..512], &meta));
    assert!(mbr.contains("分区 P1"), "{mbr}");
    assert_eq!(mbr.matches("P1").count(), 1, "P1 标题应只显示一次: {mbr}");
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn structured_output_keeps_known_sector_lines_readable() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    for lba in [0u32, 4, 6, 7, 8, 9, 11, 12] {
        let start = lba as usize * 512;
        let out = render_fields(&analyze_sector(lba, &data[start..start + 512], &meta));
        for line in out.lines() {
            assert!(
                line.chars().count() <= 120,
                "LBA{lba} 结构化输出行过长({}): {line}",
                line.chars().count()
            );
        }
    }
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn edpf_key_material_renders_as_separate_rows() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");
    let out = render_fields(&analyze_sector(7, &data[7 * 512..8 * 512], &meta));
    assert!(out.contains("pwd_crc"), "{out}");
    assert!(out.contains("key_crc"), "{out}");
    assert!(out.contains("key8"), "{out}");
    assert!(out.contains("密钥信息"), "{out}");
    assert!(
        !out.lines().any(|line| {
            line.contains("pwd_crc") && line.contains("key_crc") && line.contains("key8")
        }),
        "密钥字段不应挤在同一行: {out}"
    );
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn mbr_empty_partition_slots_are_summarized_not_expanded() {
    edpcli::ui::set_enabled_for_tests(false);
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    let view = analyze_sector(0, &data[..512], &meta);
    let out = render_fields(&view);
    assert!(out.contains("分区 P1"), "{out}");
    assert!(!out.contains("分区 P2"), "空分区不应展开: {out}");
    assert!(
        view.notes.iter().any(|note| note.contains("空分区")),
        "{:?}",
        view.notes
    );
    edpcli::ui::reset_enabled_for_tests();
}
