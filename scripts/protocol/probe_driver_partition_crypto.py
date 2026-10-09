#!/usr/bin/env python3
"""Replay official Windows EDP partition ciphers entirely inside an x64 emulator.

Read-only forensic probe: only accepts one known-hash regular PE driver image.
Never opens physical media, writes to disks, loads drivers or reveals disk keys.
All test keys and plaintext are public deterministic fixtures. Run with
    uv run --locked python scripts/protocol/probe_driver_partition_crypto.py \
        --driver /path/to/EdpEDisk64.sys
"""

import argparse
import hashlib
import json
from pathlib import Path
import stat
import struct

from probe_lba7_compatibility_driver import (
    HEAP_BASE,
    HEAP_SIZE,
    SENTINEL,
    STACK_BASE,
    STACK_SIZE,
    call_in_place,
    load_driver,
)
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64
from unicorn.x86_const import (
    UC_X86_REG_R8,
    UC_X86_REG_R9,
    UC_X86_REG_RAX,
    UC_X86_REG_RCX,
    UC_X86_REG_RDX,
    UC_X86_REG_RIP,
    UC_X86_REG_RSP,
)

DRIVER_SHA256 = "724544a96f899b9bb0f87a8adedd08a961d8e4b8ec40aa90bc8144e5ae7e0620"

# EDPF EncryptMode byte, direct copied from EdpSecDiskNew.dll mount argument.
DRIVER_MODE = {
    1: (0x13450, 0x13160),  # physical offset-tweaked EDPSECDISK AES variant
    2: (0x13F40, 0x14020),  # SM4, packed buffer descriptor
    3: (0x160E0, 0x16330),  # AES-128-ECB, direct buffer
    4: (0x18140, 0x181E0),  # separately present SM4 driver compatibility value
}


def new_emulator(driver: Path) -> Uc:
    uc = Uc(UC_ARCH_X86, UC_MODE_64)
    uc.mem_map(STACK_BASE, STACK_SIZE)
    uc.mem_map(HEAP_BASE, HEAP_SIZE)
    load_driver(uc, driver)
    return uc


def transform(driver: Path, mode: int, source: bytes, key: bytes,
              *, decrypt: bool = False, physical_byte_offset: int = 0) -> bytes:
    if mode not in DRIVER_MODE or len(key) != 16:
        raise ValueError("Mode must be 1..4, and test key must contain exactly 16 bytes")
    if not source or len(source) % 16 or len(source) > 4096:
        raise ValueError("Input must be 16-byte aligned and at most 4096 bytes")
    uc = new_emulator(driver)
    buffer_addr = HEAP_BASE + 0x1000
    key_addr = HEAP_BASE + 0x6000
    descriptor_addr = HEAP_BASE + 0x6500
    uc.mem_write(buffer_addr, source)
    uc.mem_write(key_addr, key)
    stack_pointer = STACK_BASE + STACK_SIZE - 0x1080
    uc.mem_write(stack_pointer, struct.pack("<Q", SENTINEL))
    function_addr = DRIVER_MODE[mode][1 if decrypt else 0]

    if mode == 1:
        # Official 8/16-byte legacy compatible transform, with 16-byte key.
        call_in_place(uc, function_addr, buffer_addr, len(source), key_addr,
                      key_length=16, tweak_offset=physical_byte_offset)
    else:
        if mode == 2:
            # Driver's packed {int byte_count; void* data} input descriptor.
            uc.mem_write(descriptor_addr, struct.pack("<IQ", len(source), buffer_addr))
            arguments = (descriptor_addr, key_addr, 0, 0)
        else:
            arguments = (buffer_addr, len(source), buffer_addr, key_addr)
        for register, value in (
            (UC_X86_REG_RCX, arguments[0]),
            (UC_X86_REG_RDX, arguments[1]),
            (UC_X86_REG_R8, arguments[2]),
            (UC_X86_REG_R9, arguments[3]),
            (UC_X86_REG_RSP, stack_pointer),
        ):
            uc.reg_write(register, value)
        uc.emu_start(function_addr, SENTINEL, timeout=15_000_000, count=50_000_000)
        if uc.reg_read(UC_X86_REG_RIP) != SENTINEL:
            raise RuntimeError(f"Driver transform did not return: {function_addr:#x}")
        if (uc.reg_read(UC_X86_REG_RAX) & 0xFF) != 1:
            raise RuntimeError(f"Driver transform failed: {function_addr:#x}")
    return bytes(uc.mem_read(buffer_addr, len(source)))


