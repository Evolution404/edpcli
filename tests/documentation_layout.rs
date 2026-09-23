use std::fs;
use std::path::Path;

const REQUIRED_DOCS: &[&str] = &[
    "docs/README.md",
    "docs/user/USAGE.md",
    "docs/user/RELEASE.md",
    "docs/architecture/ARCHITECTURE.md",
    "docs/backup/EDPB_FORMAT_V1.md",
    "docs/backup/DEEP_BACKUP_V1.md",
    "docs/protocol/README.md",
    "docs/protocol/EDP_LBA0_12_FIELD_GUIDE.md",
    "docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md",
    "docs/protocol/LCE.md",
    "docs/provisioning/PROVISIONING.md",
];

#[test]
fn documentation_is_grouped_into_canonical_categories() {
    for path in REQUIRED_DOCS {
        assert!(
            Path::new(path).is_file(),
            "missing canonical document: {path}"
        );
    }

    for entry in fs::read_dir("docs").expect("docs directory") {
        let path = entry.expect("docs entry").path();
        if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("md") {
            assert_eq!(
                path.file_name().and_then(|value| value.to_str()),
                Some("README.md"),
                "top-level docs/ must contain only README.md; classify the document into a subdirectory: {}",
                path.display()
            );
        }
    }
}

#[test]
fn obsolete_parallel_plans_and_handoffs_do_not_reappear() {
    for obsolete in [
        "docs/CLI_V2_REDESIGN_PLAN.md",
        "docs/HANDOFF_CLI_V2_2026-09-18.md",
        "docs/TUI_IMPLEMENTATION_PLAN_2026-09-18.md",
        "docs/ARCHITECTURE_AUDIT_ANIMATION_2026-09-22.md",
        "docs/POST_V2_OPTIMIZATION_AUDIT_2026-09-18.md",
        "docs/EDP_PROTOCOL_LIVE_STATUS.md",
        "docs/EDP_PROTOCOL_ENGINEERING_PLAN.md",
        "docs/PROVISION_NEW_USB_PLAN_2026-09-19.md",
    ] {
        assert!(
            !Path::new(obsolete).exists(),
            "obsolete document reappeared: {obsolete}"
        );
    }
}

#[test]
fn provisioning_documents_current_four_mode_product_and_future_extensions() {
    let doc = fs::read_to_string("docs/provisioning/PROVISIONING.md").unwrap();
    for required in [
        "## 1. 当前已经实现的能力",
        "## 3. 当前实现 A：官方四模式新盘制盘",
        "## 4. 当前实现 B：已有官方盘转换为“启动区和交换区二合一”",
        "缺省三分区",
        "启动区和交换区二合一",
        "整盘加密",
        "内外网通用双分区",
        "## 5. 当前产品入口",
        "edpcli provision write",
        "## 6. 后续扩展顺序",
    ] {
        assert!(
            doc.contains(required),
            "provisioning roadmap lost required boundary: {required}"
        );
    }
}

fn collect_markdown_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in
        fs::read_dir(root).unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
    {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_markdown_files(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn prose_without_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(ch);
        }
    }
    out
}

#[test]
fn markdown_prose_uses_chinese_instead_of_english_sentences() {
    let mut files = Vec::new();
    collect_markdown_files(Path::new("docs"), &mut files);
    collect_markdown_files(Path::new("audit/protocol"), &mut files);

    // 该文件由机器目录生成，其中字段 ID、类型名、轴/状态名和测试符号属于技术标识。
    files.retain(|path| path != Path::new("docs/protocol/EDP_LBA0_12_FIELD_GUIDE.md"));

    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let mut fenced_code = false;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                fenced_code = !fenced_code;
                continue;
            }
            let prose_candidate = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .unwrap_or(trimmed);
            if fenced_code
                || trimmed.is_empty()
                || prose_candidate.starts_with("http://")
                || prose_candidate.starts_with("https://")
            {
                continue;
            }

            let prose = prose_without_inline_code(trimmed);
            let ascii_words = prose
                .split(|ch: char| !ch.is_ascii_alphabetic() && ch != '-')
                .filter(|word| word.chars().filter(|ch| ch.is_ascii_alphabetic()).count() >= 3)
                .count();
            let cjk = prose
                .chars()
                .filter(|ch| matches!(*ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF))
                .count();

            assert!(
                ascii_words < 6 || cjk > 0,
                "Markdown 正文存在整句英文，请改为中文（技术标识请放入反引号）：{}:{}: {}",
                path.display(),
                index + 1,
                trimmed
            );
        }
    }
}

#[test]
fn markdown_prose_rejects_common_english_narrative_terms() {
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
        "number",
        "product",
        "issue",
        "mark",
        "clean",
        "clone",
        "coverage",
        "encoder",
        "unmapped",
        "overflow",
        "upstream",
        "restore",
        "transitional",
        "registration",
        "aligned",
        "unencrypted",
        "seek",
        "exchange",
        "bitfield",
        "round",
        "donor",
        "extent",
        "hex",
        "universal",
        "artifact",
        "including",
        "bug",
        "covered",
        "consistency",
        "message",
        "surviving",
        "text",
        "now",
        "success",
        "initialized",
        "stale",
        "geometry",
        "radio",
        "slider",
        "globals",
        "inverse",
        "append",
        "upload",
        "emulator",
        "import",
        "validator",
        "hardware",
        "slots",
        "out",
        "lead",
        "ordinal",
        "initial",
        "adapter",
        "description",
        "entropy",
        "mount",
        "deterministic",
        "drive",
        "added",
        "virus",
        "verify",
        "handler",
        "transport",
        "injection",
        "scalar",
        "retained",
        "scratch",
        "filesystem",
        "body",
        "mapping",
        "mapped",
        "cleanup",
        "threshold",
        "destination",
    ];

    let mut files = Vec::new();
    collect_markdown_files(Path::new("docs"), &mut files);
    collect_markdown_files(Path::new("audit/protocol"), &mut files);
    files.retain(|path| path != Path::new("docs/protocol/EDP_LBA0_12_FIELD_GUIDE.md"));

    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let mut fenced_code = false;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                fenced_code = !fenced_code;
                continue;
            }
            if fenced_code || trimmed.is_empty() {
                continue;
            }

            let prose = prose_without_inline_code(trimmed);
            let lower = prose.to_ascii_lowercase();
            for term in forbidden {
                let bytes = lower.as_bytes();
                let needle = term.as_bytes();
                let mut start = 0;
                while let Some(relative) = lower[start..].find(term) {
                    let at = start + relative;
                    let before = at.checked_sub(1).and_then(|pos| bytes.get(pos)).copied();
                    let after = bytes.get(at + needle.len()).copied();
                    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
                    if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
                        panic!(
                            "Markdown 正文存在未中文化叙述词 {term}：{}:{}: {}",
                            path.display(),
                            index + 1,
                            trimmed
                        );
                    }
                    start = at + needle.len();
                    if start >= lower.len() {
                        break;
                    }
                }
            }
        }
    }
}
