#!/usr/bin/env python3
"""Compatibility entrypoint for the repository-owned full test runner."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    return subprocess.call(
        [sys.executable, str(root / "scripts" / "test-full.py"), "--profile", "full"],
        cwd=root,
    )


if __name__ == "__main__":
    raise SystemExit(main())
