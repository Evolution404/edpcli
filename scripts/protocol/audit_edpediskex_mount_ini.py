#!/usr/bin/env python3
"""Verify EdpEDiskEx mount-session INI writer is not itself an LCE image writer.

SHA-bound bounded machine-code inspection, never executes OEM code or accesses disks.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
from capstone import Cs, CS_ARCH_X86, CS_MODE_32
import pefile

EXPECTED_SHA = "9a665e5c46eaa4076e14e2208d467d2110d33fd5bf1d363b34cc40c517fccaca"
BASE = 0x10000000
METHOD = (0x100010c0, 0x100012e7)
INI_IAT = 0x100302d8
INI_CALL_SITES = (0x100011ca, 0x1000121d, 0x1000125a, 0x1000127c, 0x100012b9)
MOUNT_SESSION_CALLS = (0x10008004, 0x10008292)
KEYS = {
    "GLOBAL": 0x100305d0,
    "LETTER": 0x100305ac,
    "INDEX": 0x100305a0,
    "IMAGE": 0x10030598,
    "SESSION": 0x10030590,
}


def validate_original(path: Path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != EXPECTED_SHA:
        raise ValueError("historical EdpEDiskEx OEM SHA-256 mismatch")
    pe = pefile.PE(data=raw)
    if pe.OPTIONAL_HEADER.ImageBase != BASE or pe.FILE_HEADER.Machine != 0x14C:
        raise ValueError("unexpected native architecture")
    return pe


def audit(path: Path):
    pe = validate_original(path)
    imports = [
        x.address for module in pe.DIRECTORY_ENTRY_IMPORT
        if module.dll.lower() == b"kernel32.dll"
        for x in module.imports if x.name == b"WritePrivateProfileStringA"
    ]
    if imports != [INI_IAT]:
        raise ValueError("historical INI writer IAT drift")
    for name, address in KEYS.items():
        actual = pe.get_data(address - BASE, len(name)+1)
        if actual != name.encode("ascii")+b"\0":
            raise ValueError(f"OEM INI config name drift: {name}")
    d = Cs(CS_ARCH_X86, CS_MODE_32)
    method_ins = list(d.disasm(pe.get_data(METHOD[0]-BASE, METHOD[1]-METHOD[0]), METHOD[0]))
    profile_calls = [x.address for x in method_ins if x.mnemonic == "call"
                     and x.op_str == f"dword ptr [0x{INI_IAT:x}]"]
    if profile_calls != list(INI_CALL_SITES):
        raise ValueError(f"OEM profile-only writer call drift {profile_calls}")
    if method_ins[-1].mnemonic != "ret":
        raise ValueError("bounded method ending drift")
    if any(x.mnemonic == "call" and ("0x1003026c" in x.op_str or "0x1003028c" in x.op_str)
           for x in method_ins):
        raise ValueError("bounded INI method contains direct disk I/O call")
    calls = []
    for va in MOUNT_SESSION_CALLS:
        match = list(d.disasm(pe.get_data(va-BASE, 10), va, count=1))
        if not match or match[0].mnemonic != "call" or match[0].op_str != f"0x{METHOD[0]:x}":
            raise ValueError(f"mount-session static edge drift at {va:#x}")
        calls.append(hex(va))
    return {
        "schema": 1,
        "oem_sha256": EXPECTED_SHA,
        "mount_setup_calls": calls,
        "mount_session_ini_helper_va": hex(METHOD[0]),
        "bounded_method_end_va": hex(METHOD[1]),
        "actual_ini_writer_import": "KERNEL32!WritePrivateProfileStringA",
        "actual_ini_write_call_sites": [hex(x) for x in profile_calls],
        "ini_entry_names": list(KEYS),
        "direct_disk_WriteFile_or_DeviceIoControl_within_helper": False,
        "whole_mount_pipeline_avoids_disk_writes_proven": False,
        "compatibility_LCE_first_writer_identified": False,
        "read_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--edpediskex", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.edpediskex), indent=2))


if __name__ == "__main__":
    main()
