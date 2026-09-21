#!/usr/bin/env python3
"""Pin the deleted 2025 ydcc/CEMSUsbRegsiter distribution lineage.

This audit deliberately does *not* claim that the missing 2025 DLL has been
recovered. It fixes two independent local artifacts:

* Product_audit records ydcc 2.10.0 being installed on 2025-05-13.
* VUpdateReplace.log proves that the 8.1.2502.2116 update tree contained both
  ydcc/cemsusbregsiter.dll and its .zip payload, and that the later 2026 update
  backed up the then-running DLL before replacing/deleting the cache.

The result narrows the missing join59/legacy-underlay writer search to a real
intermediate package generation without promoting any protocol bytes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
from pathlib import Path


LOG_SHA256 = "dfc7446501a26bc32d6d3c6821b6d6d71609f4483af12dcac2af10ba707301db"
DB_SHA256 = "10704b28c0068692007a1238d5f14c75b26f11896f99c8a2533521f295506c88"

OLD_BASE = "8.1.2502.2116"
NEW_BASE = "8.1.2604.0917"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(text: str, needle: str) -> int:
    offset = text.find(needle)
    if offset < 0:
        raise SystemExit(f"missing update-lineage marker: {needle}")
    return offset


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--log",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/VUpdateReplace.log"),
    )
    parser.add_argument(
        "--db",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/emsstore_decrypted.db"),
    )
    args = parser.parse_args()

    if sha256(args.log) != LOG_SHA256:
        raise SystemExit("VUpdateReplace.log SHA-256 mismatch")
    if sha256(args.db) != DB_SHA256:
        raise SystemExit("emsstore_decrypted.db SHA-256 mismatch")

    log = args.log.read_bytes().decode("utf-16le", errors="strict")
    markers = {
        "old_dll": (
            rf"C:\Program Files (x86)\VRV\CEMS\product\CEMS\base\{OLD_BASE}"
            rf"\ydcc\cemsusbregsiter.dll"
        ),
        "old_zip": (
            rf"C:\Program Files (x86)\VRV\CEMS\product\CEMS\base\{OLD_BASE}"
            rf"\ydcc\cemsusbregsiter.dll.zip"
        ),
        "new_backup_dll": (
            rf"C:\Program Files (x86)\VRV\CEMS\product\CEMS\base\{NEW_BASE}.bk"
            rf"\ydcc\cemsusbregsiter.dll"
        ),
        "new_dll": (
            rf"C:\Program Files (x86)\VRV\CEMS\product\CEMS\base\{NEW_BASE}"
            rf"\ydcc\cemsusbregsiter.dll"
        ),
    }
    offsets = {name: require(log, marker) for name, marker in markers.items()}
    require(log, "2025-05-13")
    require(log, "2026-04-30")

    con = sqlite3.connect(args.db)
    try:
        row = con.execute(
            """
            select produtType, produtName, baseVersion, installtime, updatatime
            from Product_audit where produtType = 'ydcc'
            """
        ).fetchone()
    finally:
        con.close()
    expected = (
        "ydcc",
        "移动存储(WIN)",
        "2.10.0",
        "2025-05-13 13:19:15",
        "2026-04-30 18:30:03",
    )
    if row != expected:
        raise SystemExit(f"unexpected ydcc Product_audit row: {row!r}")

    print(
        json.dumps(
            {
                "log_sha256": LOG_SHA256,
                "db_sha256": DB_SHA256,
                "ydcc_product_audit": {
                    "product_type": row[0],
                    "name": row[1],
                    "base_version": row[2],
                    "install_time": row[3],
                    "update_time": row[4],
                },
                "distribution_markers": offsets,
                "old_base": OLD_BASE,
                "new_base": NEW_BASE,
                "claim": (
                    "the missing intermediate ydcc/CEMSUsbRegsiter generation was "
                    "actually distributed and installed on this endpoint: the 2025 "
                    "8.1.2502.2116 tree contained both the DLL and zip payload, and "
                    "the 2026 replacement flow backed up the then-running DLL"
                ),
                "claim_boundary": (
                    "provenance/version-window evidence only; the deleted 2025 DLL "
                    "bytes are not recovered here, so this audit does not prove "
                    "join59 or legacy-MBR serialization and does not promote bytes"
                ),
            },
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
