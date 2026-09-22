#!/usr/bin/env python3
"""Pin the deleted 2025 ydcc runtime hashes and updater cleanup lifecycle.

The 2026 updater compared each local runtime file against its service record
before replacement.  Those comparisons preserve the MD5 of the deleted local
2025 generation even though its bytes are gone.  VUpdateReplace.log also shows
that cemsusbregsiter.dll was copied into the new-base .bk tree and that this
backup copy was then explicitly deleted.

This is acquisition/provenance evidence only.  It identifies the missing file
precisely; it does not prove join59 or legacy-MBR serialization.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

SERVICE_LOG_SHA256 = "5ec3b53e34573646b29dde6cee5595fccb527c385f0b0f571d7b8b74ebf8c517"
REPLACE_LOG_SHA256 = "dfc7446501a26bc32d6d3c6821b6d6d71609f4483af12dcac2af10ba707301db"

RUNTIME = {
    "cemssafeudisklabeltool.exe": {
        "record_md5": "987CAE5CB907048D643E1EABCA438B36",
        "file_md5": "B000D3235E4E1522BF5F3B3F886561A3",
    },
    "cemsudisk.dll": {
        "record_md5": "63EFF4FF273C28C8062D4D84EF3CDA19",
        "file_md5": "3E6116EC2E92F24E92A64AA45CC85577",
    },
    "cemsusbregsiter.dll": {
        "record_md5": "2ABA574551E59550B0D4FB50E8BECD27",
        "file_md5": "02F8CD326E8CBDA04F17B6235BDF268D",
    },
}

OLD_BASE = "8.1.2502.2116"
NEW_BASE = "8.1.2604.0917"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_ordered(lines: list[str], start: int, needles: list[str], window: int = 20) -> list[int]:
    positions: list[int] = []
    cursor = start
    stop = min(len(lines), start + window)
    for needle in needles:
        found = -1
        for index in range(cursor, stop):
            if needle in lines[index]:
                found = index
                break
        if found < 0:
            raise SystemExit(
                f"missing ordered marker after line {start + 1}: {needle!r}"
            )
        positions.append(found)
        cursor = found + 1
    return positions


def all_line_hits(lines: list[str], needle: str) -> list[int]:
    return [index for index, line in enumerate(lines) if needle in line]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--service-log",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/VUpdateService.log"),
    )
    parser.add_argument(
        "--replace-log",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/VUpdateReplace.log"),
    )
    args = parser.parse_args()

    if sha256(args.service_log) != SERVICE_LOG_SHA256:
        raise SystemExit("VUpdateService.log SHA-256 mismatch")
    if sha256(args.replace_log) != REPLACE_LOG_SHA256:
        raise SystemExit("VUpdateReplace.log SHA-256 mismatch")

    service_lines = args.service_log.read_bytes().decode("utf-16le", errors="strict").splitlines()
    comparisons = {}
    for filename, expected in RUNTIME.items():
        path_marker = rf"C:\Program Files (x86)\VRV\CEMS\ydcc\{filename}"
        starts = all_line_hits(service_lines, path_marker)
        if not starts:
            raise SystemExit(f"runtime path marker missing: {path_marker}")
        matched = None
        for start in starts:
            try:
                positions = require_ordered(
                    service_lines,
                    start,
                    [
                        "文件校验md5不一样 Record And File",
                        expected["record_md5"],
                        expected["file_md5"],
                        "需要升级",
                    ],
                    window=24,
                )
                matched = positions
                break
            except SystemExit:
                continue
        if matched is None:
            raise SystemExit(f"MD5 comparison block missing for {filename}")
        comparisons[filename] = {
            "path_line": start + 1,
            "record_md5": expected["record_md5"],
            "file_md5": expected["file_md5"],
            "record_md5_line": matched[1] + 1,
            "file_md5_line": matched[2] + 1,
        }

    replace_lines = args.replace_log.read_bytes().decode("utf-16le", errors="strict").splitlines()
    backup_path = (
        rf"C:\Program Files (x86)\VRV\CEMS\product\CEMS\base\{NEW_BASE}.bk"
        r"\ydcc\cemsusbregsiter.dll"
    )
    backup_hits = all_line_hits(replace_lines, backup_path)
    if len(backup_hits) < 2:
        raise SystemExit(f"expected backup path at least twice, got {len(backup_hits)}")

    create_hit = None
    delete_hit = None
    for hit in backup_hits:
        before = "\n".join(replace_lines[max(0, hit - 4) : hit])
        after = "\n".join(replace_lines[hit + 1 : min(len(replace_lines), hit + 7)])
        if "替换文件" in before and create_hit is None:
            create_hit = hit
        if "文件删除成功" in after and delete_hit is None:
            delete_hit = hit
    if create_hit is None:
        raise SystemExit("backup creation/copy block not found")
    if delete_hit is None:
        raise SystemExit("backup deletion-success block not found")

    result = {
        "service_log_sha256": SERVICE_LOG_SHA256,
        "replace_log_sha256": REPLACE_LOG_SHA256,
        "old_base": OLD_BASE,
        "new_base": NEW_BASE,
        "runtime_md5_comparisons": comparisons,
        "deleted_writer_fingerprint": {
            "filename": "cemsusbregsiter.dll",
            "md5": RUNTIME["cemsusbregsiter.dll"]["file_md5"],
        },
        "backup_lifecycle": {
            "path": backup_path,
            "copy_block_line": create_hit + 1,
            "delete_success_block_line": delete_hit + 1,
        },
        "claim": (
            "the pre-upgrade local 2025 ydcc runtime is now content-addressable by "
            "MD5: cemsusbregsiter.dll=02F8CD326E8CBDA04F17B6235BDF268D; "
            "the updater copied that runtime into the 8.1.2604.0917.bk tree and "
            "then explicitly deleted the backup copy"
        ),
        "claim_boundary": (
            "hash/lifecycle provenance only; the old bytes are still unavailable, "
            "so no join59 or legacy-MBR byte is promoted"
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
