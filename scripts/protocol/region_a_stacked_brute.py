#!/usr/bin/env python3
"""K0-search for an EDPSIMPLE rolling-XOR layer over Region A.

decoded_word[i] = d0[i] ^ K0 where d0[i] = base[i] ^ keystep(i)
keystep linear    : -i                 (libedpedisk.so CipherEDPSIMPLE::DecryptBlocks)
keystep quadratic : 256i - i(i+1)/2    (LBA6/7 xor_decode family)
base in {raw ciphertext, EDPAES(key8, counter=offset)-decrypted}."""
import sys
from pathlib import Path

import numpy as np

W = Path("/Users/zhangyuxi/.devspace/worktrees/edpcli-region-a-reverse-20260922")
sys.path.insert(0, str(W / "scripts" / "protocol"))
from probe_legacy_region_a_driver import DECRYPT, call_in_place, load_driver  # noqa: E402
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64  # noqa: E402

DRIVER = Path("/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDisk64.sys")
DISKS = {
    "lexar": (W / "audit/region_a/gold/lexar_region_a_lba243623933.bin",
              bytes.fromhex("dd4019e3637d390f")),
    "sandisk_nopwd": (W / "audit/region_a/live_captures/sandisk_nopwd_20260923/region_a_chs.bin",
                      bytes.fromhex("24a4cfbdc9bf4101")),
}
MAGICS = [b"EDPF", b"DRKB", b"LLGB", b"SAPF", b"FPAS", b"FAT1", b"NTFS", b"BKDP", b"VERA", b"$$$"]


def aes_layer(ct, key8):
    uc = aes_layer.uc
    uc.mem_write(aes_layer.keyp, key8)
    uc.mem_write(aes_layer.buf, ct)
    call_in_place(uc, DECRYPT, aes_layer.buf, len(ct), aes_layer.keyp)
    return bytes(uc.mem_read(aes_layer.buf, len(ct)))


uc = Uc(UC_ARCH_X86, UC_MODE_64)
uc.mem_map(0x200000, 0x40000)
uc.mem_map(0x300000, 0x10000)
load_driver(uc, DRIVER)
aes_layer.uc, aes_layer.buf, aes_layer.keyp = uc, 0x300000, 0x304000


def search(tag, layer, base, quad):
    w = np.frombuffer(base, dtype="<u2")
    i = np.arange(len(w), dtype=np.int64)
    ks = (256 * i - i * (i + 1) // 2) & 0xFFFF if quad else (-i) & 0xFFFF
    d0 = (w.astype(np.int64) ^ ks).astype(np.uint16)
    hits = set()
    for m in MAGICS:  # even-offset magic: words j, j+1
        mw1 = int.from_bytes(m[:2], "little")
        mw2 = int.from_bytes(m[2:4].ljust(2, b"\0"), "little")
        k0 = (d0[: len(d0) - 1] ^ np.uint16(mw1)).astype(np.uint16)
        ok = (d0[1:] ^ k0) == np.uint16(mw2)
        for j in np.flatnonzero(ok):
            hits.add((int(k0[j]), m.decode(errors="replace")))
    eq = d0[1:] == d0[:-1]  # decoded-zero run <=> d0 constant run
    run = 0
    for j in range(len(eq)):
        run = run + 1 if eq[j] else 0
        if run >= 7:
            k0 = int(d0[j - 6])
            hits.add((k0, "zerorun"))
    for k0, why in sorted(hits):
        dec = (d0 ^ np.uint16(k0)).tobytes()
        if why == "zerorun":
            zr = max(len(s) for s in dec.split(b"\0"))
            print(f"HIT {tag} {layer} quad={quad} K0=0x{k0:04x} zerorun={zr}B")
        else:
            print(f"HIT {tag} {layer} quad={quad} K0=0x{k0:04x} magic={why}")
        print("  head:", dec[:80].hex())
    if not hits:
        print(f"no hits: {tag} {layer} quad={quad}")


for tag, (path, key8) in DISKS.items():
    ct = path.read_bytes()
    for layer, base in (("H2_simple_only", ct), ("H1_aes_then_simple", aes_layer(ct, key8))):
        for quad in (False, True):
            search(tag, layer, base, quad)
