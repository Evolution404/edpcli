#!/usr/bin/env python3
"""Structure probe for Region A ciphertext + legacy driver transform.

1. Repeated 16B-block statistics inside each capture (no key needed).
2. ECB-vs-chained independence test for EdpEDisk64.sys sub_13160.
3. If ECB: search enc(0^16) in each ciphertext (zero-plaintext block detector).

Read-only; prints JSON summary to stdout.
"""
import sys
from collections import Counter
from pathlib import Path

WORKTREE = Path("/Users/zhangyuxi/.devspace/worktrees/edpcli-region-a-reverse-20260922")
sys.path.insert(0, str(WORKTREE / "scripts" / "protocol"))
from probe_legacy_region_a_driver import DECRYPT, ENCRYPT, call_in_place, load_driver  # noqa: E402

from unicorn import Uc, UC_ARCH_X86, UC_MODE_64  # noqa: E402

DRIVER = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDisk64.sys")
SAMPLES = {
    "lexar_disk1": (
        WORKTREE / "audit/region_a/gold/lexar_region_a_lba243623933.bin",
        bytes.fromhex("dd4019e3637d390f"),
    ),
    "aigo_disk2": (
        Path("/Users/zhangyuxi/Desktop/u_disk/analyze/last/three_disks/disk2/raw/region_a.bin"),
        None,
    ),
    "sandisk_disk3": (
        Path("/Users/zhangyuxi/Desktop/u_disk/analyze/last/three_disks/disk3/raw/region_a.bin"),
        None,
    ),
    "sandisk_nopwd": (
        WORKTREE / "audit/region_a/live_captures/sandisk_nopwd_20260923/region_a_chs.bin",
        bytes.fromhex("24a4cfbdc9bf4101"),
    ),
}

report = {"repeated_16b_blocks": {}, "ecb_independence": None, "zero_plaintext_blocks": {}}

# --- Experiment A: repeated 16B ciphertext blocks (key-free) ---
for label, (path, _) in SAMPLES.items():
    data = path.read_bytes()
    blocks = [data[i:i + 16] for i in range(0, len(data), 16)]
    counts = Counter(blocks)
    repeats = {b.hex(): [i * 16 for i, blk in enumerate(blocks) if blk == b]
               for b, n in counts.items() if n > 1}
    report["repeated_16b_blocks"][label] = {
        "sha256_len": len(data),
        "distinct_blocks": len(counts),
        "total_blocks": len(blocks),
        "repeats": repeats,
    }

# --- Experiment B: ECB independence (use lexar key8, any buffer) ---
uc = Uc(UC_ARCH_X86, UC_MODE_64)
uc.mem_map(0x200000, 0x40000)  # STACK_BASE/STACK_SIZE from the probe module
uc.mem_map(0x300000, 0x10000)  # HEAP_BASE/HEAP_SIZE from the probe module
load_driver(uc, DRIVER)
HEAP = 0x300000
lexar_ct = SAMPLES["lexar_disk1"][0].read_bytes()
key8 = SAMPLES["lexar_disk1"][1]
uc.mem_write(HEAP + 0x4000, key8)
KEYPTR = HEAP + 0x4000

# Full-buffer decrypt, then look at block 5 (offset 0x50).
full = bytearray(lexar_ct)
uc.mem_write(HEAP, bytes(full))
call_in_place(uc, DECRYPT, HEAP, len(full), KEYPTR)
full_dec_block5 = bytes(uc.mem_read(HEAP + 0x50, 16))

# Lone-block decrypt of the same ciphertext block.
uc.mem_write(HEAP, lexar_ct[0x50:0x60])
call_in_place(uc, DECRYPT, HEAP, 16, KEYPTR)
lone_dec_block5 = bytes(uc.mem_read(HEAP, 16))

# Identical-plaintext ECB test on the encrypt side.
pt = bytes(range(16)) * 2
uc.mem_write(HEAP, pt)
call_in_place(uc, ENCRYPT, HEAP, 32, KEYPTR)
enc_two = bytes(uc.mem_read(HEAP, 32))

report["ecb_independence"] = {
    "decrypt_block5_full_buffer": full_dec_block5.hex(),
    "decrypt_block5_alone": lone_dec_block5.hex(),
    "block_independent": full_dec_block5 == lone_dec_block5,
    "encrypt_identical_pair_halves": [enc_two[:16].hex(), enc_two[16:].hex()],
    "encrypt_ecb_detected": enc_two[:16] == enc_two[16:],
}

# --- Experiment C: enc(0^16) search (zero-plaintext block detector) ---
if report["ecb_independence"]["block_independent"]:
    for label in ("lexar_disk1", "sandisk_nopwd"):
        path, key = SAMPLES[label]
        ct = path.read_bytes()
        uc.mem_write(HEAP + 0x4000, key)
        uc.mem_write(HEAP, bytes(16))
        call_in_place(uc, ENCRYPT, HEAP, 16, HEAP + 0x4000)
        enc_zero = bytes(uc.mem_read(HEAP, 16))
        hits = [i * 16 for i in range(len(ct) // 16) if ct[i * 16:i * 16 + 16] == enc_zero]
        report["zero_plaintext_blocks"][label] = {
            "enc_zero_block": enc_zero.hex(),
            "ciphertext_hits": hits,
        }

import json
print(json.dumps(report, indent=2))
