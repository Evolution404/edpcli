#!/usr/bin/env python3
"""Pin the legacy endpoint -> SafeUDiskLabelTool ownership boundary.

The 2022-era endpoint control DLL is useful for locating the missing join59
producer, but it is not itself proof of the sector writer.  This audit pins the
captured binary and the registration-policy strings that delegate safe-UDisk
registration/mutation work to the separate SafeUDiskLabelTool process.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import pefile


CEMSUDISK_SHA256 = "49d9a63c0ddcf9de05add21af2ab01128db29e03729c1cba642067446a431ecd"
EXPECTED_FILE_VERSION = "8, 1, 2205, 3015"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def string_table(pe: pefile.PE) -> dict[str, str]:
    result: dict[str, str] = {}
    for block in getattr(pe, "FileInfo", []):
        for entry in block:
            if getattr(entry, "Key", b"") != b"StringFileInfo":
                continue
            for table in entry.StringTable:
                for key, value in table.entries.items():
                    result[key.decode(errors="ignore")] = value.decode(errors="ignore")
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cemsudisk",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/Edp/cemsudisk.dll"),
    )
    args = parser.parse_args()

    data = args.cemsudisk.read_bytes()
    if sha256(data) != CEMSUDISK_SHA256:
        raise SystemExit("legacy cemsudisk SHA-256 mismatch")

    pe = pefile.PE(data=data)
    meta = string_table(pe)
    if meta.get("FileVersion") != EXPECTED_FILE_VERSION:
        raise SystemExit(
            f"unexpected legacy cemsudisk FileVersion: {meta.get('FileVersion')!r}"
        )

    required = {
        "tool_path": b"SafeUDiskLabelTool\\cemsSafeUdiskLabelTool.exe",
        "tool_name": b"cemsSafeUdiskLabelTool.exe",
        "set_safe_label": b"setLabelSafeUDisk",
        "clean_safe_label": b"cleanLabelSafeUDisk",
        "init_safe_password": b"initPwdSafeUDisk",
        "registration_policy": b"safeUdiskRegManage",
    }
    offsets: dict[str, int] = {}
    lower = data.lower()
    for name, needle in required.items():
        offset = lower.find(needle.lower())
        if offset < 0:
            raise SystemExit(f"missing legacy endpoint delegation marker: {name}")
        offsets[name] = offset

    print(
        json.dumps(
            {
                "sha256": CEMSUDISK_SHA256,
                "file_version": EXPECTED_FILE_VERSION,
                "original_filename": meta.get("OriginalFilename"),
                "delegation_markers": offsets,
                "claim": (
                    "the captured legacy endpoint control plane explicitly names "
                    "SafeUDiskLabelTool for safe-UDisk registration/mutation policy; "
                    "therefore endpoint join59 readers are not substitutes for the "
                    "missing independent label-tool sector writer"
                ),
                "claim_boundary": (
                    "string-level ownership/deployment evidence only; this audit does "
                    "not prove which SafeUDiskLabelTool generation serialized join59"
                ),
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
