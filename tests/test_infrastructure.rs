use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const NON_HIL_SUITES: [&str; 8] = [
    "cli_suite",
    "backup_suite",
    "inspect_suite",
    "protocol_suite",
    "provision_suite",
    "tui_suite",
    "platform_suite",
    "repository_suite",
];

const HIL_TEST_SOURCES: [&str; 2] = [
    "tests/virtual_disk_hil.rs",
    "tests/plain_macos_virtual_hil.rs",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn top_level_suite_members() -> BTreeMap<String, Vec<String>> {
    let mut members = BTreeMap::<String, Vec<String>>::new();
    for suite in NON_HIL_SUITES {
        let suite_source = read(&format!("tests/{suite}.rs"));
        for line in suite_source.lines() {
            let Some(relative) = line
                .trim()
                .strip_prefix("#[path = \"")
                .and_then(|value| value.strip_suffix("\"]"))
            else {
                continue;
            };
            let path = Path::new(relative);
            if path.components().count() != 1
                || path.extension().and_then(|value| value.to_str()) != Some("rs")
            {
                continue;
            }
            members
                .entry(format!("tests/{relative}"))
                .or_default()
                .push(suite.to_string());
        }
    }
    members
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
    for suite in NON_HIL_SUITES {
        assert!(
            cargo.contains(&format!("name = \"{suite}\"")),
            "missing suite {suite}"
        );
    }
}

#[test]
fn suite_source_ownership_is_complete_and_unambiguous() {
    let memberships = top_level_suite_members();
    let suite_roots = NON_HIL_SUITES
        .iter()
        .map(|suite| format!("tests/{suite}.rs"))
        .collect::<BTreeSet<_>>();
    let hil_sources = HIL_TEST_SOURCES
        .iter()
        .map(|path| path.to_string())
        .collect::<BTreeSet<_>>();

    for (source, owners) in &memberships {
        assert_eq!(
            owners.len(),
            1,
            "{source} must belong to exactly one non-HIL suite, got {owners:?}"
        );
    }

    let mut ordinary_sources = BTreeSet::new();
    for entry in fs::read_dir(repo_root().join("tests")).expect("read tests directory") {
        let entry = entry.expect("read tests directory entry");
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = format!(
            "tests/{}",
            path.file_name()
                .and_then(|value| value.to_str())
                .expect("UTF-8 test filename")
        );
        if suite_roots.contains(&relative) || hil_sources.contains(&relative) {
            continue;
        }
        ordinary_sources.insert(relative);
    }

    for source in &ordinary_sources {
        let owners = memberships.get(source).unwrap_or_else(|| {
            panic!("{source} is not owned by any suite; add it to exactly one suite root")
        });
        assert_eq!(
            owners.len(),
            1,
            "{source} must belong to exactly one suite, got {owners:?}"
        );
    }

    for source in memberships.keys() {
        assert!(
            ordinary_sources.contains(source),
            "{source} is referenced by a suite but does not exist as an ordinary top-level test"
        );
    }
}

#[test]
fn fast_runner_derives_test_source_mapping_from_suite_roots() {
    let runner = read("scripts/test-full.py");
    assert!(runner.contains("def discover_test_source_suites()"));
    assert!(runner.contains("TEST_SOURCE_SUITES = discover_test_source_suites()"));
    assert!(
        !runner.contains("\"tests/cli_ux.rs\": \"cli_suite\""),
        "fast runner must not reintroduce a hand-maintained test source mapping"
    );
}

#[test]
fn fast_and_full_gate_entrypoints_are_repository_owned() {
    let fast = read("scripts/test-fast.sh");
    let full = read("scripts/test-full.py");
    assert!(fast.contains("test-full.py"));
    assert!(fast.contains("cargo clippy --all-targets --locked -- -D warnings"));
    assert!(fast.contains("clippy skipped: no Rust/Cargo inputs changed"));
    assert!(fast.contains("git diff-tree --no-commit-id --name-only -r HEAD"));
    assert!(full.contains("--message-format=json"));
    assert!(full.contains("ThreadPoolExecutor"));
    assert!(full.contains("EDPCLI_TEST_THREADS"));
    assert!(full.contains("--test-threads"));
    assert!(full.contains("[parallelism]"));
    assert!(full.contains("max_test_threads"));
    assert!(full.contains("duration"));
}

#[test]
fn ci_and_agent_policy_use_the_full_runner_instead_of_all_targets_shell_chains() {
    let ci = read(".github/workflows/ci.yml");
    let release = read(".github/workflows/release.yml");
    let compatibility = read(".github/workflows/compatibility.yml");
    let agents = read("AGENTS.md");
    assert!(ci.contains("scripts/test-full.py"));
    assert!(!ci.contains("run-cargo-test-ci.py"));
    assert!(release.contains("scripts/test-full.py"));
    assert!(compatibility.contains("scripts/test-full.py"));
    assert!(!release.contains("cargo test --all-targets"));
    assert!(!compatibility.contains("cargo test --all-targets"));
    assert!(release.contains("cargo clippy --all-targets --locked -- -D warnings"));
    assert!(compatibility.contains("cargo clippy --all-targets --locked -- -D warnings"));
    assert!(!release.contains("cargo build --release\n"));
    assert!(release.contains("cargo build --release --locked"));
    assert!(release.contains("cargo metadata --locked"));
    assert!(agents.contains("scripts/test-fast.sh"));
    assert!(agents.contains("scripts/test-full.py"));
    assert!(agents.contains("120"));
    assert!(agents.contains("durable"));
}

#[test]
fn daily_ci_splits_primary_runtime_coverage_from_secondary_arch_compile_coverage() {
    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains("quality-primary:"));
    assert!(ci.contains("quality-secondary:"));
    assert!(ci.contains("macos-arm64"));
    assert!(ci.contains("linux-x86_64"));
    assert!(ci.contains("windows-x86_64"));
    assert!(ci.contains("macos-x86_64"));
    assert!(ci.contains("linux-arm64"));
    assert!(ci.contains("windows-arm64"));
    assert_eq!(
        ci.matches("name: Rustfmt").count(),
        1,
        "rustfmt must run once rather than once per platform"
    );
    assert!(ci.contains("cargo test --locked --no-run --all-targets"));
    assert!(ci.contains("cargo check --release --all-targets --locked"));
    assert!(!ci.contains("cargo build --release --locked"));
    assert!(ci.contains("repository-audit:"));
    assert!(ci.contains("classify changes"));
    assert!(ci.contains("label: macos-arm64\n            workers: 4\n            test_threads: 2"));
    assert!(ci.contains("label: linux-x86_64\n            workers: 4\n            test_threads: 2"));
    assert!(ci.contains("label: windows-x86_64\n            workers: 2\n            test_threads: 4"));
    assert!(ci.contains("EDPCLI_TEST_WORKERS: ${{ matrix.workers }}"));
    assert!(ci.contains("EDPCLI_TEST_THREADS: ${{ matrix.test_threads }}"));
}

