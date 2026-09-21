#!/usr/bin/env python3
"""Read-only protocol baseline verifier for the designated LBA0-LBA12 gold set."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

SECTOR = 512
IMAGE_LEN = 13 * SECTOR
EMPTY_SECTOR_SHA256 = hashlib.sha256(bytes(SECTOR)).hexdigest()
REPO_ROOT = Path(__file__).resolve().parents[2]


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_manifest(path: Path) -> list[dict[str, str]]:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    header = lines[0].split("\t")
    return [dict(zip(header, line.split("\t"), strict=True)) for line in lines[1:]]


def lba(image: bytes, index: int) -> bytes:
    return image[index * SECTOR : (index + 1) * SECTOR]


def load_gold(
    manifest: list[dict[str, str]], repo_root: Path
) -> list[tuple[dict[str, str], bytes, Path]]:
    expected_paths = {row["repo_path"] for row in manifest}
    gold_root = repo_root / "audit/protocol/gold"
    actual_paths = {
        path.relative_to(repo_root).as_posix() for path in gold_root.rglob("*.bin")
    }
    if actual_paths != expected_paths:
        missing = sorted(expected_paths - actual_paths)
        unexpected = sorted(actual_paths - expected_paths)
        raise ValueError(
            "checked-in gold population drift: "
            f"missing={missing or 'none'} unexpected={unexpected or 'none'}"
        )

    out: list[tuple[dict[str, str], bytes, Path]] = []
    for row in manifest:
        if row["profile"] not in {"strict-encrypted", "authentic-nopwd"}:
            raise ValueError(f"unknown gold profile: {row['profile']}")

        path = repo_root / row["repo_path"]
        resolved = path.resolve()
        try:
            resolved.relative_to(repo_root.resolve())
        except ValueError as exc:
            raise ValueError(f"gold path escapes repository: {path}") from exc
        data = path.read_bytes()

        expected_len = int(row["bytes"])
        if len(data) != expected_len:
            raise ValueError(f"{path}: expected {expected_len} bytes, got {len(data)}")
        actual_sha = sha256(data)
        if actual_sha != row["sha256"]:
            raise ValueError(
                f"{path}: SHA-256 mismatch: expected {row['sha256']}, got {actual_sha}"
            )
        out.append((row, data, path))
    return out


def lba3_shape(raw: bytes) -> dict[str, object]:
    nonzero = any(raw)
    return {
        "zero": not nonzero,
        "head_0x00_0x03": raw[0:4].hex(),
        "bytes_0x20_0x27": raw[0x20:0x28].hex(),
        "tail_0x1f0_0x1ff_ascii": raw[0x1F0:0x200].rstrip(b"\0").decode(
            "ascii", errors="replace"
        ),
        "sha256": sha256(raw),
    }


def audit(gold: list[tuple[dict[str, str], bytes, Path]]) -> dict[str, object]:
    strict = [item for item in gold if item[0]["profile"] == "strict-encrypted"]
    nopwd = [item for item in gold if item[0]["profile"] == "authentic-nopwd"]
    if len(strict) != 21:
        raise ValueError(f"strict encrypted gold population must be 21, got {len(strict)}")
    if len(nopwd) != 1:
        raise ValueError(f"authentic no-password gold population must be 1, got {len(nopwd)}")

    lba10_nonzero: list[str] = []
    lba3_profiles: dict[str, dict[str, object]] = {}
    for row, image, path in gold:
        if any(lba(image, 10)):
            lba10_nonzero.append(str(path))
        shape = lba3_shape(lba(image, 3))
        key = str(shape["sha256"])
        entry = lba3_profiles.setdefault(
            key,
            {
                "count": 0,
                "shape": shape,
                "samples": [],
            },
        )
        entry["count"] = int(entry["count"]) + 1
        cast_samples = entry["samples"]
        assert isinstance(cast_samples, list)
        cast_samples.append(row["sample"])

    if lba10_nonzero:
        raise ValueError(
            "designated current gold unexpectedly contains nonzero LBA10: "
            + ", ".join(lba10_nonzero)
        )

    return {
        "image_bytes": IMAGE_LEN,
        "strict_encrypted_count": len(strict),
        "authentic_nopwd_count": len(nopwd),
        "lba10": {
            "all_zero": True,
            "zero_sector_sha256": EMPTY_SECTOR_SHA256,
            "status_effect": "sample dependency remains; LBA10+0x000..0x07f stays PARTIAL",
        },
        "lba3_profiles": list(lba3_profiles.values()),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest", default=REPO_ROOT / "audit/protocol/gold_samples.tsv", type=Path
    )
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    rows = parse_manifest(args.manifest)
    gold = load_gold(rows, REPO_ROOT)
    result = audit(gold)
    text = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True)
    print(text)
    if args.json_out:
        args.json_out.write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as exc:
        print(f"protocol baseline audit failed: {exc}", file=sys.stderr)
        raise SystemExit(1)

