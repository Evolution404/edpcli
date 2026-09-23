#!/usr/bin/env python3
"""Offline audit of the current YDCC update/download wire envelope.

This script deliberately performs no network I/O.  It pins the local binaries,
database and captured download log, reconstructs the built-in RSA public key,
and checks the non-sensitive routing values needed to reproduce the current
update envelope.  It does not recover the deleted 2025 writer and therefore
does not promote any LBA byte.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sqlite3
import struct
import subprocess
from pathlib import Path


VCEMS_SHA256 = "cebd0b3b2bb0ec9f08fcc6fe97bb64d92713a0302ec517c3bb6a2a43c083626b"
CEMSBASE_SHA256 = "92e7fc28f8aaef9458ad584a7b7f4286b2aa61d46f8e41d1801c512cf17c8a84"
DB_SHA256 = "10704b28c0068692007a1238d5f14c75b26f11896f99c8a2533521f295506c88"
DOWNLOAD_LOG_SHA256 = "65357617daeb30a440d4982b7583883a7ab9f877bbdbce280917cdf2ed03a597"
PUBLIC_PEM_SHA256 = "44f37e827a4dd639e11b7fc863d8c648779b60944781203e8372a29971690049"

PUBLIC_BLOB_VA = 0x100635A0
PUBLIC_BLOB_LEN = 426
SEED_VA = 0x10056CC0
SEED = b"70C2FA451FDB4E1BA67C1801DF594E25"
DERIVED_KEY_TEXT = b"51FDB"
AES_IV = bytes.fromhex("7dac3106af9f69ad3c47e9949dd46b0d")

CURRENT_BASE = "8.1.2604.0917"
CURRENT_ZIP = f"{CURRENT_BASE}/ydcc/cemsusbregsiter.dll.zip"
CURRENT_ZIP_CRC = "F9FE2852"
CURRENT_ZIP_SIZE = "533081"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_rva_to_offset(image: bytes, rva: int) -> int:
    if image[:2] != b"MZ":
        raise SystemExit("not an MZ image")
    pe_off = struct.unpack_from("<I", image, 0x3C)[0]
    if image[pe_off : pe_off + 4] != b"PE\0\0":
        raise SystemExit("not a PE image")
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
    raise SystemExit(f"RVA 0x{rva:X} not mapped by any PE section")


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


def read_va(image: bytes, va: int, size: int) -> bytes:
    base = pe_image_base(image)
    off = pe_rva_to_offset(image, va - base)
    return image[off : off + size]


def openssl_aes_cfb_decrypt(ciphertext: bytes, key: bytes, iv: bytes) -> bytes:
    result = subprocess.run(
        [
            "openssl",
            "enc",
            "-aes-128-cfb",
            "-d",
            "-nosalt",
            "-K",
            key.hex(),
            "-iv",
            iv.hex(),
        ],
        input=ciphertext,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            "openssl AES-CFB decrypt failed: "
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout


def der_read_length(buf: bytes, offset: int) -> tuple[int, int]:
    first = buf[offset]
    offset += 1
    if first < 0x80:
        return first, offset
    width = first & 0x7F
    if width == 0 or width > 4:
        raise SystemExit("unsupported DER length")
    return int.from_bytes(buf[offset : offset + width], "big"), offset + width


def parse_pkcs1_public_pem(pem: bytes) -> tuple[int, int]:
    lines = [
        line.strip()
        for line in pem.splitlines()
        if line.strip() and not line.startswith(b"-----")
    ]
    der = base64.b64decode(b"".join(lines), validate=True)
    pos = 0
    if der[pos] != 0x30:
        raise SystemExit("RSA public key is not a DER SEQUENCE")
    pos += 1
    seq_len, pos = der_read_length(der, pos)
    end = pos + seq_len
    values: list[int] = []
    while pos < end:
        if der[pos] != 0x02:
            raise SystemExit("RSA public key member is not INTEGER")
        pos += 1
        length, pos = der_read_length(der, pos)
        raw = der[pos : pos + length]
        pos += length
        values.append(int.from_bytes(raw, "big"))
    if pos != end or len(values) != 2:
        raise SystemExit("unexpected PKCS#1 RSA public-key layout")
    return values[0], values[1]


def require_bytes(blob: bytes, needle: bytes, label: str) -> int:
    offset = blob.find(needle)
    if offset < 0:
        raise SystemExit(f"missing {label}")
    return offset


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--vcems",
        type=Path,
        required=True,
    )
    parser.add_argument(
        "--cems-base",
        type=Path,
        required=True,
    )
    parser.add_argument(
        "--db",
        type=Path,
        required=True,
    )
    parser.add_argument(
        "--download-log",
        type=Path,
        required=True,
    )
    args = parser.parse_args()

    expected = {
        args.vcems: VCEMS_SHA256,
        args.cems_base: CEMSBASE_SHA256,
        args.db: DB_SHA256,
        args.download_log: DOWNLOAD_LOG_SHA256,
    }
    for path, digest in expected.items():
        actual = sha256(path)
        if actual != digest:
            raise SystemExit(f"SHA-256 mismatch for {path}: {actual}")

    vcems = args.vcems.read_bytes()
    machine_anchors = {
        "nethead_magic_store": bytes.fromhex("c7065f656470"),
        "nethead_sentinel_store": bytes.fromhex("c7460410101010"),
        "sequence_low10_mask": bytes.fromhex("81e2ff030000"),
        "netflag_prefix": b"netflag: ",
        "empty_content_type_header": b"Content-Type: ",
        "server_id_key_utf16": "ServerId".encode("utf-16le") + b"\0\0",
        "org_code_key_utf16": "OrgCode".encode("utf-16le") + b"\0\0",
        "update_rsa_log": b"session.szType = rsa",
        "buffer_crc_ok_log": b"BufferCrc  ok",
        "area_id_ok_log": b"copy  szAreaId  ok",
    }
    anchor_offsets = {
        name: require_bytes(vcems, marker, name)
        for name, marker in machine_anchors.items()
    }

    cems_base = args.cems_base.read_bytes()
    if read_va(cems_base, SEED_VA, len(SEED)) != SEED:
        raise SystemExit("cemsBase built-in RSA seed drifted")
    encrypted_public = read_va(cems_base, PUBLIC_BLOB_VA, PUBLIC_BLOB_LEN)
    aes_key = hashlib.md5(DERIVED_KEY_TEXT).digest()
    public_pem = openssl_aes_cfb_decrypt(encrypted_public, aes_key, AES_IV)
    if hashlib.sha256(public_pem).hexdigest() != PUBLIC_PEM_SHA256:
        raise SystemExit("recovered RSA public PEM SHA-256 mismatch")
    if not (
        public_pem.startswith(b"-----BEGIN RSA PUBLIC KEY-----\n")
        and public_pem.rstrip().endswith(b"-----END RSA PUBLIC KEY-----")
    ):
        raise SystemExit("recovered built-in public key is not PKCS#1 PEM")
    modulus, exponent = parse_pkcs1_public_pem(public_pem)
    if modulus.bit_length() != 2048 or exponent != 65537:
        raise SystemExit(
            f"unexpected RSA parameters: bits={modulus.bit_length()} e={exponent}"
        )

    con = sqlite3.connect(args.db)
    try:
        tcp = con.execute(
            """
            select ServerId, Type, Protocol, Project, MainDir
            from CEMS_SERVERLIST
            where Type='tcp' and Project='CEMS-C-TCP'
            order by Prefer desc, Id
            limit 1
            """
        ).fetchone()
        org_row = con.execute(
            "select OrgCode from CEMS_DEVICE where OrgCode <> '' limit 1"
        ).fetchone()
    finally:
        con.close()
    if tcp != (0, "tcp", "http", "CEMS-C-TCP", "TCPServlet"):
        raise SystemExit(f"unexpected TCP server row: {tcp!r}")
    if org_row != ("serverAreaMain",):
        raise SystemExit(f"unexpected non-sensitive OrgCode: {org_row!r}")
    netflag = f"netflag: {tcp[0]}"
    if netflag != "netflag: 0":
        raise SystemExit(f"unexpected netflag: {netflag}")

    log = args.download_log.read_bytes()
    log_markers = [
        CURRENT_ZIP.encode(),
        f'"fileCrc":"{CURRENT_ZIP_CRC}"'.encode(),
        f'"fileSize":"{CURRENT_ZIP_SIZE}"'.encode(),
        b"session.szType = rsa",
        b"BufferCrc  ok",
        b"copy  szAreaId  ok",
        b"CreateNetHeadW end",
        b"CMCurl::PostRequest CurlGetServerId ok",
    ]
    for marker in log_markers:
        require_bytes(log, marker, f"download-log marker {marker!r}")

    print(
        json.dumps(
            {
                "inputs": {
                    "vcems_download_sha256": VCEMS_SHA256,
                    "cems_base_sha256": CEMSBASE_SHA256,
                    "db_sha256": DB_SHA256,
                    "download_log_sha256": DOWNLOAD_LOG_SHA256,
                },
                "vcems_static_anchors": anchor_offsets,
                "built_in_rsa": {
                    "public_pem_sha256": PUBLIC_PEM_SHA256,
                    "modulus_bits": modulus.bit_length(),
                    "exponent": exponent,
                    "rsa_plaintext_chunk_bytes": 0xD6,
                    "rsa_ciphertext_chunk_bytes": 0x100,
                    "padding": "PKCS#1 v1.5",
                    "public_blob_cipher": "AES-128-CFB",
                    "public_blob_key_derivation": 'MD5("51FDB")',
                    "public_blob_iv_hex": AES_IV.hex(),
                },
                "wire": {
                    "fixed_header_bytes": 0x54,
                    "magic": "_edp",
                    "sentinel": "0x10101010",
                    "sequence_initial": 0,
                    "sequence_mask": "0x03ff",
                    "max_code": "00FF0900",
                    "min_code": "00FF0002",
                    "area_id": org_row[0],
                    "rsa_payload_encoding": "standard base64",
                    "crc": "standard reflected CRC-32 over raw RSA ciphertext",
                    "http_custom_header": netflag,
                    "http_content_type_header": "Content-Type: ",
                },
                "current_distribution_fixture": {
                    "file_path": CURRENT_ZIP,
                    "zip_crc": CURRENT_ZIP_CRC,
                    "zip_size": int(CURRENT_ZIP_SIZE),
                },
                "claim": (
                    "the current YDCC update transport is closed offline through the "
                    "local binary/database/log boundary: the update path uses a raw "
                    "0x54-byte CEMS_NET_HEAD plus base64-encoded RSA ciphertext, "
                    "protocol-fixed RSA rather than a login-session key, the device "
                    "OrgCode as AreaId, and decimal TCP ServerId in the netflag header"
                ),
                "claim_boundary": (
                    "offline transport/provenance evidence only; no remote request is "
                    "performed, the deleted 2025 writer bytes remain unrecovered, and "
                    "no LBA byte is promoted by this audit"
                ),
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
