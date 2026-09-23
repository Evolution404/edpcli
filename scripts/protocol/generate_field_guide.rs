//! Dependency-free projection of the canonical TSVs; also used by the drift test.
//! Run: rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide
//! Then: target/generate-field-guide [--check]
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const CATALOG: &str = include_str!("../../audit/protocol/field_catalog.tsv");
const AXES: &str = include_str!("../../audit/protocol/profile_axes.tsv");
const ZH: &str = include_str!("../../audit/protocol/field_guide_zh.tsv");
type Row<'a> = BTreeMap<&'a str, &'a str>;

fn rows(text: &str) -> Vec<Row<'_>> {
    let mut lines = text.lines();
    let header: Vec<_> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|line| {
            let values: Vec<_> = line.split('\t').collect();
            assert_eq!(header.len(), values.len());
            header.iter().copied().zip(values).collect()
        })
        .collect()
}

fn cell(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
}

fn translations() -> BTreeMap<&'static str, &'static str> {
    let mut lines = ZH.lines();
    assert_eq!(lines.next(), Some("source\tzh"));
    let mut translations = BTreeMap::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let (source, zh) = line
            .split_once('\t')
            .expect("bad field-guide translation row");
        assert!(
            !zh.trim().is_empty(),
            "empty Chinese translation for: {source}"
        );
        assert!(
            translations.insert(source, zh).is_none(),
            "duplicate Chinese translation key: {source}"
        );
    }
    translations
}

fn zh<'a>(translations: &BTreeMap<&'a str, &'a str>, text: &'a str) -> &'a str {
    translations
        .get(text)
        .copied()
        .unwrap_or_else(|| panic!("missing Chinese translation for: {text}"))
}

fn table(out: &mut String, header: &[&str], data: Vec<Vec<String>>) {
    writeln!(out, "| {} |", header.join(" | ")).unwrap();
    writeln!(out, "| {} |", vec!["---"; header.len()].join(" | ")).unwrap();
    for row in data {
        writeln!(
            out,
            "| {} |",
            row.iter().map(|v| cell(v)).collect::<Vec<_>>().join(" | ")
        )
        .unwrap();
    }
    out.push('\n');
}

