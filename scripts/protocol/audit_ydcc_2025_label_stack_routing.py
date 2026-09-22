#!/usr/bin/env python3
"""Pin the 2025 SafeUsb registration routing and vendor-metadata boundary.

The captured 2025 SafeUsbRegsiterCems binary has two explicit routes:

* V2 devices load sectorManage.dll and resolve SetUDiskSecureInfoEx.
* non-V2 devices load SecUsbInterface.dll and resolve GetDiskInfo plus
  SetReserved3Data.

SecUsbInterface then loads \\UsbInterface_c.dll from the device's :\\Costom
directory and obtains GetYDInterfaceObject.  Its debug /RTC frame descriptor
names the structured buffers m_Reserved3 (0x40 bytes) and ExtensionData
(0x112 bytes), while GetDiskInfo fills these through independent vtable calls.

This narrows the missing join59 producer search.  It deliberately does not
claim how the unavailable UsbInterface_c.dll provider maps those vendor fields
to physical media, and therefore promotes no LBA bytes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc")
DEFAULT_SAFEUSB = ROOT / "safeusbregsitercems.dll"
DEFAULT_SECTOR = ROOT / "sectormanage.dll"
DEFAULT_SECUSB = ROOT / "secusbinterface.dll"

SAFEUSB_SHA256 = "016289889f27cf9de8cb685f9efc285fd467204600fec7184b73f1b5ddf5f14e"
SECTOR_SHA256 = "63a56fa36ac61d96ce9d84352a3dd79b3c835d67ee6fc6de7e9c276c3d22bf44"
SECUSB_SHA256 = "2e6c1fd0af2224d74c4316869da5a2c2e35ed7a05ae77620fe7e71a59d2c4651"

SAFEUSB_TIMESTAMP = 1739955421
SECTOR_TIMESTAMP = 1747036729
SECUSB_TIMESTAMP = 1577958241

SAFEUSB_SELECTOR_VA = 0x10004F71
SAFEUSB_SELECTOR = bytes.fromhex(
    "8b8568f5ffff0fb6480851e8ef65010083c4040fb6d085d20f84ef040000"
)
SAFEUSB_LEGACY_VA = 0x1000547E
SAFEUSB_LEGACY = bytes.fromhex(
    "68c49b0410ff1548e20310898548f8ffff83bd48f8ffff00742e"
    "68d89b04108b9548f8ffff52ff1544e20310a34cae0510"
    "68e49b04108b8548f8ffff50ff1544e20310a350ae0510"
)

SECUSB_GETDISKINFO_THUNK_VA = 0x100B0045
SECUSB_GETDISKINFO_THUNK = bytes.fromhex("e9d6d00000")
SECUSB_SETRESERVED3_THUNK_VA = 0x100B23E5
SECUSB_SETRESERVED3_THUNK = bytes.fromhex("e916b40000")
SECUSB_SET_PAIR_VA = 0x100BD9C6
SECUSB_SET_PAIR = bytes.fromhex(
    "8b4510898594feffff8bf48d8594feffff508b4d0c518b95a0feffff"
    "8b028b8da0feffff8b504cffd23bf4e8c94cffff8945e8"
    "8bf48d8594feffff508b4d0c83c140518b95a0feffff8b028b8da0feffff8b5038ffd2"
)
SECUSB_RTC_FRAME_VA = 0x100BD61C

EXPECTED_RTC = {
    "tempErr": 0x04,
    "ExtensionData": 0x112,
    "m_Reserved3": 0x40,
    "isize": 0x04,
    "currentPath": 0x104,
    "tempPath": 0x04,
}


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
    if struct.unpack_from("<H", image, optional)[0] != 0x10B:
        raise SystemExit("expected PE32")
    image_base = struct.unpack_from("<I", image, optional + 28)[0]
    section_off = optional + optional_size
    sections = []
    for index in range(section_count):
        off = section_off + index * 40
        virtual_size, virtual_address, raw_size, raw_ptr = struct.unpack_from(
            "<IIII", image, off + 8
        )
        sections.append((virtual_address, virtual_size, raw_ptr, raw_size))
    return image_base, timestamp, sections


def va_to_offset(
    va: int,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
) -> int:
    rva = va - image_base
    for virtual_address, virtual_size, raw_ptr, raw_size in sections:
        if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
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
    off = va_to_offset(va, image_base, sections)
    return image[off : off + size]


def cstring_va(
    image: bytes,
    va: int,
    image_base: int,
    sections: list[tuple[int, int, int, int]],
) -> str:
    off = va_to_offset(va, image_base, sections)
    end = image.find(b"\0", off)
    if end < 0:
        raise SystemExit(f"unterminated string at VA 0x{va:X}")
    return image[off:end].decode("ascii")


def assert_pe(
    path: Path, expected_sha: str, expected_timestamp: int
) -> tuple[bytes, int, list[tuple[int, int, int, int]]]:
    actual_sha = sha256(path)
    if actual_sha != expected_sha:
        raise SystemExit(f"{path.name} SHA-256 mismatch: {actual_sha}")
    image = path.read_bytes()
    image_base, timestamp, sections = pe_layout(image)
    if timestamp != expected_timestamp:
        raise SystemExit(
            f"{path.name} timestamp mismatch: expected {expected_timestamp}, got {timestamp}"
        )
    return image, image_base, sections


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--safeusb", type=Path, default=DEFAULT_SAFEUSB)
    parser.add_argument("--sector", type=Path, default=DEFAULT_SECTOR)
    parser.add_argument("--secusb", type=Path, default=DEFAULT_SECUSB)
    args = parser.parse_args()

    safe, safe_base, safe_sections = assert_pe(
        args.safeusb, SAFEUSB_SHA256, SAFEUSB_TIMESTAMP
    )
    sector, _, _ = assert_pe(args.sector, SECTOR_SHA256, SECTOR_TIMESTAMP)
    sec, sec_base, sec_sections = assert_pe(
        args.secusb, SECUSB_SHA256, SECUSB_TIMESTAMP
    )

    selector = read_va(
        safe,
        SAFEUSB_SELECTOR_VA,
        len(SAFEUSB_SELECTOR),
        safe_base,
        safe_sections,
    )
    if selector != SAFEUSB_SELECTOR:
        raise SystemExit("SafeUsb V2 selector branch drifted")

    legacy = read_va(
        safe,
        SAFEUSB_LEGACY_VA,
        len(SAFEUSB_LEGACY),
        safe_base,
        safe_sections,
    )
    if legacy != SAFEUSB_LEGACY:
        raise SystemExit("SafeUsb legacy loader/GetProcAddress sequence drifted")

    safe_markers = (
        b"ydcc_branches_2.10.4_dev",
        b"SafeUsbRegsiterCems.pdb",
        b"\\sectorManage.dll",
        b"SetUDiskSecureInfoEx",
        b"SecUsbInterface.dll",
        b"GetDiskInfo",
        b"SetReserved3Data",
    )
    for marker in safe_markers:
        if marker not in safe:
            raise SystemExit(f"SafeUsb marker missing: {marker!r}")

    for marker in (
        b"SetUDiskSecureInfoEx",
        b"SectorManageImp::",
        b"WriteIIR failed",
        b"sectorManageImp.cpp",
    ):
        if marker not in sector:
            raise SystemExit(f"sectorManage marker missing: {marker!r}")

    if (
        read_va(
            sec,
            SECUSB_GETDISKINFO_THUNK_VA,
            len(SECUSB_GETDISKINFO_THUNK),
            sec_base,
            sec_sections,
        )
        != SECUSB_GETDISKINFO_THUNK
    ):
        raise SystemExit("GetDiskInfo export thunk drifted")
    if (
        read_va(
            sec,
            SECUSB_SETRESERVED3_THUNK_VA,
            len(SECUSB_SETRESERVED3_THUNK),
            sec_base,
            sec_sections,
        )
        != SECUSB_SETRESERVED3_THUNK
    ):
        raise SystemExit("SetReserved3Data export thunk drifted")
    if (
        read_va(
            sec,
            SECUSB_SET_PAIR_VA,
            len(SECUSB_SET_PAIR),
            sec_base,
            sec_sections,
        )
        != SECUSB_SET_PAIR
    ):
        raise SystemExit("SetReserved3Data vtable setter-pair sequence drifted")

    for marker in (
        b":\\Costom",
        b"\\UsbInterface_c.dll",
        b"GetYDInterfaceObject",
        b"m_Reserved3",
        b"ExtensionData",
        b"SecUsbInterface.pdb",
    ):
        if marker not in sec:
            raise SystemExit(f"SecUsbInterface marker missing: {marker!r}")

    frame = read_va(sec, SECUSB_RTC_FRAME_VA, 8, sec_base, sec_sections)
    count, table_va = struct.unpack("<II", frame)
    if count != 6:
        raise SystemExit(f"unexpected /RTC variable count: {count}")
    table = read_va(sec, table_va, count * 12, sec_base, sec_sections)
    rtc = {}
    for index in range(count):
        _, size, name_va = struct.unpack_from("<iII", table, index * 12)
        rtc[cstring_va(sec, name_va, sec_base, sec_sections)] = size
    if rtc != EXPECTED_RTC:
        raise SystemExit(f"unexpected /RTC frame descriptor: {rtc!r}")

    result = {
        "safeusb": {
            "sha256": SAFEUSB_SHA256,
            "compile_time_utc": datetime.fromtimestamp(
                SAFEUSB_TIMESTAMP, tz=timezone.utc
            ).isoformat(),
            "source_lineage": "ydcc_branches_2.10.4_dev",
            "selector_va": f"0x{SAFEUSB_SELECTOR_VA:08X}",
            "v2_route": "sectorManage.dll!SetUDiskSecureInfoEx",
            "legacy_route": "SecUsbInterface.dll!GetDiskInfo + SetReserved3Data",
        },
        "sector_manage": {
            "sha256": SECTOR_SHA256,
            "compile_time_utc": datetime.fromtimestamp(
                SECTOR_TIMESTAMP, tz=timezone.utc
            ).isoformat(),
            "family_markers": ["SectorManageImp::", "WriteIIR failed"],
        },
        "secusb_interface": {
            "sha256": SECUSB_SHA256,
            "compile_time_utc": datetime.fromtimestamp(
                SECUSB_TIMESTAMP, tz=timezone.utc
            ).isoformat(),
            "provider": r":\Costom\UsbInterface_c.dll!GetYDInterfaceObject",
            "rtc_frame": rtc,
            "getter_setter_pairs": {
                "m_Reserved3": "vtable +0x48 getter / +0x4C setter",
                "ExtensionData": "vtable +0x34 getter / +0x38 setter",
            },
        },
        "claim": (
            "the captured 2025 SafeUsb registration stack bifurcates into a V2 "
            "sectorManage/IIR route and a non-V2 SecUsbInterface vendor-metadata "
            "route; the latter exposes structured m_Reserved3=0x40 and "
            "ExtensionData=0x112 buffers rather than a direct 512-byte SAFE6 builder"
        ),
        "claim_boundary": (
            "UsbInterface_c.dll itself is unavailable, so the final physical backing "
            "of those YD fields is not claimed; this evidence narrows producer search "
            "but does not promote any of the remaining 143 LBA bytes"
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
