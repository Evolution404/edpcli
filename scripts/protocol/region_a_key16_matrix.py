#!/usr/bin/env python3
"""Region A decrypt with PROPER keylen=16 (prior probe hardcoded keylen=8).

Also uses a constant oracle: a correct old-profile partition-header plaintext
should contain known entry u64s (StartSector / SectorSize=512 / PartionSize=3072)
either plaintext or behind the 24B EDPSIMPLEKEY window at +0x18.
"""
import hashlib
import json
import math
import struct
import sys
from collections import Counter
from pathlib import Path

import numpy as np

W = Path("/Users/zhangyuxi/.devspace/worktrees/edpcli-region-a-reverse-20260922")
sys.path.insert(0, str(W / "scripts" / "protocol"))
import probe_legacy_region_a_driver as P  # noqa: E402
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64  # noqa: E402
from unicorn.x86_const import (  # noqa: E402
    UC_X86_REG_RCX, UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_R9,
    UC_X86_REG_RSP, UC_X86_REG_RBP, UC_X86_REG_RBX, UC_X86_REG_RSI, UC_X86_REG_RDI,
)

DRIVER = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDisk64.sys")
UC, BUF, KEYP, SENT = 0x10000, 0x300000, 0x304000, P.SENTINEL

uc = Uc(UC_ARCH_X86, UC_MODE_64)
uc.mem_map(0x200000, 0x40000)
uc.mem_map(0x300000, 0x10000)
P.load_driver(uc, DRIVER)


def call(keylen):
    """Replicate call_in_place but with configurable key length (arg5)."""
    rsp = 0x200000 + 0x40000 - 0x1080
    uc.mem_write(rsp, struct.pack("<Q", SENT))
    uc.mem_write(rsp + 0x28, struct.pack("<Q", keylen))
    uc.mem_write(rsp + 0x30, struct.pack("<Q", BUF))
    for reg, val in ((UC_X86_REG_RCX, BUF), (UC_X86_REG_RDX, BUF),
                     (UC_X86_REG_R8, 3072), (UC_X86_REG_R9, KEYP),
                     (UC_X86_REG_RSP, rsp), (UC_X86_REG_RBP, 0),
                     (UC_X86_REG_RBX, 0), (UC_X86_REG_RSI, 0), (UC_X86_REG_RDI, 0)):
        uc.reg_write(reg, val)
    uc.emu_start(P.DECRYPT, SENT, timeout=30_000_000, count=5_000_000)
    if uc.reg_read(0x1023) != SENT and uc.reg_read(P.UC_X86_REG_RIP if hasattr(P, "UC_X86_REG_RIP") else 0) != SENT:
        pass
    return bytes(uc.mem_read(BUF, 3072))


DISKS = {
    "lexar": (W / "audit/region_a/gold/lexar_region_a_lba243623933.bin",
              bytes.fromhex("dd4019e3637d390f"),
              bytes.fromhex("4cd18872f2eca89e"),
              [243623933, 512, 3072]),
    "sandisk_nopwd": (W / "audit/region_a/live_captures/sandisk_nopwd_20260923/region_a_chs.bin",
                      bytes.fromhex("24a4cfbdc9bf4101"),
                      bytes.fromhex("b5355e2c582ed090"),
                      [120164408, 512, 3072]),
}
PWD = b"0000aaaa"
pwdsum = sum(struct.unpack_from("<I", PWD, i)[0] for i in range(0, len(PWD), 4)) & 0xFFFFFFFF


def u64s(v):
    return struct.pack("<Q", v) + struct.pack(">Q", v) + struct.pack("<I", v & 0xFFFFFFFF)


def check(pt, consts):
    hits = []
    for v in consts:
        for pat in (struct.pack("<Q", v), struct.pack("<I", v)):
            j = pt.find(pat)
            if j >= 0:
                hits.append(f"const@{j:#x}={v}")
    for m in (b"EDPF", b"DRKB", b"LLGB", b"FAT1", b"NTFS", b"BKDP"):
        j = pt.find(m)
        if j >= 0:
            hits.append(f"magic@{j:#x}={m}")
    return hits


for tag, (path, key8, wrapped8, consts) in DISKS.items():
    ct = path.read_bytes()
    cands = {
        "md5(key8)": hashlib.md5(key8).digest(),
        "sha256(key8)[:16]": hashlib.sha256(key8).digest()[:16],
        "key8||key8": key8 + key8,
        "key8||zeros": key8 + bytes(8),
        "zeros||key8": bytes(8) + key8,
        "key8||pwdsum": key8 + struct.pack("<Q", pwdsum),
        "md5(pwd_ascii)": hashlib.md5(PWD).digest(),
        "md5(pwd_utf16le)": hashlib.md5(PWD.decode().encode("utf-16-le")).digest(),
        "key8||wrapped8": key8 + wrapped8,
        "wrapped8||key8": wrapped8 + key8,
        "wrapped8||wrapped8": wrapped8 + wrapped8,
        "md5(key8||pwd)": hashlib.md5(key8 + PWD).digest(),
        "md5(pwd||key8)": hashlib.md5(PWD + key8).digest(),
    }
    for name, key in cands.items():
        uc.mem_write(KEYP, key)
        uc.mem_write(BUF, ct)
        pt = call(len(key))
        c = Counter(pt)
        ent = -sum(n / len(pt) * math.log2(n / len(pt)) for n in c.values())
        hits = check(pt, consts)
        zeros = pt.count(0)
        if hits or zeros > 200 or ent < 7.5:
            print(f"*** {tag} {name} keylen={len(key)}: entropy={ent:.3f} zeros={zeros} hits={hits}")
            print("    head:", pt[:64].hex())
        else:
            print(f"    {tag} {name}: entropy={ent:.3f} zeros={zeros} no-hits")
