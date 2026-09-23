#!/usr/bin/env python3
"""Pin the 2024 ydcc 2.10 modFileSysCheck raw-disk reader boundary.

This read-only audit proves an otherwise misleading detail precisely: the module
opens PhysicalDrive with GENERIC_READ|GENERIC_WRITE, but its recovered protocol
sector helper fcn.10005250 calls ReadFile and contains no WriteFile IAT call.
The embedded source/PDB paths bind the binary to git_ydcc_dev_2.10.  Therefore
this component is useful same-lineage reader/acquisition evidence, not the
missing join59 sector producer.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from datetime import datetime, timezone
from pathlib import Path

DLL_SHA256 = "12072373ebaf24126b6b0b24692befa03c0dc7de46a6e1db24c600cc9b0ff8d8"
EXPECTED_TIMESTAMP = 1727400439

OPEN_SEQ_VA = 0x10004F05
OPEN_SEQ = bytes.fromhex(
    "8d85a0feffff6a006a006a036a006a0368000000c050ff1514500210894324"
)
RAW_READER_VA = 0x10005250
RAW_READER_LEN = 300
RAW_READ_SEQ = bytes.fromhex(
    "8b77388d45f86a005056ff75ecc745f800000000ff7724ff1500500210"
)
READFILE_CALL = bytes.fromhex("ff1500500210")
WRITEFILE_CALL = bytes.fromhex("ff1508500210")
CREATEFILEA_CALL = bytes.fromhex("ff1514500210")

SOURCE_PATHS = (
    b"f:\\zx\\project\\vrvproject\\git_ydcc_dev_2.10\\ydcc\\linux\\ydcc\\src\\"
    b"modcems\\modfilesyscheck\\dllmain.cpp",
    b"f:\\zx\\project\\vrvproject\\git_ydcc_dev_2.10\\ydcc\\linux\\ydcc\\src\\"
    b"modcems\\modfilesyscheck\\filesyscheck.cpp",
    b"F:\\zx\\project\\vrvproject\\git_ydcc_dev_2.10\\ydcc\\linux\\ydcc\\src\\"
    b"modCems\\modFileSysCheck\\Release\\modFileSysCheck.pdb",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_layout(image: bytes) -> tuple[int, int, list[tuple[int, int, int, int]]]:
    if image[:2] != b"MZ":
        raise SystemExit("not an MZ image")
    pe_off = struct.unpack_from("<I", image, 0x3C)[0]
    if image[pe_off : pe_off + 4] != b"PE\0\0":
        raise SystemExit("not a PE image")
    file_hdr = pe_off + 4
    section_count = struct.unpack_from("<H", image, file_hdr + 2)[0]
    timestamp = struct.unpack_from("<I", image, file_hdr + 4)[0]
    optional_size = struct.unpack_from("<H", image, file_hdr + 16)[0]
    optional = file_hdr + 20
    magic = struct.unpack_from("<H", image, optional)[0]
    if magic != 0x10B:
        raise SystemExit(f"expected PE32, got optional-header magic 0x{magic:X}")
    image_base = struct.unpack_from("<I", image, optional + 28)[0]
    section_off = optional + optional_size
    sections: list[tuple[int, int, int, int]] = []
    for index in range(section_count):
        off = section_off + index * 40
        virtual_size, virtual_address, raw_size, raw_ptr = struct.unpack_from(
            "<IIII", image, off + 8
        )
        sections.append((virtual_address, virtual_size, raw_ptr, raw_size))
    return image_base, timestamp, sections


def read_va(
    image: bytes,
    va: int,
    size: int,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
) -> bytes:
    rva = va - image_base
    for virtual_address, virtual_size, raw_ptr, raw_size in sections:
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            delta = rva - virtual_address
            if delta + size > raw_size:
                raise SystemExit(f"VA range 0x{va:X}+0x{size:X} is not file-backed")
            return image[raw_ptr + delta : raw_ptr + delta + size]
    raise SystemExit(f"VA 0x{va:X} is not mapped")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll", type=Path, required=True)
    args = parser.parse_args()

    actual_sha = sha256(args.dll)
    if actual_sha != DLL_SHA256:
        raise SystemExit(
            f"modfilesyscheck.dll SHA-256 mismatch: expected {DLL_SHA256}, got {actual_sha}"
        )

    image = args.dll.read_bytes()
    image_base, timestamp, sections = pe_layout(image)
    if timestamp != EXPECTED_TIMESTAMP:
        raise SystemExit(
            f"PE timestamp drifted: expected {EXPECTED_TIMESTAMP}, got {timestamp}"
        )

    open_seq = read_va(image, OPEN_SEQ_VA, len(OPEN_SEQ), image_base, sections)
    if open_seq != OPEN_SEQ:
        raise SystemExit(f"PhysicalDrive open sequence drifted: {open_seq.hex()}")
    if CREATEFILEA_CALL not in open_seq:
        raise SystemExit("PhysicalDrive open sequence lost CreateFileA IAT call")
    if bytes.fromhex("68000000c0") not in open_seq:
        raise SystemExit("PhysicalDrive open sequence lost GENERIC_READ|GENERIC_WRITE")

    raw_reader = read_va(
        image, RAW_READER_VA, RAW_READER_LEN, image_base, sections
    )
    if RAW_READ_SEQ not in raw_reader:
        raise SystemExit("raw sector reader lost the pinned ReadFile sequence")
    if raw_reader.count(READFILE_CALL) != 1:
        raise SystemExit(
            f"expected one ReadFile IAT call in raw reader, got {raw_reader.count(READFILE_CALL)}"
        )
    if WRITEFILE_CALL in raw_reader:
        raise SystemExit("raw sector reader unexpectedly gained a WriteFile IAT call")

    for marker in SOURCE_PATHS:
        if marker not in image:
            raise SystemExit(f"missing git_ydcc_dev_2.10 source marker: {marker!r}")
    for marker in (b"Dept=", b"LLGB", b"EETU", b"\\\\.\\PhysicalDrive%u"):
        if marker not in image:
            raise SystemExit(f"missing expected reader-family marker: {marker!r}")

    result = {
        "dll_sha256": DLL_SHA256,
        "compile_time_utc": datetime.fromtimestamp(
            timestamp, tz=timezone.utc
        ).isoformat(),
        "source_lineage": "git_ydcc_dev_2.10",
        "physical_drive_open": {
            "va": f"0x{OPEN_SEQ_VA:08X}",
            "desired_access": "0xC0000000 (GENERIC_READ|GENERIC_WRITE)",
            "createfile_iat": "0x10025014",
        },
        "raw_sector_helper": {
            "va": f"0x{RAW_READER_VA:08X}",
            "readfile_iat": "0x10025000",
            "writefile_iat": "0x10025008",
            "readfile_calls_in_pinned_body": raw_reader.count(READFILE_CALL),
            "writefile_calls_in_pinned_body": raw_reader.count(WRITEFILE_CALL),
        },
        "claim": (
            "the 2024 git_ydcc_dev_2.10 modFileSysCheck binary opens PhysicalDrive "
            "with read/write access, but the recovered protocol raw-sector helper "
            "fcn.10005250 performs ReadFile only and contains no WriteFile IAT call"
        ),
        "claim_boundary": (
            "same-lineage reader/checker evidence only; generic WriteFile imports in "
            "other CRT/file-output functions do not make this module the missing "
            "join59 producer, so no LBA byte is promoted"
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
