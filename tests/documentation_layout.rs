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
fn provisioning_keeps_current_state_and_four_mode_roadmap_separate() {
    let doc = fs::read_to_string("docs/provisioning/PROVISIONING.md").unwrap();
    for required in [
        "## 1. CURRENT",
        "## 3. ROADMAP A：完整复刻官方四模式新盘制盘",
        "## 4. ROADMAP B：已有官方盘转换为“启动区和交换区二合一”",
        "缺省三分区",
        "启动区和交换区二合一",
        "整盘加密",
        "内外网通用双分区",
        "当前 builder 不是完整的官方四模式制盘器",
    ] {
        assert!(
            doc.contains(required),
            "provisioning roadmap lost required boundary: {required}"
        );
    }
}
