#!/usr/bin/env python3
"""Repository-owned fast/full Rust test runner.

The runner compiles the requested test targets once with Cargo JSON output, then
executes the resulting test binaries directly with bounded parallelism.  This
avoids paying Cargo target-dispatch overhead once per integration test crate and
gives each binary its own timeout and duration record.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import time
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]

ALL_SUITES = (
    "cli_suite",
    "backup_suite",
    "inspect_suite",
    "protocol_suite",
    "provision_suite",
    "tui_suite",
    "platform_suite",
    "repository_suite",
)

HIL_TARGETS = {"virtual_disk_hil", "plain_macos_virtual_hil"}

def compiler_env() -> dict[str, str]:
    env = os.environ.copy()
    wrapper = env.get("RUSTC_WRAPPER")
    if not wrapper and shutil.which("sccache"):
        wrapper = "sccache"
        env["RUSTC_WRAPPER"] = wrapper
    if wrapper and Path(wrapper).name == "sccache":
        env.setdefault("CARGO_INCREMENTAL", "0")
    return env


def announce_compiler_cache(env: dict[str, str]) -> None:
    wrapper = env.get("RUSTC_WRAPPER")
    if wrapper:
        print(f"[cache] rustc wrapper={wrapper}", flush=True)
    else:
        print("[cache] sccache unavailable; using rustc directly", flush=True)

CORE_FAST_SUITES = {"protocol_suite", "platform_suite", "repository_suite"}

TEST_SOURCE_SUITES = {
    "tests/cli_ux.rs": "cli_suite",
    "tests/cli_v2_parser.rs": "cli_suite",
    "tests/cli_v2_surface_guard.rs": "cli_suite",
    "tests/cli_write_safety.rs": "cli_suite",
    "tests/identify_list.rs": "cli_suite",
    "tests/selectors.rs": "cli_suite",
    "tests/backup.rs": "backup_suite",
    "tests/backup_catalog.rs": "backup_suite",
    "tests/backup_deep.rs": "backup_suite",
    "tests/backup_metadata.rs": "backup_suite",
    "tests/edpb.rs": "backup_suite",
    "tests/inspect.rs": "inspect_suite",
    "tests/inspect_cli.rs": "inspect_suite",
    "tests/inspect_full_disk_acceptance.rs": "inspect_suite",
    "tests/inspect_target.rs": "inspect_suite",
    "tests/metainfo.rs": "inspect_suite",
    "tests/sectors_readonly.rs": "inspect_suite",
    "tests/crypto_prims.rs": "protocol_suite",
    "tests/iir.rs": "protocol_suite",
    "tests/lba7_compat.rs": "protocol_suite",
    "tests/lba7_compatibility_ledger.rs": "protocol_suite",
    "tests/protocol_byte_ledger.rs": "protocol_suite",
    "tests/protocol_documentation_contract.rs": "protocol_suite",
    "tests/protocol_field_catalog.rs": "protocol_suite",
    "tests/protocol_field_guide.rs": "protocol_suite",
    "tests/protocol_gold_crosscheck.rs": "protocol_suite",
    "tests/protocol_image.rs": "protocol_suite",
    "tests/protocol_runtime_dependency_gate.rs": "protocol_suite",
    "tests/atomic_write.rs": "provision_suite",
    "tests/plain_provision.rs": "provision_suite",
    "tests/plain_virtual_hil.rs": "provision_suite",
    "tests/provision_contract.rs": "provision_suite",
    "tests/provision_fat16.rs": "provision_suite",
    "tests/provision_filesystem.rs": "provision_suite",
    "tests/provision_generate.rs": "provision_suite",
    "tests/provision_key_material.rs": "provision_suite",
    "tests/provision_layout.rs": "provision_suite",
    "tests/provision_lce.rs": "provision_suite",
    "tests/provision_protocol_audit.rs": "provision_suite",
    "tests/provision_reprovision.rs": "provision_suite",
    "tests/provision_transaction_write.rs": "provision_suite",
    "tests/provision_validate.rs": "provision_suite",
    "tests/provision_write_plan.rs": "provision_suite",
    "tests/write_progress_events.rs": "provision_suite",
    "tests/platform_boundary.rs": "platform_suite",
    "tests/platform_cli_matrix.rs": "platform_suite",
    "tests/dev_format_hook.rs": "repository_suite",
    "tests/documentation_layout.rs": "repository_suite",
    "tests/test_infrastructure.rs": "repository_suite",
}


@dataclass(frozen=True)
class TestArtifact:
    name: str
    executable: Path


@dataclass(frozen=True)
class TestResult:
    name: str
    duration: float
    returncode: int
    stdout: str
    stderr: str
    timed_out: bool = False


def run_text(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def changed_paths() -> list[str]:
    worktree = run_text(["git", "diff", "--name-only", "HEAD"])
    if worktree.returncode == 0:
        paths = [line.strip() for line in worktree.stdout.splitlines() if line.strip()]
        if paths:
            return paths

    latest = run_text(["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"])
    if latest.returncode == 0:
        return [line.strip() for line in latest.stdout.splitlines() if line.strip()]
    return []


def suites_for_paths(paths: Iterable[str]) -> set[str]:
    selected = set(CORE_FAST_SUITES)
    for raw in paths:
        path = raw.replace("\\", "/")
        direct = TEST_SOURCE_SUITES.get(path)
        if direct:
            selected.add(direct)
            continue

        if path in {"Cargo.toml", "Cargo.lock", "src/lib.rs"}:
            return set(ALL_SUITES)
        if path.startswith("tests/tui_") or path.startswith("src/tui/"):
            selected.add("tui_suite")
        elif path.startswith("tests/cli_") or path in {
            "src/cli.rs",
            "src/cli_args.rs",
            "src/completion.rs",
            "src/main.rs",
        }:
            selected.add("cli_suite")
        elif path.startswith("tests/backup") or path.startswith("src/backup"):
            selected.add("backup_suite")
        elif path.startswith("tests/inspect") or path.startswith("src/inspect"):
            selected.add("inspect_suite")
        elif path.startswith("tests/protocol") or path.startswith("src/protocol"):
            selected.add("protocol_suite")
        elif path.startswith("tests/provision") or path.startswith("src/provision/"):
            selected.add("provision_suite")
        elif path.startswith("src/application/provision") or path == "src/application/write.rs":
            selected.add("provision_suite")
        elif path.startswith("src/application/backup") or path == "src/edpb.rs":
            selected.add("backup_suite")
        elif path.startswith("src/application/inspect") or path == "src/metainfo.rs":
            selected.add("inspect_suite")
        elif path.startswith("src/platform") or path == "src/sysinfo.rs":
            selected.add("platform_suite")
        elif path == "src/diskio.rs":
            selected.update(
                {"backup_suite", "inspect_suite", "provision_suite", "platform_suite"}
            )
        elif path.startswith(("docs/", "scripts/", ".github/")) or path in {
            "AGENTS.md",
            "README.md",
        }:
            selected.add("repository_suite")
        elif path.startswith("src/"):
            return set(ALL_SUITES)
    return selected


def cargo_compile(suites: list[str], env: dict[str, str]) -> list[TestArtifact]:
    command = [
        "cargo",
        "test",
        "--locked",
        "--no-run",
        "--message-format=json",
        "--lib",
        "--bin",
        "edpcli",
    ]
    for suite in suites:
        command.extend(["--test", suite])

    print("[compile] " + " ".join(command), flush=True)
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=None,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
        env=env,
    )
    assert process.stdout is not None

    artifacts: dict[Path, TestArtifact] = {}
    for line in process.stdout:
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            print(line.rstrip(), flush=True)
            continue

        reason = record.get("reason")
        if reason == "compiler-message":
            message = record.get("message") or {}
            rendered = message.get("rendered")
            if rendered and message.get("level") in {"warning", "error"}:
                print(rendered, file=sys.stderr, end="")
            continue
        if reason != "compiler-artifact":
            continue

        profile = record.get("profile") or {}
        executable = record.get("executable")
        target = record.get("target") or {}
        target_name = str(target.get("name", "unknown"))
        if not profile.get("test") or not executable or target_name in HIL_TARGETS:
            continue

        kinds = "+".join(target.get("kind") or [])
        src_path = Path(str(target.get("src_path", ""))).name
        label = f"{target_name}[{kinds}:{src_path}]"
        exe_path = Path(executable)
        if not exe_path.is_absolute():
            exe_path = ROOT / exe_path
        artifacts[exe_path] = TestArtifact(label, exe_path)

    returncode = process.wait()
    duration = time.monotonic() - started
    print(f"[compile] exit={returncode} duration={duration:.2f}s artifacts={len(artifacts)}")
    if returncode != 0:
        raise RuntimeError(f"cargo test --no-run failed with exit code {returncode}")

    found_suite_names = {
        artifact.name.split("[", 1)[0]
        for artifact in artifacts.values()
        if artifact.name.split("[", 1)[0] in ALL_SUITES
    }
    missing = sorted(set(suites) - found_suite_names)
    if missing:
        raise RuntimeError("Cargo did not emit requested suite executables: " + ", ".join(missing))
    if not artifacts:
        raise RuntimeError("Cargo emitted no runnable test artifacts")

    return list(artifacts.values())


def run_artifact(artifact: TestArtifact, timeout: int) -> TestResult:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            [str(artifact.executable), "--quiet", "--test-threads=4"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            errors="replace",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        return TestResult(
            artifact.name,
            time.monotonic() - started,
            completed.returncode,
            completed.stdout,
            completed.stderr,
        )
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or ""
        stderr = error.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", "replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", "replace")
        return TestResult(
            artifact.name,
            time.monotonic() - started,
            124,
            stdout,
            stderr,
            timed_out=True,
        )


def run_artifacts(
    artifacts: list[TestArtifact], workers: int, timeout: int
) -> list[TestResult]:
    results: list[TestResult] = []
    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {
            executor.submit(run_artifact, artifact, timeout): artifact
            for artifact in artifacts
        }
        for future in as_completed(futures):
            result = future.result()
            results.append(result)
            state = "TIMEOUT" if result.timed_out else ("PASS" if result.returncode == 0 else "FAIL")
            print(f"[{state}] {result.name} duration={result.duration:.2f}s", flush=True)
            if result.returncode != 0:
                if result.stdout:
                    print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
                if result.stderr:
                    print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
    return sorted(results, key=lambda item: item.duration, reverse=True)


def run_doctests(env: dict[str, str]) -> TestResult:
    started = time.monotonic()
    completed = subprocess.run(
        ["cargo", "test", "--doc", "--locked", "--quiet"],
        cwd=ROOT,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return TestResult(
        "doctest",
        time.monotonic() - started,
        completed.returncode,
        completed.stdout,
        completed.stderr,
    )


def github_escape(value: str) -> str:
    return value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def annotate_failure(summary: str) -> None:
    if os.environ.get("GITHUB_ACTIONS", "").lower() == "true":
        print(f"::error title=edpcli full test gate failed::{github_escape(summary)}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=("fast", "full"), default="full")
    parser.add_argument("--suite", action="append", choices=ALL_SUITES)
    parser.add_argument(
        "--workers",
        type=int,
        default=int(os.environ.get("EDPCLI_TEST_WORKERS", "2")),
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=int(os.environ.get("EDPCLI_TEST_BINARY_TIMEOUT_SECS", "180")),
    )
    max_seconds = os.environ.get("EDPCLI_TEST_MAX_SECONDS")
    parser.add_argument(
        "--max-seconds",
        type=float,
        default=float(max_seconds) if max_seconds else None,
        help="fail after a successful run if total profile duration exceeds this budget",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.workers < 1 or args.workers > 8:
        raise SystemExit("--workers must be between 1 and 8")
    if args.timeout < 1:
        raise SystemExit("--timeout must be positive")
    if args.max_seconds is not None and args.max_seconds <= 0:
        raise SystemExit("--max-seconds must be positive")

    env = compiler_env()
    announce_compiler_cache(env)

    if args.suite:
        suites = sorted(set(args.suite), key=ALL_SUITES.index)
    elif args.profile == "full":
        suites = list(ALL_SUITES)
    else:
        paths = changed_paths()
        suites = sorted(suites_for_paths(paths), key=ALL_SUITES.index)
        shown = ", ".join(paths) if paths else "<none>"
        print(f"[fast] changed paths: {shown}")
        print(f"[fast] selected suites: {', '.join(suites)}")

    started = time.monotonic()
    try:
        artifacts = cargo_compile(suites, env)
    except RuntimeError as error:
        annotate_failure(str(error))
        print(f"[FAIL] {error}", file=sys.stderr)
        return 1

    results = run_artifacts(artifacts, args.workers, args.timeout)
    if args.profile == "full":
        doctest = run_doctests(env)
        results.append(doctest)
        state = "PASS" if doctest.returncode == 0 else "FAIL"
        print(f"[{state}] doctest duration={doctest.duration:.2f}s")
        if doctest.returncode != 0:
            if doctest.stdout:
                print(doctest.stdout)
            if doctest.stderr:
                print(doctest.stderr, file=sys.stderr)

    failures = [result for result in results if result.returncode != 0]
    print("\n[durations]")
    for result in sorted(results, key=lambda item: item.duration, reverse=True):
        print(f"{result.duration:8.2f}s  {result.name}")

    total = time.monotonic() - started
    print(
        f"\n[summary] profile={args.profile} suites={len(suites)} "
        f"artifacts={len(artifacts)} failures={len(failures)} duration={total:.2f}s"
    )
    if failures:
        summary = "; ".join(
            f"{result.name} exit={result.returncode}" for result in failures
        )
        annotate_failure(summary)
        return 1
    if args.max_seconds is not None and total > args.max_seconds:
        summary = (
            "timing budget exceeded: "
            f"profile={args.profile} duration={total:.2f}s budget={args.max_seconds:.2f}s"
        )
        annotate_failure(summary)
        print(f"[FAIL] {summary}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
