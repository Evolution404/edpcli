#!/usr/bin/env python3
"""Query the LBA0-LBA12 byte ledger and optionally annotate a physical image."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

SECTOR = 512
IMAGE_LEN = 13 * SECTOR


@dataclass(frozen=True)
class Row:
    lba: int
    ranges: str
    status: str
    field: str
    profiles: str
    producer: str
    consumer: str
    physical: str
    notes: str


def parse_range_piece(piece: str) -> tuple[int, int]:
    if "-" in piece:
        left, right = piece.split("-", 1)
    else:
        left = right = piece
    return int(left, 16), int(right, 16)


def load_ledger(path: Path) -> dict[tuple[int, int], Row]:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    cells: dict[tuple[int, int], Row] = {}
    for line in lines[1:]:
        cols = line.split("\t")
        if len(cols) != 9:
            raise ValueError(f"bad ledger row: {line}")
        row = Row(
            lba=int(cols[0]),
            ranges=cols[1],
            status=cols[2],
            field=cols[3],
            profiles=cols[4],
            producer=cols[5],
            consumer=cols[6],
            physical=cols[7],
            notes=cols[8],
        )
        for piece in row.ranges.split(","):
            start, end = parse_range_piece(piece)
            for offset in range(start, end + 1):
                key = (row.lba, offset)
                if key in cells:
                    raise ValueError(f"overlap at LBA{row.lba}+0x{offset:03x}")
                cells[key] = row
    return cells


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", default="audit/protocol/byte_ledger.tsv", type=Path)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--lba", required=True, type=int, choices=range(13))
    parser.add_argument("--offset", type=lambda value: int(value, 0))
    args = parser.parse_args()

    cells = load_ledger(args.ledger)
    image = None
    if args.image:
        image = args.image.read_bytes()
        if len(image) < IMAGE_LEN:
            raise SystemExit(f"image must contain at least {IMAGE_LEN} bytes")
        image = image[:IMAGE_LEN]

    offsets = [args.offset] if args.offset is not None else range(SECTOR)
    print(
        "physical_offset\tlba_offset\traw\tdecoded_offset\tstatus\tfield_or_region"
        "\tprofiles\tproducer_evidence\tconsumer_evidence\tphysical_evidence"
    )
    for offset in offsets:
        if offset is None or not 0 <= offset < SECTOR:
            raise SystemExit("offset must be in 0x000..0x1ff")
        row = cells.get((args.lba, offset))
        if row is None:
            raise SystemExit(f"ledger gap at LBA{args.lba}+0x{offset:03x}")
        physical_offset = args.lba * SECTOR + offset
        raw = "--" if image is None else f"{image[physical_offset]:02x}"
        # Do not pretend that the physical byte is plaintext for encrypted
        # sectors.  Decoder-specific logical bytes are deliberately explicit.
        decoded_offset = f"0x{offset:03x} (decoder-specific)"
        print(
            f"0x{physical_offset:04x}\t0x{offset:03x}\t{raw}\t{decoded_offset}\t"
            f"{row.status}\t{row.field}\t{row.profiles}\t{row.producer}\t"
            f"{row.consumer}\t{row.physical}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

