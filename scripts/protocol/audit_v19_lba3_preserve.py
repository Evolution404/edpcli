#!/usr/bin/env python3
"""Reproducible v19.11.4.1 audit proving EDP does not own host LBA3.

This script is read-only. It pins the historical CEMSUsbRegsiter.dll by SHA-256,
enumerates every imported SetFilePointer call, symbolically recovers fixed
sector-size multipliers, and verifies that no fixed seek targets sector 3.

The claim is deliberately scoped to EDP protocol ownership. It does not decode
or assign meanings to the external manufacturer's private LBA3 payload.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import pefile
from capstone import Cs, CS_ARCH_X86, CS_MODE_32
from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_OP_REG

DLL_SHA256 = "584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814"
SAFE6_ENTRY = 0x1000CC50
SAFE6_END = 0x1000D350
GENERIC_SEEK_WRAPPERS = {0x10014B30, 0x10014DCF}
EXPECTED_SETFILEPOINTER_SITES = 31
EXPECTED_FIXED_MULTIPLIERS = {1, 2, 4, 6, 7, 8, 12}
SECTOR_SIZE_MEMBER_OFFSETS = {0x1008, 0x2134}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dll",
        type=Path,
        default=Path("/private/tmp/cemsusbregsiter_19.11.4.1.dll"),
        help="pinned historical CEMSUsbRegsiter.dll v19.11.4.1",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    data = args.dll.read_bytes()
    if sha256(data) != DLL_SHA256:
        raise SystemExit("historical CEMSUsbRegsiter.dll SHA-256 mismatch")

    pe = pefile.PE(data=data)
    base = pe.OPTIONAL_HEADER.ImageBase
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.detail = True

    imports: dict[int, tuple[str, str]] = {}
    for desc in pe.DIRECTORY_ENTRY_IMPORT:
        dll = desc.dll.decode(errors="ignore")
        for imp in desc.imports:
            if imp.name:
                imports[imp.address] = (dll, imp.name.decode(errors="ignore"))

    setfilepointer_iat = {
        address for address, (_dll, name) in imports.items() if name == "SetFilePointer"
    }
    if len(setfilepointer_iat) != 1:
        raise SystemExit("expected exactly one SetFilePointer IAT entry")

    def reg_name(reg: int) -> str | None:
        return md.reg_name(reg) if reg else None

    def operand_value(op, state):
        if op.type == X86_OP_REG:
            return state.get(reg_name(op.reg))
        if op.type == X86_OP_IMM:
            return ("const", op.imm & 0xFFFFFFFF)
        if op.type == X86_OP_MEM and op.mem.disp in SECTOR_SIZE_MEMBER_OFFSETS:
            return ("coef", 1)
        return None

    def combine(a, b, sign: int):
        if a and b and a[0] == "coef" and b[0] == "coef":
            return ("coef", a[1] + sign * b[1])
        if a and a[0] == "coef" and b == ("const", 0):
            return a
        return None

    rows: list[dict[str, object]] = []
    for section in pe.sections:
        if not (section.Characteristics & 0x20000000):
            continue

        instructions = list(md.disasm(section.get_data(), base + section.VirtualAddress))
        state: dict[str, tuple[str, int] | None] = {}
        pushes: list[tuple[int, int, tuple[str, int] | None, str]] = []

        for index, ins in enumerate(instructions):
            ops = ins.operands
            if ins.mnemonic == "mov" and len(ops) >= 2 and ops[0].type == X86_OP_REG:
                state[reg_name(ops[0].reg)] = operand_value(ops[1], state)
            elif (
                ins.mnemonic == "lea"
                and len(ops) >= 2
                and ops[0].type == X86_OP_REG
                and ops[1].type == X86_OP_MEM
            ):
                dst = reg_name(ops[0].reg)
                mem = ops[1].mem
                base_value = state.get(reg_name(mem.base)) if mem.base else ("const", 0)
                index_value = state.get(reg_name(mem.index)) if mem.index else ("const", 0)
                result = None
                if base_value and base_value[0] == "coef":
                    if not mem.index:
                        result = base_value
                    elif index_value and index_value[0] == "coef":
                        result = ("coef", base_value[1] + index_value[1] * mem.scale)
                elif not mem.base and index_value and index_value[0] == "coef":
                    result = ("coef", index_value[1] * mem.scale)
                state[dst] = result
            elif (
                ins.mnemonic in ("shl", "sal")
                and len(ops) >= 2
                and ops[0].type == X86_OP_REG
                and ops[1].type == X86_OP_IMM
            ):
                reg = reg_name(ops[0].reg)
                value = state.get(reg)
                state[reg] = (
                    ("coef", value[1] * (1 << ops[1].imm))
                    if value and value[0] == "coef"
                    else None
                )
            elif (
                ins.mnemonic in ("add", "sub")
                and len(ops) >= 2
                and ops[0].type == X86_OP_REG
            ):
                reg = reg_name(ops[0].reg)
                state[reg] = combine(
                    state.get(reg),
                    operand_value(ops[1], state),
                    1 if ins.mnemonic == "add" else -1,
                )
            elif (
                ins.mnemonic == "xor"
                and len(ops) >= 2
                and ops[0].type == X86_OP_REG
                and ops[1].type == X86_OP_REG
                and ops[0].reg == ops[1].reg
            ):
                state[reg_name(ops[0].reg)] = ("const", 0)

            if ins.mnemonic == "push" and ops:
                pushes.append((index, ins.address, operand_value(ops[0], state), ins.op_str))
                pushes = pushes[-12:]

            if ins.mnemonic == "call" and ops:
                op = ops[0]
                if (
                    op.type == X86_OP_MEM
                    and op.mem.base == 0
                    and op.mem.index == 0
                    and (op.mem.disp & 0xFFFFFFFF) in setfilepointer_iat
                ):
                    recent = [item for item in pushes if item[0] >= index - 12]
                    four = recent[-4:] if len(recent) >= 4 else recent
                    low = four[-2] if len(four) >= 4 else None
                    multiplier = (
                        low[2][1]
                        if low and low[2] and low[2][0] == "coef"
                        else None
                    )
                    rows.append(
                        {
                            "call_va": f"0x{ins.address:08X}",
                            "fixed_sector_multiplier": multiplier,
                        }
                    )

                state["eax"] = state["ecx"] = state["edx"] = None
                pushes = []

    if len(rows) != EXPECTED_SETFILEPOINTER_SITES:
        raise SystemExit(
            f"SetFilePointer site count changed: {len(rows)} != {EXPECTED_SETFILEPOINTER_SITES}"
        )

    fixed = {
        row["fixed_sector_multiplier"]
        for row in rows
        if row["fixed_sector_multiplier"] is not None
    }
    if fixed != EXPECTED_FIXED_MULTIPLIERS:
        raise SystemExit(f"fixed sector multiplier set changed: {sorted(fixed)}")
    if 3 in fixed:
        raise SystemExit("historical writer unexpectedly contains a fixed LBA3 seek")

    safe6_off = pe.get_offset_from_rva(SAFE6_ENTRY - base)
    direct_calls: set[int] = set()
    for ins in md.disasm(
        data[safe6_off : safe6_off + (SAFE6_END - SAFE6_ENTRY)],
        SAFE6_ENTRY,
    ):
        if ins.mnemonic == "call" and ins.operands and ins.operands[0].type == X86_OP_IMM:
            direct_calls.add(ins.operands[0].imm & 0xFFFFFFFF)

    if direct_calls & GENERIC_SEEK_WRAPPERS:
        raise SystemExit("SAFE6 entry unexpectedly calls a generic absolute-seek wrapper")

    print(
        json.dumps(
            {
                "dll_sha256": DLL_SHA256,
                "setfilepointer_sites": len(rows),
                "fixed_sector_multipliers": sorted(fixed),
                "lba3_fixed_seek_present": False,
                "safe6_entry": f"0x{SAFE6_ENTRY:08X}",
                "safe6_direct_generic_seek_wrapper_calls": [],
                "claim_boundary": (
                    "historical v19.11.4.1 EDP has no dedicated fixed LBA3 seek; "
                    "external manufacturer payload internals remain opaque"
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
