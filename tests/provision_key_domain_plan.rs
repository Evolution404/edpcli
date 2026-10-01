use edpcli::provision::{
    KeyDomainRole, PartitionRole, ProvisionTarget, SecretBytes, SourcePasswordKnowledge,
    TargetPasswordPolicy,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum State {
    Plain,
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

impl State {
    const ALL: [Self; 5] = [
        Self::Plain,
        Self::Mode0,
        Self::Mode1,
        Self::Mode2,
        Self::Mode3,
    ];

    const fn target(self) -> ProvisionTarget {
        match self {
            Self::Plain => ProvisionTarget::Plain,
            Self::Mode0 => ProvisionTarget::OFFICIAL[0],
            Self::Mode1 => ProvisionTarget::OFFICIAL[1],
            Self::Mode2 => ProvisionTarget::OFFICIAL[2],
            Self::Mode3 => ProvisionTarget::OFFICIAL[3],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GoldenCell {
    source: State,
    target: State,
    contract: &'static str,
}

const GOLDEN: [GoldenCell; 25] = [
    GoldenCell {
        source: State::Plain,
        target: State::Plain,
        contract: "plain-preserve-or-rebuild",
    },
    GoldenCell {
        source: State::Plain,
        target: State::Mode0,
        contract: "new-boot-share-encrypt",
    },
    GoldenCell {
        source: State::Plain,
        target: State::Mode1,
        contract: "new-combined-encrypt",
    },
    GoldenCell {
        source: State::Plain,
        target: State::Mode2,
        contract: "new-reserve-encrypt",
    },
    GoldenCell {
        source: State::Plain,
        target: State::Mode3,
        contract: "new-boot-share",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Plain,
        contract: "plain-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode0,
        contract: "per-domain-preserve-rewrap-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode1,
        contract: "encrypt-preserve-candidate-combined-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode2,
        contract: "encrypt-compatible-preserve-reserve-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode3,
        contract: "boot-share-compatible-preserve-encrypt-drop",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Plain,
        contract: "plain-rebuild",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Mode0,
        contract: "encrypt-preserve-candidate-combined-rebuild",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Mode1,
        contract: "per-domain-preserve-rewrap-rebuild",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Mode2,
        contract: "encrypt-compatible-preserve-reserve-rebuild",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Mode3,
        contract: "combined-rebuild-encrypt-drop",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Plain,
        contract: "decrypt-plain-rebuild",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Mode0,
        contract: "encrypt-compatible-preserve-boot-share-new",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Mode1,
        contract: "encrypt-compatible-preserve-combined-new",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Mode2,
        contract: "reserve-rebuild-encrypt-preserve-rewrap-rebuild",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Mode3,
        contract: "type4-to-type2-rebuild-boot-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Plain,
        contract: "plain-rebuild",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode0,
        contract: "boot-share-compatible-preserve-encrypt-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode1,
        contract: "boot-share-to-combined-rebuild-encrypt-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode2,
        contract: "share-to-encrypt-rebuild-reserve-rebuild",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode3,
        contract: "per-domain-preserve-rewrap-rebuild",
    },
];

#[test]
fn key_domain_roles_match_protocol_semantics() {
    assert_eq!(
        KeyDomainRole::from_partition_role(PartitionRole::Share),
        Some(KeyDomainRole::Share)
    );
    assert_eq!(
        KeyDomainRole::from_partition_role(PartitionRole::BootShareCombined),
        Some(KeyDomainRole::Share)
    );
    assert_eq!(
        KeyDomainRole::from_partition_role(PartitionRole::Encrypt),
        Some(KeyDomainRole::Encrypt)
    );
    assert_eq!(
        KeyDomainRole::from_partition_role(PartitionRole::Boot),
        None
    );
    assert_eq!(
        KeyDomainRole::from_partition_role(PartitionRole::CompatibilityReserve),
        None
    );
}

#[test]
fn key_domain_secret_debug_is_redacted_and_states_are_explicit() {
    let secret = SecretBytes::new(b"domain-secret");
    let debug = format!("{secret:?}");
    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("domain-secret"));

    assert_ne!(
        SourcePasswordKnowledge::Unknown,
        SourcePasswordKnowledge::DefaultVerified
    );
    assert_ne!(
        TargetPasswordPolicy::PreserveOpaque,
        TargetPasswordPolicy::InitializeNew
    );
}

#[test]
fn chapter_12_five_by_five_conversion_golden_is_complete() {
    let pairs = GOLDEN
        .iter()
        .map(|cell| (cell.source, cell.target))
        .collect::<BTreeSet<_>>();
    assert_eq!(GOLDEN.len(), 25);
    assert_eq!(pairs.len(), 25);

    for source in State::ALL {
        for target in State::ALL {
            let cell = GOLDEN
                .iter()
                .find(|cell| cell.source == source && cell.target == target)
                .expect("every source/target pair must have one golden cell");
            assert!(!cell.contract.is_empty());
            let _ = source.target();
            let _ = target.target();
        }
    }

    let m0_m1 = GOLDEN
        .iter()
        .find(|cell| cell.source == State::Mode0 && cell.target == State::Mode1)
        .unwrap();
    assert!(m0_m1.contract.contains("encrypt-preserve-candidate"));
    assert!(m0_m1.contract.contains("combined-rebuild"));

    let m2_m3 = GOLDEN
        .iter()
        .find(|cell| cell.source == State::Mode2 && cell.target == State::Mode3)
        .unwrap();
    assert!(m2_m3.contract.contains("type4-to-type2-rebuild"));

    let m1_m3 = GOLDEN
        .iter()
        .find(|cell| cell.source == State::Mode1 && cell.target == State::Mode3)
        .unwrap();
    assert!(m1_m3.contract.contains("combined-not-share"));
}

#[test]
fn chapter_12_request_model_must_not_have_one_global_password() {
    let application = include_str!("../src/application/provision.rs");
    let prepare = include_str!("../src/application/provision/prepare.rs");

    assert!(
        !application.contains("pub password: String"),
        "ProvisionRequest still exposes one global password"
    );
    assert!(
        !prepare.contains("request.password"),
        "prepare still feeds one password into source and target key domains"
    );
}

#[test]
fn chapter_12_tui_must_model_share_and_encrypt_passwords_independently() {
    let form = include_str!("../src/tui/provision/form.rs");
    let fields = include_str!("../src/tui/provision/fields.rs");

    for symbol in [
        "share_source_password",
        "share_target_password",
        "encrypt_source_password",
        "encrypt_target_password",
    ] {
        assert!(
            form.contains(symbol) || fields.contains(symbol),
            "missing independent TUI password field: {symbol}"
        );
    }
}

#[test]
fn chapter_12_review_uses_typed_final_password_effects_without_preflight_vocabulary() {
    let review = include_str!("../src/tui/provision/review.rs");
    for token in [
        "ProvisionConfirmationPasswordEffect",
        "Passthrough",
        "Rewrap",
        "NewFileKey",
        "保留原密码域",
        "使用目标密码，FileKey 保持",
        "使用目标密码，生成新 FileKey",
        "格式化重建",
    ] {
        assert!(
            review.contains(token),
            "missing Chapter 12 confirmation contract token: {token}"
        );
    }
    for banned in [
        "密码域需重建",
        "尚未获得格式化授权",
        "需要勾选格式化",
        "目标密码禁用（Opaque）",
    ] {
        assert!(
            !review.contains(banned),
            "confirmation review must not expose preflight vocabulary: {banned}"
        );
    }
}

#[test]
fn confirmation_projection_is_prepared_only_and_fail_closed() {
    let review = include_str!("../src/tui/provision/review.rs");
    for token in [
        "DiskLayoutModel::canonical_edp",
        "DiskLayoutModel::canonical_plain_plan",
        "validate_complete()",
        "PasswordDisposition::Blocked",
        "matching_segments != 1",
        "prepared.hardware_probe()",
        "prepared.expected_onlyid()",
    ] {
        assert!(
            review.contains(token),
            "confirmation projection is missing prepared-only contract token: {token}"
        );
    }
    for banned in [
        "provision_layout_model()",
        "provision_preflight()",
        "selected_device()",
        "ProvisionForm",
        "PlainProvisionForm",
    ] {
        assert!(
            !review.contains(banned),
            "confirmation projection must not read live/form state: {banned}"
        );
    }
}

#[test]
fn confirmation_renderers_are_presentation_only() {
    let renderers = [
        include_str!("../src/tui/provision/review_render.rs"),
        include_str!("../src/tui/provision/review_target_render.rs"),
        include_str!("../src/tui/provision/review_layout_render.rs"),
        include_str!("../src/tui/provision/review_plan_render.rs"),
        include_str!("../src/tui/provision/review_summary_render.rs"),
    ]
    .join("\n");
    for banned in [
        "PasswordDisposition",
        "RegionDisposition",
        "provision_preflight",
        "selected_device",
        "ProvisionForm",
        "PlainProvisionForm",
    ] {
        assert!(
            !renderers.contains(banned),
            "confirmation renderer must not recompute business semantics: {banned}"
        );
    }
}

#[test]
fn confirmation_ui_uses_one_handling_vocabulary_and_symbolic_statuses() {
    let summary = include_str!("../src/tui/provision/review_summary_render.rs");
    let review = include_str!("../src/tui/provision/review.rs");
    assert!(summary.contains("status_line("));
    assert!(summary.contains("\"处理\""));
    assert!(!summary.contains("动作"));
    for token in [
        "● 固定",
        "● 保留",
        "○ 空闲",
        "⚠ 格式化重建",
        "✓ 保留",
        "⚠ 清空",
        "— 不涉及",
        "✓ 保留原密码域",
        "↻ 使用目标密码，FileKey 保持",
        "✓ 保持",
    ] {
        assert!(
            review.contains(token),
            "missing symbolic confirmation status: {token}"
        );
    }
}

#[test]
fn provision_write_confirmation_is_consequence_first_and_non_redundant() {
    let details = include_str!("../src/tui/provision/confirmation_render.rs");
    let render = include_str!("../src/tui/provision/render.rs");
    let shared = include_str!("../src/tui/ui/confirmation.rs");

    for token in [
        "目标设备",
        "目标布局",
        "写入影响",
        "VID:PID",
        "onlyid",
        "数据",
        "密码",
        "文件系统",
    ] {
        assert!(
            details.contains(token),
            "missing confirmation token: {token}"
        );
    }
    assert!(render.contains("写入开始后不能撤销"));
    assert!(shared.contains("MediaWriteConfirmationKind::Provision => \"确认写入\""));
    for banned in [
        "view.target.device_id",
        "确认后将直接开始向",
        "输入精确 YES 后立即按已审核计划开始写盘",
    ] {
        assert!(
            !details.contains(banned) && !render.contains(banned),
            "provision confirmation must not expose redundant/long-form content: {banned}"
        );
    }
}

#[test]
fn chapter_12_cli_must_not_restore_global_password_fallback() {
    let cli = include_str!("../src/cli_args/provision.rs");
    assert!(!cli.contains("\"--password\" =>"));
    assert!(cli.contains("--share-source-password"));
    assert!(cli.contains("--share-target-password"));
    assert!(cli.contains("--encrypt-source-password"));
    assert!(cli.contains("--encrypt-target-password"));
}
