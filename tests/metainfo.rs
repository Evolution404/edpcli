mod common;

use common::*;
use edpcli::metainfo::{render, summarize};
use edpcli::protocol::semantic::SemanticContext;

fn meta_for(key: &str) -> SemanticContext {
    let (device_id, vid, pid, sectors, onlyid) = match key {
        "netac" => (
            "disk&ven_netac&prod_onlydisk",
            "0dd8",
            "2005",
            122_880_000u64,
            "1402259934",
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
    SemanticContext {
        device_id: Some(device_id.into()),
        vid: Some(vid.into()),
        pid: Some(pid.into()),
        size_bytes: sectors.checked_mul(512),
        onlyid: Some(onlyid.into()),
    }
}

#[test]
fn aigo_summary_contains_identity_and_ownership() {
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    let summary = summarize(&meta, |lba| {
        let start = lba as usize * 512;
        Ok(data[start..start + 512].to_vec())
    })
    .unwrap();

    assert_eq!(summary.onlyid.as_deref(), Some("1987718388"));
    assert_eq!(
        summary.device_id.as_deref(),
        Some("disk&ven_aigo&prod_u335&rev_pmap")
    );
    assert_eq!(summary.device_crc32.as_deref(), Some("0x2EEB4CE1"));
    assert_eq!(summary.ownership.dept.as_deref(), Some("输电运检中心"));
    assert_eq!(summary.ownership.user.as_deref(), Some("张玉玺"));
    assert_eq!(summary.ownership.label.as_deref(), Some("江苏电力!SAFE6"));
    assert_eq!(summary.safe6_label.as_deref(), Some("江苏电力!SAFE6"));
    assert_eq!(summary.safe6_user.as_deref(), Some("张玉玺"));
    assert!(!summary.partitions.is_empty());
    for partition in &summary.partitions {
        assert!(
            partition
                .kind
                .as_deref()
                .is_some_and(|kind| kind.contains(" (")),
            "partition type presentation changed: {partition:?}"
        );
        assert!(
            partition
                .size
                .as_deref()
                .is_some_and(|size| size.contains(" / ")),
            "partition size presentation changed: {partition:?}"
        );
        assert!(partition.status.is_none());
    }
    assert!(summary.safe6_register.is_none());

    edpcli::ui::set_enabled_for_tests(false);
    let out = render(&summary);
    assert!(out.contains("设备"));
    assert!(out.contains("身份"));
    assert!(out.contains("Dept"));
    assert!(out.contains("User"));
    assert!(out.contains("状态"));
    assert!(out.contains("分区摘要"));
    edpcli::ui::reset_enabled_for_tests();
}

#[test]
fn netac_summary_reads_long_department_name() {
    let data = load_disk_image("netac").expect("netac fixture");
    let meta = meta_for("netac");
    let summary = summarize(&meta, |lba| {
        let start = lba as usize * 512;
        Ok(data[start..start + 512].to_vec())
    })
    .unwrap();
    let dept = summary.ownership.dept.as_deref().unwrap_or_default();
    assert!(dept.contains("泰州供电公司"), "{dept}");
    assert_eq!(summary.ownership.user.as_deref(), Some("宋旭琳"));
}

#[test]
fn render_uses_semantic_colors_and_no_color_remains_plain() {
    let data = load_disk_image("aigo").expect("aigo fixture");
    let meta = meta_for("aigo");
    let summary = summarize(&meta, |lba| {
        let start = lba as usize * 512;
        Ok(data[start..start + 512].to_vec())
    })
    .unwrap();

    edpcli::ui::set_enabled_for_tests(true);
    let colored = render(&summary);
    assert!(colored.contains("\x1b[1;36m设备\x1b[0m"), "{colored:?}");
    assert!(colored.contains("\x1b[1;36m身份\x1b[0m"), "{colored:?}");
    assert!(colored.contains("\x1b[1;36m状态\x1b[0m"), "{colored:?}");
    assert!(colored.contains("\x1b[33m1987718388\x1b[0m"), "{colored:?}");
    assert!(
        colored.contains("\x1b[36m输电运检中心\x1b[0m"),
        "{colored:?}"
    );
    assert!(colored.contains("\x1b[35m125.83GB\x1b[0m"), "{colored:?}");
    assert!(
        colored.contains("\x1b[32m0x980E9B2F / 计算 0x980E9B2F ✓\x1b[0m"),
        "{colored:?}"
    );

    edpcli::ui::set_enabled_for_tests(false);
    let plain = render(&summary);
    assert!(!plain.contains("\x1b["));
    assert!(plain.contains("设备"));
    assert!(plain.contains("身份"));
    assert!(plain.contains("状态"));
    assert!(plain.contains("输电运检中心"));
    edpcli::ui::reset_enabled_for_tests();
}
