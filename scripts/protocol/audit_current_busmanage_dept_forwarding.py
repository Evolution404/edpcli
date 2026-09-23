#!/usr/bin/env python3
"""Pin the current label-tool -> BusManage -> CEMSUsbRegsiter Dept forwarding boundary.

This read-only audit answers one narrow join59 question: does the upper
BusManage layer truncate Dept to 59/60 bytes before the sector writer sees it?

The current usbtoolbusmanage.dll WriteNormalULabel path copies request+0x80
with wcsncpy(..., 0xBC) into the request passed through writer-object vtable
slot +0x08. The current CEMSUsbRegsiter.dll request converter then takes the
same request+0x80 field and writes it to UsbLabelParam+0x40 with capacity
0xBC bytes. Existing BuildSector6 audits independently identify
UsbLabelParam/UsbWriteParam+0x40 as Dept.

Therefore the current BusManage ABI forwards a full Dept-capacity field; it
does not introduce the historical join59 NUL at Dept[59]. The missing
join59 producer remains inside the older CEMSUsbRegsiter serialization
generation (or an older implementation behind the same writer interface).

The 2026 updater log is also pinned to recover the deleted 2025
usbtoolbusmanage.dll MD5, giving a second exact acquisition fingerprint
alongside the already-known 2025 cemsusbregsiter.dll MD5.

No protocol byte is promoted by this audit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

USBTOOL_SHA256 = "08381e33d44d11719795b063a978a6646387ce40d62c2f7aeb514c0c308459e9"
USBTOOL_MD5 = "37FF8A1C6EA63F8F5D689ACFE6AB102A"
CEMSREG_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
CEMSREG_MD5 = "2ABA574551E59550B0D4FB50E8BECD27"
SERVICE_LOG_SHA256 = "5ec3b53e34573646b29dde6cee5595fccb527c385f0b0f571d7b8b74ebf8c517"
OLD_USBTOOL_MD5 = "FC29B1C96E48F4F82EA6D641362B3364"

USBTOOL_DEPT_COPY_VA = 0x100A9B53
USBTOOL_WRITER_CALL_VA = 0x100A9DBD
CEMSREG_DEPT_CONVERT_VA = 0x100476BB

USBTOOL_DEPT_COPY = bytes.fromhex(
    "68bc000000"
    "8b550c"
    "81c280000000"
    "52"
    "8d8594f6ffff"
    "50"
    "ff15b8120d10"
    "83c40c"
)

USBTOOL_WRITER_CALL = bytes.fromhex(
    "8d8514f6ffff"
    "50"
    "8b8d1cf4ffff"
    "8b5104"
    "8b8204670000"
    "8b8d1cf4ffff"
    "8b5104"
    "8b8a04670000"
    "8b00"
    "8b5008"
    "ffd2"
)

CEMSREG_DEPT_CONVERT = bytes.fromhex(
    "8b4508"
    "0580000000"
    "50"
    "8d4dd8"
    "e8a4b0feff"
    "c745fc00000000"
    "8d4dd8"
    "51"
    "8d9548ffffff"
    "52"
    "e80d90feff"
    "83c408"
    "8985ccfeffff"
    "8b8dccfeffff"
    "e86958fcff"
    "50"
    "68bc000000"
    "8b450c"
    "83c040"
    "50"
    "e846080500"
    "83c40c"
)


def digest(path: Path, algorithm: str) -> str:
    h = hashlib.new(algorithm)
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest().upper()


def pe_layout(image: bytes) -> tuple[int, list[tuple[int, int, int, int]]]:
    if image[:2] != b"MZ":
        raise SystemExit("not an MZ image")
    pe_off = struct.unpack_from("<I", image, 0x3C)[0]
    if image[pe_off : pe_off + 4] != b"PE\0\0":
        raise SystemExit("not a PE image")
    file_hdr = pe_off + 4
    section_count = struct.unpack_from("<H", image, file_hdr + 2)[0]
    optional_size = struct.unpack_from("<H", image, file_hdr + 16)[0]
    optional = file_hdr + 20
    magic = struct.unpack_from("<H", image, optional)[0]
    if magic == 0x10B:
        image_base = struct.unpack_from("<I", image, optional + 28)[0]
    elif magic == 0x20B:
        image_base = struct.unpack_from("<Q", image, optional + 24)[0]
    else:
        raise SystemExit(f"unexpected PE optional-header magic 0x{magic:X}")

    sections: list[tuple[int, int, int, int]] = []
    section_off = optional + optional_size
    for index in range(section_count):
        off = section_off + index * 40
        virtual_size, virtual_address, raw_size, raw_ptr = struct.unpack_from(
            "<IIII", image, off + 8
        )
        sections.append((virtual_address, virtual_size, raw_ptr, raw_size))
    return image_base, sections


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
                raise SystemExit(f"VA 0x{va:X} read crosses non-file-backed section tail")
            return image[raw_ptr + delta : raw_ptr + delta + size]
    raise SystemExit(f"VA 0x{va:X} is not file-backed")


def require_bytes(
    image: bytes,
    va: int,
    expected: bytes,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
    label: str,
) -> None:
    actual = read_va(image, va, len(expected), image_base, sections)
    if actual != expected:
        raise SystemExit(
            f"{label} drifted at 0x{va:X}: expected {expected.hex()}, got {actual.hex()}"
        )


def find_md5_block(lines: list[str]) -> dict[str, int]:
    path_marker = r"C:\Program Files (x86)\VRV\CEMS\ydcc\usbtoolbusmanage.dll"
    starts = [i for i, line in enumerate(lines) if path_marker in line]
    for start in starts:
        stop = min(len(lines), start + 24)
        expected = [
            "文件校验md5不一样 Record And File",
            USBTOOL_MD5,
            OLD_USBTOOL_MD5,
            "需要升级",
        ]
        positions: list[int] = []
        cursor = start
        for needle in expected:
            found = next((i for i in range(cursor, stop) if needle in lines[i]), None)
            if found is None:
                break
            positions.append(found)
            cursor = found + 1
        if len(positions) == len(expected):
            return {
                "path_line": start + 1,
                "record_md5_line": positions[1] + 1,
                "old_file_md5_line": positions[2] + 1,
                "upgrade_line": positions[3] + 1,
            }
    raise SystemExit("usbtoolbusmanage.dll old/new MD5 comparison block not found")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--usbtool", type=Path, required=True)
    parser.add_argument("--cemsreg", type=Path, required=True)
    parser.add_argument("--service-log", type=Path, required=True)
    args = parser.parse_args()

    if digest(args.usbtool, "sha256").lower() != USBTOOL_SHA256:
        raise SystemExit("usbtoolbusmanage.dll SHA-256 mismatch")
    if digest(args.usbtool, "md5") != USBTOOL_MD5:
        raise SystemExit("usbtoolbusmanage.dll MD5 mismatch")
    if digest(args.cemsreg, "sha256").lower() != CEMSREG_SHA256:
        raise SystemExit("cemsusbregsiter.dll SHA-256 mismatch")
    if digest(args.cemsreg, "md5") != CEMSREG_MD5:
        raise SystemExit("cemsusbregsiter.dll MD5 mismatch")
    if digest(args.service_log, "sha256").lower() != SERVICE_LOG_SHA256:
        raise SystemExit("VUpdateService.log SHA-256 mismatch")

    usbtool = args.usbtool.read_bytes()
    usb_base, usb_sections = pe_layout(usbtool)
    require_bytes(
        usbtool,
        USBTOOL_DEPT_COPY_VA,
        USBTOOL_DEPT_COPY,
        usb_base,
        usb_sections,
        "WriteNormalULabel Dept wcsncpy",
    )
    require_bytes(
        usbtool,
        USBTOOL_WRITER_CALL_VA,
        USBTOOL_WRITER_CALL,
        usb_base,
        usb_sections,
        "WriteNormalULabel writer-object vtable+0x08 call",
    )
    for text in (b"GetUsbRegsiterObj\0", b"\\CEMSUsbRegsiter.dll\0"):
        if usbtool.count(text) != 1:
            raise SystemExit(f"expected one loader string {text!r}, got {usbtool.count(text)}")

    cemsreg = args.cemsreg.read_bytes()
    cems_base, cems_sections = pe_layout(cemsreg)
    require_bytes(
        cemsreg,
        CEMSREG_DEPT_CONVERT_VA,
        CEMSREG_DEPT_CONVERT,
        cems_base,
        cems_sections,
        "CEMSUsbRegsiter request+0x80 -> UsbLabelParam+0x40 conversion",
    )

    lines = args.service_log.read_bytes().decode("utf-16le", errors="strict").splitlines()
    old_md5 = find_md5_block(lines)

    result = {
        "current_usbtool": {
            "sha256": USBTOOL_SHA256,
            "md5": USBTOOL_MD5,
            "write_normal_ulabel": {
                "dept_source_offset": "request+0x80",
                "copy_kind": "wcsncpy",
                "copy_count_wchar": 0xBC,
                "writer_request_base": "ebp-0x9EC",
                "writer_interface_call": "object vtable+0x08",
            },
            "loader_strings": ["GetUsbRegsiterObj", "\\CEMSUsbRegsiter.dll"],
        },
        "current_cemsusbregsiter": {
            "sha256": CEMSREG_SHA256,
            "md5": CEMSREG_MD5,
            "request_converter": {
                "source": "request+0x80",
                "destination": "UsbLabelParam+0x40",
                "destination_capacity_bytes": 0xBC,
                "semantic_field": "Dept (pinned independently by BuildSector6/DWARF audits)",
            },
        },
        "deleted_2025_usbtool_fingerprint": {"md5": OLD_USBTOOL_MD5, **old_md5},
        "claim": (
            "current BusManage forwards Dept at full 0xBC-field capacity into "
            "CEMSUsbRegsiter; it does not insert the join59 NUL or truncate Dept "
            "to 59/60 bytes before the writer interface"
        ),
        "join59_boundary": (
            "the historical join59 producer must be sought in the missing older "
            "CEMSUsbRegsiter serialization generation (or an older implementation "
            "behind the same writer interface), not in current BusManage request packing"
        ),
        "claim_boundary": (
            "current forwarding + acquisition narrowing only; the deleted 2025 "
            "usbtoolbusmanage bytes and historical join59 writer remain unavailable, "
            "so no LBA byte is promoted"
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
