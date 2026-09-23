#!/usr/bin/env python3
"""Offline probe of the legacy 8-byte-key EdpEDisk64.sys block transform.

Requires the optional `unicorn` Python package. Reads only local files and
prints hashes/summary; it neither writes decrypted material nor accesses disks.
"""

import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import struct

from unicorn import Uc, UC_ARCH_X86, UC_MODE_64
from unicorn.x86_const import (
    UC_X86_REG_R8, UC_X86_REG_R9, UC_X86_REG_RBP, UC_X86_REG_RBX,
    UC_X86_REG_RCX, UC_X86_REG_RDI, UC_X86_REG_RDX, UC_X86_REG_RIP,
    UC_X86_REG_RSI, UC_X86_REG_RSP,
)


IMAGE_BASE = 0x10000
STACK_BASE = 0x200000
STACK_SIZE = 0x40000
HEAP_BASE = 0x300000
HEAP_SIZE = 0x10000
SENTINEL = 0xDEADBEEF
DECRYPT = 0x13160
ENCRYPT = 0x13450


def load_driver(uc: Uc, path: Path) -> None:
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    section_table = pe + 24 + optional_size
    sections = []
    for index in range(count):
        offset = section_table + index * 40
        virtual_size, rva, raw_size, raw_offset = struct.unpack_from(
            "<IIII", data, offset + 8
        )
        if rva and raw_size:
            sections.append((rva, virtual_size, raw_size, raw_offset))
    image_end = max(rva + max(virtual_size, raw_size) for rva, virtual_size, raw_size, _ in sections)
    uc.mem_map(IMAGE_BASE, (image_end + 0xFFF) & ~0xFFF)
    for rva, _, raw_size, raw_offset in sections:
        uc.mem_write(IMAGE_BASE + rva, data[raw_offset:raw_offset + raw_size])


def call_in_place(
    uc: Uc,
    function: int,
    buffer: int,
    length: int,
    key: int,
    key_length: int = 8,
    tweak_offset: int = 0,
) -> None:
    rsp = STACK_BASE + STACK_SIZE - 0x1080
    uc.mem_write(rsp, struct.pack("<Q", SENTINEL))
    uc.mem_write(rsp + 0x28, struct.pack("<Q", key_length))
    uc.mem_write(rsp + 0x30, struct.pack("<Q", buffer))
    # Real read/write calls pass the physical backing byte offset in RCX,
    # the data buffer in RDX and again as the final stack argument. The cipher
    # increments this tweak offset by 16 bytes per block.
    for register, value in (
        (UC_X86_REG_RCX, tweak_offset), (UC_X86_REG_RDX, buffer),
        (UC_X86_REG_R8, length), (UC_X86_REG_R9, key),
        (UC_X86_REG_RSP, rsp), (UC_X86_REG_RBP, 0),
        (UC_X86_REG_RBX, 0), (UC_X86_REG_RSI, 0), (UC_X86_REG_RDI, 0),
    ):
        uc.reg_write(register, value)
    uc.emu_start(function, SENTINEL, timeout=30_000_000, count=5_000_000)
    if uc.reg_read(UC_X86_REG_RIP) != SENTINEL:
        raise RuntimeError("Driver function did not return to the sentinel")


def probe(
    driver: Path,
    ciphertext: bytes,
    key8: bytes,
    decrypt: int = DECRYPT,
    encrypt: int = ENCRYPT,
    physical_byte_offset: int = 0,
    decrypted_output: Path | None = None,
) -> dict:
    if len(key8) != 8 or not ciphertext or len(ciphertext) % 16:
        raise ValueError("Key must be 8 bytes and ciphertext a nonempty multiple of 16 bytes")
    if len(ciphertext) > 0xC00:
        raise ValueError("Probe is limited to the 0xC00 LBA7 compatibility extent")
    uc = Uc(UC_ARCH_X86, UC_MODE_64)
    uc.mem_map(STACK_BASE, STACK_SIZE)
    uc.mem_map(HEAP_BASE, HEAP_SIZE)
    load_driver(uc, driver)
    buffer, key = HEAP_BASE, HEAP_BASE + 0x4000
    uc.mem_write(buffer, ciphertext)
    uc.mem_write(key, key8)
    call_in_place(
        uc, decrypt, buffer, len(ciphertext), key, tweak_offset=physical_byte_offset
    )
    plaintext = bytes(uc.mem_read(buffer, len(ciphertext)))
    if decrypted_output is not None:
        decrypted_output.write_bytes(plaintext)
    call_in_place(
        uc, encrypt, buffer, len(ciphertext), key, tweak_offset=physical_byte_offset
    )
    roundtrip = bytes(uc.mem_read(buffer, len(ciphertext))) == ciphertext
    counts = Counter(plaintext)
    entropy = -sum((n / len(plaintext)) * math.log2(n / len(plaintext)) for n in counts.values())
    return {
        "decrypt_address": f"0x{decrypt:x}",
        "encrypt_address": f"0x{encrypt:x}",
        "physical_byte_offset": physical_byte_offset,
        "ciphertext_sha256": hashlib.sha256(ciphertext).hexdigest(),
        "decrypted_sha256": hashlib.sha256(plaintext).hexdigest(),
        "roundtrip_exact": roundtrip,
        "decrypted_entropy_bits_per_byte": entropy,
        "decrypted_nonzero_bytes": sum(bool(value) for value in plaintext),
        "known_markers": {
            marker.decode(): plaintext.find(marker)
            for marker in (b"EDPF", b"FAT", b"NTFS", b"LLGB", b"IIR")
            if marker in plaintext
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--compat-extent", type=Path, required=True)
    parser.add_argument("--key8", required=True, help="16 hex digits from a CRC-validated LBA7 key")
    parser.add_argument(
        "--decrypted-output",
        type=Path,
        help="optional path for the decrypted extent (written only after a successful decrypt call)",
    )
    parser.add_argument(
        "--decrypt-address",
        type=lambda value: int(value, 0),
        default=DECRYPT,
        help=f"driver decrypt function address (default: 0x{DECRYPT:x})",
    )
    parser.add_argument(
        "--physical-byte-offset",
        type=lambda value: int(value, 0),
        required=True,
        help="physical backing byte offset used by the real driver as the tweak seed",
    )
    parser.add_argument(
        "--encrypt-address",
        type=lambda value: int(value, 0),
        default=ENCRYPT,
        help=f"driver encrypt function address (default: 0x{ENCRYPT:x})",
    )
    args = parser.parse_args()
    result = probe(
        args.driver,
        args.compat_extent.read_bytes(),
        bytes.fromhex(args.key8),
        decrypt=args.decrypt_address,
        encrypt=args.encrypt_address,
        physical_byte_offset=args.physical_byte_offset,
        decrypted_output=args.decrypted_output,
    )
    print(json.dumps(result, indent=2))
    if not result["roundtrip_exact"]:
        raise SystemExit("Driver transform roundtrip failed; do not interpret decrypted output")


if __name__ == "__main__":
    main()