#[test]
fn virtual_disk_hil_is_path_filtered_and_has_periodic_full_coverage() {
    let hil = read(".github/workflows/virtual-disk-hil.yml");
    assert!(hil.contains("paths:"));
    assert!(hil.contains("- \"src/**\""));
    assert!(!hil.contains("- \"tests/**\""));
    assert!(hil.contains("- \"tests/virtual_disk_hil.rs\""));
    assert!(hil.contains("- \"tests/plain_macos_virtual_hil.rs\""));
    assert!(hil.contains("workflow_dispatch:"));
    assert!(hil.contains("schedule:"));
    assert!(hil.contains("cron:"));
}

#[test]
fn dependency_policy_is_pinned_and_automatically_refreshed() {
    let ci = read(".github/workflows/ci.yml");
    let deny = read("deny.toml");
    let dependabot = read(".github/dependabot.yml");

    assert!(ci.contains("cargo install cargo-deny --locked --version 0.20.2"));
    assert!(ci.contains("actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830"));
    assert!(ci.contains("steps.cargo-deny-cache.outputs.cache-hit != 'true'"));
    assert!(ci.contains("cargo-deny-${{ runner.os }}-${{ runner.arch }}-0.20.2"));
    assert!(ci.contains("cargo deny check"));
    assert!(deny.contains("[advisories]"));
    assert!(deny.contains("[licenses]"));
    assert!(deny.contains("unknown-registry = \"deny\""));
    assert!(deny.contains("unknown-git = \"deny\""));
    assert!(dependabot.contains("package-ecosystem: cargo"));
    assert!(dependabot.contains("package-ecosystem: github-actions"));
}

