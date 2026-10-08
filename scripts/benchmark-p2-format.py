#!/usr/bin/env python3
"""Compare cloned vs borrowed sparse format transaction preparation on disposable file images.

Run: uv run --locked python scripts/benchmark-p2-format.py --samples 3
Each measurement is a fresh process. This runner never accesses USB device paths.
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

ROOT=Path(__file__).resolve().parents[1]
RESULT=re.compile(r'^variant=(materialized|borrowed) sectors=(\d+) prepare_ms=([\d.]+) transaction_ms=([\d.]+) image_rss_kib=(-?\d+) prepared_rss_kib=(-?\d+) peak_rss_kib=(-?\d+) peak_delta_kib=(-?\d+)$')


def parse(output: str)->dict:
    match=RESULT.fullmatch(output.strip())
    if not match:
        raise ValueError(f'Unexpected format benchmark result: {output[:160]}')
    variant,count,preparation,transaction,image,prepared,peak,delta=match.groups()
    values=dict(variant=variant,sectors=int(count),prepare_ms=float(preparation),
                transaction_ms=float(transaction),image_rss_kib=int(image),
                prepared_rss_kib=int(prepared),peak_rss_kib=int(peak),peak_delta_kib=int(delta))
    if int(peak)>=0:
        assert int(peak)-int(image)==int(delta)
    return values


def main()->int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples',type=int,default=3)
    parser.add_argument('--output',type=Path,default=ROOT/'target/performance/p2-format-current.json')
    args=parser.parse_args()
    if not 1<=args.samples<=12: parser.error('samples must be 1..12')
    subprocess.run(['cargo','build','--locked','--release','--example','p2_format_bench'],cwd=ROOT,check=True)
    executable=ROOT/'target/release/examples/p2_format_bench'
    if platform.system()=='Windows':executable=executable.with_suffix('.exe')
    report=dict(schema=1,created_utc=datetime.now(timezone.utc).isoformat(),host=platform.platform(),
                scope='synthetic sparse metadata + disposable ordinary files; no USB',samples=args.samples,results={})
    for count in (16384,32768,65536):
        group={}
        for variant in ('materialized','borrowed'):
            measurements=[]
            for _ in range(args.samples):
                result=subprocess.run([str(executable),variant,str(count)],cwd=ROOT,
                    capture_output=True,text=True,encoding='utf-8',check=True)
                measurements.append(parse(result.stdout))
            group[variant]=dict(samples=measurements,
                preparation_ms_p50=statistics.median(v['prepare_ms'] for v in measurements),
                transaction_ms_p50=statistics.median(v['transaction_ms'] for v in measurements),
                rss_delta_kib_p50=statistics.median(v['peak_delta_kib'] for v in measurements))
        report['results'][str(count)]=group
        before=group['materialized'];after=group['borrowed']
        print(f"[P2] {count} sectors: plan p50 {before['preparation_ms_p50']:.2f}->{after['preparation_ms_p50']:.2f}ms; RSS peak delta p50 {before['rss_delta_kib_p50']:.0f}->{after['rss_delta_kib_p50']:.0f}KiB; transaction p50 {before['transaction_ms_p50']:.2f}->{after['transaction_ms_p50']:.2f}ms",flush=True)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'[P2] Saved {args.output}')
    return 0

if __name__=='__main__':raise SystemExit(main())
