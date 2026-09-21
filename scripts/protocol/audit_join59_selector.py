#!/usr/bin/env python3
"""Pin the current join59/join60 consumer selector and current join60 writer.

This audit deliberately does not promote the unresolved join59 bytes.  It
proves two narrower facts:

* current CEMSUDisk reconstructs a long Dept by testing serialized Dept[59]:
  zero -> LBA9 continuation overwrites Dept[59] (join59), nonzero -> append at
  Dept[60] (join60);
* current CEMSUsbRegsiter cannot emit join59: its long-Dept writer is entered
  only for strlen(Dept) >= 64, copies 60 inline bytes, and sources continuation
  from Dept[60].
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import pefile
from capstone import CS_ARCH_X86, CS_MODE_32, Cs

CEMSUDISK_SHA256 = "32e88065725ccb9bc50e24c244f5686bf1d38335f58737f454a8f4ca2c892fd1"
CEMSUSBREG_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
IMAGE_BASE = 0x10000000


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load(path: Path, expected: str):
    data = path.read_bytes()
    if digest(data) != expected:
        raise SystemExit(f"SHA-256 mismatch: {path}")
    pe = pefile.PE(data=data)
    if pe.OPTIONAL_HEADER.ImageBase != IMAGE_BASE:
        raise SystemExit(f"unexpected image base: {path}")
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.detail = True
    return pe, md


def require(pe, md, va: int, mnemonic: str, *needles: str) -> None:
    raw = pe.get_data(va - IMAGE_BASE, 15)
    decoded = list(md.disasm(raw, va, count=1))
    if not decoded:
        raise SystemExit(f"no instruction at 0x{va:08X}")
    ins = decoded[0]
    if ins.mnemonic != mnemonic:
        raise SystemExit(
            f"0x{va:08X}: expected {mnemonic}, got {ins.mnemonic} {ins.op_str}"
        )
    op = ins.op_str.lower()
    for needle in needles:
        if needle.lower() not in op:
            raise SystemExit(
                f"0x{va:08X}: missing operand fragment {needle!r}: {ins.op_str}"
            )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cemsudisk",
        type=Path,
        default=Path(
            "/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsudisk.dll"
        ),
    )
    parser.add_argument(
        "--cemsusbregsiter",
        type=Path,
        default=Path(
            "/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll"
        ),
    )
    args = parser.parse_args()

    reader_pe, reader_md = load(args.cemsudisk, CEMSUDISK_SHA256)
    writer_pe, writer_md = load(args.cemsusbregsiter, CEMSUSBREG_SHA256)

    # Current reader: marker, 15 dwords (60 B) inline prefix, then test the
    # final copied byte.  -0xA8 + 59 == -0x6D, so this is exactly Dept[59].
    require(reader_pe, reader_md, 0x101018D0, "cmp", "0x40245e2a")
    require(reader_pe, reader_md, 0x10101910, "mov", "ecx", "0xf")
    require(reader_pe, reader_md, 0x10101915, "lea", "esi", "ebp", "0xa8")
    require(reader_pe, reader_md, 0x1010191B, "rep movsd")
    require(reader_pe, reader_md, 0x1010191D, "movsx", "ebp - 0x6d")
    require(reader_pe, reader_md, 0x10101921, "test", "ecx", "ecx")
    require(reader_pe, reader_md, 0x10101923, "jne", "0x10101940")
    require(reader_pe, reader_md, 0x10101934, "add", "edi", "0x7b")
    require(reader_pe, reader_md, 0x10101937, "mov", "ecx", "0x20")
    require(reader_pe, reader_md, 0x1010193C, "rep movsd")
    require(reader_pe, reader_md, 0x1010194F, "add", "edi", "0x7c")
    require(reader_pe, reader_md, 0x10101952, "mov", "ecx", "0x20")
    require(reader_pe, reader_md, 0x10101957, "rep movsd")

    # Current writer: long path requires strlen >= 64, emits marker + 60 bytes,
    # then continuation from source +0x7C (= Dept base +0x40 + 60) to LBA9+0x80.
    require(writer_pe, writer_md, 0x1001402D, "cmp", "0x40")
    require(writer_pe, writer_md, 0x10014034, "jae", "0x10014050")
    require(writer_pe, writer_md, 0x1001405A, "push", "0x3c")
    require(writer_pe, writer_md, 0x1001406D, "mov", "0x40245e2a")
    require(writer_pe, writer_md, 0x10014077, "push", "0x3c")
    require(writer_pe, writer_md, 0x100140BD, "sub", "edx", "0x3b")
    require(writer_pe, writer_md, 0x100140C4, "add", "eax", "0x7c")
    require(writer_pe, writer_md, 0x100140CE, "add", "ecx", "0x80")

    print(
        json.dumps(
            {
                "cemsudisk_sha256": CEMSUDISK_SHA256,
                "cemsusbregsiter_sha256": CEMSUSBREG_SHA256,
                "reader_selector": (
                    "marker long Dept: serialized Dept[59] == 0 -> join59 at "
                    "output+0x7B; nonzero -> join60 at output+0x7C"
                ),
                "reader_continuation_bytes": 128,
                "current_writer": (
                    "strlen>=64; marker + 60-byte inline prefix; continuation "
                    "source Dept[60] -> LBA9+0x80"
                ),
                "claim_boundary": (
                    "current writer is excluded as join59 producer; exact "
                    "historical join59 producer/profile selection remains open"
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
