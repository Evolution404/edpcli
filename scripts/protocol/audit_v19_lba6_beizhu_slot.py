#!/usr/bin/env python3
"""Reproducible v19.11.4.1 audit for the historical LBA6 BeiZhu slot.

The audit pins the historical DLL and proves four deliberately narrow facts:
1. BuildSector6 starts from a 512-byte UsbMainBSec template whose
   +0x1D0..+0x1F3 bytes are zero.
2. The historical writer overlays sector+0x1D0 with a 32-byte-capacity
   C-string copy sourced from caller arg8; its recovered caller zeroes the
   complete 32-byte temporary before copying BeiZhu into it.  The upstream
   object+0x2620 source field is itself populated by strcpy_s(cap=16), so a
   valid producer can carry at most 15 text bytes plus the terminating NUL.
3. ReadSector6 returns sector+0x1D0 through the same bounded C-string helper,
   not through a raw 32-byte memcpy.
4. Recovered upper callers either ignore that output or pass it onward as a
   C-string; none parses the post-NUL +0x10..+0x1D tail.

This does not claim that v19.11.4.1 produced the two physical legacy MBR
underlay profiles. Those non-zero bytes still require an earlier producer.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import pefile
from capstone import CS_ARCH_X86, CS_MODE_32, Cs
from capstone.x86 import X86_OP_IMM, X86_OP_MEM

DLL_SHA256 = "584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814"
IMAGE_BASE = 0x10000000
BUILD_SECTOR6 = 0x10006370
READ_SECTOR6 = 0x10006AC0
USB_MAIN_BSEC = 0x101BA790
EXPECTED_READ_CALLERS = {
    0x1000CF87,
    0x1000D008,
    0x1000D555,
    0x1000D5D8,
    0x1000D93A,
    0x1000D9C5,
}
EXPECTED_BUILD_CALLERS = {0x1000B2FA, 0x1000D6AF}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dll",
        type=Path,
        required=True,
        help="pinned historical CEMSUsbRegsiter.dll v19.11.4.1",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    data = args.dll.read_bytes()
    if sha256(data) != DLL_SHA256:
        raise SystemExit("historical CEMSUsbRegsiter.dll SHA-256 mismatch")

    pe = pefile.PE(data=data)
    if pe.OPTIONAL_HEADER.ImageBase != IMAGE_BASE:
        raise SystemExit("unexpected PE image base")

    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.detail = True
    def decode_at(va: int):
        raw = pe.get_data(va - IMAGE_BASE, 15)
        decoded = list(md.disasm(raw, va, count=1))
        return decoded[0] if decoded else None

    def decode_range(start: int, end: int):
        raw = pe.get_data(start - IMAGE_BASE, end - start)
        return list(md.disasm(raw, start))

    def require(va: int, mnemonic: str, *needles: str) -> None:
        ins = decode_at(va)
        if ins is None:
            raise SystemExit(f"missing instruction at 0x{va:08X}")
        if ins.mnemonic != mnemonic:
            raise SystemExit(
                f"0x{va:08X}: expected {mnemonic}, got {ins.mnemonic} {ins.op_str}"
            )
        lowered = ins.op_str.lower()
        for needle in needles:
            if needle.lower() not in lowered:
                raise SystemExit(
                    f"0x{va:08X}: missing operand fragment {needle!r}: {ins.op_str}"
                )

    def callsites(target: int) -> set[int]:
        sites: set[int] = set()
        for section in pe.sections:
            if not (section.Characteristics & 0x20000000):
                continue
            raw = section.get_data()
            section_va = IMAGE_BASE + section.VirtualAddress
            for index in range(0, max(0, len(raw) - 4)):
                if raw[index] != 0xE8:
                    continue
                rel = int.from_bytes(raw[index + 1 : index + 5], "little", signed=True)
                site = section_va + index
                if (site + 5 + rel) & 0xFFFFFFFF == target:
                    sites.add(site)
        return sites

    template = pe.get_data(USB_MAIN_BSEC - IMAGE_BASE, 0x200)
    if len(template) != 0x200:
        raise SystemExit("failed to recover complete UsbMainBSec template")
    if template[0x1D0:0x1F4] != bytes(0x24):
        raise SystemExit("v19 UsbMainBSec +0x1D0..+0x1F3 is no longer zero")
    if template[0x1FE:0x200] != b"\x55\xaa":
        raise SystemExit("v19 UsbMainBSec trailing MBR signature changed")

    # BuildSector6: template copy, then cap=0x20 C-string overlay at +0x1D0.
    require(0x1000659E, "mov", "ecx", "0x80")
    require(0x100065AA, "mov", "esi", "0x101ba790")
    require(0x100065B1, "rep movsd")
    require(0x10006648, "lea", "eax", "esp", "0x2d8")
    require(0x1000664F, "push", "esp", "0x2c")
    require(0x10006653, "push", "0x20")
    require(0x10006656, "call", "0x101473fe")

    # Pin every executable reference to the upstream BeiZhu object field.
    # One is the BuildSector6 caller read below; the other is the sole write,
    # which copies object+0x2478 into object+0x2620 with destination cap=16.
    object_beizhu_refs = []
    for section in pe.sections:
        if not (section.Characteristics & 0x20000000):
            continue
        section_va = IMAGE_BASE + section.VirtualAddress
        for ins in md.disasm(section.get_data(), section_va):
            for op in ins.operands:
                if op.type == X86_OP_MEM and op.mem.disp == 0x2620:
                    object_beizhu_refs.append(ins.address)
                    break
    if object_beizhu_refs != [0x1000B219, 0x1000CCEF]:
        raise SystemExit(
            f"object+0x2620 executable reference set changed: {object_beizhu_refs}"
        )
    require(0x1000CCE8, "lea", "eax", "esi", "0x2478")
    require(0x1000CCEE, "push", "eax")
    require(0x1000CCEF, "lea", "eax", "esi", "0x2620")
    require(0x1000CCF5, "push", "0x10")
    require(0x1000CCF7, "push", "eax")
    require(0x1000CCF8, "call", "0x101473fe")

    # Recovered writer caller: zero[32], then strcpy_s(cap=32, object+0x2620).
    require(0x1000B16C, "mov", "byte ptr [ebp - 0x24]", "0")
    require(0x1000B170, "movups", "[ebp - 0x23]", "xmm0")
    require(0x1000B17B, "movq", "[ebp - 0x13]", "xmm0")
    require(0x1000B174, "mov", "dword ptr [ebp - 0xb]", "0")
    require(0x1000B180, "mov", "word ptr [ebp - 7]", "0")
    require(0x1000B186, "mov", "byte ptr [ebp - 5]", "0")
    require(0x1000B219, "lea", "eax", "esi", "0x2620")
    require(0x1000B220, "lea", "eax", "ebp", "0x24")
    require(0x1000B223, "push", "0x20")
    require(0x1000B226, "call", "0x101473fe")
    require(0x1000B2C9, "lea", "eax", "ebp", "0x24")

    # The helper is NUL-terminated strcpy_s, not a raw 32-byte memcpy.
    require(0x10147435, "mov", "al")
    require(0x10147438, "mov", "byte ptr [edi]", "al")
    require(0x1014743B, "test", "al", "al")
    require(0x1014743F, "sub", "ecx", "1")

    # ReadSector6: cap=32 C-string return from decoded sector+0x1D0.
    require(0x10006E07, "lea", "eax", "ebp", "0x1034")
    require(0x10006E0E, "push", "0x20")
    require(0x10006E10, "push", "ebp", "0x1220")
    require(0x10006E16, "call", "0x101473fe")

    if callsites(READ_SECTOR6) != EXPECTED_READ_CALLERS:
        raise SystemExit(f"ReadSector6 caller set changed: {sorted(callsites(READ_SECTOR6))}")
    if callsites(BUILD_SECTOR6) != EXPECTED_BUILD_CALLERS:
        raise SystemExit(f"BuildSector6 caller set changed: {sorted(callsites(BUILD_SECTOR6))}")

    # Caller 1: arg8 local -0x54 is only initialized and passed to two retries.
    caller1_refs = []
    for ins in decode_range(0x1000CE00, 0x1000D349):
        for op in ins.operands:
            if (
                op.type == X86_OP_MEM
                and md.reg_name(op.mem.base) == "ebp"
                and op.mem.disp == -0x54
            ):
                caller1_refs.append(ins.address)
    if caller1_refs != [0x1000CE2A, 0x1000CF56, 0x1000CFD7]:
        raise SystemExit(f"caller1 arg8 reference set changed: {caller1_refs}")

    # Caller 2: successful branch parses another local at esp+0x18; target arg8
    # output is stable esp+0x38 and is not referenced before the next branch.
    for ins in decode_range(0x1000D614, 0x1000D68B):
        for op in ins.operands:
            if (
                op.type == X86_OP_MEM
                and md.reg_name(op.mem.base) == "esp"
                and op.mem.disp == 0x38
            ):
                raise SystemExit(
                    f"caller2 unexpectedly reads arg8 output at 0x{ins.address:08X}"
                )
    require(0x1000D614, "lea", "ecx", "esp", "0x18")

    # Caller 3: the sole post-read value use narrows the C-string into cap=16.
    caller3_refs = []
    for ins in decode_range(0x1000D760, 0x1000DB82):
        for op in ins.operands:
            if (
                op.type == X86_OP_MEM
                and md.reg_name(op.mem.base) == "ebp"
                and op.mem.disp == -0x64
            ):
                caller3_refs.append(ins.address)
    if caller3_refs != [0x1000D7F6, 0x1000D909, 0x1000D994, 0x1000DB1A]:
        raise SystemExit(f"caller3 arg8 reference set changed: {caller3_refs}")
    require(0x1000DB1A, "lea", "eax", "ebp", "0x64")
    require(0x1000DB1E, "lea", "eax", "ebx", "0x140")
    require(0x1000DB24, "push", "0x10")
    require(0x1000DB27, "call", "0x101473fe")

    print(
        json.dumps(
            {
                "dll_sha256": DLL_SHA256,
                "template_sha256": sha256(template),
                "template_lba6_1d0_1f3_zero": True,
                "writer_beizhu_slot": "sector+0x1D0 strcpy_s(cap=32, caller arg8)",
                "writer_object_beizhu": (
                    "object+0x2620 has exactly one executable writer reference: "
                    "strcpy_s(cap=16, object+0x2478), limiting valid text to 15B + NUL"
                ),
                "writer_caller_arg8": (
                    "zero[32] then strcpy_s(cap=32, object+0x2620); because the source "
                    "field is cap=16, byte15 is NUL and bytes16..31 stay zero"
                ),
                "reader_beizhu_slot": "strcpy_s(caller arg8, cap=32, sector+0x1D0)",
                "reader_callsites": [f"0x{x:08X}" for x in sorted(EXPECTED_READ_CALLERS)],
                "upper_consumer": (
                    "post-NUL tail +0x10..+0x1D is not value-parsed by recovered "
                    "callers; one caller narrows the string into a 16-byte object field"
                ),
                "claim_boundary": (
                    "v19 proves the upstream cap=16 BeiZhu limit, forces +0x1DF to "
                    "the string terminator and +0x1E0..+0x1EF to zero in this writer, "
                    "and therefore cannot produce the observed non-zero legacy MBR underlay"
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
