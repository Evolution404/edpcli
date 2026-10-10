#!/usr/bin/env python3
"""Static original vrvaud_c.dll FAT16 template references, *not* disk I/O.

Audits known SHA-256 original Windows DLLs; searches complete PE .text,
relocation targets and exports for references to the 3072B LCE plaintext.
Zero static refs do NOT prove that no runtime/indirect caller uses the template.
"""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import struct

from capstone import Cs, CS_ARCH_X86, CS_MODE_32
from capstone.x86 import X86_OP_IMM, X86_OP_MEM
import pefile

TEMPLATE_PATH = Path(__file__).resolve().parents[2] / 'audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin'
TEMPLATE_SHA256 = '386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c'
VERSIONS = {
    'legacy_2022': ('57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f', 0x1697D8),
    'current_2026': ('7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069', 0x203EE8),
}


def x86_find_static_refs(machine_code: bytes, code_va: int, target_start: int, length: int):
    """Return precise immediate/memory displacement references to VA range."""
    d = Cs(CS_ARCH_X86, CS_MODE_32)
    d.detail = True
    refs = []
    literal_c00 = []
    for instr in d.disasm(machine_code, code_va):
        for operand in instr.operands:
            if operand.type == X86_OP_IMM:
                value = operand.imm
                if value == 0xC00:
                    literal_c00.append((instr.address, instr.mnemonic, instr.op_str))
            elif operand.type == X86_OP_MEM:
                value = operand.mem.disp
                if value == 0xC00:
                    literal_c00.append((instr.address, instr.mnemonic, instr.op_str))
            else:
                continue
            if target_start <= value < target_start + length:
                refs.append((instr.address, instr.mnemonic, instr.op_str, value))
    return refs, literal_c00


