#!/usr/bin/env python3
"""Read-only, SHA-bound OEM Linux 4Kn native geometry vs 512-byte helper audit.

Disassembles exact byte sites of two original x86-64 ELF shared libraries.
Never loads ELF objects or accesses Linux block devices.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

from capstone import Cs, CS_ARCH_X86, CS_MODE_64

LIBS = {
    "mount": {
        "sha256": "e10e12dfc26fee71b5cd1cc5465e5045fcec603a34f35e676faf9e571a7f5560",
        "sites": [
            (0x6BD77,"cmp","eax, 4"),
            (0x6BDB8,"cmp","eax, 5"),
            (0x6BD86,"mov","esi, 0x1268"),
            (0x6BD8B,"call","0x20ae0"),
            (0x6BD95,"mov","eax, dword ptr [rsp + 0xc]"),
            (0x43118,"call","0x6bd60"),
            (0x4312C,"mov","qword ptr [r13 + 0x58], rax"),
            (0x43130,"lea","rsi, [rax + rdx*4]"),
            (0x43181,"call","0x6b820"),
            (0x42199,"mov","rsi, qword ptr [r14 + 0x58]"),
            (0x421DD,"imul","rsi, qword ptr [r14 + 0x58]"),
            (0x421E2,"call","0x6ba30"),
            (0x42208,"call","0x6b820"),
            (0x3FF09,"div","r8"),
            (0x401F9,"div","rsi"),
            (0x4020B,"div","rsi"),
            (0x4025A,"call","0x6c8a0"),
            (0x4114D,"movzx","byte ptr [rcx + 0x58]"),
            (0x41157,"cmp","al, 1"),
            (0x41165,"cmp","al, 2"),
            (0x41167,"jne","0x41349"),
            (0x2F38D,"and","rbp, 0xfffffffffffffe00"),
            (0x2F43D,"and","rbp, 0xfffffffffffffe00"),
        ],
    },
    "sector_helper": {
        "sha256": "77d08eb8dd9aced7f6e103ac4469cce7f356ad41e0743f7dcf22cdae83209cc7",
        "sites": [
            (0x19025,"and","r8, 0xfffffffffffffe00"),
            (0x1902C,"and","eax, 0x1ff"),
            (0x19067,"and","eax, 0x1ff"),
            (0x1906E,"add","rsi, 0x200"),
            (0x1924D,"call","0x13f00"),
            (0x1941A,"and","r8, 0xfffffffffffffe00"),
            (0x19421,"and","r14d, 0x1ff"),
            (0x1943F,"and","eax, 0x1ff"),
            (0x19450,"add","rbx, 0x200"),
            (0x194A0,"call","0x12d00"),
            (0x1BA21,"call","0x12c70"),
            (0x26C5B,"call","0x134e0"),
            (0x51EB4,"mov","esi, 0x8927"),
            (0x51ED4,"call","0x14160"),
        ],
    },
}


def elf_va_offset(raw: bytes, va: int) -> int:
    if not raw.startswith(b"\x7fELF") or raw[4:6] != b"\x02\x01":
        raise ValueError("expected 64-bit little-endian ELF")
    if struct.unpack_from("<H",raw,0x12)[0]!=62:
        raise ValueError("expected original x86-64 ELF machine")
    phoff=struct.unpack_from("<Q",raw,0x20)[0]
    entsize,n=struct.unpack_from("<HH",raw,0x36)
    for idx in range(n):
        pos=phoff+entsize*idx
        if pos+56>len(raw):raise ValueError("truncated program headers")
        ptype,flags,offset,start,paddr,filesz,memsz,align=struct.unpack_from("<IIQQQQQQ",raw,pos)
        if ptype==1 and flags&1 and start<=va<start+filesz:
            result=offset+va-start
            if result>=len(raw):break
            return result
    raise ValueError(f"VA {va:#x} is not mapped to executable file bytes")


def verify_binary(path: Path, kind: str) -> dict:
    if kind not in LIBS:raise ValueError("unknown OEM Linux module kind")
    blob=path.read_bytes()
    expected=LIBS[kind]
    actual=hashlib.sha256(blob).hexdigest()
    if actual!=expected["sha256"]:
        raise ValueError(f"original OEM Linux {kind} SHA-256 mismatch; {actual}")
    dis=Cs(CS_ARCH_X86,CS_MODE_64)
    sites=[]
    for va,mnemonic,part in expected["sites"]:
        offset=elf_va_offset(blob,va)
        decoded=list(dis.disasm(blob[offset:offset+16],va,count=1))
        if not decoded or decoded[0].mnemonic!=mnemonic or part not in decoded[0].op_str:
            observed="no opcode" if not decoded else f"{decoded[0].mnemonic} {decoded[0].op_str}"
            raise ValueError(f"original Linux {kind} machine code drift at {va:#x}: {observed}")
        ins=decoded[0]
        sites.append({"va":hex(va),"file_offset":hex(offset),
                      "op":f"{ins.mnemonic} {ins.op_str}","code_hex":ins.bytes.hex()})
    return {"sha256":actual,"length_bytes":len(blob),"verified_sites":sites}


def audit(mount_so:Path,sector_so:Path)->dict:
    libs={"libedpedisk.so":verify_binary(mount_so,"mount"),
          "libsectorManage.so":verify_binary(sector_so,"sector_helper")}
    return {
        "schema":1,
        "scope":"original Linux EDP x86-64 client offline library instructions",
        "libs":libs,
        "verified":{
            "mount_library_queries_real_logical_bytes_via_BLKSSZGET":True,
            "mount_Volume_InitDiskInfo_saves_native_bytes_at_this_plus_0x58":True,
            "mount_initial_13_sector_read_uses_native_bytes":True,
            "mount_metadata_sector_byte_offset_multiplies_native_bytes":True,
            "mount_read_and_write_partition_crypto_use_native_bytes_divisors":True,
            "partition_header_mode3_rejected_before_header_object_creation":True,
            "data_crypto_512_byte_subsector_alignment_even_with_native_4Kn":True,
            "sector_helper_ReadEncrypt_rounds_on_fixed_512_byte_boundaries":True,
            "sector_helper_WriteEncrypt_rounds_on_fixed_512_byte_boundaries":True,
            "sector_helper_uses_SM4_ECB_in_these_encrypted_metadata_helpers":True,
        },
        "not_verified":{
            "real_4096B_physical_USB_mounted_successfully":True,
            "failure_of_specific_4Kn_device_on_Linux_client":True,
            "sector_helper_hardcoded_512_is_direct_cause_of_mount_failure":True,
            "LCE_compatibility_fallback_valid_on_4Kn":True,
            "AES_CROSS_mode3_success_on_real_4Kn":True,
            "4096B_LCE_last_1024_byte_owner":True,
        },
        "model_only": {
            "offset_3584_length_512_on_4Kn": {
                "native_logical_block_bytes":4096,
                "aux_512_aligned_range":[3584,4096],
                "whole_native_4Kn_block_range":[0,4096],
                "different_alignment_granularities":True,
                "proof_of_failed_linux_block_io":False,
            },
        },
        "device_read_or_write":False,
        "oem_executable_loaded":False,
    }


def main()->None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mount-so",type=Path,required=True)
    parser.add_argument("--sector-so",type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(audit(args.mount_so,args.sector_so),ensure_ascii=False,indent=2))


if __name__=="__main__":main()
