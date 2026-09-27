#!/usr/bin/env python3
"""Benchmark repository-owned fast/full test profiles without duplicating runner logic."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts" / "test-full.py"
SUMMARY_RE = re.compile(
    r"\[summary\] profile=(?P<profile>fast|full) suites=(?P<suites>\d+) "
    r"artifacts=(?P<artifacts>\d+) failures=(?P<failures>\d+) "
    r"duration=(?P<duration>[0-9.]+)s"
)


def run_sample(
    profile: str,
    index: int,
    workers: int,
    test_threads: int,
) -> dict[str, float | int | str]:
    command = [
        sys.executable,
        str(RUNNER),
        "--profile",
        profile,
        "--workers",
        str(workers),
        "--test-threads",
        str(test_threads),
    ]
    print(
        f"[benchmark] profile={profile} sample={index} command={' '.join(command)}",
        flush=True,
    )
    started = time.monotonic()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    wall_seconds = time.monotonic() - started
    print(completed.stdout, end="" if completed.stdout.endswith("\n") else "\n")
    if completed.returncode != 0:
        raise RuntimeError(
            f"{profile} sample {index} failed with exit code {completed.returncode}"
        )

    match = SUMMARY_RE.search(completed.stdout)
    if match is None:
        raise RuntimeError(f"{profile} sample {index} did not emit a runner summary")
    failures = int(match.group("failures"))
    if failures != 0:
        raise RuntimeError(f"{profile} sample {index} reported {failures} failures")
    return {
        "profile": profile,
        "sample": index,
        "duration_seconds": float(match.group("duration")),
        "wall_seconds": round(wall_seconds, 3),
        "suites": int(match.group("suites")),
        "artifacts": int(match.group("artifacts")),
        "workers": workers,
        "test_threads": test_threads,
    }


def summarize(profile: str, samples: list[dict[str, float | int | str]]) -> dict[str, object]:
    durations = [float(sample["duration_seconds"]) for sample in samples]
    summary: dict[str, object] = {
        "profile": profile,
        "samples": samples,
        "min_seconds": min(durations),
        "median_seconds": statistics.median(durations),
        "max_seconds": max(durations),
    }
    print(
        "[benchmark] "
        f"profile={profile} repeat={len(samples)} "
        f"min={summary['min_seconds']:.2f}s "
        f"median={summary['median_seconds']:.2f}s "
        f"max={summary['max_seconds']:.2f}s",
        flush=True,
    )
    return summary


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=("fast", "full", "both"), default="both")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--test-threads", type=int, default=4)
    parser.add_argument("--json", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.repeat < 1 or args.repeat > 20:
        raise SystemExit("--repeat must be between 1 and 20")
    if args.workers < 1 or args.workers > 8:
        raise SystemExit("--workers must be between 1 and 8")
    if args.test_threads < 1 or args.test_threads > 16:
        raise SystemExit("--test-threads must be between 1 and 16")

    profiles = ("fast", "full") if args.profile == "both" else (args.profile,)
    report: dict[str, object] = {
        "repeat": args.repeat,
        "workers": args.workers,
        "test_threads": args.test_threads,
        "profiles": {},
    }

    try:
        for profile in profiles:
            samples = [
                run_sample(
                    profile,
                    index,
                    args.workers,
                    args.test_threads,
                )
                for index in range(1, args.repeat + 1)
            ]
            report["profiles"][profile] = summarize(profile, samples)
    except RuntimeError as error:
        print(f"[benchmark] FAIL: {error}", file=sys.stderr)
        return 1

    if args.json is not None:
        output = args.json if args.json.is_absolute() else ROOT / args.json
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"[benchmark] json={output}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
