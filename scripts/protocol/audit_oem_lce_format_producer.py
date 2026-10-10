#!/usr/bin/env python3
"""SHA-bound static audit of OEM CreatePartitions -> StartFormatPart -> FormatEx.

Runs no OEM binaries, opens no device, and executes no original code.
Finds code references and ABI evidence; does not establish physical LCE writes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
from capstone.x86 import X86_OP_IMM, X86_OP_MEM
import pefile


OEM_REGISTRAR_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
OEM_IMAGE_BASE = 0x10000000
GOLD_SHA256 = "386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c"
SCAN_FOLDERS = ("edp", "cems/ydcc", "cems/Edp/edpdrivers", "cems/Edp/edpdrivers_win10")
KNOWN_TEMPLATE_VERSIONS = {
    "vrvaud_c.dll": {
        "57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f",
        "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069",
    }
}

EXPECTED_CALLS = (
    # RegsiterUsb contains both the native partition generation and formatting stage.
    (0x1003BDA9, 0x1003DB50, "RegsiterUsb -> CreatePartitions"),
    (0x1003C211, 0x10046320, "RegsiterUsb -> StartFormatPart"),
    (0x1003FF24, 0x100439D0, "CreatePartitions -> MountEdpPart"),
    (0x1004637D, 0x10044210, "StartFormatPart -> FormatDisk image=true"),
    (0x100463E7, 0x10044210, "StartFormatPart -> FormatDisk image=false fallback"),
    (0x10044656, 0x10045710, "FormatDisk -> OnFormatDisk (image)"),
    (0x100448BF, 0x10045710, "FormatDisk -> OnFormatDisk (image)"),
    (0x10044DCE, 0x10045710, "FormatDisk -> OnFormatDisk (normal)"),
    (0x10045270, 0x10045710, "FormatDisk -> OnFormatDisk (normal)"),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_original(path: Path, expected: str) -> tuple[bytes, pefile.PE]:
    raw = path.read_bytes()
    if sha256(raw) != expected:
        raise ValueError(f"OEM SHA-256 mismatch for {path.name}: {sha256(raw)}")
    pe = pefile.PE(data=raw)
    if pe.FILE_HEADER.Machine != 0x14C or pe.OPTIONAL_HEADER.ImageBase != OEM_IMAGE_BASE:
        raise ValueError("unexpected native PE architecture or base")
    return raw, pe


def decode_site(pe: pefile.PE, address: int):
    d = Cs(CS_ARCH_X86, CS_MODE_32)
    out = list(d.disasm(pe.get_data(address - OEM_IMAGE_BASE, 15), address, count=1))
    if not out or out[0].address != address:
        raise ValueError(f"no opcode at {address:#x}")
    return out[0]


def assert_opcode(pe: pefile.PE, address: int, mnemonic: str, operand_part: str) -> dict:
    i = decode_site(pe, address)
    if i.mnemonic != mnemonic or operand_part not in i.op_str:
        raise ValueError(f"unexpected opcode at {address:#x}: {i.mnemonic} {i.op_str}")
    return {"va": hex(address), "opcode": f"{i.mnemonic} {i.op_str}"}


def verify_import(pe: pefile.PE, name: bytes, address: int) -> str:
    for m in pe.DIRECTORY_ENTRY_IMPORT:
        for obj in m.imports:
            if obj.name == name and obj.address == address:
                return m.dll.decode("ascii") + "!" + name.decode("ascii")
    raise ValueError(f"unexpected import for {name!r} at {address:#x}")


def validate_call_landscape(pe: pefile.PE):
    text = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".text")
    decoder = Cs(CS_ARCH_X86, CS_MODE_32)
    decoder.detail = True
    targets = {target for _, target, _ in EXPECTED_CALLS}
    calls = {target: [] for target in targets}
    for inst in decoder.disasm(text.get_data(), OEM_IMAGE_BASE + text.VirtualAddress):
        if inst.mnemonic not in ("call", "jmp"):
            continue
        for operand in inst.operands:
            if operand.type == X86_OP_IMM and operand.imm in calls:
                calls[operand.imm].append(inst.address)
    for location, target, context in EXPECTED_CALLS:
        if location not in calls[target]:
            raise ValueError(f"missing static OEM call {context} at {location:#x}")
    return {hex(k): [hex(a) for a in v] for k, v in sorted(calls.items())}


def audit_format_chain(registrar: Path):
    raw, pe = read_original(registrar, OEM_REGISTRAR_SHA256)
    imports = {
        "loader": verify_import(pe, b"LoadLibraryA", 0x100C3090),
        "lookup": verify_import(pe, b"GetProcAddress", 0x100C303C),
        "drive_type": verify_import(pe, b"GetDriveTypeW", 0x100C309C),
    }
    sites = [assert_opcode(pe, va, "call", hex(target)) for va, target, _ in EXPECTED_CALLS]
    # Two mutually relevant parameters passed to FormatDisk in OEM 32bit __thiscall.
    for va, m, text in (
        (0x10046376, "push", "1"),
        (0x10046378, "push", "1"),
        (0x100463E0, "push", "0"),
        (0x100463E2, "push", "1"),
        (0x10045750, "movsx", "[ebp + 8]"),
        (0x10045755, "mov", "[ebp - 0x1c]"),
        (0x10045759, "mov", "0x3a"),
        (0x10045762, "mov", "0x5c"),
        (0x1004576D, "mov", "[ebp - 0x16]"),
        (0x10045776, "call", "[0x100c3090]"),
        (0x100457AC, "call", "[0x100c303c]"),
        (0x100457B2, "mov", "[ebp - 0x2c]"),
        (0x100457F8, "cmp", "3"),
        (0x1004580D, "call", "[0x100c309c]"),
        (0x10045913, "push", "0x100479b0"),
        (0x10045918, "push", "0"),
        (0x1004591A, "push", "1"),
        (0x1004591C, "mov", "[ebp + 0x10]"),
        (0x10045920, "mov", "[ebp + 0xc]"),
        (0x10045924, "mov", "[ebp - 0x24]"),
        (0x10045928, "lea", "[ebp - 0x1c]"),
        (0x1004592C, "call", "[ebp - 0x2c]"),
        (0x100443B2, "cmp", "3"),
    ):
        sites.append(assert_opcode(pe, va, m, text))
    load_name = pe.get_data(0x100C870C-OEM_IMAGE_BASE, 32).split(b"\0")[0]
    lookup_name = pe.get_data(0x100C87E8-OEM_IMAGE_BASE, 32).split(b"\0")[0]
    if (load_name, lookup_name) != (b"fmifs.dll", b"FormatEx"):
        raise ValueError(f"OEM formatting API names changed: {load_name!r}, {lookup_name!r}")
    return {
        "registrar_sha256": sha256(raw),
        "static_call_landscape": validate_call_landscape(pe),
        "named_call_edges": [dict(source=hex(x), target=hex(y), meaning=desc) for x, y, desc in EXPECTED_CALLS],
        "native_opcode_sites": sites,
        "imports": imports,
        "dynamic_module": load_name.decode("ascii"),
        "dynamic_export": lookup_name.decode("ascii"),
        "on_format_disk_argument_0": "drive-letter byte -> WCHAR '<letter>:\\\\' root string",
        "on_format_disk_argument_1": "filesystem string argument supplied by FormatDisk",
        "on_format_disk_argument_2": "label argument supplied by FormatDisk",
        "format_ex_call_va": "0x1004592c",
        "format_ex_stack_arguments": [
            "root drive string ([ebp-0x1c])",
            "media selection ([ebp-0x24])",
            "filesystem ([ebp+0x0c])",
            "label ([ebp+0x10])",
            "quick-format flag 1",
            "cluster-size 0",
            "callback sub_100479b0"
        ],
        "format_disk_modes": [
            "image=true branch loops two mounted slots (indices 1,2); chooses FAT/NTFS by size/type",
            "image=false branch enumerates letters C:..Z: excluding assigned partition letters",
        ],
        "direct_lce_byte_address_in_verified_format_ex_call": False,
        "runtime_indirect_producer_excluded": False,
        "disk_access": False,
    }


def scan_oem_binary_fixtures(root: Path, gold: bytes):
    if len(gold) != 3072 or sha256(gold) != GOLD_SHA256:
        raise ValueError("historical FAT16 gold SHA-256 or size mismatch")
    report = []
    for folder in SCAN_FOLDERS:
        base = root / folder
        if not base.is_dir():
            raise FileNotFoundError(base)
        files = sorted(
            f for f in base.iterdir()
            if f.is_file()
            and f.suffix.lower() in (".exe", ".dll", ".sys")
            and f.stat().st_size < 45_000_000
        )
        hits = []
        for p in files:
            data = p.read_bytes()
            match = next(((name, needle) for name, needle in (
                ("gold3072", gold), ("gold512", gold[:512]),
                ("boot64", gold[:64]), ("boot11", gold[:11])
            ) if needle in data), None)
            if match is not None:
                label, needle = match
                digest = sha256(data)
                if label != "gold3072" or p.name != "vrvaud_c.dll" or digest not in KNOWN_TEMPLATE_VERSIONS["vrvaud_c.dll"]:
                    raise ValueError(f"unclassified native OEM boot-sector match: {p}")
                hits.append({"file": p.name, "match_level": label,
                             "file_offset": hex(data.find(needle)), "sha256": digest})
        report.append({"source_directory": folder, "file_count": len(files), "matches": hits})
    return report


def inspect_vrvaud_writer_addresses(current_dll: Path):
    """Identify exact absolute PE DWORDs / HIGHLOW relocations to writer targets.

    Near-relative code jumps do not embed absolute DWORDs; their absence does
    not rule out dynamically calculated pointers or caller-supplied callbacks.
    """
    known_sha = "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069"
    raw, pe = read_original(current_dll, known_sha)
    writer_addresses = {
        "WriteDiskEx_thunk": 0x10002EA5,
        "WriteDiskEx_implementation": 0x100DBD40,
        "WriteDisk_thunk": 0x10002B58,
        "WriteDisk_implementation": 0x100DB7B0,
    }
    slots = []
    for block in getattr(pe, "DIRECTORY_ENTRY_BASERELOC", []):
        for entry in block.entries:
            if entry.type != 3:
                continue
            offset = pe.get_offset_from_rva(entry.rva)
            if offset + 4 > len(raw):
                continue
            value = struct.unpack_from("<I", raw, offset)[0]
            if value in writer_addresses.values():
                slots.append({"file_offset": hex(offset), "points_to": hex(value)})
    counts = {name: raw.count(struct.pack("<I", va))
              for name, va in writer_addresses.items()}
    if any(counts.values()) or slots:
        raise ValueError(f"unexpected OEM static writer pointer candidates: {counts}; {slots}")
    return {
        "sha256": known_sha,
        "absolute_pe_dword_occurrences": counts,
        "highlow_relocated_writer_pointers": slots,
        "interpretation": "no embedded absolute writer pointer to these four VAs in this one OEM DLL",
        "runtime_computed_dispatch_excluded": False,
    }


def audit(registrar: Path, scan_root: Path | None = None,
          current_vrvaud: Path | None = None):
    report = {"schema": 1, "scope": "read-only bounded static OEM audit"}
    report["format_chain"] = audit_format_chain(registrar)
    if current_vrvaud:
        report["vrvaud_writer_address_scan"] = inspect_vrvaud_writer_addresses(current_vrvaud)
    if scan_root:
        gold = Path(__file__).resolve().parents[2] / "audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin"
        report["oem_binary_template_scan"] = scan_oem_binary_fixtures(scan_root, gold.read_bytes())
    report["unproven"] = [
        "runtime producer of fixed FAT16 3072-byte LCE",
        "whether Windows FormatEx or mounted other volume ever targeted physical LCE",
        "how U391 4Kn LCE block is initialized or trailing 1024 bytes owned",
    ]
    report["physical_device_io"] = False
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registrar", type=Path, required=True)
    parser.add_argument("--scan-root", type=Path)
    parser.add_argument("--current-vrvaud", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.registrar, args.scan_root, args.current_vrvaud), indent=2))


if __name__ == "__main__":
    main()
