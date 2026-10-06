#!/usr/bin/env python3
"""Generate a machine-readable manifest for all release assets."""

from __future__ import annotations

from build_config import ROOT, platforms, toolchain

import argparse
import hashlib
import json
import re
import tomllib
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
    with (ROOT / "Cargo.toml").open("rb") as handle:
        version = tomllib.load(handle)["package"]["version"]
    if args.tag != f"v{version}" or not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        raise SystemExit("Tag/version or commit identity is invalid")
    prefix = f"edpcli-{args.tag}-"
    if output.absolute() != (directory / (prefix + "release-manifest.json")).absolute() or output.is_symlink():
        raise SystemExit("Manifest output must be the expected regular release path")
    archives = [f"{prefix}{p['label']}.{p['archive']}" for p in platforms().values()]
    expected = set(archives + [name + ".sha256" for name in archives] + [
        prefix + suffix for suffix in ("Cargo.lock", "cargo-metadata.json", "rust-toolchain.txt", "sbom.cdx.json")
    ])
    entries = {path.name: path for path in directory.iterdir() if path.absolute() != output.absolute()}
    if set(entries) != expected:
        raise SystemExit(f"Asset set mismatch: missing={sorted(expected - entries.keys())}, extra={sorted(entries.keys() - expected)}")
    if any(path.is_symlink() or not path.is_file() or path.stat().st_size == 0 for path in entries.values()):
        raise SystemExit("All assets must be nonempty regular files without symlinks")
    for name in archives:
        sidecar = entries[name + ".sha256"].read_text(encoding="ascii").strip()
        match = re.fullmatch(r"([0-9a-fA-F]{64}) [ *](.+)", sidecar)
        if not match or match[2] != name or match[1].lower() != sha256(entries[name]):
            raise SystemExit(f"Independent SHA-256 verification failed: {name}")
    sbom = json.loads(entries[prefix + "sbom.cdx.json"].read_text(encoding="utf-8"))
    properties = {p["name"]: p["value"] for p in sbom["metadata"]["properties"]}
    if properties.get("git:commit") != args.commit or properties.get("build:rust-toolchain") != toolchain():
        raise SystemExit("SBOM commit/toolchain differs from release")
    if sbom["metadata"]["component"]["name"] != "edpcli" or sbom["metadata"]["component"]["version"] != version:
        raise SystemExit("SBOM application/version differs from release")
    metadata = json.loads(entries[prefix + "cargo-metadata.json"].read_text(encoding="utf-8"))
    members = set(metadata["workspace_members"])
    roots = [p for p in metadata["packages"] if p["id"] in members and p["name"] == "edpcli"]
    if len(roots) != 1 or roots[0]["version"] != version:
        raise SystemExit("Cargo metadata application/version differs from release")
    if sha256(entries[prefix + "Cargo.lock"]) != sha256(ROOT / "Cargo.lock"):
        raise SystemExit("Published Cargo.lock differs from source")
    compiler = entries[prefix + "rust-toolchain.txt"].read_text(encoding="utf-8")
    if not compiler.startswith("rustc " + toolchain() + " "):
        raise SystemExit("Compiler metadata differs from pinned toolchain")
    assets = [{"name": path.name, "size": path.stat().st_size, "sha256": sha256(path)}
              for path in sorted(entries.values(), key=lambda path: path.name)]

    manifest = {
        "schemaVersion": 1,
        "project": "edpcli",
        "tag": args.tag,
        "commit": args.commit,
        "rustToolchain": toolchain(),
        "releaseRunners": {p["label"]: p["runner"] for p in platforms().values()},
        "assets": assets,
    }
    output.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