def verify(driver: Path) -> dict:
    public_aes_key = bytes.fromhex("000102030405060708090a0b0c0d0e0f")
    public_aes_plain = bytes.fromhex("00112233445566778899aabbccddeeff")
    public_aes_cipher = bytes.fromhex("69c4e0d86a7b0430d8cdb78070b4c55a")
    public_sm4_key = bytes.fromhex("0123456789abcdeffedcba9876543210")
    public_sm4_cipher = bytes.fromhex("681edf34d206965e86b3e94f536e4246")

    def roundtrip(mode: int, source: bytes, key: bytes, offset: int = 0) -> bytes:
        encrypted = transform(driver, mode, source, key, physical_byte_offset=offset)
        decrypted = transform(driver, mode, encrypted, key, decrypt=True,
                              physical_byte_offset=offset)
        if decrypted != source:
            raise AssertionError(f"Mode {mode} is not an exact inverse")
        return encrypted

    mode2 = roundtrip(2, public_sm4_key, public_sm4_key)
    mode3 = roundtrip(3, public_aes_plain, public_aes_key)
    mode4 = roundtrip(4, public_sm4_key, public_sm4_key)
    if mode2 != public_sm4_cipher or mode4 != public_sm4_cipher:
        raise AssertionError("Official driver mode2/mode4 mismatch public SM4 test vector")
    if mode3 != public_aes_cipher:
        raise AssertionError("Official driver AES_CROSS mismatch FIPS-197 AES-128 test vector")

    repeated = roundtrip(3, public_aes_plain * 2, public_aes_key)
    if repeated[:16] != repeated[16:]:
        raise AssertionError("AES_CROSS is not 16-byte block-independent ECB")

    native_data = bytes((index * 37 + 13) % 256 for index in range(4096))
    native_cipher = roundtrip(3, native_data, public_aes_key)
    sm4_data = bytes((index * 7 + 89) % 256 for index in range(512))
    sm4_cipher = roundtrip(2, sm4_data, public_sm4_key)

    legacy_plain = public_aes_plain * 32
    legacy_zero = roundtrip(1, legacy_plain, public_aes_key, offset=0)
    legacy_offset = roundtrip(1, legacy_plain, public_aes_key, offset=4096)
    if legacy_zero == legacy_offset or legacy_zero[:16] == legacy_zero[16:32]:
        raise AssertionError("Mode1 must depend on physical byte offset, not be plain ECB")

    # Ciphertext hashes come from executing the pinned official binary. Assert
    # them as independent goldens, not just roundtripping two matching bugs.
    goldens = (
        (legacy_zero, "b290b2e6e598e027e78453d2db0a91fe5ebb9676b6e05712f986affae34094c9"),
        (legacy_offset, "163d9a5c52dbdde281042fe08272fec8f058c65ebc7eb792f0dded1df9bec66a"),
        (sm4_cipher, "12afec563ab2f58e81690eab87fc85fd99080662fd96da905ac6310cb15e88a6"),
        (native_cipher, "ddb4a6b19a1ccb322887daf997e776e98e90e158141182df47ca741e2d1cef5c"),
    )
    for actual, expected in goldens:
        if hashlib.sha256(actual).hexdigest() != expected:
            raise AssertionError("Official driver ciphertext SHA-256 golden drift")

    return {
        "status": "PASS",
        "driver_sha256": DRIVER_SHA256,
        "test_only": True,
        "mode1": {
            "key_len": 16, "physical_offset_tweak_verified": True,
            "offset_0_sha256": hashlib.sha256(legacy_zero).hexdigest(),
            "offset_4096_sha256": hashlib.sha256(legacy_offset).hexdigest(),
            "roundtrip_512_exact": True,
        },
        "mode2": {
            "algorithm": "SM4-ECB", "reference_vector_match": True,
            "ciphertext": mode2.hex(), "roundtrip_512_exact": True,
            "test_sector_sha256": hashlib.sha256(sm4_cipher).hexdigest(),
        },
        "mode3": {
            "algorithm": "AES-128-ECB", "fips197_vector_match": True,
            "ciphertext": mode3.hex(), "identical_plaintext_blocks_equal": True,
            "roundtrip_4096_exact": True,
            "4096_cipher_sha256": hashlib.sha256(native_cipher).hexdigest(),
        },
        "driver_mode4_compatibility": {
            "algorithm": "SM4-ECB", "reference_vector_match": True,
            "ciphertext": mode4.hex(),
            "note": "Not the EDP Mode4 partition layout",
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True,
                        help="known-hash ordinary EdpEDisk64.sys file (never device path)")
    args = parser.parse_args()
    file = args.driver.resolve(strict=True)
    if not stat.S_ISREG(file.stat().st_mode):
        parser.error("Driver must be an ordinary file, never a device")
    if hashlib.sha256(file.read_bytes()).hexdigest() != DRIVER_SHA256:
        parser.error("Driver SHA-256 mismatch; do not extrapolate address map")
    print(json.dumps(verify(file), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
