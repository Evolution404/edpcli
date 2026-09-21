#!/usr/bin/env python3
"""Offline audit for the Phison MP marker-page relationship to EDP LBA3.

This script never opens a device. It verifies a pinned MPALL v3.72 archive,
extracts the four FW/BN final 512-byte marker pages in memory, rotates each page
left by 16 bytes, and checks the stable host-side LBA3 tail against two
independent committed physical profiles.

The audit deliberately does *not* claim that the PC-side MPALL executable is
the host-LBA3 serializer. It only proves a reproducible structural invariant:
all four first-party FW/BN version marker pages and both physical MP-LBA3
profiles share one exact 472-byte tail after the observed 16-byte layout
reordering. The first 40 bytes remain a separate dynamic-header problem.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

ARCHIVE_SHA256 = "756da586f67b3e06a09e97ee940994547b72fea7f64308a5c559e0b1689b5364"
COMMON_TAIL_SHA256 = "5f88797f7273191052e7a9300316e1a4f0f31563db07110a86fa4e648379198f"
MARKER = b"this is mp mark\0"

MEMBERS = {
    "BN67V1292KM.BIN": {
        "sha256": "ccb05c6038e3ecb70fcdf102b5a4a229770d31106ce5d8e18085e5c86055238a",
        "page_sha256": "72e54ba0b9d2100bd63847d4e51feaf1c3d5afee33d9b2488501301412868d0c",
        "expected_same": 500,
    },
    "BN67V132M.BIN": {
        "sha256": "6c57fa6deeb5da5b980cd80690c2e071caa0e80efda1d762f394ef6f6de85f26",
        "page_sha256": "ee32547c019b20b4635769a31e1a22747157af2932308de5e1703970b51ea372",
        "expected_same": 500,
    },
    "FW67FF01V60424M.BIN": {
        "sha256": "4842c4740711302cc9251aaafa27138781a093fd97c3fc8971193fb04311e19a",
        "page_sha256": "9f146dc3564b3d2c19c88d25e7749c67bc2b27efeeba282e4e3476a911ba4e96",
        "expected_same": 496,
    },
    "FW67FF01V61110M.BIN": {
        "sha256": "24f57b8c059d771425465daadfd5ac407330362925a1e1aa2be85fff1b40cd1c",
        "page_sha256": "5da7e5e01878c025f490918af0eacf6941236ec9908397512e55f3094e9a70b1",
        "expected_same": 496,
    },
}

STRICT_NAME = (
    "disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_"
    "onlyid2135149925_20260903_121319.bin"
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def decode_hex(path: Path) -> bytes:
    return bytes.fromhex("".join(path.read_text().split()))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--archive",
        type=Path,
        default=Path("/private/tmp/phison-mpall-v3720b.zip"),
        help="pinned Phison MPALL v3.72 archive",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    archive = args.archive.read_bytes()
    if sha256(archive) != ARCHIVE_SHA256:
        raise SystemExit("MPALL archive SHA-256 mismatch")

    repo = Path(__file__).resolve().parents[2]
    strict_image = (
        repo / "audit/protocol/gold/strict-encrypted" / STRICT_NAME
    ).read_bytes()
    strict_lba3 = strict_image[3 * 512 : 4 * 512]
    historical_lba3 = decode_hex(
        repo / "tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex"
    )

    if len(strict_lba3) != 512 or len(historical_lba3) != 512:
        raise SystemExit("physical LBA3 fixture size mismatch")
    if strict_lba3[:0x28] == historical_lba3[:0x28]:
        raise SystemExit("dynamic MP headers unexpectedly collapsed to one profile")
    if strict_lba3[0x28:] != historical_lba3[0x28:]:
        raise SystemExit("independent physical MP tails no longer match")
    if sha256(strict_lba3[0x28:]) != COMMON_TAIL_SHA256:
        raise SystemExit("physical 472-byte MP tail SHA-256 changed")
    if any(strict_lba3[0x28:0x1F0]):
        raise SystemExit("physical MP body +0x028..+0x1EF is no longer all zero")
    if strict_lba3[0x1F0:] != MARKER:
        raise SystemExit("physical MP trailer marker changed")

    rows: list[dict[str, object]] = []
    with zipfile.ZipFile(args.archive) as zf:
        for short_name, expected in MEMBERS.items():
            matches = [n for n in zf.namelist() if n.endswith("/" + short_name)]
            if len(matches) != 1:
                raise SystemExit(f"expected exactly one archive member for {short_name}")
            member_name = matches[0]
            member = zf.read(member_name)
            if sha256(member) != expected["sha256"]:
                raise SystemExit(f"member SHA-256 mismatch: {short_name}")
            page = member[-512:]
            if sha256(page) != expected["page_sha256"]:
                raise SystemExit(f"final marker-page SHA-256 mismatch: {short_name}")
            if page[:16] != MARKER:
                raise SystemExit(f"final page marker changed: {short_name}")

            rotated = page[16:] + page[:16]
            if rotated[0x28:] != strict_lba3[0x28:]:
                raise SystemExit(f"472-byte rotated tail mismatch: {short_name}")
            if sha256(rotated[0x28:]) != COMMON_TAIL_SHA256:
                raise SystemExit(f"rotated tail SHA mismatch: {short_name}")

            same = sum(a == b for a, b in zip(rotated, strict_lba3))
            if same != expected["expected_same"]:
                raise SystemExit(
                    f"unexpected full-sector similarity for {short_name}: {same}/512"
                )
            rows.append(
                {
                    "member": short_name,
                    "member_sha256": expected["sha256"],
                    "page_sha256": expected["page_sha256"],
                    "full_sector_equal_bytes": same,
                    "tail_equal_bytes": 472,
                    "tail_sha256": COMMON_TAIL_SHA256,
                }
            )

    print(
        json.dumps(
            {
                "archive_sha256": ARCHIVE_SHA256,
                "physical_profiles": 2,
                "physical_dynamic_headers_distinct": True,
                "common_tail_range": "0x028..0x1FF",
                "common_tail_bytes": 472,
                "common_tail_sha256": COMMON_TAIL_SHA256,
                "body_zero_range": "0x028..0x1EF",
                "marker_range": "0x1F0..0x1FF",
                "marker": "this is mp mark\\0",
                "members": rows,
                "claim_boundary": (
                    "proves the stable marker-page-family tail after the observed "
                    "16-byte layout reordering; does not identify the exact host-LBA3 "
                    "serializer or explain the dynamic first 40 bytes"
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
