#!/usr/bin/env python3
"""Owned test-command watchdog, separate from post-run performance thresholds."""
from __future__ import annotations
import argparse
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BUDGETS = json.loads((ROOT / "scripts/test-budgets.json").read_text())


def terminate_tree(process: subprocess.Popen) -> None:
    if os.name == "posix":
        # Capture descendants before stopping the root; nested commands may own groups.
        table = subprocess.run(["ps", "-A", "-o", "pid=", "-o", "ppid="], capture_output=True, text=True, timeout=5, check=False)
        pairs = [tuple(map(int, row.split())) for row in table.stdout.splitlines() if len(row.split()) == 2]
        owned = {process.pid}
        while True:
            more = {pid for pid, parent in pairs if parent in owned}
            if more <= owned:
                break
            owned.update(more)
        for pid in owned - {process.pid}:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    else:
        # The root is still live here. /T walks its descendants before terminating it.
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10, check=False)
    if process.poll() is None:
        process.kill()
    process.wait(timeout=10)


def run_watchdog(command: list[str], deadline: float, phase: str) -> int:
    if not math.isfinite(deadline) or deadline <= 0:
        raise ValueError("deadline must be finite and positive")
    process = subprocess.Popen(command, cwd=ROOT, start_new_session=os.name == "posix")
    try:
        return process.wait(timeout=deadline)
    except subprocess.TimeoutExpired:
        print(f"[TIMEOUT] phase={phase} forced deadline={deadline:g}s", file=sys.stderr, flush=True)
        terminate_tree(process)
        return 124
    except BaseException:
        terminate_tree(process)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--budget-key", choices=tuple(BUDGETS))
    parser.add_argument("--deadline", type=float, default=float(os.environ.get("EDPCLI_GATE_DEADLINE_SECS", BUDGETS["gate_deadline_seconds"])))
    parser.add_argument("--phase", default="gate")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.budget_key:
        print(BUDGETS[args.budget_key])
        return 0
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required")
    return run_watchdog(command, args.deadline, args.phase)


if __name__ == "__main__":
    raise SystemExit(main())
