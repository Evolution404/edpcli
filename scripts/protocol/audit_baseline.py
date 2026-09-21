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
EESI_PHYSICAL_PATH = REPO_ROOT / "audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin"
EESI_PHYSICAL_META_PATH = REPO_ROOT / "audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.meta.json"
EESI_PHYSICAL_SHA256 = "3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39"


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


def load_eesi_physical() -> tuple[bytes, dict[str, object]]:
    data = EESI_PHYSICAL_PATH.read_bytes()
    if len(data) != IMAGE_LEN:
        raise ValueError(
            f"{EESI_PHYSICAL_PATH}: expected {IMAGE_LEN} bytes, got {len(data)}"
        )
    actual_sha = sha256(data)
    if actual_sha != EESI_PHYSICAL_SHA256:
        raise ValueError(
            f"{EESI_PHYSICAL_PATH}: SHA-256 mismatch: expected {EESI_PHYSICAL_SHA256}, got {actual_sha}"
        )
    meta = json.loads(EESI_PHYSICAL_META_PATH.read_text(encoding="utf-8"))
    if meta.get("device_id") != "disk&ven_netac&prod_onlydisk&rev_0000":
        raise ValueError("EESI physical metadata lost the audited Netac device_id")
    if meta.get("crc32") != "5088ee37" or meta.get("size") != IMAGE_LEN:
        raise ValueError("EESI physical metadata lost the audited CRC/size")
    if meta.get("md5") != hashlib.md5(data).hexdigest():
        raise ValueError("EESI physical metadata MD5 does not match the committed capture")
    if not any(lba(data, 10)):
        raise ValueError("EESI physical positive unexpectedly has an all-zero LBA10")
    return data, meta


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
    eesi_physical, eesi_meta = load_eesi_physical()
    strict = [item for item in gold if item[0]["profile"] == "strict-encrypted"]
    nopwd = [item for item in gold if item[0]["profile"] == "authentic-nopwd"]
    digests = [item[0]["sha256"] for item in gold]
    if len(digests) != len(set(digests)):
        raise ValueError("checked-in gold set contains duplicate SHA-256 images")
    if len(strict) != 19:
        raise ValueError(f"strict encrypted gold population must be 19, got {len(strict)}")
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
        "unique_image_count": len(gold),
        "strict_encrypted_count": len(strict),
        "authentic_nopwd_count": len(nopwd),
        "lba10": {
            "general_census_all_zero": True,
            "zero_sector_sha256": EMPTY_SECTOR_SHA256,
            "purpose_specific_positive": {
                "path": str(EESI_PHYSICAL_PATH.relative_to(REPO_ROOT)),
                "sha256": sha256(eesi_physical),
                "device_id": eesi_meta["device_id"],
            },
            "status_effect": "general census remains absent-zero; P-EESI-NETAC supplies the separate positive physical profile and LBA10+0x000..0x07f is COMPLETE",
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

