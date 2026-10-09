#!/usr/bin/env python3
"""Bounded, non-destructive key-to-first-PTY-output latency from real demo TUI.

Not a semantic screen/keypress P95: a PTY response may contain terminal control
bytes or an unrelated repaint. Every sample also requires ANSI frame growth.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import json
import platform
from pathlib import Path
import statistics
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
SCENES=('devices', 'backups', 'provision-review', 'inspect-lba8')

def percentile(samples:list[float], percentile:float)->float:
    if not samples:raise ValueError('cannot calculate latency from zero samples')
    ordered=sorted(samples)
    rank=(len(ordered)-1)*percentile
    lower=int(rank)
    return ordered[lower]+(ordered[min(lower+1,len(ordered)-1)]-ordered[lower])*(rank-lower)

def samples_for(manifest:dict, scene:str, metric:str="first_pty_output_ms")->list[float]:
    frames=manifest['frames'];steps=manifest['steps']
    if len(frames)!=len(steps) or manifest['exit_code'] != 0 or manifest['timed_out']:
        raise ValueError(f'{scene}: incomplete PTY replay')
    result=[]
    for i,step in enumerate(steps):
        if step.get('keys') in ('j','k'):
            frame=frames[i]
            if frame['ansi_offset']<=frames[i-1]['ansi_offset']:
                raise ValueError(f'{scene}: key {step["keys"]} had no PTY output')
            response=frame.get(metric)
            if response is None or response < 0 or response > 1000*step['wait']+150:
                raise ValueError(f'{scene}: missing/invalid {metric} for key {step["keys"]}: {response}')
            result.append(response)
    return result

def main()->int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repeats',type=int,default=5)
    parser.add_argument('--output',type=Path,default=ROOT/'target/performance/tui-latency.json')
    args=parser.parse_args()
    if not 2<=args.repeats<=15:parser.error('--repeats should be 2..15')
    if sys.platform=='win32':
        print('PTY benchmarks are unix-only');return 0
    bin=ROOT/'target/release/edpcli'
    if not bin.is_file():raise RuntimeError('first build release edpcli')
    output=ROOT/'target/performance/tui-latency-pty'
    output.mkdir(parents=True,exist_ok=True)
    steps=[{'wait':0.5}]+[{'keys':key,'wait':0.30} for key in ('j','k','j','k','j','k')]+[{'keys':'q','wait':0.3}]
    stepfile=output/'steps.json';stepfile.write_text(json.dumps(steps))
    all_results={}
    for scene in SCENES:
        values=[];settled_values=[];runs=[]
        for rep in range(args.repeats):
            cmd=[sys.executable,str(ROOT/'scripts/tui-replay.py'),'--binary',str(bin),'--scene',scene,'--size','120x36','--steps',str(stepfile),'--output',str(output),'--timeout','6']
            completed=subprocess.run(cmd,capture_output=True,text=True,timeout=12)
            if completed.returncode:raise RuntimeError(f'{scene} #{rep}: {completed.stderr[-550:]}')
            path=Path(completed.stdout.strip());manifest=json.loads(path.read_text())
            samples=samples_for(manifest,scene)
            settled=samples_for(manifest,scene,'settled_pty_burst_ms')
            if any(after < before for before,after in zip(samples,settled)):
                raise ValueError(f'{scene}: settled output cannot precede first output')
            runs.append({'manifest':str(path.relative_to(ROOT)),'ms':samples,'settled_ms':settled})
            values.extend(samples)
            settled_values.extend(settled)
        all_results[scene]={'samples':len(values),'p50_ms':round(statistics.median(values),3),'p95_ms':round(percentile(values,.95),3),'max_ms':round(max(values),3),'raw_ms':values,'settled_p50_ms':round(statistics.median(settled_values),3),'settled_p95_ms':round(percentile(settled_values,.95),3),'settled_raw_ms':settled_values,'runs':runs}
        row=all_results[scene]
        print(f'[PTY] {scene}: samples={row["samples"]} first-output p50={row["p50_ms"]:.2f}ms p95={row["p95_ms"]:.2f}ms settled-burst p50={row["settled_p50_ms"]:.2f}ms p95={row["settled_p95_ms"]:.2f}ms',flush=True)
    report={'schema':1,'created_utc':datetime.now(timezone.utc).isoformat(),'git_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'host':platform.platform(),'scope':'demo mode, real PTY; first-output and last-output-before-20ms-quiescence include scheduler and reader delay; neither proves semantic completion or physical devices','repeats':args.repeats,'results':all_results}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(f'[PTY] Saved {args.output}')
    return 0
if __name__=='__main__':raise SystemExit(main())