pub fn render() -> String {
    let fields = rows(CATALOG);
    let axes = rows(AXES);
    let translations = translations();
    let mut out = String::from("# EDP LBA0–LBA12 字段手册\n\n\
> 自动生成：请修改标准 TSV 后重新生成，勿直接编辑本文件。\n\n\
事实源：[field_catalog.tsv](../../audit/protocol/field_catalog.tsv)、[profile_axes.tsv](../../audit/protocol/profile_axes.tsv)。\n\
证据 ID 的类型、定位与限制见 [evidence_manifest.tsv](../../audit/protocol/evidence_manifest.tsv)。\n\
生成器：[generate_field_guide.rs](../../scripts/protocol/generate_field_guide.rs)；\n\
运行 `rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide`，\n\
再运行 `target/generate-field-guide`；加 `--check` 只校验，不写文件。\n\n\
## 阅读约定\n\n\
“偏移”为扇区内十六进制位置，区间含首尾；“长度”为字节数。每扇区 512B，整段 6656B。\n\
同一范围可因配置类型有多行：`base/all` 与各所有权轴选中的一个状态共同覆盖盘面；\n\
覆盖项只补充独立的来源或取值差异，不重复声明字节所有权。不同轴独立组合，不能按软件年代整体绑定。\n\n\
`semantic_status` 记录语义闭环；`implementation_status` 与 `behavior_test_status` 分别记录正式代码和行为测试。\n\
`planned:` 是设计目标，`UNIMPLEMENTED` 表示尚无正式链接。通用所有权测试不代表字段行为测试已经完成。\n\
`MISSING_PHYSICAL` 表示缺物理证据引用，不降低语义状态；写入端/消费端中的虚拟证据不能作为物理采集。\n\n\
`preserve` 表示按该字段编码规则保留原字节，不能擅自清零；`opaque` 表示在 EDP 层不解释内部负载；\n\
`backing` 表示当前字段语义不消费的存储内容，不能仅因样本为零就认定为必须为零的常量。\n\
具体写入、加密表示和 NUL 后行为以逐字段规则为准；这些区域仍有正式所有权。\n\
`PackedStruct`/`EncryptedRegion` 行按目录现有粒度展示，不补造目录未列出的内部字段或算法。\n\n");
    let unique_axes: BTreeSet<_> = axes.iter().map(|a| a["axis"]).collect();
    writeln!(
        out,
        "目录包含 {} 个字段 × 配置类型行、{} 个正交轴、{} 个状态。\n",
        fields.len(),
        unique_axes.len(),
        axes.len()
    )
    .unwrap();
    out.push_str("## 目录\n\n");
    for lba in 0..13 {
        writeln!(out, "- [LBA{lba}](#lba{lba})").unwrap();
    }
    out.push('\n');
    for lba in 0..13 {
        let mut local: Vec<_> = fields
            .iter()
            .filter(|r| r["lba"].parse::<usize>().unwrap() == lba)
            .collect();
        local.sort_by_key(|r| {
            (
                usize::from_str_radix(r["start"], 16).unwrap(),
                r["ownership"],
                r["field_id"],
                r["profile_axis"],
                r["profile"],
            )
        });
        writeln!(out, "## LBA{lba}\n\n### 用途与配置类型\n").unwrap();
        let meanings: BTreeSet<_> = local.iter().map(|r| r["meaning"]).collect();
        for meaning in meanings {
            writeln!(out, "- {}", cell(zh(&translations, meaning))).unwrap();
        }
        out.push_str("\n### 512B 布局与字段索引\n\n每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。\n\n");
        table(
            &mut out,
            &[
                "偏移（含首尾）",
                "长度",
                "字段 ID",
                "轴 / 状态",
                "类型",
                "所有权",
            ],
            local
                .iter()
                .map(|r| {
                    vec![
                        format!("0x{}–0x{}", r["start"], r["end"]),
                        r["length"].into(),
                        r["field_id"].into(),
                        format!("{} / {}", r["profile_axis"], r["profile"]),
                        r["semantic_type"].into(),
                        r["ownership"].into(),
                    ]
                })
                .collect(),
        );
        out.push_str("### 逐字段规则与证据\n\n");
        for r in &local {
            writeln!(
                out,
                "#### {} · {} / {}\n",
                r["field_id"], r["profile_axis"], r["profile"]
            )
            .unwrap();
            table(
                &mut out,
                &["属性", "目录值"],
                [
                    (
                        "偏移",
                        format!("0x{}–0x{}（{}B）", r["start"], r["end"], r["length"]),
                    ),
                    ("语义类型", r["semantic_type"].into()),
                    ("含义", zh(&translations, r["meaning"]).into()),
                    ("所有权", r["ownership"].into()),
                    ("解码规则", zh(&translations, r["decode_rule"]).into()),
                    ("编码规则", zh(&translations, r["encode_rule"]).into()),
                    (
                        "配置类型",
                        format!("{} / {}", r["profile_axis"], r["profile"]),
                    ),
                    ("演进类型", r["evolution_kind"].into()),
                    ("写入端证据", r["producer_evidence"].into()),
                    ("消费端证据", r["consumer_evidence"].into()),
                    ("物理证据", r["physical_evidence"].into()),
                    (
                        "实现来源",
                        zh(&translations, r["implementation_provenance"]).into(),
                    ),
                    ("语义状态", r["semantic_status"].into()),
                    ("实现状态", r["implementation_status"].into()),
                    ("行为测试状态", r["behavior_test_status"].into()),
                    ("代码符号", r["code_symbol"].into()),
                    ("测试符号", r["test_symbol"].into()),
                    ("所有权测试", r["ownership_test_symbol"].into()),
                ]
                .into_iter()
                .map(|(k, v)| vec![k.into(), v])
                .collect(),
            );
        }
        out.push_str("### 历史演进矩阵\n\n");
        let local_axes: BTreeSet<_> = local
            .iter()
            .map(|r| r["profile_axis"])
            .filter(|a| *a != "base")
            .collect();
        if local_axes.is_empty() {
            out.push_str("目录未为本扇区声明独立 配置类型轴；逐字段演进类型见上表。\n\n");
        }
        for axis in local_axes {
            writeln!(out, "**{axis}**\n").unwrap();
            table(
                &mut out,
                &[
                    "状态",
                    "角色",
                    "完整轴范围（可跨 LBA）",
                    "演进类型",
                    "差异说明",
                    "证据",
                ],
                axes.iter()
                    .filter(|a| a["axis"] == axis)
                    .map(|a| {
                        [
                            "state",
                            "role",
                            "ranges",
                            "effect_kind",
                            "description",
                            "evidence",
                        ]
                        .map(|k| {
                            if k == "description" {
                                zh(&translations, a[k]).to_string()
                            } else {
                                a[k].to_string()
                            }
                        })
                        .to_vec()
                    })
                    .collect(),
            );
            let states: Vec<_> = axes
                .iter()
                .filter(|a| a["axis"] == axis)
                .map(|a| a["state"])
                .collect();
            let mut header = vec!["区域（含首尾）"];
            header.extend(states.iter().copied());
            let axis_rows: Vec<_> = local.iter().filter(|r| r["profile_axis"] == axis).collect();
            let offset = |value: &str| usize::from_str_radix(value, 16).unwrap();
            let boundaries: BTreeSet<_> = axis_rows
                .iter()
                .flat_map(|r| [offset(r["start"]), offset(r["end"]) + 1])
                .collect();
            let boundaries: Vec<_> = boundaries.into_iter().collect();
            table(
                &mut out,
                &header,
                boundaries
                    .windows(2)
                    .filter_map(|bounds| {
                        let (start, end) = (bounds[0], bounds[1] - 1);
                        if !axis_rows
                            .iter()
                            .any(|r| offset(r["start"]) <= start && offset(r["end"]) >= end)
                        {
                            return None;
                        }
                        let mut row = vec![format!("0x{start:03x}–0x{end:03x}")];
                        for state in &states {
                            let matching: Vec<_> = axis_rows
                                .iter()
                                .filter(|r| {
                                    r["profile"] == *state
                                        && offset(r["start"]) <= start
                                        && offset(r["end"]) >= end
                                })
                                .collect();
                            assert_eq!(matching.len(), 1, "axis scope gap/overlap: {axis}/{state}");
                            let r = matching[0];
                            row.push(format!(
                                "{}: {}",
                                r["field_id"],
                                zh(&translations, r["meaning"])
                            ));
                        }
                        Some(row)
                    })
                    .collect(),
            );
        }
        out.push_str("### 实现与测试入口\n\n");
        out.push_str("正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。\n\n");
        let tests: BTreeSet<_> = local.iter().map(|r| r["ownership_test_symbol"]).collect();
        for test in tests {
            writeln!(
                out,
                "- 所有权：`{test}`（[测试文件](../../tests/protocol_field_catalog.rs)）。"
            )
            .unwrap();
        }
        out.push_str("- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。\n\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

#[allow(dead_code)]
fn main() {
    let path = "docs/protocol/EDP_LBA0_12_FIELD_GUIDE.md";
    let rendered = render();
    match std::env::args().nth(1).as_deref() {
        None => std::fs::write(path, rendered).unwrap(),
        Some("--check") => assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            rendered,
            "field guide drift; regenerate from TSV"
        ),
        _ => panic!("usage: generate-field-guide [--check] (run from repository root)"),
    }
}
