#!/usr/bin/env python3
"""Offline audit of VUpdateReplace encrypted metadata and 2025->2026 lineage.

This script performs no network I/O. It pins the local updater and encrypted
metadata, reproduces the updater's 32-round TEA-family decryptor, and proves
that the surviving metadata records the 2025 local baseline and the 2026
replacement artifact. It deliberately does not claim recovery of the deleted
2025 CEMSUsbRegsiter bytes and therefore promotes no protocol byte.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path


UPDATER_SHA256 = "9b1104570a05b4a3a5fb4ad1eec85778f70eea786897c8af97aa9de5190ab8d1"
PRODUCT_INFO_SHA256 = "4eee41f617837db2ce5d0d0f57439dd88f81f77fb36ef56467291764437c1fca"
FILE_RECORD_SHA256 = "76cf32f816ff6a8d08e312659241543a9f3f9ef141fa768642a35e547c7e3164"
UPGRADE_INDEX_SHA256 = "a0f19a93a0b5b68303b8d13aa0c96f80083d0bb871c64eee5af25144d4a718d5"

IMAGE_BASE = 0x400000
TEA_DEC_BLOCK_VA = 0x4275F0
TEA_DEC_BUFFER_VA = 0x427670
TEA_KEY_VA = 0x4BE67C
TEA_DELTA = 0x7E69AC4E
TEA_INITIAL_SUM = 0xCD3589C0
TEA_KEYS = (0x27BDB886, 0xF03E934F, 0x993BA3AE, 0xD0AAE945)
TEA_KEY_BYTES = bytes.fromhex("86b8bd274f933ef0aea33b9945e9aad0")

OLD_BASE = "8.1.2502.2116"
NEW_BASE = "8.1.2604.0917"
TARGET_ZIP = b"ydcc/cemsusbregsiter.dll.zip"
TARGET_FILE_MD5 = b"2ABA574551E59550B0D4FB50E8BECD27"
TARGET_ZIP_CRC = b"F9FE2852"
TARGET_ZIP_SIZE = b"533081"

MASK32 = 0xFFFFFFFF


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_image_base(image: bytes) -> int:
    pe_off = struct.unpack_from("<I", image, 0x3C)[0]
    file_hdr = pe_off + 4
    optional = file_hdr + 20
    magic = struct.unpack_from("<H", image, optional)[0]
    if magic == 0x10B:
        return struct.unpack_from("<I", image, optional + 28)[0]
    if magic == 0x20B:
        return struct.unpack_from("<Q", image, optional + 24)[0]
    raise SystemExit(f"unknown PE optional-header magic 0x{magic:X}")


def pe_rva_to_offset(image: bytes, rva: int) -> int:
    if image[:2] != b"MZ":
        raise SystemExit("VUpdateReplace is not an MZ image")
    pe_off = struct.unpack_from("<I", image, 0x3C)[0]
    if image[pe_off : pe_off + 4] != b"PE\0\0":
        raise SystemExit("VUpdateReplace has no PE signature")
    file_hdr = pe_off + 4
    section_count = struct.unpack_from("<H", image, file_hdr + 2)[0]
    optional_size = struct.unpack_from("<H", image, file_hdr + 16)[0]
    section_off = file_hdr + 20 + optional_size
    for index in range(section_count):
        off = section_off + index * 40
        virtual_size, virtual_address, raw_size, raw_ptr = struct.unpack_from(
            "<IIII", image, off + 8
        )
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            delta = rva - virtual_address
            if delta >= raw_size:
                raise SystemExit(f"RVA 0x{rva:X} is not file-backed")
            return raw_ptr + delta
    raise SystemExit(f"RVA 0x{rva:X} is not mapped")


def read_va(image: bytes, va: int, size: int) -> bytes:
    base = pe_image_base(image)
    off = pe_rva_to_offset(image, va - base)
    return image[off : off + size]


def tea_decrypt_block(block: bytes) -> bytes:
    v0, v1 = struct.unpack("<II", block)
    total = TEA_INITIAL_SUM
    for _ in range(32):
        mix = (
            ((v0 >> 5) + TEA_KEYS[3])
            ^ ((v0 << 4) + TEA_KEYS[2])
            ^ (total + v0)
        ) & MASK32
        v1 = (v1 - mix) & MASK32
        mix = (
            ((v1 >> 5) + TEA_KEYS[1])
            ^ ((v1 << 4) + TEA_KEYS[0])
            ^ (total + v1)
        ) & MASK32
        v0 = (v0 - mix) & MASK32
        total = (total - TEA_DELTA) & MASK32
    return struct.pack("<II", v0, v1)


def decrypt_buffer(ciphertext: bytes) -> bytes:
    out = bytearray(ciphertext)
    full = len(out) // 8 * 8
    for offset in range(0, full, 8):
        out[offset : offset + 8] = tea_decrypt_block(out[offset : offset + 8])
    return bytes(out)


def require(blob: bytes, marker: bytes, label: str) -> int:
    offset = blob.find(marker)
    if offset < 0:
        raise SystemExit(f"missing {label}: {marker!r}")
    return offset


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/product/CEMS"),
    )
    args = parser.parse_args()

    updater = args.root / "VUpdateReplace.exe"
    product_info = args.root / "UpdateProductInfo.xml.data"
    file_record = args.root / "UpdateProductFileRecord.xml.data.bk"
    upgrade_index = args.root / "UpgradeIndex.xml.enc"
    upgrade_index_base = args.root / "UpgradeIndex.xml.enc.base"

    expected = {
        updater: UPDATER_SHA256,
        product_info: PRODUCT_INFO_SHA256,
        file_record: FILE_RECORD_SHA256,
        upgrade_index: UPGRADE_INDEX_SHA256,
        upgrade_index_base: UPGRADE_INDEX_SHA256,
    }
    for path, digest in expected.items():
        actual = sha256(path)
        if actual != digest:
            raise SystemExit(f"SHA-256 mismatch for {path}: {actual}")

    exe = updater.read_bytes()
    if pe_image_base(exe) != IMAGE_BASE:
        raise SystemExit("unexpected VUpdateReplace image base")
    if read_va(exe, TEA_KEY_VA, 16) != TEA_KEY_BYTES:
        raise SystemExit("VUpdateReplace TEA key table drifted")
    if (TEA_DELTA * 32) & MASK32 != TEA_INITIAL_SUM:
        raise SystemExit("TEA initial sum no longer equals delta * 32")

    info_plain = decrypt_buffer(product_info.read_bytes())
    info = json.loads(info_plain.decode("utf-8"))
    if info.get("LocalVersionBase") != OLD_BASE:
        raise SystemExit(f"unexpected LocalVersionBase: {info.get('LocalVersionBase')!r}")
    if info.get("ServiceVersionBase") != NEW_BASE:
        raise SystemExit(
            f"unexpected ServiceVersionBase: {info.get('ServiceVersionBase')!r}"
        )

    record_plain = decrypt_buffer(file_record.read_bytes())
    index_plain = decrypt_buffer(upgrade_index.read_bytes())
    index_base_plain = decrypt_buffer(upgrade_index_base.read_bytes())
    if index_plain != index_base_plain:
        raise SystemExit("UpgradeIndex.xml.enc and .base no longer decrypt identically")

    target_markers = (
        TARGET_FILE_MD5,
        TARGET_ZIP,
        TARGET_ZIP_CRC,
        TARGET_ZIP_SIZE,
        NEW_BASE.encode(),
    )
    record_offsets = {
        marker.decode(): require(record_plain, marker, "file-record marker")
        for marker in target_markers
    }
    index_offsets = {
        marker.decode(): require(index_plain, marker, "upgrade-index marker")
        for marker in target_markers
    }
    if OLD_BASE.encode() in record_plain or OLD_BASE.encode() in index_plain:
        raise SystemExit("surviving replacement indexes unexpectedly contain the old base")

    print(
        json.dumps(
            {
                "inputs": {
                    "updater_sha256": UPDATER_SHA256,
                    "product_info_sha256": PRODUCT_INFO_SHA256,
                    "file_record_sha256": FILE_RECORD_SHA256,
                    "upgrade_index_sha256": UPGRADE_INDEX_SHA256,
                },
                "tea_family": {
                    "decrypt_block_va": f"0x{TEA_DEC_BLOCK_VA:08X}",
                    "decrypt_buffer_va": f"0x{TEA_DEC_BUFFER_VA:08X}",
                    "key_va": f"0x{TEA_KEY_VA:08X}",
                    "delta": f"0x{TEA_DELTA:08X}",
                    "initial_sum": f"0x{TEA_INITIAL_SUM:08X}",
                    "rounds": 32,
                    "keys": [f"0x{value:08X}" for value in TEA_KEYS],
                    "tail_policy": "decrypt floor(len/8) complete blocks; leave remainder",
                },
                "lineage": {
                    "local_version_base": info["LocalVersionBase"],
                    "service_version_base": info["ServiceVersionBase"],
                    "target_zip": TARGET_ZIP.decode(),
                    "target_file_md5": TARGET_FILE_MD5.decode(),
                    "target_zip_crc": TARGET_ZIP_CRC.decode(),
                    "target_zip_size": int(TARGET_ZIP_SIZE),
                    "file_record_offsets": record_offsets,
                    "upgrade_index_offsets": index_offsets,
                    "upgrade_index_base_identical": True,
                },
                "claim": (
                    "VUpdateReplace's local metadata cipher is reproducibly closed; "
                    "the surviving request proves the endpoint was on 8.1.2502.2116 "
                    "before upgrading to 8.1.2604.0917, while the surviving file/index "
                    "records describe only the new cemsusbregsiter.dll.zip artifact"
                ),
                "claim_boundary": (
                    "offline acquisition/provenance evidence only; the old 2025 file "
                    "record and deleted DLL/zip are not present, so this audit supplies "
                    "no old CRC/size/hash and promotes no LBA byte"
                ),
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
