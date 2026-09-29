# Filesystem Domain Refactor Plan — 2026-09-29

Status: **APPROVED FOR EXECUTION**

Baseline: branch `feat/device-workbench-20260928`, HEAD `9d5ed1e9d9249e0e183617a17984ed5943041cc0`.

## 1. Objective

Create one independent filesystem domain that owns filesystem detection, bounded metadata extraction, formatting, format verification and optional read-only analysis. Backup, Inspect, Provision, Restore and TUI consume stable filesystem interfaces and must not contain filesystem on-disk knowledge.

Completion criterion: adding another filesystem normally requires only `src/filesystem/<name>.rs`, one registry entry and driver-specific tests; backup/inspect/restore/provision/TUI must not gain new filesystem-name branches.

## 2. Safety and non-goals

- Do not weaken media-identity verification, geometry checks, unmount/lock/reopen, `WriteTransactionPlan`, sync, readback or rollback.
- Drivers never receive `diskN`, VID/PID, serial, onlyid, device_id or physical-device authorization state.
- Drivers never open `/dev/diskN` and never authorize a write.
- Metadata backup still does not store user files, directory listings, FAT/bitmap/MFT, free space or deleted remnants.
- Plain backup may transiently perform bounded read-only filesystem probing solely to project `filesystem_hint` / `volume_label_hint`; probed sectors never become restorable EDPB artifacts.
- EDP backup continues to obtain share/encrypt labels from verified protocol metadata such as LBA10 and does not inspect EDP data partitions merely for labels.
- EDP encryption is a partition transform, not a filesystem implementation.
- Do not introduce runtime/dynamic plugin loading.

## 3. Current technical debt

Filesystem responsibilities are split across:

- `src/provision/filesystem.rs`: FAT16/exFAT construction, label validation, sparse images and SM4-coupled transform helper.
- `src/filesystem_analysis.rs` plus FAT/exFAT children: filesystem analysis.
- `src/inspect_target.rs`: `FilesystemBootKind` and FAT12/FAT16/FAT32/exFAT/NTFS detection.
- `src/backup_metadata.rs`: FAT16 label decoding and exFAT root/FAT-chain traversal for `0x83 Volume Label`.
- `src/application/post_restore/format_operation.rs`: concrete filesystem-format dependencies and readback logic.

This duplicates format/detect/metadata rules and makes every new filesystem cross-cut multiple application domains.

## 4. Target source layout

```text
src/filesystem/
├── mod.rs
├── kind.rs
├── error.rs
├── io.rs
├── metadata.rs
├── format.rs
├── driver.rs
├── registry.rs
├── fat12.rs
├── fat16.rs
├── fat32.rs
├── exfat.rs
├── ntfs.rs
└── analysis/
    ├── mod.rs
    ├── fat.rs
    └── exfat.rs
```

Application flow:

```text
backup / inspect / provision / restore
                |
                v
       filesystem registry
                |
        filesystem driver
                |
      metadata / FormatPlan
                |
                v
        PartitionTransform
         plain / EDP-SM4
                |
                v
       WriteTransactionPlan
                |
                v
 identity -> lock -> reopen -> write -> sync -> readback -> rollback
```

## 5. Canonical filesystem model

### 5.1 FilesystemKind

Create one canonical enum:

```rust
pub enum FilesystemKind {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ntfs,
}
```

It owns canonical config/display tokens. `inspect_target::FilesystemBootKind` and `provision::OfficialFilesystemFormat` are migration-only types and must be removed by the final phase.

### 5.2 Capabilities

Detection support and formatting support are independent:

```rust
pub struct FilesystemCapabilities {
    pub detect: bool,
    pub read_metadata: bool,
    pub format: bool,
    pub verify_format: bool,
    pub analyze: bool,
}
```

Initial target capabilities:

| kind | detect | metadata | format | verify | analyze |
| --- | --- | --- | --- | --- | --- |
| FAT12 | yes | minimal | no | no | optional |
| FAT16 | yes | yes | yes | yes | yes |
| FAT32 | yes | minimal | no | no | yes |
| exFAT | yes | yes | yes | yes | yes |
| NTFS | yes | minimal | no | no | optional |

### 5.3 Partition-relative IO

Drivers receive only partition-relative sectors:

```rust
pub trait FilesystemReader {
    fn sector_size(&self) -> u32;
    fn sector_count(&self) -> u64;
    fn read_sector(&mut self, relative_lba: u64)
        -> Result<[u8; 512], FilesystemError>;
}
```

`PartitionView` in the outer disk/application layer translates relative LBA to physical `partition_start + relative_lba`.

### 5.4 Metadata projection

```rust
pub struct FilesystemMetadata {
    pub kind: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}
```

`None` means no reliable user volume label. Application code must not invent a placeholder name.

### 5.5 FormatRequest

```rust
pub struct FormatRequest {
    pub filesystem: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}
```

`None` is the domain meaning of no user label. FAT16 translates it internally to BPB `NO NAME    ` with no root label entry; exFAT omits the `0x83` label entry.

### 5.6 FormatPlan

Filesystem code must not write disks directly:

