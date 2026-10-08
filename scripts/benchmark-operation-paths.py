#!/usr/bin/env python3
"""Repeat bounded production transaction code on disposable regular files.

Measures stage-level timing, I/O calls and backup catalog. In `io_limit=1`
mode, only the interface is emulated; physical USB throughput is NOT measured.
"""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import io
import json
from pathlib import Path
import platform
import statistics
import subprocess

ROOT=Path(__file__).resolve().parents[1]
NUMERIC_TRANSACTION={'sectors','io_limit','plan_ms','preflight_ms','mirror_ms','write_ms','sync_ms','readback_ms','total_ms','single_reads','batch_reads','single_writes','batch_writes','syncs'}
NUMERIC_CATALOG={'count','cold_ms','cold_bytes','warm_p50_ms','warm_p95_ms','warm_bytes','warm_hits','peak_rss_kib'}

def strict_csv(data:str, fields:set[str], tag:str)->list[dict]:
    rows=list(csv.DictReader(io.StringIO(data)))
    if not rows or set(rows[0])!=fields:
        raise ValueError(f'{tag}: mismatched schema: {set(rows[0]) if rows else None}')
    parsed=[]
    for row in rows:
        if set(row)!=fields or any(x is None or x=='' for x in row.values()):
            raise ValueError(f'{tag}: partial CSV record')
        parsed.append({k:(v if k in ('shape','dataset') else float(v)) for k,v in row.items()})
    return parsed

def validate_transaction(rows:list[dict])->None:
    combos={(int(x['sectors']),x['shape'],int(x['io_limit'])) for x in rows}
    if len(rows)!=16 or len(combos)!=16:
        raise ValueError('expected 16 distinct transactions')
    for row in rows:
        assert row['syncs']==2, 'transaction must sync preflight and post-write'
        assert row['single_reads']+row['batch_reads']>0, 'missing rollback mirror/readback'
        assert row['single_writes']+row['batch_writes']>0, 'missing writes'
        if row['shape']=='strided' or row['io_limit']==1:
            assert row['batch_reads']==row['batch_writes']==0, 'unsafe batch crossing gap or disabled'
        if row['io_limit']==1:
            assert row['single_reads']==(row['sectors']+1)*2, 'mirror + readback cardinality'
            assert row['single_writes']==row['sectors']+1, 'exact data + commit writes'

def run(*args:str)->str:
    return subprocess.run(args,cwd=ROOT,text=True,capture_output=True,check=True,timeout=150).stdout

def main()->int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples',type=int,default=3)
    parser.add_argument('--output',type=Path,default=ROOT/'target/performance/operation-paths.json')
    args=parser.parse_args()
    if not 1<=args.samples<=8:parser.error('--samples must be 1..8')
    run('cargo','build','--locked','--release','--example','s3_transaction_io_profile','--example','catalog_refresh_bench')
    io_records=[]; catalog_records=[]
    for sample in range(args.samples):
        raw=run(str(ROOT/'target/release/examples/s3_transaction_io_profile'))
        transaction=strict_csv(raw,NUMERIC_TRANSACTION|{'shape'},'transaction')
        validate_transaction(transaction)
        for row in transaction:row['sample']=sample;io_records.append(row)
        cat=run(str(ROOT/'target/release/examples/catalog_refresh_bench'))
        parts=cat.strip().splitlines()
        if not parts[-1].startswith('cancel_result=') or '目录扫描已取消' not in parts[-1]:
            raise ValueError('backup catalog cancel safety check missing')
        catalog=strict_csv('\n'.join(parts[:-1]),NUMERIC_CATALOG|{'dataset'},'catalog')
        if len(catalog)!=6 or any(x['warm_bytes']!=0 or x['warm_hits']!=x['count'] for x in catalog):
            raise ValueError('warm catalog cache validation failed')
        for row in catalog:row['sample']=sample;catalog_records.append(row)
        print(f'[OPS] sample {sample+1}/{args.samples} valid: 16 transactions, 6 backup datasets, cancellation',flush=True)
    report=dict(schema=1,created_utc=datetime.now(timezone.utc).isoformat(),git_sha=run('git','rev-parse','HEAD').strip(),host=platform.platform(),samples=args.samples,
                scope='real production transaction executor, simulated max=1 vs max=128 on disposable files; backup catalog synthetic files; no raw USB device',transactions=io_records,catalog=catalog_records)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    for shape in ('contiguous','strided'):
        for batch in (1,128):
            samples=[r['total_ms'] for r in io_records if r['sectors']==16384 and r['shape']==shape and r['io_limit']==batch]
            print(f'[OPS] 16384 sectors {shape} batch {batch}: median {statistics.median(samples):.2f}ms',flush=True)
    for dataset in ('valid','mixed'):
        rows=[r for r in catalog_records if r['dataset']==dataset and r['count']==1000]
        print(f'[OPS] catalog {dataset}/1000 cold median={statistics.median(r["cold_ms"] for r in rows):.2f}ms warm median={statistics.median(r["warm_p50_ms"] for r in rows):.2f}ms',flush=True)
    print(f'[OPS] saved {args.output}')
    return 0
if __name__=='__main__':raise SystemExit(main())
