# LCE（LBA7 Compatibility Extent）

## 1. Status

LCE 是项目对 LBA7 legacy compatibility extent 的统一简称。本文记录 LCE 的 producer、physical payload、crypto、consumer 和写入边界。

The former project name **Region A** is retired. The later name **legacy type4 extent** is also retired: first-party producer code proves that the fixed 0xC00-byte physical object is not type4-specific.

Canonical project terminology:

- metadata table: **LBA7 legacy EDP partition table**
- first-party structure names: `tagEdpPartionInfo` / `EDP_PARTION_INFO`
- physical payload: **LCE (LBA7 Compatibility Extent)**
- physical payload size: `0xC00` bytes = 3072 bytes = 6 sectors at 512 B/sector
- current-format counterpart: LBA12 `tagNewEdpPartionInfo`
- IIR: a separate protocol object; it is not this extent

The producer conclusions below are derived from the captured first-party Windows labeling stack, not inferred from disk samples.

### 1.1 Closure matrix

| LCE question | Status | Verified boundary |
| --- | --- | --- |
| Physical locator / size | **COMPLETE** | official `CreatePartitions` CHS-derived locator, aligned `0xC00`; LBA7 pointer is authoritative |
| LBA7 producer / pointer generation | **COMPLETE** | official mode selector -> `PartionType[]` -> entry0/later-entry geometry -> LBA7 serializer |
| Payload plaintext | **COMPLETE** | fixed six-sector FAT16 compatibility image, 3072B |
| Encryption / decryption | **COMPLETE** | EDPSECDISK zero8 family + physical backing byte-offset tweak; Lexar/SanDisk bit-exact reconstruction |
| Legacy consumer / mount | **COMPLETE** | old LBA7 fallback converts entry to runtime geometry and reaches `EdpMountFile` |
| Driver physical read/write mapping | **COMPLETE** | mounted virtual I/O maps through `backing_offset + virtual_offset`; complete 0xC00 extent is addressable |
| Current normal LBA12 path | **SEPARATE** | current normal type4 mount is not LCE and must not be used as LCE evidence |
| IIR relationship | **SEPARATE** | IIR is a different protocol object |
| Upper-layer business trigger: who deliberately rewrites LCE, when, and why | **OPEN** | low-level writable path is proven, but the exact business workflow/event that decides to modify the payload has not been closed end-to-end |
| All historical producer-version equivalence | **OPEN** | 2026 first-party producer is closed; older versions require independent verification |

因此，**LCE 的底层生产、定位、内容、加解密、消费和驱动物理写入路径已经闭环；但不能说“所有写入行为 100% 完全搞明白”**。剩余缺口是上层业务触发 provenance：哪个业务流程在什么条件下决定修改 LCE，以及不同历史版本是否完全相同。

## 2. First-party producer chain

The end-to-end chain is:

`cemssafeudisklabeltool_orig.exe`
→ `usbtoolbusmanage.dll`
→ `cemsusbregsiter.dll`
→ `CUsbRegsiter::CreatePartitions`
→ legacy `EDP_PARTION_INFO[3]`
→ LBA7 serializer.

Machine-readable evidence is in:

`audit/protocol/lba7_compatibility/evidence/official_lba7_producer_20260923.json`

Captured binary identities:

| Binary | SHA-256 |
| --- | --- |
| `cemssafeudisklabeltool_orig.exe` | `b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11` |
| `usbtoolbusmanage.dll` | `08381e33d44d11719795b063a978a6646387ce40d62c2f7aeb514c0c308459e9` |
| `cemsusbregsiter.dll` | `122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb` |
| `edpediskctrl.dll` | `5be85c0f85dc65dd8f89e59a78f584a8325e5208441fc461e2a612501a5b3e08` |

## 3. `PartionType` official semantics

`edpediskctrl.dll::CEdpDiskControl::InitDiskInfo` scans the legacy/runtime partition list and sets three explicit booleans. This directly binds the numeric values:

| `PartionType` | First-party consumer meaning | Project name |
| ---: | --- | --- |
| `1` | `m_bHasBootPart` | Boot / 启动区 |
| `2` | `m_bHasSharePart` | Share / 交换区 |
| `4` | `m_bHasEncryptPart` | Encrypt / 保密区 |

These values are now encoded in `src/protocol/edpf.rs` as `EdpPartitionType`.

## 4. Official labeling modes

The original label tool exposes four radio buttons. Their UI object offsets, object names and runtime selector writes are continuously bound by the original EXE machine code.

| mode | Official UI text | Producer `PartionType[]` |
| ---: | --- | --- |
| `0` | 缺省三分区 | `[1, 2, 4]` |
| `1` | 启动区和交换区二合一 | `[2, 4]` |
| `2` | 整盘加密 | `[1, 4]` |
| `3` | 内外网通用双分区 | `[1, 2]` |

The data path does not remap the selector:

1. label-tool serialization writes the selected mode into compact field `+0x34`;
2. `sub_42e8e0` copies it to `LabelInfo+0x7db`;
3. `usbtoolbusmanage.dll::WriteNormalULabel` copies `+0x7db` unchanged into the registration input;
4. `cemsusbregsiter.dll::sub_10046e80` reads that same byte and expands the four cases above.

`src/protocol/lba7.rs::Lba7PartitionMode` codifies this matrix. Unknown historical sequences remain unclassified rather than being guessed.

### 4.1 Whole-disk-encrypted special case

