mod common;

use common::*;
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
    assert_eq!(&v12.decoded[368..], &data[12 * 512 + 368..13 * 512]);
    assert!(v12.method.contains("前 368B"));
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
