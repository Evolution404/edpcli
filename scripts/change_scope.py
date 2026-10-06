#!/usr/bin/env python3
"""One path contract shared by CI routing and the local fast runner."""
from __future__ import annotations
import argparse
from fnmatch import fnmatchcase
import sys

PROTOCOL = ("audit/protocol/*", "scripts/protocol/*", "docs/protocol/*", "docs/EDP_PROTOCOL*", "src/protocol/*", "tests/protocol*", "tests/fixtures/protocol/*", "backup/*")
RUST = ("docs/ui/*", "docs/backup/*","src/*", "tests/*", "scripts/*", "Cargo.toml", "Cargo.lock", "rust-toolchain*", ".cargo/*", ".github/workflows/*", "build.rs", "backup/*")
DEPS = (".github/release-platforms.json", "Cargo.toml", "Cargo.lock", "deny.toml", ".github/dependabot.yml", ".github/workflows/*")

def matches(path: str, patterns: tuple[str, ...]) -> bool:
    return any(fnmatchcase(path, pattern) for pattern in patterns)

def classify(paths: list[str]) -> dict[str, bool]:
    result = {key: any(matches(path, patterns) for path in paths) for key, patterns in (("rust", RUST), ("protocol", PROTOCOL), ("deps", DEPS))}
    # Unknown source/executable paths fail closed, including deleted files.
    harmless = ("docs/*", "audit/*", "*.md", "LICENSE*", ".gitignore", ".gitattributes")
    result["rust"] |= any(not matches(path, harmless) for path in paths)
    return result

def documentation_suites(path: str) -> set[str]:
    if not path.startswith(("docs/", "audit/")):
        return set()
    suites = {"repository_suite"}
    if matches(path, PROTOCOL): suites.add("protocol_suite")
    if path.startswith("docs/ui/"): suites.add("tui_suite")
    if path.startswith("docs/backup/"): suites.add("backup_suite")
    return suites

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="*")
    args = parser.parse_args()
    paths = args.paths or [line.strip() for line in sys.stdin if line.strip()]
    for key, value in classify(paths).items(): print(f"{key}={str(value).lower()}")

if __name__ == "__main__": main()