```rust
pub struct FormatPlan {
    pub filesystem: FilesystemKind,
    pub geometry: FilesystemGeometry,
    pub writes: Vec<FilesystemWrite>,
    pub expected_metadata: FilesystemMetadata,
}

pub struct FilesystemWrite {
    pub relative_lba: u64,
    pub data: [u8; 512],
}
```

Reuse/adapt the existing sparse image implementation; do not rewrite working format-generation logic without need.

### 5.7 Typed errors

```rust
pub enum FilesystemErrorKind {
    Unsupported,
    InvalidGeometry,
    InvalidBootSector,
    InvalidMetadata,
    InvalidVolumeLabel,
    ReadFailure,
    FormatUnsupported,
    CorruptFilesystem,
    ScanBudgetExceeded,
}
```

TUI/application must not infer filesystem state from error-string text.

## 6. FilesystemDriver

One driver owns normal filesystem-specific behavior:

```rust
pub trait FilesystemDriver: Send + Sync {
    fn kind(&self) -> FilesystemKind;
    fn capabilities(&self) -> FilesystemCapabilities;
    fn detect(&self, source: &mut dyn FilesystemReader)
        -> Result<DetectionResult, FilesystemError>;
    fn read_metadata(&self, source: &mut dyn FilesystemReader)
        -> Result<FilesystemMetadata, FilesystemError>;
    fn validate_format_request(&self, request: &FormatRequest)
        -> Result<(), FilesystemError>;
    fn build_format_plan(&self, geometry: FilesystemGeometry, request: &FormatRequest)
        -> Result<FormatPlan, FilesystemError>;
    fn verify_format(&self, source: &mut dyn FilesystemReader, expected: &FilesystemMetadata)
        -> Result<FormatVerification, FilesystemError>;
}
```

Unsupported capabilities return typed unsupported errors; no dummy formatting implementation is required.

## 7. Detection registry

Use a static compile-time registry. Detection returns confidence, not a bool:

```rust
pub enum DetectionConfidence { NoMatch, Weak, Strong, Exact }
pub struct DetectionResult {
    pub kind: FilesystemKind,
    pub confidence: DetectionConfidence,
}
```

The registry selects the strongest result. Equal nontrivial confidence from multiple drivers is an ambiguity and fails closed instead of depending on registration order.

## 8. Encryption boundary

Final architecture:

```text
FilesystemDriver::build_format_plan()
             |
             v
       plaintext FormatPlan
             |
             v
       PartitionTransform
       - IdentityTransform
       - EdpSm4Transform
             |
             v
       WriteTransactionPlan
```

Filesystem modules must not import FileKey wrap/unwrap, password policy, LBA7/LBA12, onlyid/device_id or EDP key-domain semantics. `encrypt_sparse_mode2` and equivalent responsibilities move to a neutral partition-transform layer.

## 9. Analysis boundary

Current `filesystem_analysis` moves under `filesystem/analysis/` as an optional extension. Core support is detect/metadata/format/verify; directory traversal, used/free accounting and file payload streaming are optional and must not be required to register a filesystem.

## 10. Business-domain behavior after migration

### Backup

Plain backup becomes `PartitionView -> registry.detect -> driver.read_metadata -> ManifestPartition hints`. `backup_metadata.rs` must contain no FAT/exFAT offsets, GBK decoding, root-cluster traversal or `0x83` constants.

EDP backup remains protocol-first: LBA0-12, validated LBA7 compatibility extent, confirmed tail structures, LBA12 geometry/key-domain metadata and LBA10 share/encrypt labels. It does not inspect EDP data-partition filesystems for labels.

### Inspect

`inspect_target` delegates filesystem detection to the registry and optional deeper analysis to the selected driver/analyzer. It contains no FAT/exFAT/NTFS signature parser.

### Provision

Provisioning chooses `FilesystemKind`, builds a generic `FormatRequest`, receives a `FormatPlan`, applies any partition transform and then uses the existing physical write-safety transaction. It must not directly call `build_empty_fat16` / `build_empty_exfat`.

### Restore

`PartitionFormatRequest` uses `FilesystemKind`. Volume label becomes `Option<String>`: reliable backup hint -> `Some`, user clears -> `None`, old/unknown backup -> `None`. Independent restore/format/reinitialize confirmations remain unchanged.

## 11. Mode-aware EDP manifest roles

Correct the current PartionType-only role projection during this refactor:

- mode0: `boot`, `share`, `encrypt`;
- mode1: `boot_share_combined`, `encrypt`;
- mode2: `compatibility_reserve`, `encrypt`;
- mode3: `boot`, `share`.

Raw protocol bytes remain the restore truth source; this change improves typed manifest semantics only.

## 12. Legacy cleanup targets

By completion remove or eliminate direct use of:

- `inspect_target::FilesystemBootKind`;
- `inspect_target::detect_plain_filesystem`;
- `provision::OfficialFilesystemFormat`;
- `backup_metadata::decode_fat16_boot_label`;
- `backup_metadata::read_exfat_volume_label`;
- filesystem-specific format branches outside `src/filesystem`;
- filesystem-specific label validation outside `src/filesystem`;
- filesystem-specific format readback verification outside `src/filesystem`;
- top-level `src/filesystem_analysis.rs` and old child paths after migration;
- EDP encryption helpers housed inside filesystem formatting.

