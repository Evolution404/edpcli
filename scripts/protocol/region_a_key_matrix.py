#!/usr/bin/env python3
"""Key-candidate matrix for the legacy 8B-key transform on sandisk_nopwd Region A."""
import hashlib
import json
import math
import sys
from collections import Counter
from pathlib import Path

WORKTREE = Path("/Users/zhangyuxi/.devspace/worktrees/edpcli-region-a-reverse-20260922")
sys.path.insert(0, str(WORKTREE / "scripts" / "protocol"))
from probe_legacy_region_a_driver import DECRYPT, ENCRYPT, call_in_place, load_driver  # noqa: E402
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64  # noqa: E402

DRIVER = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDisk64.sys")
CT = (WORKTREE / "audit/region_a/live_captures/sandisk_nopwd_20260923/region_a_chs.bin").read_bytes()

uc = Uc(UC_ARCH_X86, UC_MODE_64)
uc.mem_map(0x200000, 0x40000)
uc.mem_map(0x300000, 0x10000)
load_driver(uc, DRIVER)
HEAP, BUF, KEYP = 0x300000, 0x300000, 0x304000

const16 = bytes(uc.mem_read(0x191CC, 16))  # .m addresses are already image-based (base 0x10000)
print(f"const@0x191cc = {const16.hex()}")

key8 = bytes.fromhex("24a4cfbdc9bf4101")
wrapped8 = bytes.fromhex("b5355e2c582ed090")
expanded16 = bytes(key8[i % 8] ^ const16[i] for i in range(16))

CANDIDATES = {
    "key8": (key8, 8),
    "wrapped8": (wrapped8, 8),
    "zero8": (bytes(8), 8),
    "key8_rev": (key8[::-1], 8),
    "expanded16_as_key": (expanded16, 16),
    "md5(key8)_16": (hashlib.md5(key8).digest(), 16),
}

def run(tag, key, keylen):
    uc.mem_write(KEYP, key)
    uc.mem_write(BUF, CT)
    call_in_place(uc, DECRYPT, BUF, len(CT), KEYP)
    pt = bytes(uc.mem_read(BUF, len(CT)))
    # roundtrip check under this key
    uc.mem_write(BUF, pt)
    call_in_place(uc, ENCRYPT, BUF, len(CT), KEYP)
    rt = bytes(uc.mem_read(BUF, len(CT))) == CT
    c = Counter(pt)
    ent = -sum(n / len(pt) * math.log2(n / len(pt)) for n in c.values())
    printable = sum(32 <= b < 127 for b in pt)
    zeros = pt.count(0)
    magics = {m.decode(): pt.find(m) for m in (b"EDPF", b"FAT", b"NTFS", b"LLGB", b"IIR") if m in pt}
    print(json.dumps({
        "key": tag, "keylen": keylen, "roundtrip": rt,
        "entropy": round(ent, 3), "printable_ascii": printable, "zero_bytes": zeros,
        "dec_sha256": hashlib.sha256(pt).hexdigest()[:16], "magics": magics,
    }))

for tag, (key, keylen) in CANDIDATES.items():
    run(tag, key, keylen)

# Also: does .sys.old share the const table? Search raw bytes for const16.
old = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDisk64.sys.old").read_bytes()
print(json.dumps({"sys_old_contains_const16": old.find(const16) != -1,
                  "sys_old_const_offset": old.find(const16)}))
