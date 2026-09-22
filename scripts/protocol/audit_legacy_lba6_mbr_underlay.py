#!/usr/bin/env python3
"""Reproduce the legacy LBA6 MBR-underlay observation from pinned evidence.

The core audit is clean-clone reproducible and uses only checked-in physical
evidence.  Optional --scan-root arguments provide a supplemental search for the
common MBR byte prefix in locally captured PE images; those negative scan
results are intentionally not required for the core claim.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

SECTOR = 512
LBA6_K0 = 0x4DAA

AIGO_PATH = Path(
    "audit/protocol/gold/strict-encrypted/"
    "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_"
    "onlyid1987718388_20260827_191701.bin"
)
AIGO_SHA256 = "150d705efb303a247e61f50df564fde321d37f988fff3e2fd9e82be7134a4dbc"

SANDISK_LBA6_PATH = Path(
    "tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba6.hex"
)
SANDISK_LBA6_SHA256 = "16dfa23098588b1c2c7d8cb4a7d7539f5f397fbd06df18cfd144defec3b31cf4"

AUTHENTIC_NOPWD_PATH = Path(
    "audit/protocol/gold/authentic-nopwd/sandisk_ultra_20260823_lba0_12.bin"
)
AUTHENTIC_NOPWD_SHA256 = (
    "d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258"
)

EXPECTED_AIGO_ENTRY = bytes.fromhex("0000c1ff07efffff1ca87d0ee3f42700")
EXPECTED_SANDISK_ENTRY = bytes.fromhex("0000c1ff07efffffb28a050e773c4c00")
COMMON_VISIBLE_PREFIX = bytes.fromhex("c1ff07efffff")
PE_SUFFIXES = {".dll", ".exe", ".sys", ".ocx", ".cpl"}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_sha(path: Path, expected: str) -> None:
    actual = sha256(path)
    if actual != expected:
        raise SystemExit(f"SHA-256 mismatch for {path}: {actual}")


def decode_hex_fixture(path: Path) -> bytes:
    raw = bytes.fromhex("".join(path.read_text(encoding="utf-8").split()))
    if len(raw) != SECTOR:
        raise SystemExit(f"{path}: expected 512 decoded bytes, got {len(raw)}")
    return raw


def lba6_decode(raw: bytes) -> bytes:
    if len(raw) != SECTOR:
        raise SystemExit(f"LBA6 must be 512 bytes, got {len(raw)}")
    out = bytearray(raw[:0x1FC])
    key = LBA6_K0
    for index in range(len(out) // 2):
        offset = index * 2
        word = int.from_bytes(out[offset : offset + 2], "little") ^ key
        out[offset : offset + 2] = word.to_bytes(2, "little")
        key = (key + 0x100 - index - 1) & 0xFFFF
    return bytes(out) + raw[0x1FC:]


def decode_chs(chs: bytes) -> tuple[int, int, int]:
    if len(chs) != 3:
        raise ValueError("CHS must be exactly 3 bytes")
    head = chs[0]
    sector = chs[1] & 0x3F
    cylinder = ((chs[1] & 0xC0) << 2) | chs[2]
    return cylinder, head, sector


def parse_entry(entry: bytes) -> dict[str, object]:
    if len(entry) != 16:
        raise ValueError("MBR entry must be 16 bytes")
    return {
        "raw_hex": entry.hex(),
        "boot": entry[0],
        "start_chs": decode_chs(entry[1:4]),
        "type": entry[4],
        "end_chs": decode_chs(entry[5:8]),
        "start_lba": int.from_bytes(entry[8:12], "little"),
        "sector_count": int.from_bytes(entry[12:16], "little"),
    }


def scan_pe_roots(roots: list[Path]) -> dict[str, object]:
    scanned = 0
    hits: list[str] = []
    errors: list[str] = []
    for root in roots:
        if not root.exists():
            errors.append(f"missing root: {root}")
            continue
        paths = [root] if root.is_file() else root.rglob("*")
        for path in paths:
            if not path.is_file() or path.suffix.lower() not in PE_SUFFIXES:
                continue
            scanned += 1
            try:
                blob = path.read_bytes()
            except OSError as exc:
                errors.append(f"{path}: {exc}")
                continue
            if not blob.startswith(b"MZ"):
                continue
            if COMMON_VISIBLE_PREFIX in blob:
                hits.append(str(path))
    return {
        "roots": [str(root) for root in roots],
        "pe_files_scanned": scanned,
        "common_prefix_hex": COMMON_VISIBLE_PREFIX.hex(),
        "hits": hits,
        "errors": errors,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scan-root",
        action="append",
        type=Path,
        default=[],
        help="optional local tree to scan for the common legacy MBR byte prefix",
    )
    args = parser.parse_args()

    require_sha(AIGO_PATH, AIGO_SHA256)
    require_sha(SANDISK_LBA6_PATH, SANDISK_LBA6_SHA256)
    require_sha(AUTHENTIC_NOPWD_PATH, AUTHENTIC_NOPWD_SHA256)

    aigo_image = AIGO_PATH.read_bytes()
    if len(aigo_image) != 13 * SECTOR:
        raise SystemExit(f"{AIGO_PATH}: expected 6656 bytes")
    aigo_plain = lba6_decode(aigo_image[6 * SECTOR : 7 * SECTOR])

    sandisk_plain = lba6_decode(decode_hex_fixture(SANDISK_LBA6_PATH))

    nopwd_image = AUTHENTIC_NOPWD_PATH.read_bytes()
    if len(nopwd_image) != 13 * SECTOR:
        raise SystemExit(f"{AUTHENTIC_NOPWD_PATH}: expected 6656 bytes")
    nopwd_plain = lba6_decode(nopwd_image[6 * SECTOR : 7 * SECTOR])

    aigo_entry = aigo_plain[0x1DE:0x1EE]
    sandisk_entry = sandisk_plain[0x1DE:0x1EE]
    nopwd_entry = nopwd_plain[0x1DE:0x1EE]

    if aigo_entry != EXPECTED_AIGO_ENTRY:
        raise SystemExit(f"Aigo legacy MBR entry drifted: {aigo_entry.hex()}")
    if sandisk_entry != EXPECTED_SANDISK_ENTRY:
        raise SystemExit(f"SanDisk original legacy MBR entry drifted: {sandisk_entry.hex()}")
    if any(nopwd_entry):
        raise SystemExit(
            "authentic no-password SanDisk must remain a distinct zero-underlay profile"
        )

    aigo = parse_entry(aigo_entry)
    sandisk = parse_entry(sandisk_entry)
    for label, entry in (("Aigo", aigo), ("SanDisk original", sandisk)):
        if entry["start_chs"] != (1023, 0, 1):
            raise SystemExit(f"{label}: unexpected start CHS {entry['start_chs']}")
        if entry["type"] != 0x07:
            raise SystemExit(f"{label}: unexpected partition type {entry['type']}")
        if entry["end_chs"] != (1023, 239, 63):
            raise SystemExit(f"{label}: unexpected end CHS {entry['end_chs']}")

    if aigo_entry[:8] != sandisk_entry[:8]:
        raise SystemExit("legacy entries lost their shared boot/CHS/type prefix")
    if aigo_entry[8:] == sandisk_entry[8:]:
        raise SystemExit("legacy entries unexpectedly lost disk-specific LBA geometry")

    result: dict[str, object] = {
        "inputs": {
            "aigo_sha256": AIGO_SHA256,
            "sandisk_lba6_fixture_sha256": SANDISK_LBA6_SHA256,
            "authentic_nopwd_sha256": AUTHENTIC_NOPWD_SHA256,
        },
        "lba6_entry3_underlay": {
            "offset": "0x1DE..0x1ED",
            "partial_ledger_slice": "0x1E0..0x1ED",
            "aigo": aigo,
            "sandisk_original": sandisk,
            "authentic_nopwd_hex": nopwd_entry.hex(),
            "shared_first_8_bytes_hex": aigo_entry[:8].hex(),
            "disk_specific_geometry_bytes": {
                "aigo": aigo_entry[8:].hex(),
                "sandisk_original": sandisk_entry[8:].hex(),
            },
        },
        "claim": (
            "two independent legacy physical profiles preserve a standard 16-byte "
            "MBR partition entry under the LBA6 BeiZhu overlay: boot/CHS/type bytes "
            "are identical while start-LBA/sector-count bytes vary with the disk; "
            "the authentic no-password SanDisk is a separate zero-underlay profile"
        ),
        "claim_boundary": (
            "this closes the physical byte shape and proves the underlay is dynamic "
            "partition geometry rather than one fixed constant; it does not identify "
            "the historical copy site/profile selector, so LBA6 0x1E0..0x1ED remains PARTIAL"
        ),
    }
    if args.scan_root:
        result["optional_local_pe_scan"] = scan_pe_roots(args.scan_root)

    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
