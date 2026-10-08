#!/usr/bin/env python3
"""Repeatable, host-only P0 performance baseline (no raw device access).

Run: uv run --locked python scripts/benchmark-p0.py [--compare old.json]
Output is under target/performance/; only --compare enforces optional regression limits.
"""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import io
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
TUI_ROW = re.compile(r'^count=(\d+) groups=(\d+) build_ms=([\d.]+) cached_snapshot_us=([\d.]+) frame_ms=([\d.]+) sort_ms=([\d.]+) filter_ms=([\d.]+) search_ms=([\d.]+)$', re.MULTILINE)


def read_tui(output: str) -> dict[str, dict[str, float | int]]:
    rows = {}
    for match in TUI_ROW.finditer(output):
        count, groups, *measurements = match.groups()
        rows[count] = dict(groups=int(groups), **dict(zip(
            ('build_ms', 'cached_snapshot_us', 'frame_ms', 'sort_ms', 'filter_ms', 'search_ms'),
            map(float, measurements), strict=True,
        )))
    if set(rows) != {'100', '1000', '10000'}:
        raise ValueError('missing 100/1000/10000 TUI measurements')
    return rows


def read_csv(output: str, required: set[str]) -> list[dict[str, str]]:
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    rows = list(csv.DictReader(io.StringIO('\n'.join(lines[:len(lines) - 1] if lines[-1].startswith('cancel_result=') else lines))))
    if not rows or not required.issubset(rows[0]):
        raise ValueError(f'benchmark CSV missing columns {required}')
    return rows


def regression_issues(current: dict, baseline: dict, ratio: float) -> list[str]:
    """Review noise-tolerant, stable read-side metrics; no hard hardware thresholds."""
    checks = (
        ('catalog', 'valid-1000', 'warm_p50_ms'),
        ('tui', '10000', 'frame_ms'),
        ('tui', '10000', 'sort_ms'),
        ('tui', '10000', 'search_ms'),
    )
    issues = []
    for section, index, metric in checks:
        new = float(current[section][index][metric])
        old = float(baseline[section][index][metric])
        if old > 0 and new > old * ratio and new - old > 5:
            issues.append(f'{section}[{index}].{metric}: {new:.3f}ms > {ratio}x {old:.3f}ms')
    return issues


def run(*args: str) -> str:
    completed = subprocess.run(args, cwd=ROOT, check=True, text=True,
                               encoding='utf-8', capture_output=True, errors='replace')
    return completed.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compare', type=Path, help='prior JSON from same host/toolchain')
    parser.add_argument('--max-ratio', type=float, default=2.0)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/performance/p0-current.json')
    args = parser.parse_args()
    if args.max_ratio <= 1:
        parser.error('--max-ratio must be >1')
    print('[P0] Running synthetic catalog, virtual-sector and TUI benchmarks', flush=True)
    catalog_output = run('cargo', 'run', '--locked', '--release', '--quiet', '--example', 'catalog_refresh_bench')
    catalog = read_csv(catalog_output, {'dataset', 'count', 'cold_ms', 'warm_p50_ms', 'peak_rss_kib'})
    catalog = {f'{row["dataset"]}-{row["count"]}': row for row in catalog}
    disk_output = run('cargo', 'run', '--locked', '--release', '--quiet', '--example', 'p0_disk_bench')
    disk = read_csv(disk_output, {'sectors', 'plan_ms', 'total_ms', 'peak_rss_kib'})
    tui_output = run('cargo', 'test', '--locked', '--lib', 'desktop_projection_benchmark', '--', '--ignored', '--nocapture')
    tui = read_tui(tui_output)
    mock_output = run('cargo', 'test', '--locked', '--lib', 'mock_device_scan_benchmark', '--', '--ignored', '--nocapture') if platform.system() == 'Darwin' else ''
    mock_match = re.search(r'mock_device_scan_disks=(\d+) loops=(\d+) avg_ms=([\d.]+)', mock_output)
    if platform.system() == 'Darwin' and mock_match is None:
        raise ValueError('synthetic device scan did not produce metrics')
    device_scan = (dict(disks=int(mock_match[1]), loops=int(mock_match[2]), average_ms=float(mock_match[3]))
                   if mock_match else {'status': 'macos-fixture-only'})
    timings = []
    for _ in range(9):
        started = time.perf_counter()
        run(str(ROOT / 'target/release/edpcli'), '--help')
        timings.append((time.perf_counter() - started) * 1000)
    report = {
        'schema': 1,
        'utc': datetime.now(timezone.utc).isoformat(),
        'git': run('git', 'rev-parse', 'HEAD').strip(),
        'system': dict(os=platform.platform(), processor=platform.machine(), python=platform.python_version()),
        'scope': {'physical_usb': False, 'device_scan': 'fixture-only, no physical discovery',
                  'inspect': '1,000 file-backed reads', 'provision': 'transaction on temporary regular file'},
        'startup_help_ms': dict(p50=statistics.median(timings), p95=max(timings), samples=timings),
        'catalog': catalog,
        'disk': {row['sectors']: row for row in disk},
        'device_scan_mock': device_scan,
        'tui': tui,
        'notes': ['Host-local measurements; debug TUI bench and release disk benchmarks are not cross-build comparable.',
                  'Manual --compare only: GitHub CI performance varies by shared runner load.',
                  'Memory from maxrss in child benchmarks; not an isolated stage peak.'],
    }
    issues = []
    if args.compare:
        old = json.loads(args.compare.read_text(encoding='utf-8'))
        # CSV catalog keys are intentionally domain-qualified, avoid false baseline comparisons.
        issues = regression_issues(report, old, args.max_ratio)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f'[P0] Saved {args.output}', flush=True)
    for count, metrics in tui.items():
        print(f'[P0] TUI {count}: build {metrics["build_ms"]:.2f}ms, sort {metrics["sort_ms"]:.2f}ms, search {metrics["search_ms"]:.2f}ms, frame {metrics["frame_ms"]:.2f}ms', flush=True)
    if mock_match:
        print(f'[P0] Simulated device discovery+probe (2 disks): {device_scan["average_ms"]:.4f}ms', flush=True)
    else:
        print('[P0] Synthetic diskutil discovery fixture is macOS-only; skipped', flush=True)
    print(f'[P0] CLI help p50={report["startup_help_ms"]["p50"]:.3f}ms', flush=True)
    for issue in issues:
        print(f'[P0] REGRESSION: {issue}', flush=True)
    return 1 if issues else 0


if __name__ == '__main__':
    raise SystemExit(main())
