#!/usr/bin/env python3
"""Validate historical six-sector LCE compatibility notice image without devices.

This is a FAT16-*labelled* 3072B extent with an upgrade warning text file,
not evidence of a standard mountable FAT16 volume or OEM write timing.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct

import pefile

GOLD_SHA256 = "386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c"
ROOT_PATH = Path(__file__).resolve().parents[2]
GOLD_PATH = ROOT_PATH / "audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin"
KNOWN_OEMS = {
    "legacy_2022": {"sha256": "57a290d900bc2c97e515b8549a2cffdf9f57ba90ce8f489cfe03c455a2797b1f",
                    "file_offset": 0x1697D8},
    "current_2026": {"sha256": "7b1fc2aae299ee296a774d7e8d693e77f04767cf2326692c453b43eaee913069",
                     "file_offset": 0x203EE8},
}
EXPECTED_TEXT = "您的U盘数据已经升级，但客户端得程序是旧版本的，如果要使用U盘数据请升级客户端程序。"
EXPECTED_FILENAME = "您的U盘数据已经升级,但客户端得程序是旧版本的,如果要使用U盘数据请升级客户端程序.txt"
EXPECTED_FOLDER = "您的U盘数据已经升级，但客户端得程序是旧版本的，如果要使用U盘数据请升级客户端程序。"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def u16(data: bytes, ofs: int) -> int:
    return struct.unpack_from("<H", data, ofs)[0]


def u32(data: bytes, ofs: int) -> int:
    return struct.unpack_from("<I", data, ofs)[0]


def dos_timestamp(date: int, time: int) -> str:
    return (f"{1980 + (date >> 9):04d}-{((date >> 5) & 15):02d}-{date & 31:02d}"
            f"T{time >> 11:02d}:{((time >> 5) & 63):02d}:{(time & 31) * 2:02d}")


def vfat_checksum(short_name: bytes) -> int:
    if len(short_name) != 11:
        raise ValueError("VFAT short name must be 11 bytes")
    value = 0
    for x in short_name:
        value = (((value & 1) << 7) | (value >> 1)) + x
        value &= 0xFF
    return value


def lfn_payload(entry: bytes) -> str:
    if len(entry) != 32 or entry[11] != 0x0F:
        raise ValueError("expected one VFAT LFN record")
    chunks = entry[1:11] + entry[14:26] + entry[28:32]
    units = [u16(chunks, i) for i in range(0, 26, 2)]
    result = []
    for val in units:
        if val in (0, 0xFFFF):
            break
        result.append(val)
    return bytes().join(struct.pack("<H", v) for v in result).decode("utf-16le")


def lfn_name(records: list[bytes], checksum: int) -> str:
    if len(records) != 4:
        raise ValueError("historical warning uses exactly four VFAT LFN records")
    result = []
    for i, entry in enumerate(records):
        expected_sequence = 0x40 | 4 if i == 0 else 4 - i
        if entry[0] != expected_sequence or entry[11] != 0x0F or entry[13] != checksum:
            raise ValueError(f"invalid VFAT LFN sequence/checksum at index {i}")
        result.append(lfn_payload(entry))
    return "".join(reversed(result))


def analyze_image(raw: bytes):
    if len(raw) != 3072 or sha(raw) != GOLD_SHA256:
        raise ValueError("historical 3072-byte gold SHA-256/size mismatch")
    bps = u16(raw, 11)
    sectors_per_cluster = raw[13]
    reserved = u16(raw, 14)
    fat_count = raw[16]
    root_entries = u16(raw, 17)
    fat_sectors = u16(raw, 22)
    total = u16(raw, 19) or u32(raw, 32)
    if (bps, sectors_per_cluster, reserved, fat_count, root_entries,
        fat_sectors, total) != (512, 1, 1, 1, 16, 2, 6):
        raise ValueError("historical FAT-like BPB layout changed")
    if raw[54:62] != b"FAT16   " or raw[510:512] != b"\x55\xaa":
        raise ValueError("boot sector FAT16 marker/signature changed")
    root_offset = (reserved + fat_count * fat_sectors) * bps
    records = [raw[root_offset + i*32:root_offset+(i+1)*32] for i in range(16)]
    folder_entry = records[6]
    if folder_entry[11] != 0x10 or folder_entry[0] == 0xE5:
        raise ValueError("active upgrade warning folder missing")
    folder_checksum = vfat_checksum(folder_entry[:11])
    folder = lfn_name(records[2:6], folder_checksum)
    if folder != EXPECTED_FOLDER or u16(folder_entry, 26) != 2:
        raise ValueError("active upgrade warning folder has unexpected LFN or cluster")
    short = records[13]
    if short[11] != 0x20 or short[0] == 0xE5:
        raise ValueError("active warning TXT directory entry missing")
    checksum = vfat_checksum(short[:11])
    name = lfn_name(records[9:13], checksum)
    if name != EXPECTED_FILENAME:
        raise ValueError("active TXT long filename no longer matches OEM notice")
    length = u32(short, 28)
    cluster = u16(short, 26)
    first_data_sector = reserved + fat_count * fat_sectors + (root_entries * 32 + bps - 1) // bps
    if (length, cluster, first_data_sector) != (82, 3, 4):
        raise ValueError("warning TXT cluster/size or extent geometry mismatch")
    data_sector = first_data_sector + (cluster - 2) * sectors_per_cluster
    data = raw[data_sector * bps:data_sector * bps+length]
    notice = data.decode("gbk")
    if notice != EXPECTED_TEXT:
        raise ValueError("warning GBK file contents no longer match")
    # Older deleted file entry is a historical artifact, not the active warning file.
    if records[8][0] != 0xE5 or records[8][11] != 0x20:
        raise ValueError("historical deleted warning entry changed")
    created = dos_timestamp(u16(short, 16), u16(short, 14))
    modified = dos_timestamp(u16(short, 24), u16(short, 22))
    if (created, modified) != ("2009-09-28T16:55:16", "2009-09-28T17:02:36"):
        raise ValueError("historical warning entry metadata changed")
    return {
        "historical_gold_sha256": GOLD_SHA256,
        "sector_size_bytes": bps,
        "total_sectors": total,
        "total_bytes": len(raw),
        "fat_label": raw[54:62].decode().strip(),
        "root_entry_count": root_entries,
        "data_region_first_sector": first_data_sector,
        "cluster_count_implied_by_bpb": (total-first_data_sector)//sectors_per_cluster,
        "standard_fat16_cluster_count_satisfied": False,
        "warning_folder": {
            "long_filename": folder,
            "vfat_checksum": hex(folder_checksum),
            "directory_entry_index": 6,
            "cluster": 2,
            "entry_created_dos_time": dos_timestamp(u16(folder_entry, 16), u16(folder_entry, 14)),
        },
        "warning_txt": {
            "long_filename": name,
            "vfat_checksum": hex(checksum),
            "directory_entry_index": 13,
            "cluster": cluster,
            "data_sector": data_sector,
            "size_bytes": length,
            "content_encoding": "GBK",
            "content": notice,
            "entry_created_dos_time": created,
            "entry_modified_dos_time": modified,
            "historical_dos_time_not_physical_write_time": True,
        },
        "deleted_directory_entry": {
            "index": 8,
            "first_byte": hex(records[8][0]),
            "classification": "deleted 8.3 TXT entry; original name not fully recoverable",
        },
        "physical_lce_write_event_proven": False,
        "is_windows_format_ex_output_proven": False,
        "origin_of_warning_image_proven": False,
    }


def inspect_embedded_oem(path: Path, version: str, gold: bytes):
    spec = KNOWN_OEMS[version]
    data = path.read_bytes()
    if sha(data) != spec["sha256"]:
        raise ValueError(f"OEM binary SHA-256 mismatch: {path}")
    pe = pefile.PE(data=data)
    ofs = spec["file_offset"]
    if pe.FILE_HEADER.Machine != 0x14C or pe.OPTIONAL_HEADER.ImageBase != 0x10000000:
        raise ValueError("unexpected original PE architecture")
    if data.count(gold) != 1 or data.find(gold) != ofs:
        raise ValueError(f"OEM image occurrence/offset drift: {version}")
    return {"version": version, "sha256": sha(data), "image_file_offset": hex(ofs),
            "fully_embedded_image_matches_gold": True,
            "warning_data_file_offset": hex(ofs+5*512)}


def audit(legacy_path: Path, current_path: Path):
    gold = GOLD_PATH.read_bytes()
    report = {
        "schema": 1,
        "scope": "offline only; 3072B FAT16-like warning image semantics",
        "image": analyze_image(gold),
        "oem_origins": [
            inspect_embedded_oem(legacy_path, "legacy_2022", gold),
            inspect_embedded_oem(current_path, "current_2026", gold),
        ],
        "unresolved": [
            "who first assembled the six-sector warning image and when it was physically written",
            "if or when OEM provisioning writes warning image into historical LCE",
            "whether 4Kn has compatible warning payload or any owned trailing 1024 bytes",
        ],
        "accessed_physical_device": False,
    }
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--legacy", required=True, type=Path)
    parser.add_argument("--current", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.legacy, args.current), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
