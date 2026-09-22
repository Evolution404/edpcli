#!/usr/bin/env python3
"""Reproduce the semantic join59/join60 long-Dept closure evidence.

This audit deliberately separates *wire semantics* from exact historical
producer provenance.

It pins four independent facts:

1. Current CEMSUDisk selects the long-Dept join from serialized Dept[59]:
   zero -> join59, nonzero -> join60.
2. Two independent CEMS2.0 FileOpHook builds (x86/x64) hard-code join59:
   copy 60 inline bytes, then overlay the 128-byte LBA9 continuation at
   logical Dept offset 59.
3. Seven independent committed long-Dept physical images reconstruct to the
   same 76-byte Dept. A standalone join59 sector fixture is verified as an
   exact duplicate of already-counted Lexar sector bytes and is used only as a
   regression fixture, never as an eighth independent physical observation.
   Every observed LBA9 continuation is a NUL-terminated string followed by
   consumer-ignored post-NUL backing; the currently observed backing is zero.
4. The 12 committed short-Dept physical images terminate their Dept before
   byte63, while LBA6+0x03F is both zero and non-zero across those samples,
   proving that byte is ordinary post-NUL backing in the short profile.
5. The currently available writer emits join60 only. Therefore this audit
   closes the observed wire-to-Dept interpretation, but it does *not* claim
   to recover the exact historical executable/profile selector that emitted
   join59.

No physical device is accessed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

SECTOR = 512
LBA6_K0 = 0x4DAA
MARKER = 0x40245E2A

CURRENT_READER_SHA256 = "32e88065725ccb9bc50e24c244f5686bf1d38335f58737f454a8f4ca2c892fd1"
CURRENT_WRITER_SHA256 = "122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb"
FILEOP_X86_SHA256 = "db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65"
FILEOP_X64_SHA256 = "93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9"

FULL_IMAGES = {
    "audit/protocol/gold/strict-encrypted/disk26_491520000_vid3535_pid2000_disk&ven_aigo&prod_l8302_onlyid1911491440_20260903_120554.bin":
        ("964f0f765e0ca027c4f5b43bfbf66f9263365eab2bd7b178383f4008ea1c210b", 59),
    "audit/protocol/gold/strict-encrypted/disk4_120832000_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid680322626_20260828_221831.bin":
        ("348dcecb6d90e6d9d2add62624ea04bcb13aac93f4a8c1f54bba8fcbedaace2f", 59),
    "audit/protocol/gold/strict-encrypted/disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin":
        ("04874b9c0e7a4e4c982fcf8a77f9d686cbd24578f17c85058432da314dfa585a", 60),
    "audit/protocol/gold/strict-encrypted/disk4_1953525168_vid174c_pid55aa_disk&ven_aigo&prod_hd806_onlyid-1615488206_20260903_121800.bin":
        ("b28c9d721b758434607084a6c7dd56ae66d1d99b12f96ed6aa7b3f838d81fb14", 60),
    "audit/protocol/gold/strict-encrypted/disk4_1953525168_vid174c_pid55aa_disk&ven_aigo&prod_hd806_onlyid-1833210541_20260903_121552.bin":
        ("e7e5abc23b81978f8a7f4cb46ea3a43f731af3e0ae99e9cd2deb3c681772d639", 60),
    "audit/protocol/gold/strict-encrypted/disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335_onlyid2071754312_20260828_120922.bin":
        ("784ae05ac7009d47ce7a26f02b09002f10224bcf381209e436acbdb7c7142b29", 60),
    "audit/protocol/gold/strict-encrypted/disk6_120832000_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid1808795831_20260910_172855.bin":
        ("8743b292e0b540204bc59908cac92633bfc72e08247abf43393664d00702ae34", 59),
}

FIXTURE_LBA6 = Path("tests/fixtures/protocol_evidence/lexar_join59_lba6.hex")
FIXTURE_LBA9 = Path("tests/fixtures/protocol_evidence/lexar_join59_lba9.hex")
FIXTURE_LBA6_SHA256 = "0d84ed009b874c8487256d2d46ecf839ba4d1343e7f5cc7bc7202976a8f48664"
FIXTURE_LBA9_SHA256 = "bfa087d98347990404390c27e796b3afa54aa128a337bc9af5f56b8f4a162ef2"

EXPECTED_DEPT_LEN = 76
EXPECTED_FULL_JOIN_COUNTS = {59: 3, 60: 4}
EXPECTED_INDEPENDENT_JOIN_COUNTS = {59: 3, 60: 4}
EXPECTED_SHORT_COUNT = 12
EXPECTED_SHORT_BYTE63_VALUES = {0x00, 0x8B}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_hex_sector(path: Path, expected: str) -> bytes:
    raw = bytes.fromhex("".join(path.read_text(encoding="utf-8").split()))
    if len(raw) != SECTOR:
        raise SystemExit(f"{path}: expected 512 bytes, got {len(raw)}")
    if sha256(raw) != expected:
        raise SystemExit(f"{path}: SHA-256 mismatch")
    return raw


def lba6_decode(raw: bytes) -> bytes:
    if len(raw) != SECTOR:
        raise SystemExit(f"LBA6 expected 512 bytes, got {len(raw)}")
    out = bytearray(raw[:0x1FC])
    key = LBA6_K0
    for index in range(len(out) // 2):
        offset = index * 2
        word = int.from_bytes(out[offset : offset + 2], "little") ^ key
        out[offset : offset + 2] = word.to_bytes(2, "little")
        key = (key + 0x100 - index - 1) & 0xFFFF
    return bytes(out) + raw[0x1FC:]


def reconstruct(raw6: bytes, raw9: bytes) -> dict[str, object]:
    plain6 = lba6_decode(raw6)
    if int.from_bytes(plain6[:4], "little") != MARKER:
        raise SystemExit("missing long-Dept marker")
    inline = plain6[4:0x40]
    join = 59 if inline[59] == 0 else 60
    continuation = raw9[0x80:0x100]
    try:
        nul = continuation.index(0)
    except ValueError as exc:
        raise SystemExit("long-Dept continuation has no NUL") from exc
    rebuilt = inline[:join] + continuation[:nul]
    tail = continuation[nul + 1 :]
    if any(tail):
        raise SystemExit("observed continuation has non-zero post-NUL tail")
    return {
        "join": join,
        "inline_59": inline[59],
        "continuation_nul_index": nul,
        "dept": rebuilt,
        "post_nul_tail_bytes": len(tail),
        "post_nul_tail_nonzero": 0,
    }


def pe_reader(path: Path, expected: str):
    data = path.read_bytes()
    if sha256(data) != expected:
        raise SystemExit(f"{path}: SHA-256 mismatch")
    if data[:2] != b"MZ":
        raise SystemExit(f"{path}: not an MZ image")
    pe_off = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_off : pe_off + 4] != b"PE\0\0":
        raise SystemExit(f"{path}: not a PE image")
    file_hdr = pe_off + 4
    section_count = struct.unpack_from("<H", data, file_hdr + 2)[0]
    optional_size = struct.unpack_from("<H", data, file_hdr + 16)[0]
    optional = file_hdr + 20
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic == 0x10B:
        image_base = struct.unpack_from("<I", data, optional + 28)[0]
    elif magic == 0x20B:
        image_base = struct.unpack_from("<Q", data, optional + 24)[0]
    else:
        raise SystemExit(f"{path}: unexpected PE optional magic 0x{magic:X}")
    section_off = optional + optional_size
    sections: list[tuple[int, int, int, int]] = []
    for index in range(section_count):
        off = section_off + index * 40
        virtual_size, virtual_address, raw_size, raw_ptr = struct.unpack_from(
            "<IIII", data, off + 8
        )
        sections.append((virtual_address, max(virtual_size, raw_size), raw_ptr, raw_size))

    def read_va(va: int, size: int) -> bytes:
        rva = va - image_base
        for virtual_address, span, raw_ptr, raw_size in sections:
            if virtual_address <= rva < virtual_address + span:
                delta = rva - virtual_address
                if delta + size > raw_size:
                    raise SystemExit(f"{path}: VA 0x{va:X} crosses file-backed section")
                return data[raw_ptr + delta : raw_ptr + delta + size]
        raise SystemExit(f"{path}: VA 0x{va:X} not file-backed")

    return read_va


def require_bytes(read_va, va: int, expected_hex: str, label: str) -> None:
    expected = bytes.fromhex(expected_hex)
    actual = read_va(va, len(expected))
    if actual != expected:
        raise SystemExit(
            f"{label} drifted at 0x{va:X}: expected {expected.hex()}, got {actual.hex()}"
        )


def audit_binary_geometry(args: argparse.Namespace) -> dict[str, object]:
    current_reader = pe_reader(args.current_reader, CURRENT_READER_SHA256)
    current_writer = pe_reader(args.current_writer, CURRENT_WRITER_SHA256)
    x86 = pe_reader(args.fileop_x86, FILEOP_X86_SHA256)
    x64 = pe_reader(args.fileop_x64, FILEOP_X64_SHA256)

    # Current selector: marker, 60B prefix, Dept[59] branch, then +0x7B/+0x7C.
    require_bytes(current_reader, 0x101018D0, "81bd54ffffff2a5e2440", "current marker compare")
    require_bytes(current_reader, 0x10101910, "b90f000000", "current 60B dword copy")
    require_bytes(current_reader, 0x1010191D, "0fbe4d9385c9751b", "current Dept[59] selector")
    require_bytes(current_reader, 0x10101934, "83c77b", "current join59 source offset")
    require_bytes(current_reader, 0x1010194F, "83c77c", "current join60 source offset")

    # Current writer is join60 only: length gate >=0x40, 60B inline, Dept+0x7C -> LBA9+0x80.
    require_bytes(current_writer, 0x1001402D, "83bd28ffffff40", "current writer length gate")
    require_bytes(current_writer, 0x1001405A, "6a3c", "current writer 60B prefix")
    require_bytes(current_writer, 0x100140C4, "83c07c", "current writer Dept[60] source")
    require_bytes(current_writer, 0x100140CE, "81c180000000", "current writer LBA9 destination")

    # CEMS2.0 x86 fixed join59: 60B inline + 128B continuation at logical +0x3B.
    require_bytes(x86, 0x100232ED, "81bd1cedffff2a5e2440", "x86 marker compare")
    require_bytes(x86, 0x10023305, "6a3c", "x86 60B inline prefix")
    require_bytes(x86, 0x1002331D, "6880000000", "x86 128B continuation")
    require_bytes(x86, 0x1002332E, "8d8d4fecffff", "x86 continuation destination")
    if (-0x13B1) - (-0x13EC) != 0x3B:
        raise AssertionError("x86 join offset arithmetic changed")

    # x64 independently encodes the same geometry, then scans rebuilt output to NUL.
    require_bytes(x64, 0x18002724F, "81bc24c00000002a5e2440", "x64 marker compare")
    require_bytes(x64, 0x18002726C, "41b83c000000", "x64 60B inline prefix")
    require_bytes(x64, 0x180027277, "488d8c243b010000", "x64 continuation destination")
    require_bytes(x64, 0x180027287, "41b880000000", "x64 128B continuation")
    if 0x13B - 0x100 != 0x3B:
        raise AssertionError("x64 join offset arithmetic changed")
    require_bytes(x64, 0x1800272A0, "488dbc2400010000", "x64 rebuilt-string base")
    require_bytes(x64, 0x1800272A8, "f2ae48f7d1", "x64 NUL scan")
    require_bytes(x64, 0x1800272AD, "4c8d41ff", "x64 NUL length")

    return {
        "current_reader": "Dept[59]==0 -> join59; nonzero -> join60",
        "current_writer": "strlen>=64; 60B inline; continuation starts at Dept[60]",
        "fileop_x86": "60B inline + 128B continuation overlay at logical Dept[59]",
        "fileop_x64": "60B inline + 128B continuation overlay at logical Dept[59], then C-string NUL scan",
    }


def audit_physical_profiles() -> dict[str, object]:
    observations: list[dict[str, object]] = []
    counts = {59: 0, 60: 0}
    depts: list[bytes] = []

    for name, (expected_sha, expected_join) in FULL_IMAGES.items():
        path = Path(name)
        data = path.read_bytes()
        if sha256(data) != expected_sha:
            raise SystemExit(f"{path}: SHA-256 mismatch")
        if len(data) != 13 * SECTOR:
            raise SystemExit(f"{path}: expected 6656 bytes")
        item = reconstruct(
            data[6 * SECTOR : 7 * SECTOR],
            data[9 * SECTOR : 10 * SECTOR],
        )
        if item["join"] != expected_join:
            raise SystemExit(f"{path}: join profile changed")
        if len(item["dept"]) != EXPECTED_DEPT_LEN:
            raise SystemExit(f"{path}: unexpected Dept length")
        counts[expected_join] += 1
        depts.append(item["dept"])
        observations.append(
            {
                "source": name,
                "kind": "full-physical-image",
                "join": item["join"],
                "continuation_nul_index": item["continuation_nul_index"],
                "post_nul_tail_bytes": item["post_nul_tail_bytes"],
            }
        )

    if counts != EXPECTED_FULL_JOIN_COUNTS:
        raise SystemExit(f"full-image join counts changed: {counts}")

    fixture6 = read_hex_sector(FIXTURE_LBA6, FIXTURE_LBA6_SHA256)
    fixture9 = read_hex_sector(FIXTURE_LBA9, FIXTURE_LBA9_SHA256)
    fixture = reconstruct(fixture6, fixture9)
    if fixture["join"] != 59 or len(fixture["dept"]) != EXPECTED_DEPT_LEN:
        raise SystemExit("standalone join59 fixture changed")
    duplicate_matches = []
    for name, (expected_sha, expected_join) in FULL_IMAGES.items():
        if expected_join != 59:
            continue
        data = Path(name).read_bytes()
        if data[6 * SECTOR : 7 * SECTOR] == fixture6 and data[9 * SECTOR : 10 * SECTOR] == fixture9:
            duplicate_matches.append(name)
    if len(duplicate_matches) < 1:
        raise SystemExit("join59 fixture is no longer a duplicate of counted physical sectors")

    if counts != EXPECTED_INDEPENDENT_JOIN_COUNTS:
        raise SystemExit(f"independent full-image join counts changed: {counts}")
    if len(set(depts)) != 1:
        raise SystemExit("join59/join60 observations no longer reconstruct one Dept")
    dept = depts[0]

    # In join59, LBA6+0x03F is the NUL discriminator and LBA9 starts Dept[59].
    # In join60, LBA6+0x03F is Dept[59] and LBA9 starts Dept[60].
    if observations[0]["continuation_nul_index"] != 17:
        raise SystemExit("join59 continuation length changed")
    if any(
        obs["continuation_nul_index"] != (17 if obs["join"] == 59 else 16)
        for obs in observations
    ):
        raise SystemExit("profile-specific continuation NUL position changed")

    short_count = 0
    short_byte63_values: set[int] = set()
    for path in sorted(Path("audit/protocol/gold/strict-encrypted").glob("*.bin")):
        data = path.read_bytes()
        if len(data) != 13 * SECTOR:
            continue
        plain6 = lba6_decode(data[6 * SECTOR : 7 * SECTOR])
        if int.from_bytes(plain6[:4], "little") == MARKER:
            continue
        slot = plain6[:0x40]
        try:
            nul = slot.index(0)
        except ValueError as exc:
            raise SystemExit(f"{path}: short Dept slot has no NUL") from exc
        if nul >= 0x3F:
            raise SystemExit(f"{path}: byte63 is not post-NUL backing in short profile")
        short_count += 1
        short_byte63_values.add(slot[0x3F])

    if short_count != EXPECTED_SHORT_COUNT:
        raise SystemExit(f"short-Dept physical count changed: {short_count}")
    if short_byte63_values != EXPECTED_SHORT_BYTE63_VALUES:
        raise SystemExit(
            f"short-Dept byte63 backing profile changed: {sorted(short_byte63_values)}"
        )

    return {
        "independent_full_image_observations": len(observations),
        "independent_join_counts": {str(key): value for key, value in counts.items()},
        "regression_fixture": {
            "counted_as_independent": False,
            "duplicate_full_image_matches": duplicate_matches,
        },
        "reconstructed_dept_len": len(dept),
        "reconstructed_dept_sha256": sha256(dept),
        "post_nul_tail_observed_nonzero_bytes": 0,
        "post_nul_tail_semantics": "consumer-ignored backing after continuation NUL; current observations are zero",
        "short_profiles": {
            "count": short_count,
            "byte63_values": [f"0x{value:02X}" for value in sorted(short_byte63_values)],
            "meaning": "post-NUL backing, not a fixed zero byte",
        },
        "profiles": observations,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--current-reader",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsudisk.dll"),
    )
    parser.add_argument(
        "--current-writer",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll"),
    )
    parser.add_argument(
        "--fileop-x86",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/Edp/fileophook.dll"),
    )
    parser.add_argument(
        "--fileop-x64",
        type=Path,
        default=Path("/Users/zhangyuxi/Desktop/u_disk/VRV/cems/Edp/fileophook64.dll"),
    )
    args = parser.parse_args()

    result = {
        "binary_geometry": audit_binary_geometry(args),
        "physical_profiles": audit_physical_profiles(),
        "semantic_claim": (
            "for the observed long-Dept profiles, LBA6+0x03F is either the join59 "
            "NUL discriminator, Dept[59], or short-profile post-NUL backing; "
            "LBA9+0x080..0x0FF is a C-string continuation beginning at Dept[59] or "
            "Dept[60] followed by consumer-ignored post-NUL backing (observed zero); "
            "x86/x64 first-party readers independently encode "
            "the join59 mapping"
        ),
        "claim_boundary": (
            "this closes observed wire-to-Dept semantics and physical profile behavior; "
            "the exact historical executable/profile selector that emitted join59 is "
            "still unavailable and must remain explicit provenance, not be invented"
        ),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
