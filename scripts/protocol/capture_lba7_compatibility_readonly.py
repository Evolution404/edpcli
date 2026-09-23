#!/usr/bin/env python3
"""Read the LBA7 pointer area and both CHS/physical tail candidates.

Run with read permission on the whole disk. This tool opens the device O_RDONLY
and never issues a write or a disk-management command.
"""

import argparse
import hashlib
import json
import os
import plistlib
import re
import subprocess
from pathlib import Path

CHS_TRACK_SECTORS = 255 * 63
COMPAT_BACKOFF_BYTES = 0xE0000
COMPAT_SIZE = 0xC00


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--disk", required=True, help="Whole disk name, e.g. disk5")
    parser.add_argument("--expected-bytes", required=True, type=int)
    parser.add_argument("--expected-media-name", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    if not re.fullmatch(r"disk[0-9]+", args.disk):
        parser.error("--disk must name a whole disk")
    info = plistlib.loads(
        subprocess.check_output(["diskutil", "info", "-plist", f"/dev/{args.disk}"])
    )
    sector_size = int(info["DeviceBlockSize"])
    total_bytes = int(info["TotalSize"])
    if (
        not info.get("WholeDisk")
        or info.get("BusProtocol") != "USB"
        or total_bytes != args.expected_bytes
        or info.get("MediaName") != args.expected_media_name
        or sector_size != 512
    ):
        raise SystemExit("device identity, size, or sector size changed; no read performed")

    total_sectors = total_bytes // sector_size
    chs_sectors = total_sectors // CHS_TRACK_SECTORS * CHS_TRACK_SECTORS
    compat_offset = chs_sectors * sector_size - COMPAT_BACKOFF_BYTES
    physical_tail_offset = total_bytes - COMPAT_BACKOFF_BYTES
    device = f"/dev/r{args.disk}"
    fd = os.open(device, os.O_RDONLY | os.O_CLOEXEC)
    try:
        lba7 = os.pread(fd, sector_size, 7 * sector_size)
        compatibility_extent = os.pread(fd, COMPAT_SIZE, compat_offset)
        neighborhood = os.pread(fd, COMPAT_SIZE + 0x1000, compat_offset - 0x800)
        physical_tail_candidate = os.pread(fd, COMPAT_SIZE, physical_tail_offset)
    finally:
        os.close(fd)
    if any(len(data) != length for data, length in (
        (lba7, sector_size), (compatibility_extent, COMPAT_SIZE),
        (neighborhood, COMPAT_SIZE + 0x1000),
        (physical_tail_candidate, COMPAT_SIZE),
    )):
        raise SystemExit("short disk read; no capture saved")

    args.out.mkdir(parents=True, exist_ok=True)
    files = {
        "lba7_raw.bin": lba7,
        "lba7_compat_extent.bin": compatibility_extent,
        "lba7_compat_neighborhood.bin": neighborhood,
        "physical_minus_e0000.bin": physical_tail_candidate,
    }
    owner = args.out.stat()
    for name, data in files.items():
        path = args.out / name
        path.write_bytes(data)
        os.chown(path, owner.st_uid, owner.st_gid)
    manifest = {
        "device": device,
        "media_name": info["MediaName"],
        "total_bytes": total_bytes,
        "total_sectors": total_sectors,
        "chs_sectors": chs_sectors,
        "sector_size": sector_size,
        "lba7_compat_lba": compat_offset // sector_size,
        "lba7_compat_byte_offset": compat_offset,
        "physical_minus_e0000_lba": physical_tail_offset // sector_size,
        "physical_minus_e0000_byte_offset": physical_tail_offset,
        "files": {name: {"length": len(data), "sha256": sha256(data),
                         "nonzero_bytes": sum(byte != 0 for byte in data)}
                  for name, data in files.items()},
    }
    manifest_path = args.out / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    os.chown(manifest_path, owner.st_uid, owner.st_gid)
    print(json.dumps(manifest, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
