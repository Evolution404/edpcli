#!/usr/bin/env python3
"""SHA-bound 2022/2026 original embedded kernel driver transport comparison.

Verifies native x64 kernel write/read call sites, virtual extent bounds and
byte-offset mapping without ever loading or executing Windows drivers.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from capstone import CS_ARCH_X86, CS_MODE_64, Cs
from capstone.x86 import X86_OP_MEM
from capstone.x86_const import X86_REG_RIP
import pefile

from scripts.protocol.audit_embedded_edpediskex_versions import HOSTS, find_embedded, sha_guard

DRIVERS = {
    "2022": {
        "x86_sha": "9c40e55d5a58c4ec11517399c290370acfd85814a086f56202cde4c36bbca897",
        "x86_len": 66120,
        "x64_sha": "2eeb792f63cbd3b44e685c14127708a5bed411010ca945d588916b98449dbea7",
        "x64_len": 76208,
        "write_iat": 0x170B0, "read_iat": 0x17128,
        "write_va": 0x122DD, "read_va": 0x12409,
        "write_range": (0x12096, 0x122FC),
        "sites": [
            (0x11B37, "cmp", "0x8200e00c"),
            (0x11B49, "cmp", "0x8200e000"),
            (0x1205B, "mov", "[rbx + 0x30], 0x200"),
            (0x120A4, "mov", "r13d, dword ptr [rbx + 8]"),
            (0x120B9, "test", "rax, rax"),
            (0x120C2, "lea", "[r13 + rax]"),
            (0x120C7, "cmp", "[r15 + 0x20]"),
            (0x12114, "mov", "[r15 + 0x28]"),
            (0x12121, "add", "[rbx + 0x18]"),
            (0x1212A, "mov", "[rsp + 0x50]"),
            (0x122C2, "mov", "[rsp + 0x30], r13d"),
            (0x122D3, "mov", "[rsp + 0x28], rbp"),
            (0x12379, "cmp", "[r15 + 0x30], 0x200"),
            (0x12383, "mov", "[r15 + 0x30], 0x200"),
        ],
    },
    "2026": {
        "x86_sha": "348c320050a356000ac15ff7926ed1ae6722268a819e70a333c4eefc5a11eb25",
        "x86_len": 55552,
        "x64_sha": "724544a96f899b9bb0f87a8adedd08a961d8e4b8ec40aa90bc8144e5ae7e0620",
        "x64_len": 70400,
        "write_iat": 0x190B0, "read_iat": 0x19128,
        "write_va": 0x1232E, "read_va": 0x1245A,
        "write_range": (0x120C6, 0x1234D),
        "sites": [
            (0x11B37, "cmp", "0x8200e00c"),
            (0x11B54, "cmp", "0x8200e000"),
            (0x1208B, "mov", "[rbx + 0x30], 0x200"),
            (0x120D4, "mov", "r13d, dword ptr [rbx + 8]"),
            (0x120F2, "test", "rax, rax"),
            (0x120FB, "lea", "[r13 + rax]"),
            (0x12100, "cmp", "[r15 + 0x20]"),
            (0x1214D, "mov", "[r15 + 0x28]"),
            (0x1215A, "add", "[rbx + 0x18]"),
            (0x12163, "mov", "[rsp + 0x50]"),
            (0x12313, "mov", "[rsp + 0x30], r13d"),
            (0x12324, "mov", "[rsp + 0x28], rbp"),
            (0x123CA, "cmp", "[r15 + 0x30], 0x200"),
            (0x123D4, "mov", "[r15 + 0x30], 0x200"),
        ],
    },
}


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def resource_bytes(ex: pefile.PE, kind: str, resource_id: int) -> bytes:
    payload, meta = find_embedded(ex, kind, resource_id)
    if meta["language"] != 2052:
        raise ValueError(f"embedded driver resource language changed: {resource_id}")
    return payload


def x64_sites(pe: pefile.PE, sites):
    d = Cs(CS_ARCH_X86, CS_MODE_64)
    base = pe.OPTIONAL_HEADER.ImageBase
    out = []
    for va, mnemonic, fragment in sites:
        ins = list(d.disasm(pe.get_data(va-base, 16), va, count=1))
        if not ins or ins[0].mnemonic != mnemonic or fragment not in ins[0].op_str:
            actual = "no instruction" if not ins else ins[0].mnemonic + " " + ins[0].op_str
            raise ValueError(f"original driver opcode mismatch at {va:#x}: {actual}")
        out.append({"va": hex(va), "instruction": ins[0].mnemonic+" "+ins[0].op_str})
    return out


def import_iat(pe: pefile.PE, name: bytes) -> int:
    hits = [i.address for m in pe.DIRECTORY_ENTRY_IMPORT
            if m.dll.lower() == b"ntoskrnl.exe" for i in m.imports if i.name == name]
    if len(hits) != 1:
        raise ValueError(f"original driver import missing/ambiguous {name!r}")
    return hits[0]


def rip_call_sites(pe: pefile.PE, iat: int):
    text_sec = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".text")
    d = Cs(CS_ARCH_X86, CS_MODE_64)
    d.detail = True
    matches = []
    for ins in d.disasm(text_sec.get_data(), pe.OPTIONAL_HEADER.ImageBase + text_sec.VirtualAddress):
        if ins.mnemonic != "call":
            continue
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                if ins.address + ins.size + op.mem.disp == iat:
                    matches.append(ins.address)
    return matches


def audit_release(path: Path, release: str):
    host = sha_guard(path, HOSTS[release]["sha256"])
    ex_blob = resource_bytes(host, "SECDISKDLL", 1006)
    if sha256(ex_blob) != HOSTS[release]["resource_sha256"]:
        raise ValueError(f"embedded EdpEDiskEx SHA mismatch {release}")
    ex = pefile.PE(data=ex_blob)
    k = DRIVERS[release]
    blobs = {
        "x86": resource_bytes(ex, "SECDISKSYS", 1000),
        "x64": resource_bytes(ex, "SECDISKSYS", 1001),
    }
    results = {}
    for arch, blob in blobs.items():
        if sha256(blob) != k[arch+"_sha"] or len(blob) != k[arch+"_len"]:
            raise ValueError(f"OEM embedded {release} {arch} driver SHA/size mismatch")
        pe = pefile.PE(data=blob)
        expected_machine = 0x8664 if arch == "x64" else 0x14C
        if (pe.FILE_HEADER.Machine != expected_machine
            or pe.OPTIONAL_HEADER.ImageBase != 0x10000
            or pe.OPTIONAL_HEADER.Subsystem != 1):
            raise ValueError(f"invalid {release} embedded {arch} driver PE")
        results[arch] = {"sha256": sha256(blob), "size": len(blob),
                         "machine": hex(expected_machine), "subsystem": 1}
    driver = pefile.PE(data=blobs["x64"])
    write_iat = import_iat(driver, b"ZwWriteFile")
    read_iat = import_iat(driver, b"ZwReadFile")
    if (write_iat, read_iat) != (k["write_iat"], k["read_iat"]):
        raise ValueError(f"OEM {release} NT kernel IAT mismatch")
    actual_writes = rip_call_sites(driver, write_iat)
    actual_reads = rip_call_sites(driver, read_iat)
    if actual_writes != [k["write_va"]] or actual_reads != [k["read_va"]]:
        raise ValueError(f"OEM {release} NT write/read callsite drift: {actual_writes} {actual_reads}")
    wr_lo, wr_hi = k["write_range"]
    if not (wr_lo < k["write_va"] < wr_hi and all(not (wr_lo <= x < wr_hi) for x in actual_reads)):
        raise ValueError("unexpected ZwReadFile within bounded virtual IRP write slice")
    results["x64"]["write_iat"] = hex(write_iat)
    results["x64"]["read_iat"] = hex(read_iat)
    results["x64"]["direct_ZwWriteFile_site"] = hex(actual_writes[0])
    results["x64"]["direct_ZwReadFile_site"] = hex(actual_reads[0])
    results["x64"]["verified_machine_opcodes"] = x64_sites(driver, k["sites"])
    results["x64"]["bounded_write_handler_range"] = [hex(wr_lo),hex(wr_hi)]
    results["x64"]["direct_ZwReadFile_within_write_handler"] = False
    results["x64"]["native_logical_sector_bytes_from_device_identified"] = False
    results["x64"]["write_transport_byte_offset"] = "backing +0x28 plus IRP byte offset +0x18"
    results["x64"]["write_extent_bound_bytes"] = "IRP byte offset + byte count <= virtual extent +0x20"
    results["x64"]["read_minimum_clamp_constant"] = 0x200
    results["x64"]["device_default_block_size_constant"] = 0x200
    return results


def audit(old: Path, current: Path, current_standalone: Path | None = None):
    results = {"schema": 1, "scope": "original nested PE resources; x64 virtual IRP backing writes",
               "versions": {"2022": audit_release(old,"2022"),
                            "2026": audit_release(current,"2026")},
               "same_x64_driver_across_2022_and_2026": (DRIVERS["2022"]["x64_sha"] == DRIVERS["2026"]["x64_sha"]),
               "current_x64_matches_existing_standalone_original": None,
               "claims_not_supported": [
                   "that OEM provisioning ever mounted or wrote historical 3072B LCE notice",
                   "that 4Kn physical sector-sized read-modify-write is implemented",
                   "that 4Kn final 1024B belong to the LCE payload",
                   "that the kernel uses 0x200 as native physical sector size on a real 4Kn device",
               ],
               "executed_oem_code": False, "accessed_physical_disk": False}
    if current_standalone is not None:
        actual = sha256(current_standalone.read_bytes())
        if actual != DRIVERS["2026"]["x64_sha"]:
            raise ValueError("standalone current OEM EdpEDisk64.sys SHA-256 mismatch")
        results["current_x64_matches_existing_standalone_original"] = True
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy-controller", type=Path, required=True)
    parser.add_argument("--current-controller", type=Path, required=True)
    parser.add_argument("--current-standalone-driver", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.legacy_controller, args.current_controller,
                           args.current_standalone_driver),ensure_ascii=False,indent=2))


if __name__ == "__main__":
    main()
