#!/usr/bin/env python3
"""P2 local benchmark: isolated disposable image per process, no physical USB.

Run: uv run --locked python scripts/benchmark-p2.py [--samples 5]
Records baseline-compatible RSS delta and transaction duration; not a CI gate.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MEASURE = re.compile(r'^count=(\d+) plan_ms=([\d.]+) transaction_ms=([\d.]+) '
                     r'rss_after_plan_kib=(-?\d+) peak_rss_kib=(-?\d+) peak_delta_kib=(-?\d+)$')


def parse_measure(line: str) -> dict:
    match = MEASURE.fullmatch(line.strip())
    if not match:
        raise ValueError(f'missing P2 measurement: {line[:160]}')
    count, plan, transaction, after_plan, peak, delta = match.groups()
    values = dict(sectors=int(count), plan_ms=float(plan), transaction_ms=float(transaction),
                  rss_after_plan_kib=int(after_plan), peak_rss_kib=int(peak),
                  peak_delta_kib=int(delta))
    if values['peak_rss_kib'] >= 0 and values['rss_after_plan_kib'] >= 0:
        assert values['peak_rss_kib'] - values['rss_after_plan_kib'] == values['peak_delta_kib']
    return values


def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, capture_output=True, text=True,
                          encoding='utf-8', errors='replace', check=True).stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--output', type=Path,
                        default=ROOT / 'target/performance/p2-current.json')
    args = parser.parse_args()
    if not 1 <= args.samples <= 50:
        parser.error('--samples must be in 1..50')
    run('cargo', 'build', '--locked', '--release', '--example', 'p2_transaction_bench')
    exe = ROOT / 'target/release/examples/p2_transaction_bench'
    if platform.system() == 'Windows':
        exe = exe.with_suffix('.exe')
    sizes = {}
    for count in (32768, 65536):
        measurements = [parse_measure(run(str(exe), str(count))) for _ in range(args.samples)]
        sizes[str(count)] = {
            'samples': measurements,
            'transaction_ms_median': statistics.median(m['transaction_ms'] for m in measurements),
            'peak_delta_kib_median': statistics.median(m['peak_delta_kib'] for m in measurements),
        }
        print(f'[P2] {count} sectors: transaction median {sizes[str(count)]["transaction_ms_median"]:.2f}ms, RSS peak delta median {sizes[str(count)]["peak_delta_kib_median"]}KiB', flush=True)
    report = {
        'schema': 1,
        'utc': datetime.now(timezone.utc).isoformat(),
        'git': run('git', 'rev-parse', 'HEAD'),
        'system': platform.platform(),
        'scope': 'Only disposable file-backed transactions; no physical USB',
        'sizes': sizes,
        'caveat': 'Maximum RSS is process high-water, not instantaneous allocator usage; compare same host and toolchain only.',
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f'[P2] Report: {args.output}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
