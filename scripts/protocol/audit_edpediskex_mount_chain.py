#!/usr/bin/env python3
"""Static, SHA-guarded proof of OEM mount-IOCTL-to-physical-write transport.

Only reads explicit original binary paths; never opens a disk or runs OEM code.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
from capstone import Cs, CS_ARCH_X86, CS_MODE_32, CS_MODE_64
import pefile

EX_SHA = "9a665e5c46eaa4076e14e2208d467d2110d33fd5bf1d363b34cc40c517fccaca"
SYS_SHA = "724544a96f899b9bb0f87a8adedd08a961d8e4b8ec40aa90bc8144e5ae7e0620"
# (binary, virtual address, instruction mnemonic, expected operand substring)
EX_SITES = [
    (0x100098e2, "call", "0x10007480"),
    (0x10007852, "call", "0x10007c10"),
    (0x10007c98, "mov", "0x8200e000"),
    (0x1000807c, "mov", "0x8200e00c"),
    (0x100080ba, "call", "[0x1003028c]"),
]
SYS_SITES = [
    (0x11b37, "cmp", "0x8200e00c"), (0x11b54, "cmp", "0x8200e000"),
    (0x11b3c, "je", "0x11bfe"), (0x11b59, "je", "0x11bfe"),
    (0x120f2, "test", "rax"), (0x12100, "cmp", "[r15 + 0x20]"),
    (0x1214d, "mov", "[r15 + 0x28]"), (0x1215a, "add", "[rbx + 0x18]"),
    (0x12163, "mov", "[rsp + 0x50]"), (0x1218e, "call", "0x183c0"),
    (0x12313, "mov", "r13d"), (0x12324, "mov", "rbp"),
    (0x1232e, "call", "[rip + 0x6d7c]"),
]


def verified_pe(path: Path, expected_sha: str, machine: int, image_base: int):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != expected_sha:
        raise ValueError(f"Original OEM SHA-256 mismatch for {path.name}: {digest}")
    pe = pefile.PE(str(path))
    if pe.FILE_HEADER.Machine != machine or pe.OPTIONAL_HEADER.ImageBase != image_base:
        raise ValueError("Original OEM PE machine/image-base mismatch")
    return pe


def check_sites(pe, sites):
    dis = Cs(CS_ARCH_X86, CS_MODE_32 if pe.FILE_HEADER.Machine == 0x14c else CS_MODE_64)
    rows = []
    for va, expected_mnemonic, operand_fragment in sites:
        rva = va - pe.OPTIONAL_HEADER.ImageBase
        instructions = list(dis.disasm(pe.get_data(rva, 15), va, count=1))
        if not instructions:
            raise ValueError(f"Missing OEM opcode at 0x{va:x}")
        inst = instructions[0]
        if inst.mnemonic != expected_mnemonic or operand_fragment not in inst.op_str:
            raise ValueError(f"Original OEM instruction drift at 0x{va:x}: {inst.mnemonic} {inst.op_str}")
        rows.append({"va": f"0x{va:x}", "instruction": inst.mnemonic + " " + inst.op_str})
    return rows


def check_import(pe, module: bytes, name: bytes, expected_va: int):
    matches = [i.address for m in pe.DIRECTORY_ENTRY_IMPORT if m.dll.lower() == module.lower()
               for i in m.imports if i.name == name]
    if matches != [expected_va]:
        raise ValueError(f"Original OEM IAT mismatch: {module!r}!{name!r}")
    return {"symbol": module.decode() + "!" + name.decode(), "iat": f"0x{expected_va:x}"}


def audit(ex_path: Path, sys_path: Path):
    ex = verified_pe(ex_path, EX_SHA, 0x14c, 0x10000000)
    sys = verified_pe(sys_path, SYS_SHA, 0x8664, 0x10000)
    exports = {e.name: e.address for e in ex.DIRECTORY_ENTRY_EXPORT.symbols if e.name}
    if exports.get(b"EdpMountFile") != 0x98b0:
        raise ValueError("Original OEM EdpMountFile export changed")
    return {
        "binary_sha256": {"EdpEDiskEx.dll": EX_SHA, "EdpEDisk64.sys": SYS_SHA},
        "import_boundaries": [
            check_import(ex, b"KERNEL32.dll", b"DeviceIoControl", 0x1003028c),
            check_import(sys, b"ntoskrnl.exe", b"ZwWriteFile", 0x190b0),
        ],
        "verified_user_mount_instructions": check_sites(ex, EX_SITES),
        "verified_kernel_dispatch_and_write_instructions": check_sites(sys, SYS_SITES),
        "mount_path": "EdpMountFile RVA 0x98b0 -> 0x10007480 -> 0x10007c10 -> DeviceIoControl(0x8200e000|0x8200e00c) -> kernel shared 0x11bfe",
        "mount_selection": "0x8200e00c if sub_10007c10 arg1 == 1, else 0x8200e000",
        "write_mapping": "virtual length [rbx+8]; bound [r15+0x20]; physical offset [r15+0x28]+[rbx+0x18]; copied pool buffer rbp; ZwWriteFile 0x1232e",
        "verified_scope": "conditional mount transport and virtual-write-to-backing-file transport ONLY",
        "unproven": ["LCE virtual extent was mounted during provisioning",
                     "which caller supplied FAT16 3072B template",
                     "any current U391 4096B payload producer", "last 1024B owner"],
        "read_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--edpediskex", type=Path, required=True)
    parser.add_argument("--edpedisk64", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(audit(args.edpediskex, args.edpedisk64), indent=2))


if __name__ == "__main__":
    main()
