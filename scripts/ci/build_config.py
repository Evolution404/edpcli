#!/usr/bin/env python3
"""Canonical toolchain/platform facts shared by CI and release generators."""
from __future__ import annotations
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from python_runtime import require_python
require_python()
import json
import os
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def toolchain() -> str:
    with (ROOT / "rust-toolchain.toml").open("rb") as handle:
        return tomllib.load(handle)["toolchain"]["channel"]


def platforms() -> dict:
    return json.loads((ROOT / ".github/release-platforms.json").read_text(encoding="utf-8"))


def matrices() -> dict:
    result = {"platforms": platforms()}
    for group in ("primary", "secondary"):
        result[group] = {"include": [
            {"os": value["runner"], "label": value["label"],
             **{key: value[key] for key in ("workers", "test_threads") if key in value}}
            for value in result["platforms"].values() if value["group"] == group
        ]}
    for system in ("linux", "windows"):
        result[system] = {"include": [
            {"os": value["runner"], "arch": key.removeprefix(system + "_")}
            for key, value in result["platforms"].items() if key.startswith(system + "_")
        ]}
    result["compatibility"] = {"include": [
        {"os": value["label"].split("-", 1)[0] + "-latest" if value["group"] == "primary" else value["runner"], "label": value["label"]}
        for value in result["platforms"].values() if value["group"] != "merge"
    ]}
    return result


def main() -> None:
    if destination := os.environ.get("GITHUB_ENV"):
        epoch = subprocess.check_output(["git", "show", "-s", "--format=%ct", "HEAD"], text=True).strip()
        with open(destination, "a", encoding="utf-8") as handle:
            handle.write(f"EDPCLI_RUST_TOOLCHAIN={toolchain()}\nSOURCE_DATE_EPOCH={epoch}\n")
    lines = [f"{key}={json.dumps(value, separators=(',', ':'))}" for key, value in matrices().items()]
    if destination := os.environ.get("GITHUB_OUTPUT"):
        with open(destination, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    else:
        print("\n".join(lines))


if __name__ == "__main__":
    main()
