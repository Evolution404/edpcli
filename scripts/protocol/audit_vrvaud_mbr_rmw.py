#!/usr/bin/env python3
"""SHA-bound historical OEM MBR partition-entry read-modify-write trace.

Original Windows PE files are READ ONLY. This never opens a block device or
executes OEM code. The simulation operates on caller-provided in-memory bytes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
import pefile

GOLD = bytes.fromhex(
    "0001010004fe3f823f00000001000000"
    "0002020008fe3f824000000001000000"
    "0003030008fe3f824100000001000000"
)
GOLD_SHA = "73b404e53fe7ef9142051444654b874de63af8557f21caa032e462e519c0bd81"
ENTRY_START = 0x1CE  # second MBR partition slot, i.e. 462 bytes
VERSIONS = {
    "legacy_2022": {
        "sha": "57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f",
        "raw_offset": 0x1693B8, "source_va": 0x1016B5B8,
        "impl": 0x10030BC0, "thunk": 0x10002360, "copy": 0x100DBE20,
        "iat": {"ReadFile": 0x1023459C, "SetFilePointer": 0x10234598,
                "WriteFile": 0x1023453C},
        "sites": [
            (0x10002360, "jmp", "0x10030bc0"),
            (0x10030BEC, "push", "0x1000"),
            (0x10030BF3, "lea", "[ebp - 0x2018]"),
            (0x10030C2A, "mov", "[0x1017250c]"),
            (0x10030C38, "call", "[0x10234598]"),
            (0x10030C4A, "mov", "[0x1017250c]"),
            (0x10030C51, "lea", "[ebp - 0x2018]"),
            (0x10030C5C, "call", "[0x1023459c]"),
            (0x10030C62, "push", "0x200"),
            (0x10030C67, "lea", "[ebp - 0x2018]"),
            (0x10030C6E, "lea", "[ebp - 0x1010]"),
            (0x10030C75, "call", "0x100dbe20"),
            (0x10030C7D, "push", "0x30"),
            (0x10030C7F, "push", "0x1016b5b8"),
            (0x10030C84, "lea", "[ebp - 0x1e4a]"),
            (0x10030C8B, "call", "0x100dbe20"),
            (0x10030CA5, "call", "[0x10234598]"),
            (0x10030CB1, "mov", "[0x1017250c]"),
            (0x10030CB8, "lea", "[ebp - 0x2018]"),
            (0x10030CC3, "call", "[0x1023453c]"),
        ],
    },
    "current_2026": {
        "sha": "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069",
        "raw_offset": 0x2039E0, "source_va": 0x102055E0,
        "impl": 0x10039510, "thunk": 0x10002798, "copy": 0x10002D01,
        "iat": {"ReadFile": 0x102D1778, "SetFilePointer": 0x102D16E4,
                "WriteFile": 0x102D1774},
        "sites": [
            (0x10002798, "jmp", "0x10039510"),
            (0x1003953E, "push", "0x1000"),
            (0x10039545, "lea", "[ebp - 0x2018]"),
            (0x1003957C, "mov", "[0x1020bf2c]"),
            (0x1003958A, "call", "[0x102d16e4]"),
            (0x1003959C, "mov", "[0x1020bf2c]"),
            (0x100395A3, "lea", "[ebp - 0x2018]"),
            (0x100395AE, "call", "[0x102d1778]"),
            (0x100395B6, "push", "0x200"),
            (0x100395BB, "lea", "[ebp - 0x2018]"),
            (0x100395C2, "lea", "[ebp - 0x1010]"),
            (0x100395C9, "call", "0x10002d01"),
            (0x100395D3, "push", "0x30"),
            (0x100395D5, "push", "0x102055e0"),
            (0x100395DA, "lea", "[ebp - 0x1e4a]"),
            (0x100395E1, "call", "0x10002d01"),
            (0x100395FB, "call", "[0x102d16e4]"),
            (0x10039607, "mov", "[0x1020bf2c]"),
            (0x1003960E, "lea", "[ebp - 0x2018]"),
            (0x10039619, "call", "[0x102d1774]"),
        ],
    },
}


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def simulated_lba0_patch(original_sector: bytes, native_sector_size: int) -> bytes:
    """Pure-memory model of second-through-fourth partition entry replacement.

    This is an exact byte-range model of the confirmed copy, *not* device I/O,
    and it does not simulate the surrounding OEM execution or full MBR logic.
    """
    if native_sector_size not in (512, 4096) or len(original_sector) != native_sector_size:
        raise ValueError("must provide exactly one 512B or 4096B native sector")
    result = bytearray(original_sector)
    result[ENTRY_START:ENTRY_START + len(GOLD)] = GOLD
    return bytes(result)


def audit_original(path: Path, version: str) -> dict:
    if version not in VERSIONS:
        raise ValueError(f"unknown historical OEM edition {version}")
    spec = VERSIONS[version]
    raw = path.read_bytes()
    if sha(raw) != spec["sha"]:
        raise ValueError(f"original OEM binary SHA-256 mismatch: {path.name}")
    pe = pefile.PE(data=raw)
    if pe.FILE_HEADER.Machine != 0x14C or pe.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError("unexpected OEM x86 PE identity")
    offset = spec["raw_offset"]
    if raw[offset:offset+48] != GOLD or sha(raw[offset:offset+48]) != GOLD_SHA:
        raise ValueError("original OEM MBR partition template mismatch")
    if 0x10000000 + pe.get_rva_from_offset(offset) != spec["source_va"]:
        raise ValueError("original OEM MBR address mapping mismatch")
    imports = {i.name.decode():i.address for m in pe.DIRECTORY_ENTRY_IMPORT
               if m.dll.lower()==b"kernel32.dll" for i in m.imports
               if i.name in (b"SetFilePointer",b"ReadFile",b"WriteFile")}
    if imports != spec["iat"]:
        raise ValueError(f"Win32 MBR transport imports changed: {imports}")
    disassembler = Cs(CS_ARCH_X86, CS_MODE_32)
    sites = []
    for va, expected, fragment in spec["sites"]:
        op = list(disassembler.disasm(pe.get_data(va-0x10000000,15),va,count=1))
        if not op or op[0].mnemonic != expected or fragment not in op[0].op_str:
            actual = "missing" if not op else f"{op[0].mnemonic} {op[0].op_str}"
            raise ValueError(f"original MBR machine code mismatch {va:#x}: {actual}")
        sites.append({"va":hex(va),"opcode":f"{op[0].mnemonic} {op[0].op_str}"})
    entries=[]
    for index in range(3):
        e=GOLD[index*16:(index+1)*16]
        entries.append({"MBR_slot":index+2,"partition_type":f"0x{e[4]:02x}",
                        "start_lba":struct.unpack_from("<I",e,8)[0],
                        "length_lba":struct.unpack_from("<I",e,12)[0]})
    return {
        "OEM_original_sha256": spec["sha"],
        "MBR_entry_template_sha256": GOLD_SHA,
        "MBR_entry_template_file_offset":hex(offset),
        "MBR_entry_template_va":hex(spec["source_va"]),
        "implementation_va":hex(spec["impl"]),
        "thunk_va":hex(spec["thunk"]),
        "MBR_copy_destination_stack_offset":hex(-0x1E4A & 0xFFFFFFFF),
        "MBR_work_buffer_stack_offset":hex(-0x2018 & 0xFFFFFFFF),
        "MBR_copy_relative_offset":ENTRY_START,
        "MBR_copy_length_bytes":48,
        "native_sector_bytes_source":"runtime global, one native sector",
        "actual_hardware_execution_observed":False,
        "verified_machine_opcodes":sites,
        "MBR_slots_2_to_4":entries,
        "test_only_native_512_patch_changes_only_462_to_509": True,
        "test_only_native_4096_patch_preserves_510_to_4095": True,
    }


def audit(old:Path,current:Path) -> dict:
    if sha(GOLD)!=GOLD_SHA:raise ValueError("source MBR 48B template SHA mismatch")
    editions={"legacy_2022":audit_original(old,"legacy_2022"),
              "current_2026":audit_original(current,"current_2026")}
    sample=bytes((i*11+3)%256 for i in range(4096))
    for width in (512,4096):
        after=simulated_lba0_patch(sample[:width],width)
        if after[:ENTRY_START]!=sample[:ENTRY_START] or after[ENTRY_START+48:]!=sample[ENTRY_START+48:width]:
            raise ValueError("offline MBR simulator touched bytes outside three entries")
    return {
        "schema":1,"scope":"x86 OEM MBR code template and proven write call, not LCE",
        "editions":editions,
        "directly_references_3072B_FAT16_warning":False,
        "proves_OEM_4Kn_device_success":False,
        "first_LCE_FAT16_physical_writer_identified":False,
        "read_only_host_files":True,"no_device_access":True,
    }


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old",type=Path,required=True)
    parser.add_argument("--current",type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(audit(args.old,args.current),ensure_ascii=False,indent=2))


if __name__=="__main__":
    main()
