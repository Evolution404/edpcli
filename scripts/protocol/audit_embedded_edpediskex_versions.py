#!/usr/bin/env python3
"""Audit embedded OEM EdpEDiskEx mounts across 2022/2026 original releases.

Parses original PE resources in memory. Never executes extracted binaries,
mounts a volume, or accesses any physical device.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile

from capstone import CS_ARCH_X86, CS_MODE_32, Cs
import pefile

HOSTS = {
    "2022": {
        "sha256": "14038a7fd755ee94638bc45938673bbdda5117c409214f816adafe05ae090070",
        "resource_sha256": "9a665e5c46eaa4076e14e2208d467d2110d33fd5bf1d363b34cc40c517fccaca",
        "length": 431616,
        "export_rva": 0x98B0,
        "iat": 0x1003028C,
        "sites": [
            (0x100098E2, "call", "0x10007480"),
            (0x10007852, "call", "0x10007c10"),
            (0x1000783A, "mov", "[ebp + 8]"),
            (0x1000783D, "mov", "[eax + 0x160]"),
            (0x1000784A, "push", "eax"),
            (0x10007C98, "mov", "0x8200e000"),
            (0x10008076, "cmp", "[ebp + 0xc], 1"),
            (0x1000807C, "mov", "0x8200e00c"),
            (0x1000809B, "movzx", "[edx + 0xed]"),
            (0x100080A2, "add", "eax, 0xef"),
            (0x100080BA, "call", "[0x1003028c]"),
        ],
    },
    "2026": {
        "sha256": "5be85c0f85dc65dd8f89e59a78f584a8325e5208441fc461e2a612501a5b3e08",
        "resource_sha256": "e988a62076e1b62b14beaca18155cb9e76f7b75caffa942b654f975804433127",
        "length": 424776,
        "export_rva": 0x9A10,
        "iat": 0x10030274,
        "sites": [
            (0x10009A44, "call", "0x10007390"),
            (0x10007963, "call", "0x10007d40"),
            (0x1000794B, "mov", "[ebp + 8]"),
            (0x1000794E, "mov", "[eax + 0x160]"),
            (0x1000795B, "push", "eax"),
            (0x10007DC8, "mov", "0x8200e000"),
            (0x100081B8, "cmp", "[ebp + 0xc], 1"),
            (0x100081BE, "mov", "0x8200e00c"),
            (0x100081DD, "movzx", "[edx + 0xed]"),
            (0x100081E4, "add", "eax, 0xef"),
            (0x100081FC, "call", "[0x10030274]"),
        ],
    },
}
ZIP_HOST_SHA = "c05fc97f53a5e171959f92b69cf4065ee478e36abae848b7d0a6fc863eb5e890"
ZIP_SHA = "c3fd4b060afde3042ab0ec3d0c159c8bf6e6d7241532cdc45d7283fd21b8136f"


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def sha_guard(path: Path, value: str) -> pefile.PE:
    raw = path.read_bytes()
    actual = sha(raw)
    if actual != value:
        raise ValueError(f"Original OEM SHA-256 mismatch: {path.name}, got {actual}")
    pe = pefile.PE(data=raw)
    if pe.FILE_HEADER.Machine != 0x14C:
        raise ValueError("unexpected original host PE machine")
    return pe


def find_embedded(host: pefile.PE, resource_type: str, resource_id: int):
    top = [r for r in host.DIRECTORY_ENTRY_RESOURCE.entries if str(r.name) == resource_type]
    if len(top) != 1:
        raise ValueError(f"original resource type mismatch {resource_type}")
    entries = [r for r in top[0].directory.entries if r.id == resource_id]
    if len(entries) != 1:
        raise ValueError(f"original resource ID mismatch {resource_id}")
    languages = entries[0].directory.entries
    if len(languages) != 1 or not hasattr(languages[0], "data"):
        raise ValueError(f"original resource languages/leaf mismatch {resource_id}")
    chunk = languages[0].data.struct
    body = host.get_data(chunk.OffsetToData, chunk.Size)
    if len(body) != chunk.Size:
        raise ValueError("original resource bytes incomplete")
    return body, {"type": resource_type, "id": resource_id,
                  "language": languages[0].id, "rva": hex(chunk.OffsetToData),
                  "size": chunk.Size}


def audit_sites(pe: pefile.PE, records):
    d = Cs(CS_ARCH_X86, CS_MODE_32)
    base = pe.OPTIONAL_HEADER.ImageBase
    output = []
    for va, mnemonic, target in records:
        lines = list(d.disasm(pe.get_data(va-base, 16), va, count=1))
        if not lines or lines[0].mnemonic != mnemonic or target not in lines[0].op_str:
            found = "missing" if not lines else f"{lines[0].mnemonic} {lines[0].op_str}"
            raise ValueError(f"Embedded original x86 opcode changed at {va:#x}: {found}")
        output.append({"va": hex(va), "opcode": lines[0].mnemonic+" "+lines[0].op_str})
    return output


def audit_embedded_ex(host_path: Path, release: str):
    config = HOSTS[release]
    host = sha_guard(host_path, config["sha256"])
    payload, locator = find_embedded(host, "SECDISKDLL", 1006)
    if sha(payload) != config["resource_sha256"] or len(payload) != config["length"]:
        raise ValueError("embedded EdpEDiskEx resource SHA-256/size mismatch")
    ex = pefile.PE(data=payload)
    if ex.FILE_HEADER.Machine != 0x14C or ex.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError("embedded EdpEDiskEx PE identity mismatch")
    symbols = [e.address for e in ex.DIRECTORY_ENTRY_EXPORT.symbols if e.name == b"EdpMountFile"]
    if symbols != [config["export_rva"]]:
        raise ValueError(f"embedded EdpMountFile export drift: {symbols}")
    imports = [imp.address for mod in ex.DIRECTORY_ENTRY_IMPORT
               if mod.dll.lower() == b"kernel32.dll"
               for imp in mod.imports if imp.name == b"DeviceIoControl"]
    if imports != [config["iat"]]:
        raise ValueError(f"embedded DeviceIoControl IAT changed: {imports}")
    return {
        "host_sha256": config["sha256"],
        "embedded_ex_sha256": sha(payload),
        "resource": locator,
        "edp_mount_file_export_rva": hex(config["export_rva"]),
        "device_io_control_iat": hex(config["iat"]),
        "x86_sites": audit_sites(ex, config["sites"]),
        "mount_descriptor_length": "0xEF + uint16_le(input + 0xED)",
        "descriptor_source": "EdpMountFile caller-provided buffer",
        "extra_arg_source": "mount buffer +0x160 passed separately to internal helper",
        "ioctl_default": "0x8200E000",
        "ioctl_alternative": "0x8200E00C when second argument equals 1",
        "new_oem_vs_2022_unconditional_4kn_support_proven": False,
        "actual_virtual_or_physical_write_proven": False,
    }


def inspect_legacy_installer_zip(path: Path, standalone_ctrl: Path):
    host = sha_guard(path, ZIP_HOST_SHA)
    payload, locator = find_embedded(host, "SECDISKCTRLDLL", 187)
    if sha(payload) != ZIP_SHA or not zipfile.is_zipfile(io.BytesIO(payload)):
        raise ValueError("legacy embedded controller ZIP identity mismatch")
    with zipfile.ZipFile(io.BytesIO(payload)) as z:
        members = z.infolist()
        if len(members) != 1 or members[0].filename != "EdpEDiskCtrl.dll":
            raise ValueError("legacy embedded controller ZIP member drift")
        unpacked = z.read(members[0])
    reference = standalone_ctrl.read_bytes()
    if sha(unpacked) != HOSTS["2022"]["sha256"] or unpacked != reference:
        raise ValueError("legacy embedded controller differs from standalone source")
    return {
        "zip_host_sha256": ZIP_HOST_SHA,
        "zip_sha256": ZIP_SHA,
        "resource": locator,
        "contained_names": ["EdpEDiskCtrl.dll"],
        "content_equal_to_standalone_controller": True,
        "contains_updater_executables": False,
    }


def audit(legacy_ctrl: Path, current_ctrl: Path, legacy_exe: Path | None = None):
    report = {
        "schema": 1,
        "scope": "read-only original PE resource and ABI proof",
        "versions": {
            "2022": audit_embedded_ex(legacy_ctrl, "2022"),
            "2026": audit_embedded_ex(current_ctrl, "2026"),
        },
        "mount_descriptor_contract_equivalent_between_releases": True,
        "first_3072_byte_upgrade_notice_writer_identified": False,
        "four_kn_trailing_1024_byte_owner_identified": False,
        "executed_any_oem_binary": False,
        "accessed_physical_device": False,
    }
    if legacy_exe:
        report["historical_edpedisk_installer"] = inspect_legacy_installer_zip(legacy_exe, legacy_ctrl)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy-controller", type=Path, required=True)
    parser.add_argument("--current-controller", type=Path, required=True)
    parser.add_argument("--legacy-exe", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.legacy_controller, args.current_controller,
                           args.legacy_exe), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
