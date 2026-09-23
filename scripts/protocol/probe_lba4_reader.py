#!/usr/bin/env python3
"""Read-only Unicorn probe for current CEMSUsbRegsiter.dll::ReadSector4.

This harness never opens a physical disk and never invokes a producer/writer.
It feeds the checked-in authentic no-password LBA4 sector directly to the
official x86 ReadSector4 machine code and stubs only allocator / MSVC string /
atoi runtime boundaries needed by that function.

The external ``emu_framework.py`` is an analysis helper from the local VRV
workspace; it is intentionally not vendored into this repository.  Both the
official DLL and the checked-in gold image are pinned by SHA-256 before any
emulation runs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

CURRENT_DLL_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
AUTHENTIC_GOLD_SHA256 = "d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258"
READ_SECTOR4_RVA = 0x15090
READ_SECTOR4_END_RVA = 0x15296


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_hash(path: Path, expected: str) -> None:
    actual = sha256(path)
    if actual != expected:
        raise SystemExit(f"SHA-256 mismatch for {path}: expected {expected}, got {actual}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dll",
        type=Path,
        required=True,
    )
    parser.add_argument(
        "--framework-dir",
        type=Path,
        required=True,
        help="directory containing emu_framework.py",
    )
    parser.add_argument(
        "--image",
        type=Path,
        default=Path(
            "audit/protocol/gold/authentic-nopwd/sandisk_ultra_20260823_lba0_12.bin"
        ),
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    require_hash(args.dll, CURRENT_DLL_SHA256)
    require_hash(args.image, AUTHENTIC_GOLD_SHA256)

    sys.path.insert(0, str(args.framework_dir))
    try:
        from emu_framework import EmuFramework  # type: ignore
        from unicorn import UC_HOOK_CODE
    except ImportError as exc:
        raise SystemExit(f"analysis dependency unavailable: {exc}") from exc

    image = args.image.read_bytes()
    if len(image) != 13 * 512:
        raise SystemExit(f"expected 6656-byte LBA0-LBA12 image, got {len(image)} bytes")
    raw = image[4 * 512 : 5 * 512]

    emu = EmuFramework(str(args.dll), stack_size=0x300000)
    stub_hits: list[str] = []

    def hit(name: str, value: int = 0) -> int:
        stub_hits.append(name)
        return value

    emu.stub_function(0x67E3F, lambda _e, _esp: hit("security-cookie"))
    emu.stub_function(
        0x67E5E,
        lambda e, esp: (stub_hits.append("allocator") or e.alloc(max(1, e.read_dword(esp + 4)))),
    )
    emu.stub_function(0x67E9F, lambda _e, _esp: hit("free"))

    captured = {"cstr": 0}

    def string_ctor(_e, arg: int) -> int:
        stub_hits.append("std::string(char const*)")
        captured["cstr"] = arg
        return 0

    emu.stub_function_n(0x0A1A0, 1, string_ctor)
    emu.stub_function(
        0x0CF60,
        lambda _e, _esp: (stub_hits.append("std::string::c_str") or captured["cstr"]),
    )
    emu.stub_function(0x0ACA0, lambda _e, _esp: hit("std::string::~string"))

    def atoi_stub(e, esp: int) -> int:
        stub_hits.append("atoi")
        ptr = e.read_dword(esp + 4)
        text = e.read_str(ptr, 64).decode("ascii").split("\x00", 1)[0]
        return int(text, 10) & 0xFFFFFFFF

    emu.stub_function(0x8A3D8, atoi_stub)

    executed: list[int] = []
    start = emu.image_base + READ_SECTOR4_RVA
    end = emu.image_base + READ_SECTOR4_END_RVA - 1

    def core_hook(_uc, address: int, _size: int, _user_data) -> None:
        executed.append(address - emu.image_base)

    emu.uc.hook_add(UC_HOOK_CODE, core_hook, begin=start, end=end)

    obj = emu.alloc(0x100)
    raw_ptr = emu.alloc(512)
    node = emu.alloc(0x2F)
    onlyid_ptr = emu.alloc(4)
    emu.mem_write(obj, bytes(0x100))
    emu.mem_write(obj + 0x40, struct.pack("<I", 512))
    emu.mem_write(raw_ptr, raw)
    emu.mem_write(node, bytes([0xCC]) * 0x2F)
    emu.mem_write(onlyid_ptr, bytes([0xCC]) * 4)

    ret = emu.call_thiscall(
        READ_SECTOR4_RVA,
        obj,
        raw_ptr,
        node,
        onlyid_ptr,
        timeout=8_000_000,
    )
    decoded_node = bytes(emu.mem_read(node, 0x2F))
    onlyid = struct.unpack("<I", bytes(emu.mem_read(onlyid_ptr, 4)))[0]

    result = {
        "dll_sha256": CURRENT_DLL_SHA256,
        "gold_sha256": AUTHENTIC_GOLD_SHA256,
        "function": "CEMSUsbRegsiter.dll::ReadSector4",
        "function_rva": f"0x{READ_SECTOR4_RVA:05X}",
        "return": ret,
        "onlyid": onlyid,
        "onlyid_xor8": f"0x{struct.unpack_from('<I', decoded_node, 0)[0]:08X}",
        "second_onlyid": f"0x{struct.unpack_from('<I', decoded_node, 4)[0]:08X}",
        "hserial": decoded_node[8:0x1C].hex(),
        "new_lab_flag": decoded_node[0x21:0x25].decode("ascii"),
        "version": struct.unpack_from("<I", decoded_node, 0x25)[0],
        "sector_tuple": decoded_node[0x29:0x2D].hex(),
        "wire_flags": raw[0x45:0x47].hex(),
        "reader_flags": decoded_node[0x2D:0x2F].hex(),
        "executed_core_first_rva": f"0x{min(executed):05X}" if executed else None,
        "executed_core_last_rva": f"0x{max(executed):05X}" if executed else None,
        "executed_core_instruction_count": len(executed),
        "stub_hits": stub_hits,
        "emulator_exceptions": [
            item for item in emu._trace_log if item.startswith("EXCEPTION") or item.startswith("UNMAPPED")
        ],
    }
    print(json.dumps(result, indent=2))

    expected_guard = onlyid ^ 0x88888888
    if ret != 0 or struct.unpack_from("<I", decoded_node, 0)[0] != expected_guard:
        raise SystemExit("official ReadSector4 did not return a valid restore node")
    if raw[0x45:0x47] != bytes.fromhex("0000"):
        raise SystemExit("authentic gold wire flags changed")
    if decoded_node[0x2D:0x2F] != bytes.fromhex("d4d9"):
        raise SystemExit("official reader-view flags changed")
    if result["emulator_exceptions"]:
        raise SystemExit("emulator encountered an unexpected exception/unmapped access")


if __name__ == "__main__":
    main()
