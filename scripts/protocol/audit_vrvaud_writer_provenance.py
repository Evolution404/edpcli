#!/usr/bin/env python3
"""SHA-bound offline provenance trace for vrvaud_c.dll policy callbacks and WriteDiskEx.

No OEM code execution, devices, registry access, secrets or disk writes.
Scope: identified callback slots, direct code references, native-sector writer ABI.
Missing direct xrefs do not exclude runtime-computed or cross-module call paths.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
from capstone.x86 import X86_OP_IMM, X86_OP_MEM
import pefile

BASE = 0x10000000
VERSIONS = {
    "legacy_2022": {
        "sha": "57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f",
        "template_offset": 0x1697d8, "object": 0x1022df18,
        "export": 0x10001474, "factory": 0x100a39c0, "consumer": 0x10035a90,
        "init_thunk": 0x10002478, "init": 0x10089c80,
        "callback_calls": (0x10035aa5, 0x10035ab8),
        "string_vas": (0x101460c8, 0x101460b4, 0x101460a0),
        "pushes": (0x10089ca0, 0x10089cba, 0x10089cd2),
    },
    "current_2026": {
        "sha": "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069",
        "template_offset": 0x203ee8, "object": 0x102c8850,
        "export": 0x10001370, "factory": 0x100c55d0, "consumer": 0x100434d0,
        "init_thunk": 0x1000290f, "init": 0x100a69c0,
        "callback_calls": (0x100434e5, 0x100434f8),
        "string_vas": (0x101d1c2c, 0x101d1c18, 0x101d1c04),
        "pushes": (0x100a69e0, 0x100a69fa, 0x100a6a12),
    },
}


def original_pe(path: Path, expected: str):
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != expected:
        raise ValueError(f"OEM vrvaud SHA-256 mismatch: {path.name}, got {digest}")
    pe = pefile.PE(data=raw)
    if pe.FILE_HEADER.Machine != 0x14c or pe.OPTIONAL_HEADER.ImageBase != BASE:
        raise ValueError("OEM PE machine or base mismatch")
    return raw, pe


def site(pe, va: int, mnemonic: str, operand_part: str):
    decoder = Cs(CS_ARCH_X86, CS_MODE_32)
    matches = list(decoder.disasm(pe.get_data(va - BASE, 15), va, count=1))
    if not matches or matches[0].mnemonic != mnemonic or operand_part not in matches[0].op_str:
        actual = "missing" if not matches else matches[0].mnemonic + " " + matches[0].op_str
        raise ValueError(f"OEM opcode mismatch at 0x{va:x}, got {actual}")
    i = matches[0]
    return {"va": f"0x{va:x}", "opcode": f"{i.mnemonic} {i.op_str}"}


def validate_export(pe, address: int):
    names = {x.name: BASE + x.address for x in pe.DIRECTORY_ENTRY_EXPORT.symbols if x.name}
    if names.get(b"GetPolicyObject") != address:
        raise ValueError("GetPolicyObject export has changed")


def as_cstring(pe, va: int):
    return pe.get_data(va - BASE, 64).split(b"\x00")[0].decode("ascii")


def direct_target_refs(pe, targets: set[int]):
    """Collect exact immediate and absolute-memory operands in linear .text scan."""
    d = Cs(CS_ARCH_X86, CS_MODE_32)
    d.detail = True
    results = {target: [] for target in targets}
    for section in pe.sections:
        if section.Name.rstrip(b"\x00") != b".text":
            continue
        for ins in d.disasm(section.get_data(), BASE + section.VirtualAddress):
            if any(
                op.type == X86_OP_IMM and op.imm in targets
                or op.type == X86_OP_MEM and op.mem.disp in targets
                for op in ins.operands
            ):
                for op in ins.operands:
                    target = op.imm if op.type == X86_OP_IMM else op.mem.disp if op.type == X86_OP_MEM else None
                    if target in results:
                        results[target].append(f"0x{ins.address:x}")
    return {f"0x{target:x}": refs for target, refs in sorted(results.items())}


def inspect_version(path: Path, version: str):
    spec = VERSIONS[version]
    raw, pe = original_pe(path, spec["sha"])
    validate_export(pe, spec["export"])
    obj = spec["object"]
    factory = spec["factory"]
    cb_a, cb_b = spec["callback_calls"]
    init_thunk = spec["init_thunk"]
    init = spec["init"]
    slots = [
        site(pe, spec["export"], "jmp", f"0x{factory:x}"),
        site(pe, factory + 3, "mov", f"0x{obj:x}"),
        site(pe, spec["consumer"] + 3, "mov", f"0x{obj:x}"),
        site(pe, cb_a, "call", f"[0x{obj + 12:x}]"),
        site(pe, cb_b, "call", f"[0x{obj + 16:x}]"),
        site(pe, init_thunk, "jmp", f"0x{init:x}"),
    ]
    for va, string_va in zip(spec["pushes"], spec["string_vas"]):
        slots.append(site(pe, va, "push", f"0x{string_va:x}"))
    strings = [as_cstring(pe, va) for va in spec["string_vas"]]
    if strings != ["DeviceNumber.dll", "EDP_DiskNumber", "EDP_DeviceNumber"]:
        raise ValueError(f"DeviceNumber dynamic function names changed: {strings}")
    data = next(s for s in pe.sections if s.Name.rstrip(b"\x00") == b".data")
    rva = obj - BASE
    if not (data.VirtualAddress + data.SizeOfRawData <= rva < data.VirtualAddress + data.Misc_VirtualSize):
        raise ValueError("Policy object must be within zero-initialized PE .data extension")
    begin = spec["template_offset"]
    plain = Path(__file__).resolve().parents[2].joinpath(
        "audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin"
    ).read_bytes()
    if len(plain) != 3072 or raw[begin:begin+3072] != plain:
        raise ValueError("OEM 3072B FAT16 gold mismatch")
    trailing = raw[begin+3072:begin+4096]
    if len(trailing) != 1024 or sum(x != 0 for x in trailing) != 275:
        raise ValueError("Unexpected 4KiB-after-template host PE layout")
    for marker in (b".?AVCPolicy@@", b".?AVCPolicyBase@@", b".?AVCProcessManager@@"):
        if marker not in trailing:
            raise ValueError("RTTI descriptor missing in 1024B beyond FAT16 fixture")
    return {
        "version": version,
        "sha256": spec["sha"],
        "policy_object": f"0x{obj:x}",
        "policy_object_is_pe_zero_initialized_storage": True,
        "callback_slots": [f"0x{obj+12:x}", f"0x{obj+16:x}"],
        "dynamic_module": strings[0],
        "dynamic_exports": strings[1:],
        "verified_opcode_sites": slots,
        "template_file_offset": f"0x{begin:x}",
        "fat16_template_bytes": 3072,
        "following_1024_pe_bytes_nonzero": 275,
        "following_1024_pe_bytes_sha256": hashlib.sha256(trailing).hexdigest(),
        "following_bytes_include_rtti_not_disk_payload": True,
    }, pe


def audit(legacy_path: Path, current_path: Path):
    legacy, old_pe = inspect_version(legacy_path, "legacy_2022")
    current, new_pe = inspect_version(current_path, "current_2026")
    writer = 0x100dbd40
    thunk = 0x10002ea5
    write_disk = 0x100db7b0
    write_disk_thunk = 0x10002b58
    # Verify exact original x86 argument propagation, not decompiler variable aliases.
    writer_opcodes = [
        site(new_pe, thunk, "jmp", f"0x{writer:x}"),
        site(new_pe, write_disk_thunk, "jmp", f"0x{write_disk:x}"),
        site(new_pe, writer + 0x2c, "mov", "0x200"),
        site(new_pe, 0x100dbebf, "mov", "[ebp + 0x14]"),
        site(new_pe, 0x100dbec2, "imul", "[ebp - 0x14]"),
        site(new_pe, 0x100dbec7, "mov", "[ebp + 0x18]"),
        site(new_pe, 0x100dbecf, "call", "[0x102d1774]"),
        site(new_pe, 0x100dbea3, "call", "[0x102d16e4]"),
        site(new_pe, 0x100db962, "mov", "[ebp + 0x14]"),
        site(new_pe, 0x100db96a, "mov", "[ebp + 0x18]"),
    ]
    refs = direct_target_refs(new_pe, {writer, thunk, write_disk, write_disk_thunk})
    if refs[f"0x{writer:x}"] != [f"0x{thunk:x}"] or refs[f"0x{write_disk:x}"] != [f"0x{write_disk_thunk:x}"]:
        raise ValueError("OEM writer implementation inbound direct call landscape changed")
    if refs[f"0x{thunk:x}"] or refs[f"0x{write_disk_thunk:x}"]:
        raise ValueError("OEM previously-unreferenced writer thunk now has a static caller")
    exps = {e.name: e.address for e in new_pe.DIRECTORY_ENTRY_EXPORT.symbols if e.name}
    if writer - BASE in exps.values() or thunk - BASE in exps.values():
        raise ValueError("Writer became directly exported")
    imports = {
        x.name: x.address for m in new_pe.DIRECTORY_ENTRY_IMPORT
        if m.dll.lower() == b"kernel32.dll"
        for x in m.imports if x.name
    }
    if imports.get(b"WriteFile") != 0x102d1774 or imports.get(b"SetFilePointer") != 0x102d16e4:
        raise ValueError("OEM WriteFile/SetFilePointer imports changed")
    return {
        "policy_callbacks": [legacy, current],
        "writer_2026": {
            "WriteDiskEx_impl": f"0x{writer:x}", "WriteDiskEx_thunk": f"0x{thunk:x}",
            "WriteDisk_impl": f"0x{write_disk:x}", "WriteDisk_thunk": f"0x{write_disk_thunk:x}",
            "verified_machine_code": writer_opcodes,
            "direct_static_code_refs": refs,
            "start_sector": "two DWORD args (arg1 low, arg2 high) passed to integer multiply helper with bytes_per_sector",
            "sector_count": "arg3 at [ebp+0x14]",
            "payload_buffer": "arg4 at [ebp+0x18] (verified push before Win32 WriteFile)",
            "write_length": "arg3 * native_bytes_per_sector",
            "bytes_per_sector": "0x200 fallback; runtime native geometry query may replace it",
            "exported_writer": False,
            "static_caller_found": False,
            "dynamic_or_external_caller_excluded": False,
        },
        "scope": "SHA-bound bounded PE static investigation, not OEM LCE physical write proof",
        "not_proven": [
            "caller which originally supplied legacy FAT16 template to physical or virtual writer",
            "4096B U391 writer and last 1024B semantics",
            "whether these writer thunks are reached by runtime-computed control flow",
        ],
        "disk_access": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(audit(args.legacy, args.current), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
