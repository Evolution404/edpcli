#!/usr/bin/env python3
"""Emulate only the authentic OEM CHS-minus-0xE0000 locator x86 routine.

No operating-system calls, device access, credential reads, filesystem writes,
Windows virtual driver activation, or physical LCE payload producer emulation.
Requires pefile/unicorn/capstone already present in the project's uv lock.
"""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import struct

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EBP, UC_X86_REG_ESP
import pefile

DLL_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
FN_VA = 0x10040110
FN_END_VA = 0x1004013E
ALIGN_START_VA = 0x1003DE8B
ALIGN_END_VA = 0x1003DED0
ALIGN_IMAGE_ADDR = 0x1003D000
FRAME_ADDR = 0x24000000
FRAME_EBP = FRAME_ADDR + 0x2000
CHS_TAIL_DISTANCE_BYTES = 0xE0000
PAGE = 0x1000
IMAGE_ADDR = 0x10040000
OBJECT_ADDR = 0x21000000
STACK_ADDR = 0x22000000
EXIT_ADDR = 0x23000000


def audited_slice(pe: pefile.PE, raw: bytes, start_va: int, end_va: int, *, allow_internal_branches: bool) -> bytes:
    start = pe.get_offset_from_rva(start_va - pe.OPTIONAL_HEADER.ImageBase)
    body = raw[start:start + end_va - start_va]
    instructions = list(Cs(CS_ARCH_X86, CS_MODE_32).disasm(body, start_va))
    if not instructions or instructions[0].address != start_va or instructions[-1].address + instructions[-1].size != end_va:
        raise ValueError("OEM instruction boundaries changed")
    forbidden = ("call", "int", "sys", "out", "in")
    if any(x.mnemonic.startswith(forbidden) for x in instructions):
        raise ValueError("OEM audited slice unexpectedly requires an external helper")
    if not allow_internal_branches and any(x.mnemonic.startswith("j") for x in instructions):
        raise ValueError("OEM CHS locator unexpectedly branches")
    if allow_internal_branches:
        # All branches must remain in the audited byte slice, or exit exactly
        # at the exclusive end of the snippet. Never execute untrusted code.
        for x in instructions:
            if x.mnemonic.startswith("j"):
                dest = int(x.op_str, 0)
                if not start_va <= dest <= end_va:
                    raise ValueError("OEM branch escapes audited slice")
    return body


def load_original_instructions(dll_path: Path) -> tuple[bytes, bytes]:
    raw = dll_path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != DLL_SHA256:
        raise ValueError("wrong OEM DLL identity; refuse to emulate unverified instructions")
    pe = pefile.PE(data=raw, fast_load=True)
    if pe.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError("OEM DLL image base changed")
    locator = audited_slice(pe, raw, FN_VA, FN_END_VA, allow_internal_branches=False)
    alignment = audited_slice(pe, raw, ALIGN_START_VA, ALIGN_END_VA, allow_internal_branches=True)
    assert locator[-1] == 0xC3, "OEM locator must return to the fake caller"
    return locator, alignment


def emulate_real_chs_locator(program: bytes, chs_bytes: int) -> int:
    if not 0 <= chs_bytes < (1 << 64):
        raise ValueError("CHS geometry out of range")
    machine = Uc(UC_ARCH_X86, UC_MODE_32)
    for address in (IMAGE_ADDR, OBJECT_ADDR, STACK_ADDR):
        machine.mem_map(address, PAGE)
    machine.mem_write(FN_VA, program)
    machine.mem_write(OBJECT_ADDR + 0x6A0, struct.pack("<Q", chs_bytes))
    stack_pointer = STACK_ADDR + PAGE - 16
    machine.mem_write(stack_pointer, struct.pack("<I", EXIT_ADDR))
    machine.reg_write(UC_X86_REG_ECX, OBJECT_ADDR)
    machine.reg_write(UC_X86_REG_ESP, stack_pointer)
    machine.emu_start(FN_VA, EXIT_ADDR, count=48)
    return (machine.reg_read(UC_X86_REG_EDX) << 32) | machine.reg_read(UC_X86_REG_EAX)


def emulate_real_compat_extent_rounding(program: bytes, sector_bytes: int) -> int:
    if not 0 < sector_bytes <= 0x10000:
        raise ValueError("invalid logical sector width")
    machine = Uc(UC_ARCH_X86, UC_MODE_32)
    machine.mem_map(ALIGN_IMAGE_ADDR, PAGE)
    machine.mem_map(FRAME_ADDR, PAGE * 4)
    machine.mem_write(ALIGN_START_VA, program)
    machine.mem_write(FRAME_EBP - 0x141C, struct.pack("<I", sector_bytes))
    machine.reg_write(UC_X86_REG_EBP, FRAME_EBP)
    machine.reg_write(UC_X86_REG_ESP, FRAME_EBP + 0x100)
    machine.emu_start(ALIGN_START_VA, ALIGN_END_VA, count=48)
    return struct.unpack("<I", machine.mem_read(FRAME_EBP - 0x1424, 4))[0]


def test_oem_vectors(program: bytes, align_program: bytes) -> list[tuple[str, int, int, int]]:
    vectors = [
        ("U391_4Kn", 3889, 4096, 62_476_561),
        ("Lexar_512B", 15_165, 512, 243_623_933),
    ]
    output: list[tuple[str, int, int, int]] = []
    for label, cylinders, sector_bytes, expected_lba in vectors:
        chs_bytes = cylinders * 255 * 63 * sector_bytes
        native = emulate_real_chs_locator(program, chs_bytes)
        expected_bytes = chs_bytes - CHS_TAIL_DISTANCE_BYTES
        assert native == expected_bytes, f"{label}: real OEM machine code vs model disagrees"
        assert native // sector_bytes == expected_lba, f"{label}: LCE LBA changed"
        output.append((label, sector_bytes, native, expected_lba))
    # The genuine x86 routine must preserve 64-bit carry/borrow across the
    # low/high DWORD boundary, not wrap an unsigned 32-bit byte address.
    for source in [0x100001, 0xFFFFFFFF, 0x100000001, 0x123456789ABC, 0xFEFFF000]:
        if source >= CHS_TAIL_DISTANCE_BYTES:
            assert emulate_real_chs_locator(program, source) == source - CHS_TAIL_DISTANCE_BYTES
    for sector_bytes in [256, 512, 1024, 2048, 4096, 8192, 16384]:
        actual = emulate_real_compat_extent_rounding(align_program, sector_bytes)
        expected = ((0xC00 + sector_bytes - 1) // sector_bytes) * sector_bytes
        assert actual == expected, f"OEM real x86 rounding differs for {sector_bytes}B sector"
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll", type=Path, required=True, help="unchanged original OEM DLL")
    args = parser.parse_args()
    body, align_program = load_original_instructions(args.dll)
    for label, sector_bytes, physical_byte_address, lba in test_oem_vectors(body, align_program):
        print(f"{label}: native_sector_bytes={sector_bytes} physical_byte_address={physical_byte_address} lce_lba={lba} x86_emulation=PASS")
    print("ORIGINAL_X86_LOCATOR_64BIT_CARRY: PASS")
    print("ORIGINAL_X86_CREATEPARTITIONS_3072B_ROUNDING: 512B=>3072B/6 blocks, 4096B=>4096B/1 block, PASS")
    print("OEM_CREATEPARTITIONS_LCE_PAYLOAD_WRITE: NOT_EMULATED_OR_PROVEN")
    print("USB_OR_VIRTUAL_DISK_IO: NONE")


if __name__ == "__main__":
    main()
