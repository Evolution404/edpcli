#!/usr/bin/env python3
"""Calculate a reflected CRC-32 with zero initial state and no final XOR.

This variant uses polynomial 0xEDB88320 and differs from zlib.crc32.
Inputs are bytes; --text uses UTF-8 and --hex decodes hexadecimal bytes.
"""

from __future__ import annotations

import argparse
import sys


POLYNOMIAL = 0xEDB88320


def crc32_bare(data: bytes) -> int:
    """Return CRC-32 (init=0, no final XOR) as an unsigned 32-bit integer."""
    state = 0
    for byte in data:
        state ^= byte
        for _ in range(8):
            state = (state >> 1) ^ (POLYNOMIAL if state & 1 else 0)
    return state & 0xFFFFFFFF


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--text", help="UTF-8 source text")
    source.add_argument("--hex", dest="hex_bytes", help="hexadecimal input bytes")
    source.add_argument("--stdin", action="store_true", help="read exact bytes from standard input")
    args = parser.parse_args(argv)
    if args.text is not None:
        data = args.text.encode("utf-8")
    elif args.hex_bytes is not None:
        try:
            data = bytes.fromhex(args.hex_bytes)
        except ValueError as error:
            parser.error(f"invalid hexadecimal input: {error}")
    else:
        data = sys.stdin.buffer.read()
    print(f"0x{crc32_bare(data):08X}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
