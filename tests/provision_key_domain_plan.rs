use edpcli::provision::ProvisionTarget;
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
        contract: "migrate-or-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode0,
        contract: "per-domain-preserve-rewrap-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode1,
        contract: "encrypt-preserve-candidate-combined-migrate-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode2,
        contract: "encrypt-compatible-preserve-reserve-rebuild",
    },
    GoldenCell {
        source: State::Mode0,
        target: State::Mode3,
        contract: "boot-share-compatible-preserve-encrypt-drop-migrate",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Plain,
        contract: "migrate-or-rebuild",
    },
    GoldenCell {
        source: State::Mode1,
        target: State::Mode0,
        contract: "encrypt-preserve-candidate-combined-migrate-rebuild",
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
        contract: "combined-not-share-encrypt-drop-migrate",
    },
    GoldenCell {
        source: State::Mode2,
        target: State::Plain,
        contract: "decrypt-migrate-or-rebuild",
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
        contract: "type4-to-type2-migrate-rebuild-boot-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Plain,
        contract: "migrate-or-rebuild",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode0,
        contract: "boot-share-compatible-preserve-encrypt-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode1,
        contract: "boot-share-to-combined-migrate-rebuild-encrypt-new",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode2,
        contract: "share-to-encrypt-migrate-rebuild-reserve-rebuild",
    },
    GoldenCell {
        source: State::Mode3,
        target: State::Mode3,
        contract: "per-domain-preserve-rewrap-rebuild",
    },
];

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
    assert!(m0_m1.contract.contains("combined-migrate-rebuild"));

    let m2_m3 = GOLDEN
        .iter()
        .find(|cell| cell.source == State::Mode2 && cell.target == State::Mode3)
        .unwrap();
    assert!(m2_m3.contract.contains("type4-to-type2-migrate-rebuild"));

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
