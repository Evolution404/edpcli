#!/usr/bin/env python3
"""Generate a deterministic CycloneDX 1.5 SBOM from Cargo metadata/lockfile."""

from __future__ import annotations

import argparse
import json
import subprocess
import tomllib
import urllib.parse
import uuid
from pathlib import Path


def cargo_metadata() -> dict:
    raw = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        text=True,
    )
    return json.loads(raw)


def lock_checksums() -> dict[tuple[str, str, str], str]:
    with Path("Cargo.lock").open("rb") as handle:
        lock = tomllib.load(handle)
    result: dict[tuple[str, str, str], str] = {}
    for package in lock.get("package", []):
        checksum = package.get("checksum")
        if not checksum:
            continue
        result[
            (
                package["name"],
                package["version"],
                package.get("source", ""),
            )
        ] = checksum
    return result


def package_ref(package: dict) -> str:
    name = urllib.parse.quote(package["name"], safe="")
    version = urllib.parse.quote(package["version"], safe="")
    source = package.get("source") or "workspace"
    source_q = urllib.parse.quote(source, safe="")
    return f"pkg:cargo/{name}@{version}?source={source_q}"


def component(package: dict, checksums: dict[tuple[str, str, str], str]) -> dict:
    source = package.get("source") or ""
    item = {
        "type": "application" if package["name"] == "edpcli" else "library",
        "name": package["name"],
        "version": package["version"],
        "bom-ref": package_ref(package),
        "purl": f"pkg:cargo/{urllib.parse.quote(package['name'], safe='')}@{urllib.parse.quote(package['version'], safe='')}",
    }
    if package.get("license"):
        item["licenses"] = [{"expression": package["license"]}]
    checksum = checksums.get((package["name"], package["version"], source))
    if checksum:
        item["hashes"] = [{"alg": "SHA-256", "content": checksum}]
    if source:
        item["properties"] = [{"name": "cargo:source", "value": source}]
    return item


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    metadata = cargo_metadata()
    checksums = lock_checksums()
    packages = metadata["packages"]
    workspace_ids = set(metadata["workspace_members"])
    root = next(
        package
        for package in packages
        if package["id"] in workspace_ids and package["name"] == "edpcli"
    )
    refs = {package["id"]: package_ref(package) for package in packages}

    dependencies = []
    resolve = metadata.get("resolve") or {}
    for node in resolve.get("nodes", []):
        ref = refs.get(node["id"])
        if not ref:
            continue
        depends_on = sorted(refs[dep] for dep in node.get("dependencies", []) if dep in refs)
        dependencies.append({"ref": ref, "dependsOn": depends_on})
    dependencies.sort(key=lambda item: item["ref"])

    serial = uuid.uuid5(
        uuid.NAMESPACE_URL,
        f"https://github.com/Evolution404/edpcli@{args.commit}:{root['version']}",
    )
    root_component = component(root, checksums)
    document = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "component": root_component,
            "properties": [
                {"name": "git:commit", "value": args.commit},
                {"name": "build:rust-toolchain", "value": "1.98.1"},
            ],
        },
        "components": sorted(
            (component(package, checksums) for package in packages if package["id"] != root["id"]),
            key=lambda item: item["bom-ref"],
        ),
        "dependencies": dependencies,
    }

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
