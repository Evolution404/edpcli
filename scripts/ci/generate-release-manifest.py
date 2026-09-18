#!/usr/bin/env python3
"""Generate a machine-readable manifest for all release assets."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets-dir", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()

    directory = Path(args.assets_dir)
    output = Path(args.output)
    assets = []
    for path in sorted(directory.iterdir(), key=lambda item: item.name):
        if not path.is_file() or path.resolve() == output.resolve():
            continue
        assets.append(
            {
                "name": path.name,
                "size": path.stat().st_size,
                "sha256": sha256(path),
            }
        )

    manifest = {
        "schemaVersion": 1,
        "project": "edpcli",
        "tag": args.tag,
        "commit": args.commit,
        "rustToolchain": "1.98.1",
        "releaseRunners": {
            "macOS-arm64": "macos-15",
            "macOS-x86_64": "macos-15-intel",
            "macOS-universal": "merged from native arm64 + x86_64 artifacts on macos-15",
            "Linux-arm64": "ubuntu-24.04-arm",
            "Linux-x86_64": "ubuntu-24.04",
            "Windows-arm64": "windows-11-vs2026-arm",
            "Windows-x86_64": "windows-2025",
        },
        "assets": assets,
    }
    output.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
