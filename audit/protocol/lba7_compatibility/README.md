# LBA7 legacy compatibility audit

This directory is the canonical machine-readable evidence set for the legacy EDP partition table at LBA7 and the fixed 0xC00-byte physical compatibility extent referenced by later old-table entries.

Canonical terminology:

- metadata object: `LBA7 legacy EDP partition table`
- first-party structure: `tagEdpPartionInfo` / `EDP_PARTION_INFO`
- logical types: `1=boot`, `2=share/exchange`, `4=encrypt/private`
- physical object: `LBA7 legacy compatibility extent`
- payload size: `0xC00` bytes (six 512-byte sectors)
- new-format counterpart: LBA12 `tagNewEdpPartionInfo`

The extent is not type4-specific. First-party `CreatePartitions` preserves the logical `PartionType` of later entries but rewrites their geometry to the same fixed compatibility extent. Consequently mode 0 `[1,2,4]` legitimately has type2 and type4 pointing to the same six-sector block, and mode 3 `[1,2]` can point to it with type2.

Files:

- `evidence/official_lba7_producer_20260923.json`: producer-derived mode matrix, type semantics, and entry geometry rules.
- `byte_ledger.tsv`: exact 3072-byte payload coverage.
- `profile_coverage.tsv`: verified physical-profile coverage.
- `evidence_manifest.tsv`: evidence identities, hashes and scope boundaries.
- `gold/`: committed physical ciphertext and recovered plaintext fixtures.
- `live_captures/`: bounded read-only physical captures.

IIR is a separate protocol object and is intentionally not modeled as this extent.
