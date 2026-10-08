#!/usr/bin/env python3
"""Non-destructive PTY acceptance of production-rendered in-memory demo workflows.

Runs existing tui-replay.py (its process-group ownership + timeout safeguards),
never opens physical media. JSON captures remain in target/performance/pty-s2.
"""
from __future__ import annotations
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
OUTPUT=ROOT/'target/performance/pty-s2'
REPLAY=ROOT/'scripts/tui-replay.py'
BINARY=ROOT/'target/release/edpcli'
ESC='\x1b'
CASES={
    'devices': ('120x36',["j","i",ESC,"j",ESC,"q"]),
    'backups': ('120x36',["j","\t",ESC,"j",ESC,"q"]),
    'inspect-lba8': ('100x30',["J","8","\r",ESC,ESC,"q"]),
    'provision-form': ('120x36',["\t","j",ESC,ESC,"q"]),
    'provision-review': ('120x36',[ESC,"\t",ESC,"q"]),
    'provision-result-success': ('120x36',["j","\t",ESC,"q"]),
    'error-state': ('80x24',["\x1bOQ",ESC,"q"]),
}


def main()->int:
    if sys.platform=='win32':
        print('S2 PTY is macOS/Linux only; rely on TUI pure render/interaction CI on Windows')
        return 0
    OUTPUT.mkdir(parents=True,exist_ok=True)
    results=[]
    for scene,(size,keys) in CASES.items():
        steps=[{'wait':0.3}]
        for key in keys:steps.append({'keys':key,'wait':0.15})
        steps += [{'resize':[40,12],'wait':0.25},{'resize':[160,45],'wait':0.25}]
        path=OUTPUT/f'{scene}-steps.json'
        path.write_text(json.dumps(steps),encoding='utf-8')
        completed=subprocess.run([sys.executable,str(REPLAY),'--binary',str(BINARY),
            '--scene',scene,'--size',size,'--steps',str(path),
            '--output',str(OUTPUT),'--timeout','12'],capture_output=True,text=True,timeout=28)
        if not completed.stdout.strip():
            raise RuntimeError(f'{scene} replay did not produce a manifest: {completed.stderr[-400:]}')
        manifest=json.loads(Path(completed.stdout.strip()).read_text())
        capture=Path(completed.stdout.strip()).with_name('session.ansi')
        result=dict(scene=scene,exit_code=manifest['exit_code'],timed_out=manifest['timed_out'],
            elapsed_seconds=manifest['elapsed_seconds'],child_cpu_seconds=manifest['child_cpu_seconds'],
            frames=len(manifest['frames']),ansi_bytes=capture.stat().st_size,
            sha256=manifest['sha256'])
        results.append(result)
        print(f'[S2] {scene}: exit={result["exit_code"]} timeout={result["timed_out"]} frames={result["frames"]} captured={result["ansi_bytes"]}B',flush=True)
        if completed.returncode or result['timed_out'] or result['exit_code'] != 0 or result['ansi_bytes'] < 200:
            raise RuntimeError(f'{scene} PTY acceptance failed: {result}, {completed.stderr[-300:]}')
    report=dict(schema=1,created_utc=datetime.now(timezone.utc).isoformat(),
        scope='production renderer on in-memory demo scenes, no live USB',results=results)
    path=OUTPUT/'s2-acceptance-summary.json'
    path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(f'[S2] {len(results)} / {len(CASES)} PTY scenes passed; {path}')
    return 0

if __name__=='__main__':raise SystemExit(main())