#[test]
fn compiler_cache_is_optional_locally_and_pinned_in_ci() {
    let runner = read("scripts/test-full.py");
    assert!(runner.contains("shutil.which(\"sccache\")"));
    assert!(runner.contains("RUSTC_WRAPPER"));
    assert!(runner.contains("CARGO_INCREMENTAL"));
    assert!(runner.contains("[cache] sccache"));
    assert!(runner.contains("configure_console_encoding"));
    assert!(runner.contains("encoding=\"utf-8\""));

    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains("mozilla-actions/sccache-action@fc920bf0ec8de6ee65d409111f7ec508035751ba"));
    assert!(ci.contains("version: \"v0.17.0\""));
    assert!(ci.contains("SCCACHE_GHA_ENABLED: \"true\""));
    assert!(ci.contains("RUSTC_WRAPPER: \"sccache\""));
    assert!(ci.contains("CARGO_INCREMENTAL: \"0\""));
    assert_eq!(
        ci.matches("CARGO_PROFILE_DEV_DEBUG: \"0\"").count(),
        3,
        "repository-audit plus both daily quality matrices should disable CI-only dev debug info"
    );
    assert_eq!(
        ci.matches("CARGO_PROFILE_TEST_DEBUG: \"0\"").count(),
        3,
        "repository-audit plus both daily quality matrices should disable CI-only test debug info"
    );
}

#[test]
fn test_profile_benchmark_reuses_repository_runner_and_reports_distribution() {
    let benchmark = read("scripts/test-benchmark.py");
    assert!(benchmark.contains("test-full.py"));
    assert!(benchmark.contains("ROOT / \"scripts\""));
    assert!(benchmark.contains("statistics.median"));
    assert!(benchmark.contains("--repeat"));
    assert!(benchmark.contains("--workers"));
    assert!(benchmark.contains("--test-threads"));
    assert!(benchmark.contains("--json"));
    assert!(benchmark.contains("[benchmark]"));

    let workflow = read(".github/workflows/test-runner-benchmark.yml");
    assert!(workflow.contains("perf/test-runner-*"));
    assert!(workflow.contains("linux-x86_64"));
    assert!(workflow.contains("macos-arm64"));
    assert!(workflow.contains("windows-x86_64"));
    assert!(workflow.contains("--workers 1 --test-threads 4"));
    assert!(workflow.contains("--workers 2 --test-threads 2"));
    assert!(workflow.contains("--workers 2 --test-threads 4"));
    assert!(workflow.contains("--workers 4 --test-threads 1"));
    assert!(workflow.contains("--workers 4 --test-threads 2"));
}

#[test]
fn timing_regression_gate_is_explicit_and_overrideable() {
    let fast = read("scripts/test-fast.sh");
    assert!(fast.contains("EDPCLI_FAST_MAX_SECONDS"));
    assert!(fast.contains("--max-seconds"));
    assert!(fast.contains("60"));

    let runner = read("scripts/test-full.py");
    assert!(runner.contains("--max-seconds"));
    assert!(runner.contains("EDPCLI_TEST_MAX_SECONDS"));
    assert!(runner.contains("timing budget exceeded"));

    let ci = read(".github/workflows/ci.yml");
    assert!(ci.contains("EDPCLI_TEST_MAX_SECONDS: \"180\""));
}