def audit_original_dll(path: Path, template: bytes, version: str):
    if version not in VERSIONS:
        raise ValueError('unrecognized OEM binary version')
    expected_hash, expected_offset = VERSIONS[version]
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected_hash:
        raise ValueError('incorrect original OEM file SHA256')
    if hashlib.sha256(template).hexdigest() != TEMPLATE_SHA256 or len(template) != 3072:
        raise ValueError('noncanonical historical LCE FAT16 template')
    pe = pefile.PE(data=data, fast_load=False)
    if pe.FILE_HEADER.Machine != 0x14C or pe.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError('unexpected OEM PE architecture or image base')
    if data.count(template) != 1 or data.find(template) != expected_offset:
        raise ValueError('unexpected OEM template occurrence or location')
    template_rva = pe.get_rva_from_offset(expected_offset)
    template_va = pe.OPTIONAL_HEADER.ImageBase + template_rva
    owning = [s for s in pe.sections if s.PointerToRawData <= expected_offset < s.PointerToRawData + s.SizeOfRawData]
    if len(owning) != 1 or owning[0].Name.rstrip(b'\0') != b'.data':
        raise ValueError('LCE template moved outside expected initialized data section')
    # Capture the complete set of *direct* kernel32!WriteFile callsites in
    # addition to the template xrefs; this does not include indirect wrappers.
    writefile_iat = [item.address for module in getattr(pe, 'DIRECTORY_ENTRY_IMPORT', [])
                     for item in module.imports
                     if module.dll.lower() == b'kernel32.dll' and item.name == b'WriteFile']
    all_writefile_calls = []
    all_refs = []
    all_c00 = []
    # Include the surrounding .data island. A missing direct reference to just
    # the 3072B payload could hide an address to an adjacent array header or
    # alignment region followed by an indexed read. This broad sweep identifies
    # the closest *observed* statically encoded .text references on either side.
    nearby_low, nearby_high = 0x1800, 0x2300
    nearby = []
    for section in pe.sections:
        if section.Name.rstrip(b'\0') != b'.text':
            continue
        refs, c00 = x86_find_static_refs(
            section.get_data(), pe.OPTIONAL_HEADER.ImageBase + section.VirtualAddress,
            template_va, len(template),
        )
        all_refs.extend(refs)
        all_c00.extend(c00)
        neighbor_refs, _ = x86_find_static_refs(
            section.get_data(), pe.OPTIONAL_HEADER.ImageBase + section.VirtualAddress,
            template_va - nearby_low, nearby_low + nearby_high,
        )
        nearby.extend(neighbor_refs)
        decoder = Cs(CS_ARCH_X86, CS_MODE_32)
        decoder.detail = True
        for instruction in decoder.disasm(section.get_data(), pe.OPTIONAL_HEADER.ImageBase + section.VirtualAddress):
            if instruction.mnemonic == 'call' and any(
                op.type == X86_OP_MEM and op.mem.disp in writefile_iat
                for op in instruction.operands
            ):
                all_writefile_calls.append(instruction.address)
    relocated = []
    for block in getattr(pe, 'DIRECTORY_ENTRY_BASERELOC', []):
        for entry in block.entries:
            if entry.type != 3:  # IMAGE_REL_BASED_HIGHLOW
                continue
            raw_offset = pe.get_offset_from_rva(entry.rva)
            if raw_offset + 4 <= len(data):
                value = struct.unpack_from('<I', data, raw_offset)[0]
                if template_va <= value < template_va + len(template):
                    relocated.append((entry.rva, value))
    exports = getattr(pe, 'DIRECTORY_ENTRY_EXPORT', None)
    names = [((entry.name or b'').decode('ascii', 'replace'), entry.address)
             for entry in exports.symbols] if exports else []
    export_template_pointers = [(name, rva) for name, rva in names
                                if template_rva <= rva < template_rva + len(template)]
    return {
        'version': version,
        'template_file_offset': expected_offset,
        'template_virtual_address': template_va,
        'template_matches_history': True,
        'template_static_text_ref_count': len(all_refs),
        'template_relocated_pointer_count': len(relocated),
        'template_data_export_count': len(export_template_pointers),
        'neighbor_window_relative_bytes': [-nearby_low, nearby_high],
        'neighbor_first_address_ref_below_template': (
            max((x[3] - template_va for x in nearby if x[3] < template_va), default=None)
        ),
        'neighbor_first_address_ref_above_template': (
            min((x[3] - template_va for x in nearby if x[3] >= template_va), default=None)
        ),
        'neighbor_distinct_va_count': len({x[3] for x in nearby}),
        'neighbor_text_ref_count': len(nearby),
        'neighbor_template_plus_rtti_unreferenced': not any(
            0 <= x[3] - template_va < 0x1174 for x in nearby
        ),
        'neighbor_target_refs': [
            {'target_delta': x[3]-template_va, 'instruction_va': hex(x[0]),
             'instruction': x[1] + ' ' + x[2]}
            for x in sorted(nearby, key=lambda x:(x[3],x[0]))
            if (-0x1400 < x[3]-template_va < 0x1450)
        ],
        'direct_kernel32_writefile_calls': all_writefile_calls,
        'code_3072_immediates': all_c00,
        'export_names': [name for name, _ in names],
        'bounded_interpretation': 'no direct xrefs is not proof of no indirect/runtime use',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--old', type=Path, required=True, help='2022 original OEM DLL')
    parser.add_argument('--current', type=Path, required=True, help='2026 original OEM DLL')
    args = parser.parse_args()
    template = TEMPLATE_PATH.read_bytes()
    for name, source in [('legacy_2022', args.old), ('current_2026', args.current)]:
        result = audit_original_dll(source, template, name)
        print('OEM_VARIANT', result['version'])
        for key in ('template_file_offset', 'template_virtual_address', 'template_matches_history',
                    'template_static_text_ref_count', 'template_relocated_pointer_count',
                    'template_data_export_count', 'export_names'):
            print(f' {key}={result[key]}')
        print(f' direct_kernel32_WriteFile_call_count={len(result["direct_kernel32_writefile_calls"])}')
        print(f' direct_kernel32_WriteFile_calls={[hex(addr) for addr in result["direct_kernel32_writefile_calls"]]}')
        print(f' exact_0xC00_integer_operands={[(hex(ip), mnemonic, op) for ip, mnemonic, op in result["code_3072_immediates"]]}')
        print(' INTERPRETATION=Cannot infer actual template writer from embedding alone; indirect calls not excluded')
    print('PHYSICAL_DISK_OR_CREDENTIAL_ACCESS=NONE')


if __name__ == '__main__':
    main()
