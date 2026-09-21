# Protocol audit reproducibility baseline

This directory is the machine-readable companion to
`docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`.  It does not replace the canonical
document; it makes the evidence behind that document reproducible and rejects
silent byte-accounting drift.

## Evidence classes

- `physical`: bytes captured from an allowed real-device gold source.
- `virtual`: output or behavior produced by executing an official binary in an
  isolated file/memory-backed harness.  Virtual evidence is never relabeled as
  a physical capture.
- `static`: PE/ELF/DWARF/disassembly evidence such as an exact write, read,
  branch, function address, or ABI field boundary.

`evidence_manifest.tsv` records the concrete artifacts and anchors.  The
`gold_samples.tsv` file freezes the current two-source gold population by name
and SHA-256.  Host-local binaries are identified by digest rather than copied
into this repository.

The baseline harness is intentionally fail-closed on population drift: a new
non-`_nopwd_` backup is not silently included or ignored.  It must first be
reviewed and explicitly added to `gold_samples.tsv` with its digest.

## Gold sources

The only protocol gold sources are:

1. `~/.edpcli-backup`: exactly 21 non-`_nopwd_` 6656-byte original-generation
   captures in this baseline.  The two `_nopwd_` files are excluded.
2. `~/Desktop/u_disk/analyze/disk_data/no_password_disk4`: the authentic
   SanDisk no-password capture.  Only LBA0-LBA12 (the first 6656 bytes of
   `raw/LBA0_13_concat.bin`) are in protocol scope.

The old `/private/tmp/audit22` harness mixed a third SanDisk EESI capture into
its population.  Its source/executable hashes are retained in the manifest for
provenance, but its population definition is obsolete.  The checked-in
`scripts/protocol/audit_baseline.py` reproduces the useful census behavior
against the current two-source policy instead of depending on `/private/tmp`.

## Byte ledger

`byte_ledger.tsv` partitions every byte in LBA0-LBA12 exactly once.  Each row
records status, known profile applicability, and separate producer, consumer,
and physical evidence IDs.  `tests/protocol_byte_ledger.rs` expands all range
expressions and rejects overlap, gaps, bad evidence references, and divergence
from the canonical strict-progress table.

For a human-readable byte dump, run:

```text
python3 scripts/protocol/query_byte_ledger.py --lba 4
python3 scripts/protocol/query_byte_ledger.py --lba 3 --offset 0x20
python3 scripts/protocol/query_byte_ledger.py --image <6656-byte-image> --lba 10
```

The image form prints physical offsets and bytes next to the ledger status,
field/region, profiles, and evidence IDs.  Decryption-specific offsets remain
in the canonical field descriptions until the relevant decoder is explicitly
registered in the query tool; the query tool must not invent a decrypted view.

## Re-run baseline checks

```text
python3 scripts/protocol/audit_baseline.py
cargo test --test protocol_byte_ledger
cargo test --test protocol_documentation_contract
```

The baseline audit is read-only.  It never opens a raw disk device and never
writes any gold capture.

## Local executable integrity

`audit/protocol/labeltool_variant_diff.md` records the byte-level comparison
between the three local `cemssafeudisklabeltool*.exe` copies. Only
`cemssafeudisklabeltool_orig.exe` is treated as an official front-end
baseline; the other two contain locally applied policy/validation bypasses and
must not be used as producer evidence.

