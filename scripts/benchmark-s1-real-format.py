#!/usr/bin/env python3
"""Measure actual FAT16/FAT32/exFAT format metadata + SM4 on disposable file images.

Each sample launches a fresh release executable process; no USB paths are opened.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CASES = (
    ('fat16', 20_417), ('fat16', 262_144), ('fat16', 1_048_576),
    ('fat32', 262_144), ('fat32', 16_777_216), ('fat32', 67_108_864),
    ('exfat', 32_768), ('exfat', 524_288), ('exfat', 16_777_216),
    ('exfat', 1_000_000_000),
)
METRICS = ('build_ms', 'transform_ms', 'verify_ms', 'prepare_ms', 'transaction_ms',
           'rss_before_kib', 'rss_build_kib', 'rss_transform_kib', 'rss_peak_kib')


def parse(line: str) -> dict:
    tokens = line.strip().split(' ')
    values = dict(token.split('=', 1) for token in tokens)
    required = {'fs','volume','mode','metadata_sectors','payload_bytes','estimate_working_bytes', *METRICS}
    if values.keys() != required:
        raise ValueError(f'Unexpected S1 bench fields: {sorted(values.keys())}')
    for field in ('volume','metadata_sectors','payload_bytes','estimate_working_bytes'):
        values[field] = int(values[field])
    for field in METRICS:
        values[field] = float(values[field])
    if values['metadata_sectors'] * 512 != values['payload_bytes']:
        raise ValueError('metadata sector payload mismatch')
    if values['payload_bytes'] * 8 != values['estimate_working_bytes']:
        raise ValueError('resource estimate changed unexpectedly')
    return values


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples',type=int,default=3)
    parser.add_argument('--output',type=Path,default=ROOT/'target/performance/s1-real-format-current.json')
    args = parser.parse_args()
    if not 1 <= args.samples <= 8:
        parser.error('--samples must be 1..8')
    subprocess.run(['cargo','build','--locked','--release','--example','s1_real_filesystem_bench'],
        cwd=ROOT,check=True)
    binary = ROOT/'target/release/examples/s1_real_filesystem_bench'
    if platform.system() == 'Windows': binary = binary.with_suffix('.exe')
    report = dict(schema=1, created_utc=datetime.now(timezone.utc).isoformat(),
        host=platform.platform(),git_sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        samples=args.samples,scope='real format driver metadata; ordinary disposable file, not raw USB',
        measurements=[])
    for fs,volume in CASES:
        for mode in ('plain','sm4'):
            results=[]
            for _ in range(args.samples):
                completed=subprocess.run([str(binary),fs,str(volume),mode],cwd=ROOT,
                    capture_output=True,text=True,check=False,timeout=180)
                if completed.returncode:
                    raise RuntimeError(f'Format {fs}/{volume}/{mode} failed: {completed.stderr[-900:]}')
                results.append(parse(completed.stdout))
            summary = dict(fs=fs,volume=volume,mode=mode,samples=results)
            summary['p50']={key:statistics.median(float(r[key]) for r in results) for key in METRICS}
            summary['metadata_sectors']=results[0]['metadata_sectors']
            summary['payload_bytes']=results[0]['payload_bytes']
            report['measurements'].append(summary)
            p50=summary['p50']
            print(f'[S1] {fs:5} {volume:>10} sectors {mode:5} metadata={summary["metadata_sectors"]:>7} build={p50["build_ms"]:.2f}ms SM4={p50["transform_ms"]:.2f}ms transaction={p50["transaction_ms"]:.2f}ms RSS={p50["rss_peak_kib"]:.0f}KiB',flush=True)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'[S1] Report: {args.output}')
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
