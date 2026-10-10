#!/usr/bin/env python3
"""Bounded OEM upgrade control-flow trace: disk flags vs client files vs external updater.

Offline, SHA-bound, x86-only static analysis. This module never executes OEM
programs or opens disks. An external subprocess launched by OEM is a trace
boundary, not proof it actually ran or generated LCE.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
import pefile

KNOWN_SHA = {
    "legacy_2022": "57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f",
    "current_2026": "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069",
}
NEW_SITES = [
    (0x100A2683, "push", "0x101d0fd8"),
    (0x100A270F, "movzx", "[0x1020bf39]"),
    (0x100A271A, "cmp", "[0x1029f0d4]"),
    (0x100A2723, "cmp", "[0x1029f0d8]"),
    (0x100A27E9, "movzx", "[0x10206c99]"),
    (0x100A27F1, "call", "0x10002414"),
    (0x10002414, "jmp", "0x100b0f30"),
    (0x100A1E33, "call", "[0x102d1668]"),
    (0x100A2127, "call", "0x10002a22"),
    (0x100A2299, "call", "0x10002a22"),
    (0x100A23A2, "call", "0x10002a22"),
    (0x10002A22, "jmp", "0x100a0400"),
    (0x100A05CD, "call", "0x10002a22"),
    (0x100A0665, "call", "[0x102d168c]"),
    (0x100A06F3, "call", "[0x102d168c]"),
    (0x100A07F4, "call", "[0x102d1634]"),
    (0x100A1695, "push", "0x6b4"),
    (0x100A16A5, "call", "[0x102d1774]"),
    (0x100A17BD, "push", "0x1000385a"),
    (0x100A17C6, "call", "[0x102d16f0]"),
    (0x1000385A, "jmp", "0x100a7230"),
    (0x100A7620, "call", "[0x102d176c]"),
    (0x100A76BA, "call", "[0x102d177c]"),
    (0x100A76CE, "call", "[0x102d1664]"),
]
OLD_SITES = [
    (0x10086493, "push", "0x10145884"),
    (0x10086505, "cmp", "[ecx + 0x4c]"),
    (0x1008650E, "cmp", "[edx + 0x8c]"),
    (0x1008655E, "call", "0x10001feb"),
    (0x10001FEB, "jmp", "0x100923d0"),
]
IAT = {
    b"GetDiskFreeSpaceExA": 0x102D1668,
    b"DeleteFileA": 0x102D168C,
    b"RemoveDirectoryA": 0x102D1634,
    b"WriteFile": 0x102D1774,
    b"CreateThread": 0x102D16F0,
    b"CreateProcessA": 0x102D176C,
    b"WaitForSingleObject": 0x102D177C,
    b"GetExitCodeProcess": 0x102D1664,
}


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def load_original(path: Path, release: str):
    raw = path.read_bytes()
    if sha256(raw) != KNOWN_SHA[release]:
        raise ValueError(f"OEM SHA-256 mismatch for {release}: {path.name}")
    pe = pefile.PE(data=raw)
    if pe.FILE_HEADER.Machine != 0x14C or pe.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError("unexpected original x86 PE")
    return raw, pe


def exact_site(pe: pefile.PE, address: int, mnemonic: str, operand: str):
    decoder = Cs(CS_ARCH_X86, CS_MODE_32)
    lines = list(decoder.disasm(pe.get_data(address-0x10000000, 15), address, count=1))
    if not lines or lines[0].mnemonic != mnemonic or operand not in lines[0].op_str:
        actual = "MISSING" if not lines else lines[0].mnemonic+" "+lines[0].op_str
        raise ValueError(f"OEM opcode changed at {address:#x}: {actual}")
    i = lines[0]
    return {"va": hex(address), "opcode": i.mnemonic+" "+i.op_str}


def bounded_direct_writes(pe: pefile.PE, start: int, end: int, write_file_iat: int) -> list[str]:
    decoder = Cs(CS_ARCH_X86, CS_MODE_32)
    rows = [i for i in decoder.disasm(pe.get_data(start-0x10000000, end-start), start)
            if i.mnemonic == "call" and f"[0x{write_file_iat:x}]" in i.op_str]
    return [hex(x.address) for x in rows]


def inspect_oem(old_path: Path, new_path: Path):
    legacy, old = load_original(old_path, "legacy_2022")
    current, new = load_original(new_path, "current_2026")
    old_sites = [exact_site(old, *row) for row in OLD_SITES]
    new_sites = [exact_site(new, *row) for row in NEW_SITES]
    for name, va in IAT.items():
        imports = [i.address for module in new.DIRECTORY_ENTRY_IMPORT
                   if module.dll.lower() == b"kernel32.dll"
                   for i in module.imports if i.name == name]
        if imports != [va]:
            raise ValueError(f"OEM IAT changed: {name!r}")
    # Boundaries obtained from the actual disassembled returns and next functions.
    no_raw_writes = {
        "IsNeedUpdateDisk_2026": bounded_direct_writes(new, 0x100A2670, 0x100A285C, IAT[b"WriteFile"]),
        "CheckUpdateDiskApp_2026": bounded_direct_writes(new, 0x100A1CB0, 0x100A2464, IAT[b"WriteFile"]),
        "DeleteDirectory_2026": bounded_direct_writes(new, 0x100A0400, 0x100A08C3, IAT[b"WriteFile"]),
        "upgrade_external_launch_thread_2026": bounded_direct_writes(new, 0x100A7230, 0x100A776F, IAT[b"WriteFile"]),
    }
    if any(no_raw_writes.values()):
        raise ValueError(f"unexpected original direct raw write in bounded upgrade function: {no_raw_writes}")
    literals = ("EdpUUpdate.exe", "EdpEDisk.exe", "UlogVer.exe", "NUpdateDisk.dat",
                "EdpEdisk_GJDW.DLL", "edpdisk.chm", "FileVersion")
    string_addresses = {}
    for name in literals:
        needle = name.encode("ascii")+b"\0"
        match = current.find(needle)
        if match < 0:
            raise ValueError(f"OEM upgrade literal absent: {name}")
        string_addresses[name] = hex(0x10000000 + new.get_rva_from_offset(match))
    return {
        "schema": 1,
        "oem_sha256": dict(KNOWN_SHA),
        "verified_x86_sites": {"legacy": old_sites, "current": new_sites},
        "string_va": string_addresses,
        "version_check": {
            "old": "sub_10086480 -> sub_10001FEB -> sub_100923D0 status query",
            "current": "sub_100A2670 -> sub_10002414 -> sub_100B0F30 status query",
            "global_conditions": "0x1029F0D8, byte_1020BF39, 0x1029F0D4",
            "profile_fields": "0x0C mode, 0x4C second partition type, 0x8C extra partition state",
        },
        "client_app_upgrade": {
            "check": "sub_100A1CB0 (CheckUpdateDiskApp)",
            "space_check": "GetDiskFreeSpaceExA at 0x100A1E33",
            "directory_cleanup": "sub_100A0400 via 0x10002A22, DeleteFileA/RemoveDirectoryA",
            "app_files": ["EdpEDisk.exe", "EdpEdisk_GJDW.DLL", "edpdisk.chm"],
        },
        "policy_to_disk_update_boundary": {
            "policy_file_write": "sub_100A13C0: WriteFile(0x6B4) at 0x100A16A5 (not 0xC00)",
            "thread_create": "sub_100A13C0: push 0x1000385A at 0x100A17BD, CreateThread at 0x100A17C6",
            "thread_entry": "0x1000385A -> sub_100A7230",
            "external_process": "sub_100A7230 invokes CreateProcessA at 0x100A7620",
            "process_wait": "WaitForSingleObject at 0x100A76BA; GetExitCodeProcess at 0x100A76CE",
            "commands_include": ["EdpUUpdate.exe", "EdpEDisk.exe", "UlogVer.exe"],
            "external_app_invocation_observed": False,
        },
        "bounded_direct_WriteFile_sites": no_raw_writes,
        "extent_image_producer_proven": False,
        "unknown_external_binary_behavior": True,
        "physical_disk_access": False,
    }


def optional_bundle_inventory(root: Path, report: dict):
    paths = {
        "EdpUUpdate.exe": list(root.rglob("EdpUUpdate.exe")),
        "UlogVer.exe": list(root.rglob("UlogVer.exe")),
        "EdpEDisk.exe": list(root.rglob("edpedisk.exe")),
    }
    report["original_bundle_file_inventory"] = {
        name: {"count": len(found), "relative_names": sorted(str(p.relative_to(root)) for p in found)[:25]}
        for name, found in paths.items()
    }
    # Also inspect named application binaries for the fixed historical warning
    # asset; a negative search is not proof that code cannot synthesize it.
    gold_file = (Path(__file__).resolve().parents[2] /
                 "audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin")
    gold = gold_file.read_bytes()
    content = gold[2560:2560+82]
    binaries = []
    for file in paths["EdpEDisk.exe"]:
        blob = file.read_bytes()
        binaries.append({
            "relative_name": str(file.relative_to(root)),
            "sha256": sha256(blob),
            "size_bytes": len(blob),
            "full_3072_gold_occurrences": blob.count(gold),
            "warning_82_byte_GBK_occurrences": blob.count(content),
        })
    report["external_legacy_app_binary_asset_checks"] = binaries
    # A missing copy of an OEM executable cannot be substituted with a guessed producer.
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--bundle-root", type=Path)
    args = parser.parse_args()
    report = inspect_oem(args.legacy, args.current)
    if args.bundle_root:
        optional_bundle_inventory(args.bundle_root, report)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
