use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn cargo_registers_bounded_integration_suites() {
    let cargo = read("Cargo.toml");
    assert!(cargo.contains("autotests = false"));
    assert_eq!(
        cargo.matches("[[test]]").count(),
        10,
        "keep eight non-HIL suites plus two explicit virtual-HIL targets"
    );
    for suite in [
        "cli_suite",
        "backup_suite",
        "inspect_suite",
        "protocol_suite",
        "provision_suite",
        "tui_suite",
        "platform_suite",
        "repository_suite",
    ] {
        assert!(
            cargo.contains(&format!("name = \"{suite}\"")),
            "missing suite {suite}"
        );
    }
}

#[test]
fn fast_and_full_gate_entrypoints_are_repository_owned() {
    let fast = read("scripts/test-fast.sh");
    let full = read("scripts/test-full.py");
    assert!(fast.contains("test-full.py"));
    assert!(full.contains("--message-format=json"));
    assert!(full.contains("ThreadPoolExecutor"));
    assert!(full.contains("duration"));
}

#[test]
fn ci_and_agent_policy_use_the_full_runner_instead_of_all_targets_shell_chains() {
    let ci = read(".github/workflows/ci.yml");
    let agents = read("AGENTS.md");
    assert!(ci.contains("scripts/test-full.py"));
    assert!(!ci.contains("run-cargo-test-ci.py"));
    assert!(agents.contains("scripts/test-fast.sh"));
    assert!(agents.contains("scripts/test-full.py"));
    assert!(agents.contains("120"));
    assert!(agents.contains("durable"));
}

#[test]
fn compiler_cache_is_optional_locally_and_pinned_in_ci() {
    let runner = read("scripts/test-full.py");
    assert!(runner.contains("shutil.which(\"sccache\")"));
    assert!(runner.contains("RUSTC_WRAPPER"));
    assert!(runner.contains("CARGO_INCREMENTAL"));
    assert!(runner.contains("[cache] sccache"));

    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains("mozilla-actions/sccache-action@fc920bf0ec8de6ee65d409111f7ec508035751ba"));
    assert!(ci.contains("version: \"v0.17.0\""));
    assert!(ci.contains("SCCACHE_GHA_ENABLED: \"true\""));
    assert!(ci.contains("RUSTC_WRAPPER: \"sccache\""));
    assert!(ci.contains("CARGO_INCREMENTAL: \"0\""));
}

#[test]
fn test_profile_benchmark_reuses_repository_runner_and_reports_distribution() {
    let benchmark = read("scripts/test-benchmark.py");
    assert!(benchmark.contains("test-full.py"));
    assert!(benchmark.contains("ROOT / \"scripts\""));
    assert!(benchmark.contains("statistics.median"));
    assert!(benchmark.contains("--repeat"));
    assert!(benchmark.contains("--json"));
    assert!(benchmark.contains("[benchmark]"));
}

#[test]
fn timing_regression_gate_is_explicit_and_overrideable() {
    let fast = read("scripts/test-fast.sh");
    assert!(fast.contains("EDPCLI_FAST_MAX_SECONDS"));
    assert!(fast.contains("--max-seconds"));
    assert!(fast.contains("45"));

    let runner = read("scripts/test-full.py");
    assert!(runner.contains("--max-seconds"));
    assert!(runner.contains("EDPCLI_TEST_MAX_SECONDS"));
    assert!(runner.contains("timing budget exceeded"));

    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains("EDPCLI_TEST_MAX_SECONDS: \"120\""));
}
