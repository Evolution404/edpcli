#!/usr/bin/env python3
"""Pin the Netac FormatExA -> MBR-template initialization boundary.

This is a static, read-only audit of the captured Netac_USB_API.dll.  It proves
that the known Netac formatter copies the embedded 512-byte MBR template, then
explicitly clears the three trailing partition entries (+0x1CE..+0x1FD) before
patching the first entry.  That excludes the embedded/current Netac MBR
initialization path as a direct source of the non-zero legacy LBA6 entry3
underlay.  It does not identify the older EDP copy site that preserved that
underlay, so no protocol byte is promoted by this audit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

DLL_SHA256 = "b12a249ab86cad28ed8f733f06355c22e16dd10c95cc714d361e48b2c520796f"

FORMAT_EX_A = 0x1000A030
FORMAT_TO_GEOMETRY_CALL = 0x1000A205
GEOMETRY_HELPER = 0x100040A0
GEOMETRY_TO_MBR_CALL = 0x100040EB
MBR_BUILDER = 0x10003880
TEMPLATE_VA = 0x1014BA58
MEMSET_VA = 0x100FE5A0

FORMAT_CALL_BYTES = bytes.fromhex("e8969effff")
GEOMETRY_CALL_BYTES = bytes.fromhex("e890f7ffff")

BRANCH1 = bytes.fromhex(
    "8bf86a3005ce0100006a00b980000000be58ba141050f3a5e830ac0f00"
)
BRANCH2 = bytes.fromhex(
    "8bf86a3005ce0100006a00b980000000be58ba141050f3a5e8bdab0f00"
)
MEMSET_PREFIX = bytes.fromhex(
    "8b54240c8b4c240485d2746933c08a44240884c0"
)

EXPECTED_TEMPLATE_ENTRY1 = bytes.fromhex(
    "8001010006003fff20000000e0e70300"
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


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


def va_to_offset(
    image: bytes,
    va: int,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
) -> int:
    rva = va - image_base
    for virtual_address, virtual_size, raw_ptr, raw_size in sections:
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            delta = rva - virtual_address
            if delta >= raw_size:
                raise SystemExit(f"VA 0x{va:X} is not file-backed")
            return raw_ptr + delta
    raise SystemExit(f"VA 0x{va:X} is not mapped")


def read_va(
    image: bytes,
    va: int,
    size: int,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
) -> bytes:
    off = va_to_offset(image, va, image_base, sections)
    return image[off : off + size]


def require_at(
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll", type=Path, required=True)
    args = parser.parse_args()

    actual_sha = sha256(args.dll)
    if actual_sha != DLL_SHA256:
        raise SystemExit(
            f"Netac_USB_API.dll SHA-256 mismatch: expected {DLL_SHA256}, got {actual_sha}"
        )

    image = args.dll.read_bytes()
    image_base, sections = pe_layout(image)

    require_at(
        image,
        FORMAT_TO_GEOMETRY_CALL,
        FORMAT_CALL_BYTES,
        image_base,
        sections,
        "FormatExA -> geometry helper call",
    )
    require_at(
        image,
        GEOMETRY_TO_MBR_CALL,
        GEOMETRY_CALL_BYTES,
        image_base,
        sections,
        "geometry helper -> MBR builder call",
    )
    require_at(
        image,
        0x10003953,
        BRANCH1,
        image_base,
        sections,
        "MBR initialization branch 1",
    )
    require_at(
        image,
        0x100039C6,
        BRANCH2,
        image_base,
        sections,
        "MBR initialization branch 2",
    )
    require_at(
        image,
        MEMSET_VA,
        MEMSET_PREFIX,
        image_base,
        sections,
        "memset argument/dispatch prefix",
    )

    template = read_va(image, TEMPLATE_VA, 512, image_base, sections)
    if template[0x1BE:0x1CE] != EXPECTED_TEMPLATE_ENTRY1:
        raise SystemExit(
            "embedded Netac MBR entry1 drifted: " + template[0x1BE:0x1CE].hex()
        )
    if any(template[0x1CE:0x1FE]):
        raise SystemExit(
            "embedded Netac MBR trailing three entries are no longer all zero"
        )
    if template[0x1FE:0x200] != b"\x55\xaa":
        raise SystemExit("embedded Netac MBR signature drifted")

    result = {
        "dll_sha256": DLL_SHA256,
        "call_chain": {
            "FormatExA_NetacAPI": f"0x{FORMAT_EX_A:08X}",
            "FormatExA_callsite": f"0x{FORMAT_TO_GEOMETRY_CALL:08X}",
            "geometry_helper": f"0x{GEOMETRY_HELPER:08X}",
            "geometry_callsite": f"0x{GEOMETRY_TO_MBR_CALL:08X}",
            "mbr_builder": f"0x{MBR_BUILDER:08X}",
        },
        "template": {
            "va": f"0x{TEMPLATE_VA:08X}",
            "entry1_hex": template[0x1BE:0x1CE].hex(),
            "entry2_to_entry4_hex": template[0x1CE:0x1FE].hex(),
            "signature_hex": template[0x1FE:0x200].hex(),
        },
        "initialization": {
            "branch1_va": "0x10003953",
            "branch2_va": "0x100039C6",
            "operation": (
                "copy 512-byte template, then memset(output+0x1CE, 0, 0x30); "
                "subsequent direct patches shown in both branches target entry1 "
                "offsets +0x1BE/+0x1C2/+0x1C6/+0x1CA"
            ),
            "memset_va": f"0x{MEMSET_VA:08X}",
        },
        "claim": (
            "the captured Netac FormatExA path reaches the MBR builder, whose known "
            "template initialization clears partition entries 2-4 before dynamically "
            "patching entry1; therefore the embedded/current Netac MBR initialization "
            "cannot directly supply the observed non-zero LBA6 +0x1DE entry3 underlay"
        ),
        "claim_boundary": (
            "negative producer-boundary evidence only: an older formatter or an EDP "
            "copy/reuse path could still have preserved entry3. The exact historical "
            "LBA6 underlay producer/profile selector remains missing, so the 14-byte "
            "PARTIAL range is not promoted"
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
