#!/usr/bin/env python3
"""Non-destructive PTY acceptance of production-rendered in-memory demo workflows.

Runs existing tui-replay.py (its process-group ownership + timeout safeguards),
never opens physical media. JSON captures remain in target/performance/pty-s2.
"""
from __future__ import annotations
from datetime import datetime, timezone
import json
from pathlib import Path
import argparse
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
MORE_SCENES = (
    'device-edp', 'device-plain', 'inspect-elabel-expanded',
    'inspect-sector-raw', 'inspect-sector-decode', 'inspect-sector-meta',
    'provision-select', 'provision-running', 'provision-running-long',
    'provision-result-warning', 'provision-result-failure',
    'provision-result-partial', 'provision-result-rollback-failure',
    'backup-detail', 'backup-coverage', 'backup-verify-running',
    'empty-state', 'overview',
)


def steps_for(keys):
    # Quit must come last: otherwise the process exits before PTY resize is
    # exercised (the older S2 acceptance claimed to cover it but did not).
    if not keys or keys[-1] != 'q':
        raise ValueError('PTY test keys must end with q')
    steps=[{'wait':0.3}]
    for key in keys[:-1]:
        steps.append({'keys':key,'wait':0.15})
    steps += [
        {'resize':[40,12], 'wait':0.3},
        {'resize':[160,45], 'wait':0.3},
        {'keys':'q', 'wait':0.3},
    ]
    return steps


def check_capture(manifest, capture, steps, scene):
    frames = manifest['frames']
    if len(frames) != len(steps):
        raise ValueError(f'{scene}: process stopped before finishing PTY resize or quit')
    offsets = [frame['ansi_offset'] for frame in frames]
    if any(b<a for a,b in zip(offsets,offsets[1:])):
        raise ValueError(f'{scene}: ANSI frames not monotonic')
    stream = capture.read_bytes()
    if offsets[-1] != len(stream) or not stream or offsets[0] < 200:
        raise ValueError(f'{scene}: empty/stale terminal capture')
    # Verify actual terminal output occurred during both viewport changes;
    # a manifest merely recording the steps is insufficient.
    for index in [-3,-2]:
        previous = offsets[index-1]
        current = offsets[index]
        if current <= previous:
            raise ValueError(f'{scene}: viewport change produced no redraw')
    return offsets[-3]-offsets[-4], offsets[-2]-offsets[-3]


def main()->int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--all-scenes',action='store_true',help='exercise all production demo fixtures')
    parser.add_argument('--idle-seconds',type=int,default=0,
        help='additional idle seconds to measure idle CPU (0..30)')
    args=parser.parse_args()
    if not 0 <= args.idle_seconds <= 30:parser.error('--idle-seconds must be 0..30')
    if sys.platform=='win32':
        print('S2 PTY is macOS/Linux only; rely on TUI pure render/interaction CI on Windows')
        return 0
    OUTPUT.mkdir(parents=True,exist_ok=True)
    results=[]
    cases=dict(CASES)
    if args.all_scenes:
        for scene in MORE_SCENES:
            cases[scene]=('120x36',['j','\x1b','q'])
    for scene,(size,keys) in cases.items():
        steps=steps_for(keys)
        if args.idle_seconds and scene == 'devices':
            # Keep both resize steps at -3/-2 for exact frame assertions.
            steps.insert(-3, {'wait': args.idle_seconds})
        path=OUTPUT/f'{scene}-steps.json'
        path.write_text(json.dumps(steps),encoding='utf-8')
        completed=subprocess.run([sys.executable,str(REPLAY),'--binary',str(BINARY),
            '--scene',scene,'--size',size,'--steps',str(path),
            '--output',str(OUTPUT),'--timeout','12'],capture_output=True,text=True,timeout=28)
        if not completed.stdout.strip():
            raise RuntimeError(f'{scene} replay did not produce a manifest: {completed.stderr[-400:]}')
        manifest=json.loads(Path(completed.stdout.strip()).read_text())
        capture=Path(completed.stdout.strip()).with_name('session.ansi')
        narrow_bytes,wide_bytes=check_capture(manifest,capture,steps,scene)
        result=dict(scene=scene,exit_code=manifest['exit_code'],timed_out=manifest['timed_out'],
            elapsed_seconds=manifest['elapsed_seconds'],child_cpu_seconds=manifest['child_cpu_seconds'],
            frames=len(manifest['frames']),ansi_bytes=capture.stat().st_size,
            narrow_redraw_bytes=narrow_bytes,wide_redraw_bytes=wide_bytes,
            sha256=manifest['sha256'])
        if args.idle_seconds and scene == 'devices':
            result['idle_cpu_to_wall_fraction']=manifest['child_cpu_seconds']/manifest['elapsed_seconds']
            if result['idle_cpu_to_wall_fraction'] > 0.20:
                raise RuntimeError(f'{scene}: excessive CPU during idle {result["idle_cpu_to_wall_fraction"]:.1%}')
        results.append(result)
        print(f'[S2] {scene}: exit={result["exit_code"]} timeout={result["timed_out"]} frames={result["frames"]} captured={result["ansi_bytes"]}B',flush=True)
        if completed.returncode or result['timed_out'] or result['exit_code'] != 0 or result['ansi_bytes'] < 200:
            raise RuntimeError(f'{scene} PTY acceptance failed: {result}, {completed.stderr[-300:]}')
    report=dict(schema=2,created_utc=datetime.now(timezone.utc).isoformat(),
        scope='production renderer on in-memory demo scenes, no live USB',
        all_scenes=args.all_scenes,idle_seconds=args.idle_seconds,results=results)
    path=OUTPUT/'s2-acceptance-summary.json'
    path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(f'[S2] {len(results)} / {len(cases)} PTY scenes passed; {path}')
    return 0

if __name__=='__main__':raise SystemExit(main())
