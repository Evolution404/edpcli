# Region A audit

Canonical human documentation: `docs/REGION_A_REVERSE_ENGINEERING.md`.

Machine-readable sources:

- `evidence_manifest.tsv`: evidence identity, modality, hashes, proofs and limitations.
- `wire_byte_ledger.tsv`: exact 0xC00 physical Region A wire coverage.
- `plain_byte_ledger.tsv`: exact 0x800 conditional IIR plaintext coverage.
- `profile_coverage.tsv`: profile-level semantic/physical/decrypt status.
- `gold/`: checked-in physical ciphertext evidence.
- `evidence/`: bounded reproducible dynamic outputs; failed candidates remain negative evidence and must not be promoted.