Audit `PARTITION_PREFIX_SECTORS` / `PARTITION_SUFFIX_SECTORS` and similar historical filesystem-metadata extent assumptions; delete them if no current protocol/migration consumer exists.

## 13. Execution phases

### F0 — Freeze behavior with tests

Status: `PENDING`

Lock FAT12/16/32/exFAT/NTFS detection; FAT16 label/no-label/format/readback; exFAT `0x83` label, bounded root-chain traversal, no-label/format/readback; Plain hint-only backup; EDP LBA10 labels without filesystem reads; encrypted-format byte behavior; restore original/override/None label behavior; existing safety gates.

Exit: focused tests pass before production extraction.

### F1 — Establish filesystem domain skeleton

Status: `PENDING`

Create `mod.rs`, `kind.rs`, `error.rs`, `io.rs`, `metadata.rs`, `format.rs`, `driver.rs`, `registry.rs`. Add adapters/conversions around existing code with no behavior change.

Exit: all targets compile and new-domain tests pass.

### F2 — Migrate FAT16

Status: `PENDING`

Move FAT16 detection, BPB validation, label codec, no-label semantics, empty filesystem construction, format validation and readback verification into `filesystem/fat16.rs`. Route backup/inspect/provision/restore through the driver, then delete duplicate FAT16 knowledge.

### F3 — Migrate exFAT

Status: `PENDING`

Move exFAT detection, geometry parsing, bounded root/FAT-chain `0x83` lookup, no-label semantics, empty filesystem construction, format validation and verification into `filesystem/exfat.rs`. Delete duplicate exFAT knowledge.

### F4 — Normalize detect-only drivers and business call sites

Status: `PENDING`

Create FAT12/FAT32/NTFS drivers with current supported capabilities. Route backup, inspect, provision, post-restore and TUI request types through `FilesystemKind` / registry. Correct mode-aware EDP manifest roles. Remove legacy enums or temporary aliases.

### F5 — Extract partition transforms

Status: `PENDING`

Move SM4 formatting transform outside the filesystem domain. The same plaintext filesystem driver must serve Plain and encrypted EDP partitions. Preserve current encrypted format/reinitialize safety semantics.

### F6 — Move analysis and remove technical debt

Status: `PENDING`

Move `filesystem_analysis` under `filesystem/analysis`, update formal migration consumers, delete superseded modules/adapters/helpers and run grep gates proving concrete filesystem knowledge no longer leaks into business domains.

## 14. Required completion grep gates

```text
rg "detect_plain_filesystem" src
rg "FilesystemBootKind" src
rg "OfficialFilesystemFormat" src
rg "decode_fat16_boot_label|read_exfat_volume_label" src
rg "build_empty_fat16|build_empty_exfat" src --glob '!filesystem/**'
rg "0x83" src --glob '!filesystem/**'
```

Results must be empty or restricted to explicitly documented migration compatibility/test declarations. Concrete filesystem names must not drive backup/inspect/restore/provision business logic.

## 15. Validation gates

Per phase: `cargo fmt --all`, `git diff --check`, focused tests, `cargo check --all-targets` for public/module changes and `./scripts/test-fast.sh` before phase commit.

At broad milestones/final completion: `python3 scripts/test-full.py --profile full`. Run Virtual Disk HIL when filesystem bytes or write planning changes. Do not automatically perform destructive real-USB testing.

## 16. Commit sequence

1. `docs(architecture): plan filesystem domain refactor`
2. `test(filesystem): freeze filesystem behavior`
3. `refactor(filesystem): add domain interfaces`
4. `refactor(filesystem): migrate fat16 driver`
5. `refactor(filesystem): migrate exfat driver`
6. `refactor(filesystem): route application through registry`
7. `refactor(filesystem): extract partition transforms`
8. `refactor(filesystem): move analysis and remove legacy APIs`

Keep phases reviewable; do not hide behavior changes by silently collapsing the migration into one commit.

## 17. Final acceptance criteria

1. One canonical `FilesystemKind` exists.
2. FAT16/exFAT detect, metadata, format and verify logic lives only in filesystem drivers.
3. FAT12/FAT32/NTFS detection lives only in filesystem drivers.
4. `backup_metadata` contains no filesystem on-disk offsets/signatures.
5. `inspect_target` contains no FAT/exFAT/NTFS parser.
6. provision/restore request formatting generically.
7. encryption is a post-filesystem partition transform.
8. filesystem drivers contain no EDP protocol/key-domain knowledge.
9. drivers never receive `diskN` or physical identity.
10. metadata backup stores no filesystem/user-data artifacts.
11. old backups without labels remain `None`; no generated placeholder label.
12. EDP LBA10 labels work without reading EDP data partitions.
13. mode1/mode2 manifest roles are semantically correct.
14. fast/full gates pass.
15. adding a new filesystem normally requires only its driver, registry entry and tests.