Mode 2 still constructs the compatibility sequence `[1, 4]`. The producer then forces the type1 configured size to `0x7E00` bytes and subtracts that amount from type4. The UI description nevertheless says the user-visible mode contains only the private/encrypted area. Therefore the type1 record in this mode must not be described as a normal user-visible boot partition without additional evidence.

## 5. Why type2 and type4 can point to the same 3072-byte address

This is the central producer result.

`CUsbRegsiter::CreatePartitions` does **not** serialize all old-table entries with the same geometry rule.

### 5.1 Entry0

The first legacy entry retains the first logical partition's type and normal logical geometry. In the observed producer path it uses the normal first-partition start (`StartSector=63`) and a configured partition size adjusted by the 63-sector prefix.

### 5.2 Entry1 and entry2

For subsequent legacy entries the producer preserves each entry's own `PartionType`, but overwrites the physical geometry with a compatibility representation:

- `StartSector` = result of `sub_10040110()`;
- `PartionSize` = aligned `0xC00` bytes;
- on 512-byte media this is exactly six sectors.

`sub_10040110()` computes the address from classic `DISK_GEOMETRY`:

`CHS_geometry_bytes - 0xE0000`

Thus later entries can carry **different logical types while pointing to the same physical 3072-byte block**.

This is an old-table serialization rule. It is **not** evidence that one logical partition aliases another.

### 5.3 Consequences by official mode

Mode 0, `[1, 2, 4]`:

- entry0 type1: normal first-partition geometry;
- entry1 type2: fixed compatibility extent;
- entry2 type4: the same fixed compatibility extent.

Therefore a physical LBA7 table with type2 and type4 at the same `StartSector` / `PartionSize=3072` is expected producer output. **type2 is not a type4 alias.**

Mode 1, `[2, 4]`:

- entry0 type2: normal large share/combined geometry;
- entry1 type4: fixed compatibility extent.

This explains the observed no-password SanDisk profile where type2 starts at LBA63 while type4 points near the CHS tail.

Mode 3, `[1, 2]`:

- entry0 type1: normal first-partition geometry;
- entry1 type2: fixed compatibility extent.

This producer case proves decisively that the fixed physical extent is **not type4-specific**.

## 6. LBA7 serialization

The legacy table is three 0x40-byte slots (`0xC0` bytes maximum) followed by the legacy 14-byte password/status structure. The producer's LBA7 serializer copies those bytes, applies the legacy rolling-XOR storage transform, and emits the sector-7 representation.

The typed parser is implemented in:

- `src/protocol/edpf.rs`
- `src/protocol/lba7.rs`

The fixed compatibility-address helper is implemented separately in:

- `src/protocol/lba7_compat.rs`

This separation is intentional: logical partition type and physical compatibility extent are different concepts.

## 7. Physical extent content and crypto closure

Previous work on the physical six-sector block remains valid after the naming correction:

- the extent is exactly 3072 bytes;
- committed Lexar and live no-password SanDisk captures are real physical ciphertexts at their LBA7 pointers;
- the recovered plaintext is a fixed FAT16 compatibility image;
- the legacy EDPSECDISK transform with zero8 key and the physical backing byte-offset tweak regenerates the verified device ciphertexts bit-for-bit.

Those facts describe the payload bytes. They do not change the producer conclusion that the same physical object may be referenced by type2 or type4 depending on entry position and official mode.

Canonical fixtures now live under:

`audit/protocol/lba7_compatibility/`

The byte ledger covers the full `+0x000..+0xBFF` range without gaps.

## 8. Consumer boundary

Historical `EdpEDiskCtrl` can fall back from the preferred new-label path to the legacy LBA7 table, convert an old entry into runtime form, and submit its backing geometry to the legacy virtual-disk path. The legacy driver can consequently map a host virtual write onto the physical compatibility extent.

That proves a conditional consumer/write path. It does not mean every disk or every login takes this fallback.

The current normal LBA12 path is a distinct geometry source and must not be cited as evidence that the compatibility extent is the current type4 filesystem partition.

## 9. IIR is separate

IIR work is retained in `src/protocol/iir.rs` and `tests/iir.rs`.

LCE and IIR are separate protocol objects. Prior attempts to bind the old “Region A” name to IIR are not part of the current model.

## 10. Version scope

The four-mode producer matrix is proven for the captured first-party 2026 Windows stack identified above. It must not automatically be projected onto every historical label-tool release.

Older producer binaries should be checked separately before claiming that the exact same UI-mode matrix and entry geometry rules applied in all years.

The first-party code exposes structure names and field semantics, but no standalone symbol naming the 0xC00 payload object was found. **LCE / LBA7 Compatibility Extent** is therefore an explicit project descriptive term, not a claimed vendor symbol.

## 11. Current protocol invariants

The repository now treats the following as regression invariants:

1. `PartionType=1/2/4` maps to boot/share/encrypt.
2. Official mode 0/1/2/3 maps to `[1,2,4]`, `[2,4]`, `[1,4]`, `[1,2]` respectively.
3. LBA7 mode classification is exact; unknown sequences are not guessed.
4. A 3072-byte compatibility pointer is selected by legacy entry position/geometry, not by requiring `PartionType=4`.
5. Multiple later entries may point to the same compatibility extent while retaining distinct logical types.
6. The LBA7 pointer remains authoritative; the CHS formula is an independent producer cross-check.
7. IIR remains a separate module and evidence chain.
