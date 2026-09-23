# Deep filesystem analysis v1

Deep is a read-only superset of Metadata. It does not read ordinary file payload
clusters. Raw metadata evidence remains `evidence_only`; summary and file-list
artifacts are `derived_only`. No decoded artifact is produced until a verified
partition decryption implementation exists. Protocol-sector decoding is not a
partition filesystem decryption implementation.

## Analysis contract

`edpcli.deep.filesystem.v1` reports `parsed`, `locked`, `unsupported`,
`parse_failed`, or `not_captured`. Only `parsed` has an inventory and space/count
statistics; other states use JSON null, never an invented empty volume. The
partition geometry's `partition_bytes` is separate from filesystem `total_bytes`.
A metadata prefix by itself never proves a complete inventory.

FAT16 and FAT32 are supported using BPB cluster counts, allocation tables and a
bounded directory walk. Paths begin with `/`; directories end with `/`. Root is
listed, but excluded from directory_count. File count counts paths, not deleted
entries or volume labels. FAT free space counts zero FAT entries; used bytes
include allocated clusters, reserved structures and any trailing partial cluster.
Allocated file size comes from its entire FAT chain. Times are filesystem-local
ISO 8601 wall times without invented UTC offsets; ctime means creation time.
Invalid/unavailable timestamps are null. Long names require valid UTF-16,
sequence and short-name checksum. Non-ASCII short names currently fail closed
because an OEM code page cannot be inferred safely.

The parser rejects cycles, cross-links, invalid names, invalid sizes, inconsistent
mirrored FATs, short sectors and reads outside the partition. Work limits are
4,194,304 data clusters, 100,000 entries, 4096-byte paths, 32 MiB of directory
metadata, and 64 MiB of raw read evidence per partition. Exceeding a limit is a
parse failure, not a complete partial listing. Ordinary file data is never read.

The initial implementation reports exFAT and NTFS as unsupported. Their bitmap,
directory/index and allocation parsers remain a separate next stage; a boot
signature alone must not produce volume statistics. Encrypted type2/type4
partitions report locked even if ciphertext happens to contain a filesystem
signature. `PartitionReader` exposes only relative sector reads; the future
DecryptedPartitionReader must validate keys and preserve decoded evidence
separately, including derivation back to raw evidence.

## Sources

FAT layout, cluster classification, directory entries and long filename rules:
[Microsoft FAT specification v1.03](https://www.cs.fsu.edu/~cop4610t/assignments/project3/spec/fatspec.pdf).

Existing EDP partition/key evidence is in
[EDP protocol reverse engineering](EDP_PROTOCOL_REVERSE_ENGINEERING.md), sections
6.1 onward. No password guessing or fabricated key material is used.

## Capture and replay

`edpcli backup create --disk N --deep` uses the same O_RDONLY device-opening
path as Metadata. Without `--deep`, manual and automatic backups remain Metadata.
Deep first acquires Metadata (including the LBA7 compatibility extent), then stores additional raw
read ranges and `derived.partition.<index>.filesystem_summary` plus
`derived.partition.<index>.file_list` in the same EDPB. Numeric indices are used
to avoid collisions if the partition table has more than one entry of a type.
Each JSON also identifies partition_type. The file list contains null entries
when analysis did not complete. No sidecar is created. Parser failure does not
abort preservation of successful raw reads or pre-existing Metadata artifacts.

Raw reads are cached and grouped into contiguous evidence extents. The captured
Metadata bytes take precedence over later reads of those same sectors. This is
not a filesystem freeze: a concurrently changing source may produce a failure or
an inconsistent snapshot, so it must not be described as an atomic filesystem
snapshot. Deep never unmounts or changes the source volume state.

For read-only assessment of an existing Metadata EDPB:

```sh
cargo run --example deep_assess_metadata -- /path/to/backup.edpb
```

This validates the source container and reports prefix assessment only; it does
not relabel the source as Deep or fabricate sectors missing from Metadata.

The 2026-09-22 Lexar LBA7 compatibility-extent capture (container SHA-256
`0a52a9cb52e47ca5e11d3a74d8c6f9f07937b0dc3d06f4d0802f3fb5917e254f`)
was replayed offline with this tool. Its type2 partition is 118477684736 bytes and
type4 is 6234963968 bytes. Both are `locked`; filesystem type, counts, listing
and usage statistics remain null. No source device was opened for this replay.
