#!/usr/bin/env python3
"""Bounded, versioned PTY captures; cleanup is restricted to the spawned session."""
import argparse
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import resource
import selectors
import signal
import struct
import subprocess
import termios
import time
from datetime import datetime


READ_ONLY_KEYS = {"q", "\x1b", "\t", "j", "k", "g", "G", "r", "i", ":devices\r", ":backups\r"}


def parse_size(value):
    width, height = map(int, value.lower().split("x"))
    if not (20 <= width <= 1000 and 10 <= height <= 300):
        raise ValueError("size must be within 20x10..1000x300")
    return width, height


def load_steps(path, live):
    steps = json.loads(Path(path).read_text()) if path else [{"wait": 3}, {"keys": "q", "wait": 1}]
    if not isinstance(steps, list) or not steps:
        raise ValueError("steps must be a nonempty JSON array")
    for step in steps:
        if set(step) - {"keys", "wait", "resize"}:
            raise ValueError("unknown step field")
        if not 0 <= step.get("wait", 1) <= 60:
            raise ValueError("each wait must be within 0..60 seconds")
        if live and step.get("keys", "q") not in READ_ONLY_KEYS:
            raise ValueError("live capture only allows read-only navigation; write actions are excluded")
        if "resize" in step:
            parse_size("x".join(map(str, step["resize"])))
    return steps


def run(args):
    binary = Path(args.binary).expanduser().resolve(strict=True)
    width, height = parse_size(args.size)
    steps = load_steps(args.steps, args.live_read_only)
    if args.sudo and not args.live_read_only:
        raise ValueError("--sudo is only for explicitly selected live read-only capture")
    run_id = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"
    base = Path(args.output).expanduser() if args.output else Path.home() / ".local/state/edpcli/tui-replay"
    output = base / run_id
    output.mkdir(parents=True, exist_ok=False)
    version = subprocess.run([str(binary), "version", "--verbose"], capture_output=True, text=True, timeout=10, check=True).stdout
    metadata = {"run_id": run_id, "binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "version": version, "size": [width, height], "mode": "live-read-only" if args.live_read_only else "demo", "scene": args.scene, "steps": steps, "timeout_seconds": args.timeout, "pid": None, "exit_code": None, "timed_out": False, "frames": []}
    manifest = output / "manifest.json"
    def save():
        manifest.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
    save()
    master, slave = pty.openpty()
    selector = selectors.DefaultSelector()
    process = None
    raw = None
    pending = b""
    def resize(size):
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", size[1], size[0], 0, 0))
    resize((width, height))
    def setup():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    def terminate_owned():
        if process is None or process.poll() is not None:
            return
        os.write(master, b"q")
        try:
            process.wait(timeout=2)
            return
        except subprocess.TimeoutExpired:
            pass
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(process.pid, sig)
            except ProcessLookupError:
                return
            except PermissionError:
                if not args.sudo:
                    raise
                subprocess.run(["sudo", "-n", "kill", f"-{sig.value}", "--", f"-{process.pid}"], timeout=5, check=True)
            try:
                process.wait(timeout=2)
                return
            except subprocess.TimeoutExpired:
                pass
    started = time.monotonic()
    deadline = started + args.timeout
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    try:
        command = [str(binary)] + (["tui"] if args.live_read_only else ["demo", "--scene", args.scene])
        if args.sudo:
            command = ["sudo", "-n"] + command
        env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor")
        env.pop("NO_COLOR", None)
        process = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave, preexec_fn=setup, env=env)
        metadata["pid"] = process.pid
        save()
        os.close(slave)
        slave = None
        selector.register(master, selectors.EVENT_READ)
        raw = (output / "session.ansi").open("wb")
        for index, step in enumerate(steps):
            if "resize" in step:
                resize(step["resize"])
            if "keys" in step:
                os.write(master, step["keys"].encode())
            end = min(deadline, time.monotonic() + step.get("wait", 1))
            while time.monotonic() < end:
                if not selector.select(min(0.1, end - time.monotonic())):
                    continue
                try:
                    data = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
                if not data:
                    break
                raw.write(data)
                pending += data
                # Terminal capability/cursor queries can arrive across read boundaries.
                for query, reply in [(b"\x1b[6n", b"\x1b[1;1R"), (b"\x1b[c", b"\x1b[?1;2c"), (b"\x1b[?u", b"\x1b[?0u")]:
                    occurrences = pending.count(query)
                    if occurrences:
                        os.write(master, reply * occurrences)
                        pending = pending.replace(query, b"")
                pending = pending[-16:]
            raw.flush()
            metadata["frames"].append({"step": index, "elapsed_seconds": time.monotonic() - started, "ansi_offset": raw.tell()})
            save()
            if process.poll() is not None:
                break
            if time.monotonic() >= deadline:
                metadata["timed_out"] = True
                break
    finally:
        try:
            terminate_owned()
        finally:
            if process is not None:
                metadata["exit_code"] = process.poll()
            usage = resource.getrusage(resource.RUSAGE_CHILDREN)
            metadata["child_cpu_seconds"] = (usage.ru_utime - usage_before.ru_utime) + (usage.ru_stime - usage_before.ru_stime)
            metadata["elapsed_seconds"] = time.monotonic() - started
            if raw:
                raw.close()
            selector.close()
            os.close(master)
            if slave is not None:
                os.close(slave)
            save()
    print(manifest)
    return 124 if metadata["timed_out"] else (metadata["exit_code"] or 0)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="~/.local/bin/edpcli")
    parser.add_argument("--scene", default="devices")
    parser.add_argument("--size", default="200x60")
    parser.add_argument("--steps", help="JSON array: keys/wait/resize")
    parser.add_argument("--output", help="parent directory; each run has a unique child directory")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--live-read-only", action="store_true")
    parser.add_argument("--sudo", action="store_true")
    parsed = parser.parse_args()
    if not 1 <= parsed.timeout <= 600:
        parser.error("timeout must be within 1..600 seconds")
    raise SystemExit(run(parsed))
