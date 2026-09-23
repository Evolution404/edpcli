#!/usr/bin/env python3
"""Run the release test gate and surface failures as a GitHub annotation.

GitHub's public Checks API exposes annotations even when raw Actions logs require
authentication.  Keep this wrapper platform-neutral so Windows/macOS/Linux run
the exact same Cargo command while a failed matrix job remains diagnosable.
"""

from __future__ import annotations

from collections import deque
import subprocess
import sys


COMMAND = ["cargo", "test", "--all-targets", "--locked"]


def github_escape(value: str) -> str:
    return (
        value.replace("%", "%25")
        .replace("\r", "%0D")
        .replace("\n", "%0A")
    )


def main() -> int:
    tail: deque[str] = deque(maxlen=120)
    interesting: deque[str] = deque(maxlen=200)
    process = subprocess.Popen(
        COMMAND,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
    )
    assert process.stdout is not None
    keywords = (
        "FAILED",
        "failures:",
        "panicked at",
        "error:",
        "test result:",
        "assertion",
    )
    for line in process.stdout:
        print(line, end="", flush=True)
        clean = line.rstrip("\r\n")
        tail.append(clean)
        if any(key in clean for key in keywords):
            interesting.append(clean)

    code = process.wait()
    if code == 0:
        return 0

    diagnostic_lines = (list(interesting)[-100:] + list(tail)[-60:])[-140:]
    diagnostic = "\n".join(diagnostic_lines)
    if len(diagnostic) > 12_000:
        diagnostic = diagnostic[-12_000:]
    print(
        "::error title=cargo test --all-targets --locked failed::"
        + github_escape(diagnostic),
        flush=True,
    )
    return code


if __name__ == "__main__":
    raise SystemExit(main())
