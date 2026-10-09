#!/usr/bin/env python3
"""Run actual EDP/Plain multi-partition planning and budget checks in fresh processes.

No disk access. The RSS peak is process-wide high-water and is not allocator
live heap; samples are materialized planning only, not commit/rollback.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import statistics
import subprocess

ROOT=Path(__file__).resolve().parents[1]
CASES=('official-small','official-large','plain-large','budget-reject')
REQUIRED_OK={'scenario','status','selected_partitions','metadata_sectors','plan_sectors','estimate_payload_bytes','estimate_working_bytes','actual_image_payload_bytes','shared_images','encrypted_images','resource_ms','plan_ms','rss_start_kib','rss_estimate_kib','rss_plan_kib','rss_peak_kib'}
REQUIRED_REJECT={'scenario','status','metadata_sectors','estimate_payload_bytes','estimate_working_bytes','peak_rss_kib'}

def parse(line:str,case:str)->dict:
    row=dict(field.split('=',1) for field in line.strip().split())
    fields=REQUIRED_REJECT if case=='budget-reject' else REQUIRED_OK
    if set(row)!=fields or row['scenario']!=case or row['status']!=('rejected-before-alloc' if case=='budget-reject' else 'ok'):
        raise ValueError(f'invalid plan profile output for {case}: {row}')
    parsed={k:(v if k in ('scenario','status') else float(v)) for k,v in row.items()}
    if case=='budget-reject':
        if parsed['estimate_payload_bytes']<=64*1024*1024 or parsed['peak_rss_kib']>=32768:
            raise ValueError('budget rejection did not occur before materializing full image')
    else:
        if parsed['selected_partitions']!=3 or parsed['estimate_payload_bytes']!=parsed['metadata_sectors']*512:
            raise ValueError('multi-partition resource mismatch')
        if parsed['estimate_working_bytes']!=parsed['estimate_payload_bytes']*8:
            raise ValueError('resource budget multiplication changed')
        if case.startswith('official'):
            if parsed['plan_sectors']!=parsed['metadata_sectors'] or parsed['shared_images']!=1 or parsed['encrypted_images']!=2:
                raise ValueError('expected 1 alias and 2 distinct encrypted partition images')
        elif parsed['plan_sectors']!=parsed['metadata_sectors']+12:
            raise ValueError('Plain did not include 12 required metadata cleanup sectors')
    return parsed

def main()->int:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--samples',type=int,default=5)
    p.add_argument('--output',type=Path,default=ROOT/'target/performance/s4-multi-format.json')
    args=p.parse_args()
    if not 1<=args.samples<=10:p.error('samples must be 1..10')
    subprocess.run(['cargo','build','--release','--locked','--example','s4_multi_partition_memory_profile'],cwd=ROOT,check=True)
    binary=ROOT/'target/release/examples/s4_multi_partition_memory_profile'
    results={}
    for case in CASES:
        readings=[]
        for _ in range(args.samples):
            r=subprocess.run([str(binary),case],cwd=ROOT,capture_output=True,text=True,check=True,timeout=120)
            readings.append(parse(r.stdout,case))
        summary={'samples':readings}
        keys=('plan_ms','rss_peak_kib','metadata_sectors','estimate_working_bytes') if case!='budget-reject' else ('peak_rss_kib','metadata_sectors','estimate_working_bytes')
        summary['median']={k:statistics.median(x[k] for x in readings) for k in keys}
        results[case]=summary
        print(f'[MULTI] {case}: {summary["median"]}',flush=True)
    report={'schema':1,'git_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'utc':datetime.now(timezone.utc).isoformat(),'host':platform.platform(),'scope':'Real library planning and materialization; zero disk access; process-wide RSS high-water; fresh process each sample; no full device transaction','cases':results}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(f'[MULTI] Saved {args.output}')
    return 0
if __name__=='__main__':raise SystemExit(main())
