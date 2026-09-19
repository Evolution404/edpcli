mod common;

use common::*;
use edpcli::crypto::{a7f0_full, crc32_bare, xor_rolling};
use edpcli::diskio::parse_backup_name;
use edpcli::inspect::{analyze_sector, render_fields, render_hex, FieldStyle, InspectMeta};

fn meta_for(key: &str) -> InspectMeta {
    let (name, _) = fixture(key).expect("fixture metadata");
    let parsed = parse_backup_name(name).expect("parse fixture backup name");
    InspectMeta::from_backup_meta(&parsed)
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
        .any(|f| f.label == "模板/版本扩展区" && f.start == 0x1e0 && f.end == 0x1f0));
    assert!(v
        .fields
        .iter()
        .any(|f| f.label == "m_encrypt" && f.start == 0x1f0 && f.end == 0x1f4));
    assert!(!v.fields.iter().any(|f| f.label == "模板值"));
    assert!(!v.fields.iter().any(|f| f.label == "注册标志"));
}

#[test]
fn lba11_can_decrypt_from_backup_filename_metadata() {
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");
    let v = analyze_sector(11, &data[11 * 512..12 * 512], &meta);
    assert!(v.method.contains("PDKB"), "{}", v.method);
    assert_eq!(&v.decoded[0x100..0x104], b"PDKB");
    assert!(v.fields.iter().any(|f| f.label == "PDKB device_id"));
}

#[test]
fn lba4_zero_ciphertext_byte_is_decrypted_unless_whole_short_gap_is_unwritten() {
    let onlyid = 949_028_302u32;
    let k0 = (onlyid & 0xffff) ^ (onlyid >> 16);
    let mut plain = vec![0u8; 512];
    let header = b"$$$949028302$$$";
    plain[..header.len()].copy_from_slice(header);
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
    assert!(view.fields.iter().any(|field| field.value == "交换区"));
    assert!(view.fields.iter().any(|field| field.value == "保密区"));
}

#[test]
fn lba9_decodes_independent_eetu_sapf_and_eppe_regions() {
    let device_id = "disk&ven_test&prod_lba9";
    let crc = crc32_bare(device_id.as_bytes());
    let key = crc.to_le_bytes();
    let mut raw = vec![0u8; 512];

    let mut eetu = [0u8; 0x80];
    eetu[..4].copy_from_slice(b"EETU");
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
    assert_eq!(&view.decoded[0x180..0x184], b"EPPE");
    assert!(view.fields.iter().any(|field| field.label == "EETU magic"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "partition type" && field.value == "0x0B"));
    assert!(view
        .fields
        .iter()
        .any(|field| field.label == "起始 LBA" && field.value == "63"));
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

    let encrypted_len = (logical_len + 15) & !15;
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
