//! Dependency-free projection of the canonical TSVs; also used by the drift test.
//! Run: rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide
//! Then: target/generate-field-guide [--check]
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const CATALOG: &str = include_str!("../../audit/protocol/field_catalog.tsv");
const AXES: &str = include_str!("../../audit/protocol/profile_axes.tsv");
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
    let mut out = String::from("# EDP LBA0–LBA12 字段手册\n\n\
> 自动生成：请修改 canonical TSV 后重新生成，勿直接编辑本文件。\n\n\
事实源：[field_catalog.tsv](../audit/protocol/field_catalog.tsv)、[profile_axes.tsv](../audit/protocol/profile_axes.tsv)。\n\
证据 ID 的 modality、定位与限制见 [evidence_manifest.tsv](../audit/protocol/evidence_manifest.tsv)。\n\
生成器：[generate_field_guide.rs](../scripts/protocol/generate_field_guide.rs)；\n\
运行 `rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide`，\n\
再运行 `target/generate-field-guide`；加 `--check` 只校验，不写文件。\n\n\
## 阅读约定\n\n\
Offset 为扇区内十六进制位置，区间含首尾；Length 为字节数。每扇区 512B，整段 6656B。\n\
同一范围可因 profile 有多行：`base/all` 与各 owner axis 选中的一个 state 共同覆盖盘面；\n\
overlay 只补充独立的来源或取值差异，不重复声明字节所有权。不同 axis 独立组合，不能按软件年代整体绑定。\n\n\
`semantic_status` 记录语义闭环；`implementation_status` 与 `behavior_test_status` 分别记录正式代码和行为测试。\n\
`planned:` 是设计目标，`UNIMPLEMENTED` 表示尚无正式链接。通用 ownership 测试不代表字段行为测试已经完成。\n\
`MISSING_PHYSICAL` 表示缺物理证据引用，不降低语义状态；producer/consumer 中的 virtual 证据不能作为 physical capture。\n\n\
preserve 表示按该字段 encode rule 保留原字节，不能擅自清零；opaque 表示在 EDP 层不解释内部负载；\n\
backing 表示当前字段语义不消费的存储内容，不能仅因样本为零就认定为必须为零的常量。\n\
具体写入、加密表示和 NUL 后行为以逐字段规则为准；这些区域仍有正式 ownership。\n\
PackedStruct/EncryptedRegion 行按目录现有粒度展示，不补造目录未列出的内部字段或算法。\n\n");
    let unique_axes: BTreeSet<_> = axes.iter().map(|a| a["axis"]).collect();
    writeln!(
        out,
        "目录包含 {} 个 field × profile 行、{} 个正交 axis、{} 个 state。\n",
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
        writeln!(out, "## LBA{lba}\n\n### 用途与 profile\n").unwrap();
        let meanings: BTreeSet<_> = local.iter().map(|r| r["meaning"]).collect();
        for meaning in meanings {
            writeln!(out, "- {}", cell(meaning)).unwrap();
        }
        out.push_str("\n### 512B 布局与字段索引\n\n每行标出范围和 profile；overlay 行是对应 owner 范围的注解。\n\n");
        table(
            &mut out,
            &[
                "Offset（含首尾）",
                "Length",
                "Field ID",
                "Axis / state",
                "Type",
                "Ownership",
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
                        "Offset",
                        format!("0x{}–0x{}（{}B）", r["start"], r["end"], r["length"]),
                    ),
                    ("Semantic type", r["semantic_type"].into()),
                    ("Meaning", r["meaning"].into()),
                    ("Ownership", r["ownership"].into()),
                    ("Decode rule", r["decode_rule"].into()),
                    ("Encode rule", r["encode_rule"].into()),
                    (
                        "Profile",
                        format!("{} / {}", r["profile_axis"], r["profile"]),
                    ),
                    ("Evolution kind", r["evolution_kind"].into()),
                    ("Producer evidence", r["producer_evidence"].into()),
                    ("Consumer evidence", r["consumer_evidence"].into()),
                    ("Physical evidence", r["physical_evidence"].into()),
                    (
                        "Implementation provenance",
                        r["implementation_provenance"].into(),
                    ),
                    ("Semantic status", r["semantic_status"].into()),
                    ("Implementation status", r["implementation_status"].into()),
                    ("Behavior-test status", r["behavior_test_status"].into()),
                    ("Code symbol", r["code_symbol"].into()),
                    ("Test symbol", r["test_symbol"].into()),
                    ("Ownership test", r["ownership_test_symbol"].into()),
                ]
                .into_iter()
                .map(|(k, v)| vec![k.into(), v])
                .collect(),
            );
        }
        out.push_str("### 历史演进 matrix\n\n");
        let local_axes: BTreeSet<_> = local
            .iter()
            .map(|r| r["profile_axis"])
            .filter(|a| *a != "base")
            .collect();
        if local_axes.is_empty() {
            out.push_str("目录未为本扇区声明独立 profile axis；逐字段 evolution kind 见上表。\n\n");
        }
        for axis in local_axes {
            writeln!(out, "**{axis}**\n").unwrap();
            table(
                &mut out,
                &[
                    "State",
                    "Role",
                    "完整 axis 范围（可跨 LBA）",
                    "Evolution",
                    "差异说明",
                    "Evidence",
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
                        .map(|k| a[k].to_string())
                        .to_vec()
                    })
                    .collect(),
            );
            let states: Vec<_> = axes
                .iter()
                .filter(|a| a["axis"] == axis)
                .map(|a| a["state"])
                .collect();
            let mut header = vec!["Region（含首尾）"];
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
                            row.push(format!("{}: {}", r["field_id"], r["meaning"]));
                        }
                        Some(row)
                    })
                    .collect(),
            );
        }
        out.push_str("### 实现与测试入口\n\n");
        out.push_str("正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。\n\n");
        let tests: BTreeSet<_> = local.iter().map(|r| r["ownership_test_symbol"]).collect();
        for test in tests {
            writeln!(
                out,
                "- Ownership：`{test}`（[测试文件](../tests/protocol_field_catalog.rs)）。"
            )
            .unwrap();
        }
        out.push_str("- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。\n\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

#[allow(dead_code)]
fn main() {
    let path = "docs/EDP_LBA0_12_FIELD_GUIDE.md";
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
