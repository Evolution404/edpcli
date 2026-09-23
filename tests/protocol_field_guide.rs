#[path = "../scripts/protocol/generate_field_guide.rs"]
mod generator;

#[test]
fn field_guide_is_the_exact_catalog_and_axes_projection() {
    assert_eq!(
        include_str!("../docs/protocol/EDP_LBA0_12_FIELD_GUIDE.md"),
        generator::render(),
        "regenerate the field guide after changing canonical TSVs"
    );
}

#[test]
fn guide_covers_every_field_in_sector_order_with_rules_evidence_and_status() {
    let guide = generator::render();
    let sector_headers: Vec<_> = guide
        .lines()
        .filter(|line| line.starts_with("## LBA"))
        .collect();
    assert_eq!(
        sector_headers,
        (0..13)
            .map(|lba| format!("## LBA{lba}"))
            .collect::<Vec<_>>()
    );
    let catalog = include_str!("../audit/protocol/field_catalog.tsv");
    let mut lines = catalog.lines();
    let header: Vec<_> = lines.next().unwrap().split('\t').collect();
    for line in lines {
        let cols: Vec<_> = line.split('\t').collect();
        let get = |key| cols[header.iter().position(|name| *name == key).unwrap()];
        let heading = format!(
            "#### {} · {} / {}\n",
            get("field_id"),
            get("profile_axis"),
            get("profile")
        );
        assert_eq!(
            guide.matches(&heading).count(),
            1,
            "missing/duplicate {heading}"
        );
        let block = guide
            .split_once(&heading)
            .unwrap()
            .1
            .split("\n###")
            .next()
            .unwrap();
        for label in [
            "偏移",
            "语义类型",
            "含义",
            "所有权",
            "解码规则",
            "编码规则",
            "配置类型",
            "演进类型",
            "写入端证据",
            "消费端证据",
            "物理证据",
            "实现来源",
            "语义状态",
            "实现状态",
            "行为测试状态",
            "代码符号",
            "测试符号",
            "所有权测试",
        ] {
            assert!(
                block.contains(&format!("| {label} |")),
                "missing {label}: {heading}"
            );
        }
    }
}

#[test]
fn chinese_translation_table_is_complete_unique_and_has_no_common_english_prose() {
    use std::collections::BTreeSet;

    let table = include_str!("../audit/protocol/field_guide_zh.tsv");
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("source\tzh"));

    let forbidden = [
        "profile",
        "current",
        "writer",
        "producer",
        "consumer",
        "legacy",
        "reader",
        "backing",
        "wire",
        "preserve",
        "metadata",
        "physical",
        "semantic",
        "caller",
        "provenance",
        "payload",
        "fixture",
        "canonical",
        "opaque",
        "overlay",
        "capture",
        "static",
        "owner",
        "layout",
        "state",
        "manifest",
        "xref",
        "encryption",
        "inspect",
        "repair",
        "formatter",
        "serializer",
        "classifier",
        "deep",
        "core",
        "restore",
        "transitional",
        "artifact",
    ];

    let mut sources = BTreeSet::new();
    let mut count = 0usize;
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (source, zh) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("中文映射表格式错误：第 {} 行", index + 2));
        assert!(
            !source.trim().is_empty(),
            "中文映射源文本为空：第 {} 行",
            index + 2
        );
        assert!(!zh.trim().is_empty(), "中文映射为空：第 {} 行", index + 2);
        assert!(
            sources.insert(source),
            "中文映射源文本重复：第 {} 行：{}",
            index + 2,
            source
        );

        let lower = zh.to_ascii_lowercase();
        for term in forbidden {
            let bytes = lower.as_bytes();
            let needle = term.as_bytes();
            let mut start = 0usize;
            while let Some(relative) = lower[start..].find(term) {
                let at = start + relative;
                let before = at.checked_sub(1).and_then(|pos| bytes.get(pos)).copied();
                let after = bytes.get(at + needle.len()).copied();
                let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
                assert!(
                    before.is_some_and(is_ident) || after.is_some_and(is_ident),
                    "中文映射仍含英文叙述词 {term}：第 {} 行：{}",
                    index + 2,
                    zh
                );
                start = at + needle.len();
                if start >= lower.len() {
                    break;
                }
            }
        }
        count += 1;
    }

    assert_eq!(count, 271, "中文映射条目数变化，需审计生成器输入");
}
