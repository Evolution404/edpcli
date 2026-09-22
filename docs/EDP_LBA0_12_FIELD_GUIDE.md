# EDP LBA0–LBA12 字段手册

> 自动生成：请修改 canonical TSV 后重新生成，勿直接编辑本文件。

事实源：[field_catalog.tsv](../audit/protocol/field_catalog.tsv)、[profile_axes.tsv](../audit/protocol/profile_axes.tsv)。
证据 ID 的 modality、定位与限制见 [evidence_manifest.tsv](../audit/protocol/evidence_manifest.tsv)。
生成器：[generate_field_guide.rs](../scripts/protocol/generate_field_guide.rs)；
运行 `rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide`，
再运行 `target/generate-field-guide`；加 `--check` 只校验，不写文件。

## 阅读约定

Offset 为扇区内十六进制位置，区间含首尾；Length 为字节数。每扇区 512B，整段 6656B。
同一范围可因 profile 有多行：`base/all` 与各 owner axis 选中的一个 state 共同覆盖盘面；
overlay 只补充独立的来源或取值差异，不重复声明字节所有权。不同 axis 独立组合，不能按软件年代整体绑定。

`semantic_status` 记录语义闭环；`implementation_status` 与 `behavior_test_status` 分别记录正式代码和行为测试。
`planned:` 是设计目标，`UNIMPLEMENTED` 表示尚无正式链接。通用 ownership 测试不代表字段行为测试已经完成。
`MISSING_PHYSICAL` 表示缺物理证据引用，不降低语义状态；producer/consumer 中的 virtual 证据不能作为 physical capture。

preserve 表示按该字段 encode rule 保留原字节，不能擅自清零；opaque 表示在 EDP 层不解释内部负载；
backing 表示当前字段语义不消费的存储内容，不能仅因样本为零就认定为必须为零的常量。
具体写入、加密表示和 NUL 后行为以逐字段规则为准；这些区域仍有正式 ownership。
PackedStruct/EncryptedRegion 行按目录现有粒度展示，不补造目录未列出的内部字段或算法。

目录包含 142 个 field × profile 行、18 个正交 axis、44 个 state。

## 目录

- [LBA0](#lba0)
- [LBA1](#lba1)
- [LBA2](#lba2)
- [LBA3](#lba3)
- [LBA4](#lba4)
- [LBA5](#lba5)
- [LBA6](#lba6)
- [LBA7](#lba7)
- [LBA8](#lba8)
- [LBA9](#lba9)
- [LBA10](#lba10)
- [LBA11](#lba11)
- [LBA12](#lba12)

## LBA0

### 用途与 profile

- Cross-profile preserved compatibility backing.
- Explicit-zero absent bootstrap.
- Four standard MBR partition entries.
- Legacy MBR error-message pointer bytes.
- MBR 55AA signature.
- Netac embedded MBR bootstrap.
- Official UsbMainBSec MBR bootstrap.
- Preserved compatibility backing after optional SectorSize.
- SectorSize overlay absent.
- SectorSize overlay stores LE 512.
- Standard MBR disk signature.
- Standard MBR reserved word.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x18f | 400 | lba0.bootstrap | lba0_bootstrap / netac-mbr | PackedStruct | owner |
| 0x000–0x18f | 400 | lba0.bootstrap | lba0_bootstrap / usb-main-bsec | PackedStruct | owner |
| 0x000–0x18f | 400 | lba0.bootstrap | lba0_bootstrap / zero | CompatibilityField | owner |
| 0x190–0x19f | 16 | lba0.compat_190_19f | base / all | UnownedBacking | owner |
| 0x1a0–0x1a3 | 4 | lba0.sector_size_overlay | lba0_sector_size_overlay / absent | Scalar | owner |
| 0x1a0–0x1a3 | 4 | lba0.sector_size_overlay | lba0_sector_size_overlay / sector-size-512 | Scalar | owner |
| 0x1a4–0x1b4 | 17 | lba0.compat_1a4_1b4 | base / all | UnownedBacking | owner |
| 0x1b5–0x1b7 | 3 | lba0.legacy_message_ptrs | base / all | CompatibilityField | owner |
| 0x1b8–0x1bb | 4 | lba0.mbr_disk_signature | base / all | Scalar | owner |
| 0x1bc–0x1bd | 2 | lba0.mbr_reserved | base / all | UnownedBacking | owner |
| 0x1be–0x1fd | 64 | lba0.partition_table | base / all | PackedStruct | owner |
| 0x1fe–0x1ff | 2 | lba0.signature_55aa | base / all | Scalar | owner |

### 逐字段规则与证据

#### lba0.bootstrap · lba0_bootstrap / netac-mbr

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x18f（400B） |
| Semantic type | PackedStruct |
| Meaning | Netac embedded MBR bootstrap. |
| Ownership | owner |
| Decode rule | Select this profile-level bootstrap image. |
| Encode rule | Write exact first-party image or explicit zero. |
| Profile | lba0_bootstrap / netac-mbr |
| Evolution kind | ProducerChanged |
| Producer evidence | S-NETAC-MBR |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Selector provenance is separate from wire closure. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.bootstrap · lba0_bootstrap / usb-main-bsec

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x18f（400B） |
| Semantic type | PackedStruct |
| Meaning | Official UsbMainBSec MBR bootstrap. |
| Ownership | owner |
| Decode rule | Select this profile-level bootstrap image. |
| Encode rule | Write exact first-party image or explicit zero. |
| Profile | lba0_bootstrap / usb-main-bsec |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Selector provenance is separate from wire closure. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.bootstrap · lba0_bootstrap / zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x18f（400B） |
| Semantic type | CompatibilityField |
| Meaning | Explicit-zero absent bootstrap. |
| Ownership | owner |
| Decode rule | Select this profile-level bootstrap image. |
| Encode rule | Write exact first-party image or explicit zero. |
| Profile | lba0_bootstrap / zero |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Selector provenance is separate from wire closure. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.compat_190_19f · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x190–0x19f（16B） |
| Semantic type | UnownedBacking |
| Meaning | Cross-profile preserved compatibility backing. |
| Ownership | owner |
| Decode rule | No EDP payload semantics. |
| Encode rule | Preserve bytes. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-NETAC-MBR |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.sector_size_overlay · lba0_sector_size_overlay / absent

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1a0–0x1a3（4B） |
| Semantic type | Scalar |
| Meaning | SectorSize overlay absent. |
| Ownership | owner |
| Decode rule | Decode LE DWORD; zero is absent, 512 populated. |
| Encode rule | Preserve absent or write 512. |
| Profile | lba0_sector_size_overlay / absent |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Independent from bootstrap family. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.sector_size_overlay · lba0_sector_size_overlay / sector-size-512

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1a0–0x1a3（4B） |
| Semantic type | Scalar |
| Meaning | SectorSize overlay stores LE 512. |
| Ownership | owner |
| Decode rule | Decode LE DWORD; zero is absent, 512 populated. |
| Encode rule | Preserve absent or write 512. |
| Profile | lba0_sector_size_overlay / sector-size-512 |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Independent from bootstrap family. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.compat_1a4_1b4 · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1a4–0x1b4（17B） |
| Semantic type | UnownedBacking |
| Meaning | Preserved compatibility backing after optional SectorSize. |
| Ownership | owner |
| Decode rule | No EDP payload semantics. |
| Encode rule | Preserve bytes. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-NETAC-MBR |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.legacy_message_ptrs · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1b5–0x1b7（3B） |
| Semantic type | CompatibilityField |
| Meaning | Legacy MBR error-message pointer bytes. |
| Ownership | owner |
| Decode rule | Interpret only with UsbMainBSec bootstrap. |
| Encode rule | Keep template bytes or zero state. |
| Profile | base / all |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.mbr_disk_signature · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1b8–0x1bb（4B） |
| Semantic type | Scalar |
| Meaning | Standard MBR disk signature. |
| Ownership | owner |
| Decode rule | Decode LE u32. |
| Encode rule | Preserve standard signature. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.mbr_reserved · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1bc–0x1bd（2B） |
| Semantic type | UnownedBacking |
| Meaning | Standard MBR reserved word. |
| Ownership | owner |
| Decode rule | No EDP business meaning. |
| Encode rule | Preserve bytes. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-NETAC-MBR |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.partition_table · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1be–0x1fd（64B） |
| Semantic type | PackedStruct |
| Meaning | Four standard MBR partition entries. |
| Ownership | owner |
| Decode rule | Decode four 16-byte entries. |
| Encode rule | Serialize/restore MBR entries. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-NETAC-MBR |
| Consumer evidence | S-REPAIR-CURRENT |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.signature_55aa · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1fe–0x1ff（2B） |
| Semantic type | Scalar |
| Meaning | MBR 55AA signature. |
| Ownership | owner |
| Decode rule | Validate 55 AA. |
| Encode rule | Write 55 AA. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-NETAC-MBR |
| Consumer evidence | S-REPAIR-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba0::parse_lba0 |
| Test symbol | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba0_bootstrap**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 0:000-18f | ProducerChanged | LBA0 bootstrap family; upper-layer selector provenance is separate from wire semantics. | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |
| usb-main-bsec | owner | 0:000-18f | ProducerChanged | LBA0 bootstrap family; upper-layer selector provenance is separate from wire semantics. | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |
| netac-mbr | owner | 0:000-18f | ProducerChanged | LBA0 bootstrap family; upper-layer selector provenance is separate from wire semantics. | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |

| Region（含首尾） | zero | usb-main-bsec | netac-mbr |
| --- | --- | --- | --- |
| 0x000–0x18f | lba0.bootstrap: Explicit-zero absent bootstrap. | lba0.bootstrap: Official UsbMainBSec MBR bootstrap. | lba0.bootstrap: Netac embedded MBR bootstrap. |

**lba0_sector_size_overlay**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 0:1a0-1a3 | AddedRemoved | Optional SAFE1/legacy SectorSize overlay independent from bootstrap family. | S-WIN-CURRENT;P-GOLD-ENC |
| sector-size-512 | owner | 0:1a0-1a3 | AddedRemoved | Optional SAFE1/legacy SectorSize overlay independent from bootstrap family. | S-WIN-CURRENT;P-GOLD-ENC |

| Region（含首尾） | absent | sector-size-512 |
| --- | --- | --- |
| 0x1a0–0x1a3 | lba0.sector_size_overlay: SectorSize overlay absent. | lba0.sector_size_overlay: SectorSize overlay stores LE 512. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA1

### 用途与 profile

- Absent GPT primary-header sector.
- GPT primary header and template-owned remainder.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba1.gpt_primary | gpt_layout / absent | CompatibilityField | owner |
| 0x000–0x1ff | 512 | lba1.gpt_primary | gpt_layout / enabled | PackedStruct | owner |

### 逐字段规则与证据

#### lba1.gpt_primary · gpt_layout / absent

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | CompatibilityField |
| Meaning | Absent GPT primary-header sector. |
| Ownership | owner |
| Decode rule | No GPT payload. |
| Encode rule | Preserve absent zero. |
| Profile | gpt_layout / absent |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Physical corpus covers absent profile. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba1::parse_lba1 |
| Test symbol | gpt_behavior::gpt_absent_is_explicit_and_unknown_never_defaults_to_absent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba1.gpt_primary · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | PackedStruct |
| Meaning | GPT primary header and template-owned remainder. |
| Ownership | owner |
| Decode rule | Decode EFI PART header/LBAs/GUIDs/CRCs. |
| Encode rule | Build official GPT header and CRCs. |
| Profile | gpt_layout / enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | V-GPT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Official virtual positive, not physical. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba1::parse_lba1 |
| Test symbol | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**gpt_layout**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 presence is independent from SAFE6 metadata profiles. | V-GPT;P-GOLD-ENC |
| enabled | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 presence is independent from SAFE6 metadata profiles. | V-GPT;P-GOLD-ENC |

| Region（含首尾） | absent | enabled |
| --- | --- | --- |
| 0x000–0x1ff | lba1.gpt_primary: Absent GPT primary-header sector. | lba1.gpt_primary: GPT primary header and template-owned remainder. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA2

### 用途与 profile

- Absent GPT entry sector.
- Active GPT entry0.
- Unused GPT entries1..3.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba2.gpt_entries | gpt_layout / absent | CompatibilityField | owner |
| 0x000–0x07f | 128 | lba2.gpt_entry0 | gpt_layout / enabled | PackedStruct | owner |
| 0x080–0x1ff | 384 | lba2.gpt_unused | gpt_layout / enabled | UnownedBacking | owner |

### 逐字段规则与证据

#### lba2.gpt_entries · gpt_layout / absent

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | CompatibilityField |
| Meaning | Absent GPT entry sector. |
| Ownership | owner |
| Decode rule | No GPT entries. |
| Encode rule | Preserve absent zero. |
| Profile | gpt_layout / absent |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Physical corpus covers absent profile. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba2::parse_lba2 |
| Test symbol | gpt_behavior::gpt_absent_is_explicit_and_unknown_never_defaults_to_absent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba2.gpt_entry0 · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x07f（128B） |
| Semantic type | PackedStruct |
| Meaning | Active GPT entry0. |
| Ownership | owner |
| Decode rule | Decode 128-byte entry. |
| Encode rule | Serialize official entry0. |
| Profile | gpt_layout / enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | V-GPT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Official virtual positive. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba2::parse_lba2 |
| Test symbol | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba2.gpt_unused · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x1ff（384B） |
| Semantic type | UnownedBacking |
| Meaning | Unused GPT entries1..3. |
| Ownership | owner |
| Decode rule | Zero TypeGUID means residual is ignored. |
| Encode rule | Keep unused entries unowned. |
| Profile | gpt_layout / enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | V-GPT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Residual is semantically ignored after zero TypeGUID. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba2::parse_lba2 |
| Test symbol | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**gpt_layout**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 presence is independent from SAFE6 metadata profiles. | V-GPT;P-GOLD-ENC |
| enabled | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 presence is independent from SAFE6 metadata profiles. | V-GPT;P-GOLD-ENC |

| Region（含首尾） | absent | enabled |
| --- | --- | --- |
| 0x000–0x07f | lba2.gpt_entries: Absent GPT entry sector. | lba2.gpt_entry0: Active GPT entry0. |
| 0x080–0x1ff | lba2.gpt_entries: Absent GPT entry sector. | lba2.gpt_unused: Unused GPT entries1..3. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA3

### 用途与 profile

- All-zero manufacturer metadata.
- Distinct historical MP profile B.
- Manufacturer MP profile A.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / historical-mp-b | OpaquePreserve | owner |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / kingston-mp-a | OpaquePreserve | owner |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / zero | OpaquePreserve | owner |

### 逐字段规则与证据

#### lba3.manufacturer_metadata · lba3_metadata / historical-mp-b

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | OpaquePreserve |
| Meaning | Distinct historical MP profile B. |
| Ownership | owner |
| Decode rule | EDP does not decode payload. |
| Encode rule | Preserve exact sector; never normalize. |
| Profile | lba3_metadata / historical-mp-b |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| Consumer evidence | S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Manufacturer serializer is outside EDP boundary. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba3::parse_lba3 |
| Test symbol | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba3.manufacturer_metadata · lba3_metadata / kingston-mp-a

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | OpaquePreserve |
| Meaning | Manufacturer MP profile A. |
| Ownership | owner |
| Decode rule | EDP does not decode payload. |
| Encode rule | Preserve exact sector; never normalize. |
| Profile | lba3_metadata / kingston-mp-a |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| Consumer evidence | S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Manufacturer serializer is outside EDP boundary. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba3::parse_lba3 |
| Test symbol | basic_behavior::basic_parsers_replay_all_committed_physical_gold |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba3.manufacturer_metadata · lba3_metadata / zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | OpaquePreserve |
| Meaning | All-zero manufacturer metadata. |
| Ownership | owner |
| Decode rule | EDP does not decode payload. |
| Encode rule | Preserve exact sector; never normalize. |
| Profile | lba3_metadata / zero |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| Consumer evidence | S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Manufacturer serializer is outside EDP boundary. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba3::parse_lba3 |
| Test symbol | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba3_metadata**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 3:000-1ff | ProducerChanged | Manufacturer MP metadata states share the EDP preserve-only lifecycle. | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |
| kingston-mp-a | owner | 3:000-1ff | ProducerChanged | Manufacturer MP metadata states share the EDP preserve-only lifecycle. | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |
| historical-mp-b | owner | 3:000-1ff | ProducerChanged | Manufacturer MP metadata states share the EDP preserve-only lifecycle. | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |

| Region（含首尾） | zero | kingston-mp-a | historical-mp-b |
| --- | --- | --- | --- |
| 0x000–0x1ff | lba3.manufacturer_metadata: All-zero manufacturer metadata. | lba3.manufacturer_metadata: Manufacturer MP profile A. | lba3.manufacturer_metadata: Distinct historical MP profile B. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA4

### 用途与 profile

- Clear $$$onlyid$$$ header.
- Current HSerial vector is zero.
- Current host-hardinfo is zero.
- HSerialCRC[5] vector
- Historical host DiskNumber/DeviceNumber-derived identity.
- Legacy ABI carries caller HSerial[5].
- MyHardinfo mirror
- NewLabFlag LLGB
- OnllyID2Nd activation key seed
- OnlyIdXor8 guard
- Representation carrier/backing
- Second key comes from independent GUID CRC.
- Second key reuses main onlyid.
- Sector tuple 08 04 0C 01
- SingleUsbFlg
- Trailing LLGB
- Version 1
- bConnetServer
- bDataToServer

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x017 | 24 | lba4.onlyid_header | base / all | CString | owner |
| 0x018–0x01b | 4 | lba4.onlyid_xor8 | lba4_encoding / ordinary-rolling | Scalar | owner |
| 0x018–0x01b | 4 | lba4.onlyid_xor8 | lba4_encoding / post-xor | Scalar | owner |
| 0x01c–0x01f | 4 | overlay.lba4.second_key_source | lba4_second_key_source / current-main-onlyid | CompatibilityField | overlay |
| 0x01c–0x01f | 4 | overlay.lba4.second_key_source | lba4_second_key_source / legacy-guid-crc | CompatibilityField | overlay |
| 0x01c–0x01f | 4 | lba4.second_key | lba4_encoding / ordinary-rolling | Scalar | owner |
| 0x01c–0x01f | 4 | lba4.second_key | lba4_encoding / post-xor | Scalar | owner |
| 0x020–0x033 | 20 | overlay.lba4.hserial_source | lba4_hserial_source / current-zero | CompatibilityField | overlay |
| 0x020–0x033 | 20 | overlay.lba4.hserial_source | lba4_hserial_source / legacy-caller-vector | CompatibilityField | overlay |
| 0x020–0x033 | 20 | lba4.hserial | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x020–0x033 | 20 | lba4.hserial | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x034–0x034 | 1 | lba4.single_usb | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x034–0x034 | 1 | lba4.single_usb | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x035–0x038 | 4 | overlay.host_hardinfo.lba4 | host_hardinfo_source / current-zero | CompatibilityField | overlay |
| 0x035–0x038 | 4 | overlay.host_hardinfo.lba4 | host_hardinfo_source / legacy-host-identity | CompatibilityField | overlay |
| 0x035–0x038 | 4 | lba4.my_hardinfo | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x035–0x038 | 4 | lba4.my_hardinfo | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x039–0x03c | 4 | lba4.new_lab_flag | lba4_encoding / ordinary-rolling | Scalar | owner |
| 0x039–0x03c | 4 | lba4.new_lab_flag | lba4_encoding / post-xor | Scalar | owner |
| 0x03d–0x040 | 4 | lba4.version | lba4_encoding / ordinary-rolling | Scalar | owner |
| 0x03d–0x040 | 4 | lba4.version | lba4_encoding / post-xor | Scalar | owner |
| 0x041–0x044 | 4 | lba4.sector_tuple | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x041–0x044 | 4 | lba4.sector_tuple | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x045–0x045 | 1 | lba4.data_to_server | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x045–0x045 | 1 | lba4.data_to_server | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x046–0x046 | 1 | lba4.connect_server | lba4_encoding / ordinary-rolling | CompatibilityField | owner |
| 0x046–0x046 | 1 | lba4.connect_server | lba4_encoding / post-xor | CompatibilityField | owner |
| 0x047–0x1fb | 437 | lba4.representation_backing | lba4_encoding / ordinary-rolling | UnownedBacking | owner |
| 0x047–0x1fb | 437 | lba4.representation_backing | lba4_encoding / post-xor | UnownedBacking | owner |
| 0x1fc–0x1ff | 4 | lba4.trailing_llgb | lba4_encoding / ordinary-rolling | Scalar | owner |
| 0x1fc–0x1ff | 4 | lba4.trailing_llgb | lba4_encoding / post-xor | Scalar | owner |

### 逐字段规则与证据

#### lba4.onlyid_header · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x017（24B） |
| Semantic type | CString |
| Meaning | Clear $$$onlyid$$$ header. |
| Ownership | owner |
| Decode rule | Parse main onlyid. |
| Encode rule | Format canonical header. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.onlyid_xor8 · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x018–0x01b（4B） |
| Semantic type | Scalar |
| Meaning | OnlyIdXor8 guard |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.onlyid_xor8 · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x018–0x01b（4B） |
| Semantic type | Scalar |
| Meaning | OnlyIdXor8 guard |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.second_key_source · lba4_second_key_source / current-main-onlyid

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01c–0x01f（4B） |
| Semantic type | CompatibilityField |
| Meaning | Second key reuses main onlyid. |
| Ownership | overlay |
| Decode rule | Overlay annotation; owner is lba4_encoding. |
| Encode rule | Producer source changes only. |
| Profile | lba4_second_key_source / current-main-onlyid |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Composes with either encoding. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.second_key_source · lba4_second_key_source / legacy-guid-crc

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01c–0x01f（4B） |
| Semantic type | CompatibilityField |
| Meaning | Second key comes from independent GUID CRC. |
| Ownership | overlay |
| Decode rule | Overlay annotation; owner is lba4_encoding. |
| Encode rule | Producer source changes only. |
| Profile | lba4_second_key_source / legacy-guid-crc |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Composes with either encoding. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.second_key · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01c–0x01f（4B） |
| Semantic type | Scalar |
| Meaning | OnllyID2Nd activation key seed |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.second_key · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01c–0x01f（4B） |
| Semantic type | Scalar |
| Meaning | OnllyID2Nd activation key seed |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.hserial_source · lba4_hserial_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x020–0x033（20B） |
| Semantic type | CompatibilityField |
| Meaning | Current HSerial vector is zero. |
| Ownership | overlay |
| Decode rule | Overlay annotation; owner is lba4_encoding. |
| Encode rule | Zero/caller producer varies independently. |
| Profile | lba4_hserial_source / current-zero |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-191141;S-BUS-2020 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Caller value-generation algorithm is not invented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.hserial_source · lba4_hserial_source / legacy-caller-vector

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x020–0x033（20B） |
| Semantic type | CompatibilityField |
| Meaning | Legacy ABI carries caller HSerial[5]. |
| Ownership | overlay |
| Decode rule | Overlay annotation; owner is lba4_encoding. |
| Encode rule | Zero/caller producer varies independently. |
| Profile | lba4_hserial_source / legacy-caller-vector |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-191141;S-BUS-2020 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Caller value-generation algorithm is not invented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.hserial · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x020–0x033（20B） |
| Semantic type | CompatibilityField |
| Meaning | HSerialCRC[5] vector |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.hserial · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x020–0x033（20B） |
| Semantic type | CompatibilityField |
| Meaning | HSerialCRC[5] vector |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.single_usb · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x034–0x034（1B） |
| Semantic type | CompatibilityField |
| Meaning | SingleUsbFlg |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.single_usb · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x034–0x034（1B） |
| Semantic type | CompatibilityField |
| Meaning | SingleUsbFlg |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba4 · host_hardinfo_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x035–0x038（4B） |
| Semantic type | CompatibilityField |
| Meaning | Current host-hardinfo is zero. |
| Ownership | overlay |
| Decode rule | Overlay annotation; semantic owner is base/LBA4 encoding. |
| Encode rule | Producer axis is independent of LBA4 encoding and UsbOnlyInfo. |
| Profile | host_hardinfo_source / current-zero |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Cross-LBA mirror is independent. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba4 · host_hardinfo_source / legacy-host-identity

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x035–0x038（4B） |
| Semantic type | CompatibilityField |
| Meaning | Historical host DiskNumber/DeviceNumber-derived identity. |
| Ownership | overlay |
| Decode rule | Overlay annotation; semantic owner is base/LBA4 encoding. |
| Encode rule | Producer axis is independent of LBA4 encoding and UsbOnlyInfo. |
| Profile | host_hardinfo_source / legacy-host-identity |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Cross-LBA mirror is independent. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.my_hardinfo · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x035–0x038（4B） |
| Semantic type | CompatibilityField |
| Meaning | MyHardinfo mirror |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.my_hardinfo · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x035–0x038（4B） |
| Semantic type | CompatibilityField |
| Meaning | MyHardinfo mirror |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.new_lab_flag · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x039–0x03c（4B） |
| Semantic type | Scalar |
| Meaning | NewLabFlag LLGB |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.new_lab_flag · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x039–0x03c（4B） |
| Semantic type | Scalar |
| Meaning | NewLabFlag LLGB |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.version · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03d–0x040（4B） |
| Semantic type | Scalar |
| Meaning | Version 1 |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.version · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03d–0x040（4B） |
| Semantic type | Scalar |
| Meaning | Version 1 |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.sector_tuple · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x041–0x044（4B） |
| Semantic type | CompatibilityField |
| Meaning | Sector tuple 08 04 0C 01 |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.sector_tuple · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x041–0x044（4B） |
| Semantic type | CompatibilityField |
| Meaning | Sector tuple 08 04 0C 01 |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.data_to_server · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x045–0x045（1B） |
| Semantic type | CompatibilityField |
| Meaning | bDataToServer |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.data_to_server · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x045–0x045（1B） |
| Semantic type | CompatibilityField |
| Meaning | bDataToServer |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.connect_server · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x046–0x046（1B） |
| Semantic type | CompatibilityField |
| Meaning | bConnetServer |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.connect_server · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x046–0x046（1B） |
| Semantic type | CompatibilityField |
| Meaning | bConnetServer |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.representation_backing · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x047–0x1fb（437B） |
| Semantic type | UnownedBacking |
| Meaning | Representation carrier/backing |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.representation_backing · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x047–0x1fb（437B） |
| Semantic type | UnownedBacking |
| Meaning | Representation carrier/backing |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.trailing_llgb · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1fc–0x1ff（4B） |
| Semantic type | Scalar |
| Meaning | Trailing LLGB |
| Ownership | owner |
| Decode rule | Apply official rolling reader; logical flags follow rolling-decoded view. |
| Encode rule | Serialize through ordinary rolling without post-XOR flag reinterpretation. |
| Profile | lba4_encoding / ordinary-rolling |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.trailing_llgb · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1fc–0x1ff（4B） |
| Semantic type | Scalar |
| Meaning | Trailing LLGB |
| Ownership | owner |
| Decode rule | Apply official rolling reader; keep wire and reader flag views distinct. |
| Encode rule | Full rolling then post-XOR overwrite +0x45/+0x46 from node flags. |
| Profile | lba4_encoding / post-xor |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Encoding composes independently with producer overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba4::parse_lba4 |
| Test symbol | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**host_hardinfo_source**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo mirrors share an independent producer axis. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-host-identity | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo mirrors share an independent producer axis. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| Region（含首尾） | current-zero | legacy-host-identity |
| --- | --- | --- |
| 0x035–0x038 | overlay.host_hardinfo.lba4: Current host-hardinfo is zero. | overlay.host_hardinfo.lba4: Historical host DiskNumber/DeviceNumber-derived identity. |

**lba4_encoding**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| post-xor | owner | 4:018-1ff | EncodingChanged | Restore-node logical fields are stable while wire representation differs. | S-WIN-CURRENT;S-WIN-191141;V-LBA4;V-LBA4-V19 |
| ordinary-rolling | owner | 4:018-1ff | EncodingChanged | Restore-node logical fields are stable while wire representation differs. | S-WIN-CURRENT;S-WIN-191141;V-LBA4;V-LBA4-V19 |

| Region（含首尾） | post-xor | ordinary-rolling |
| --- | --- | --- |
| 0x018–0x01b | lba4.onlyid_xor8: OnlyIdXor8 guard | lba4.onlyid_xor8: OnlyIdXor8 guard |
| 0x01c–0x01f | lba4.second_key: OnllyID2Nd activation key seed | lba4.second_key: OnllyID2Nd activation key seed |
| 0x020–0x033 | lba4.hserial: HSerialCRC[5] vector | lba4.hserial: HSerialCRC[5] vector |
| 0x034–0x034 | lba4.single_usb: SingleUsbFlg | lba4.single_usb: SingleUsbFlg |
| 0x035–0x038 | lba4.my_hardinfo: MyHardinfo mirror | lba4.my_hardinfo: MyHardinfo mirror |
| 0x039–0x03c | lba4.new_lab_flag: NewLabFlag LLGB | lba4.new_lab_flag: NewLabFlag LLGB |
| 0x03d–0x040 | lba4.version: Version 1 | lba4.version: Version 1 |
| 0x041–0x044 | lba4.sector_tuple: Sector tuple 08 04 0C 01 | lba4.sector_tuple: Sector tuple 08 04 0C 01 |
| 0x045–0x045 | lba4.data_to_server: bDataToServer | lba4.data_to_server: bDataToServer |
| 0x046–0x046 | lba4.connect_server: bConnetServer | lba4.connect_server: bConnetServer |
| 0x047–0x1fb | lba4.representation_backing: Representation carrier/backing | lba4.representation_backing: Representation carrier/backing |
| 0x1fc–0x1ff | lba4.trailing_llgb: Trailing LLGB | lba4.trailing_llgb: Trailing LLGB |

**lba4_hserial_source**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:020-033 | ProducerChanged | HSerialCRC vector is independently absent or caller supplied. | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19;P-GOLD-ENC |
| legacy-caller-vector | overlay | 4:020-033 | ProducerChanged | HSerialCRC vector is independently absent or caller supplied. | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19;P-GOLD-ENC |

| Region（含首尾） | current-zero | legacy-caller-vector |
| --- | --- | --- |
| 0x020–0x033 | overlay.lba4.hserial_source: Current HSerial vector is zero. | overlay.lba4.hserial_source: Legacy ABI carries caller HSerial[5]. |

**lba4_second_key_source**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| current-main-onlyid | overlay | 4:01c-01f | ProducerChanged | OnllyID2Nd meaning is stable while producer seed source changes. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-guid-crc | overlay | 4:01c-01f | ProducerChanged | OnllyID2Nd meaning is stable while producer seed source changes. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| Region（含首尾） | current-main-onlyid | legacy-guid-crc |
| --- | --- | --- |
| 0x01c–0x01f | overlay.lba4.second_key_source: Second key reuses main onlyid. | overlay.lba4.second_key_source: Second key comes from independent GUID CRC. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA5

### 用途与 profile

- Opaque write-protection probe scratch sector.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba5.write_probe_scratch | base / all | OpaquePreserve | owner |

### 逐字段规则与证据

#### lba5.write_probe_scratch · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | OpaquePreserve |
| Meaning | Opaque write-protection probe scratch sector. |
| Ownership | owner |
| Decode rule | Treat payload as opaque. |
| Encode rule | Preserve exact sector. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba5::parse_lba5 |
| Test symbol | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

目录未为本扇区声明独立 profile axis；逐字段 evolution kind 见上表。

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA6

### 用途与 profile

- 2*CRC32(device_id) compatibility guard.
- Autonum slot; post-NUL backing.
- BeiZhu first byte/empty NUL.
- BeiZhu tail or post-NUL backing.
- Byte63 NUL seam.
- Byte63 post-NUL backing.
- Byte63 stores Dept[59].
- CRC32(device_id).
- Dedicated BeiZhu NUL.
- Dedicated GSerial NUL.
- GSerial prefix/terminator position.
- GSerial tail or post-NUL backing.
- Label slot with backing.
- Marker plus Dept[0..58].
- Office slot; post-NUL backing.
- Post-NUL zero backing.
- SAFE6 checksum over first 508 bytes.
- Short Dept C-string/backing.
- Static bootstrap/message template.
- Static template material.
- Static zero tail.
- Surviving MBR entry3 bytes tied to LBA12 type4 geometry.
- UsbMainBSec static template.
- User slot; post-NUL backing.
- Write-only !SAFE m_encrypt metadata.
- Zero compatibility tail.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x03e | 63 | lba6.dept_prefix | dept_layout / join59 | CString | owner |
| 0x000–0x03e | 63 | lba6.dept_prefix | dept_layout / join60 | CString | owner |
| 0x000–0x03e | 63 | lba6.dept_prefix | dept_layout / short | CString | owner |
| 0x03f–0x03f | 1 | lba6.dept_seam | dept_layout / join59 | ProfileSelector | owner |
| 0x03f–0x03f | 1 | lba6.dept_seam | dept_layout / join60 | ProfileSelector | owner |
| 0x03f–0x03f | 1 | lba6.dept_seam | dept_layout / short | ProfileSelector | owner |
| 0x040–0x04f | 16 | lba6.template_040_04f | base / all | CompatibilityField | owner |
| 0x050–0x06f | 32 | lba6.user_slot | base / all | CString | owner |
| 0x070–0x07f | 16 | lba6.autonum_slot | base / all | CString | owner |
| 0x080–0x0bf | 64 | lba6.office_slot | base / all | CString | owner |
| 0x0c0–0x0ff | 64 | lba6.template_0c0_0ff | base / all | CompatibilityField | owner |
| 0x100–0x103 | 4 | lba6.device_crc | base / all | Scalar | owner |
| 0x104–0x107 | 4 | lba6.device_crc_guard | base / all | Scalar | owner |
| 0x108–0x187 | 128 | lba6.template_108_187 | base / all | CompatibilityField | owner |
| 0x188–0x1bf | 56 | lba6.label_slot | base / all | CString | owner |
| 0x1c0–0x1c8 | 9 | lba6.gserial_prefix | base / all | CString | owner |
| 0x1c9–0x1ce | 6 | lba6.gserial_tail | base / all | CString | owner |
| 0x1cf–0x1cf | 1 | lba6.gserial_terminator | base / all | Scalar | owner |
| 0x1d0–0x1d0 | 1 | lba6.beizhu_head | base / all | CString | owner |
| 0x1d1–0x1de | 14 | lba6.beizhu_tail | base / all | CString | owner |
| 0x1df–0x1df | 1 | lba6.beizhu_terminator | base / all | Scalar | owner |
| 0x1e0–0x1ed | 14 | lba6.mbr_underlay | lba6_mbr_underlay / legacy-mbr-snapshot | Snapshot | owner |
| 0x1e0–0x1ed | 14 | lba6.mbr_underlay | lba6_mbr_underlay / zero-underlay | UnownedBacking | owner |
| 0x1ee–0x1ef | 2 | lba6.compat_1ee_1ef | base / all | CompatibilityField | owner |
| 0x1f0–0x1f3 | 4 | lba6.encrypt_generation_flag | base / all | Scalar | owner |
| 0x1f4–0x1fb | 8 | lba6.zero_tail | base / all | CompatibilityField | owner |
| 0x1fc–0x1ff | 4 | lba6.checksum | base / all | Checksum | owner |

### 逐字段规则与证据

#### lba6.dept_prefix · dept_layout / join59

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x03e（63B） |
| Semantic type | CString |
| Meaning | Marker plus Dept[0..58]. |
| Ownership | owner |
| Decode rule | Zero seam selects join59; overlay continuation at Dept[59]. |
| Encode rule | Write NUL seam and continuation from Dept[59]. |
| Profile | dept_layout / join59 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Historical join59 executable provenance is not required for wire semantics. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_prefix · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x03e（63B） |
| Semantic type | CString |
| Meaning | Marker plus Dept[0..58]. |
| Ownership | owner |
| Decode rule | Nonzero seam selects join60; append at Dept[60]. |
| Encode rule | Write Dept[59] inline and continuation from Dept[60]. |
| Profile | dept_layout / join60 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Historical join59 executable provenance is not required for wire semantics. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_prefix · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x03e（63B） |
| Semantic type | CString |
| Meaning | Short Dept C-string/backing. |
| Ownership | owner |
| Decode rule | Decode Dept only from LBA6 C-string. |
| Encode rule | Write short Dept; preserve LBA9 backing. |
| Profile | dept_layout / short |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Historical join59 executable provenance is not required for wire semantics. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / join59

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03f–0x03f（1B） |
| Semantic type | ProfileSelector |
| Meaning | Byte63 NUL seam. |
| Ownership | owner |
| Decode rule | Zero seam selects join59; overlay continuation at Dept[59]. |
| Encode rule | Write NUL seam and continuation from Dept[59]. |
| Profile | dept_layout / join59 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Exact per-state byte role is closed. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03f–0x03f（1B） |
| Semantic type | ProfileSelector |
| Meaning | Byte63 stores Dept[59]. |
| Ownership | owner |
| Decode rule | Nonzero seam selects join60; append at Dept[60]. |
| Encode rule | Write Dept[59] inline and continuation from Dept[60]. |
| Profile | dept_layout / join60 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Exact per-state byte role is closed. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03f–0x03f（1B） |
| Semantic type | ProfileSelector |
| Meaning | Byte63 post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode Dept only from LBA6 C-string. |
| Encode rule | Write short Dept; preserve LBA9 backing. |
| Profile | dept_layout / short |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Exact per-state byte role is closed. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_040_04f · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x040–0x04f（16B） |
| Semantic type | CompatibilityField |
| Meaning | UsbMainBSec static template. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.user_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x050–0x06f（32B） |
| Semantic type | CString |
| Meaning | User slot; post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.autonum_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x070–0x07f（16B） |
| Semantic type | CString |
| Meaning | Autonum slot; post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.office_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0bf（64B） |
| Semantic type | CString |
| Meaning | Office slot; post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_0c0_0ff · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x0c0–0x0ff（64B） |
| Semantic type | CompatibilityField |
| Meaning | Static template material. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.device_crc · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x103（4B） |
| Semantic type | Scalar |
| Meaning | CRC32(device_id). |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.device_crc_guard · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x104–0x107（4B） |
| Semantic type | Scalar |
| Meaning | 2*CRC32(device_id) compatibility guard. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_108_187 · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x108–0x187（128B） |
| Semantic type | CompatibilityField |
| Meaning | Static bootstrap/message template. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.label_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x188–0x1bf（56B） |
| Semantic type | CString |
| Meaning | Label slot with backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_prefix · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1c0–0x1c8（9B） |
| Semantic type | CString |
| Meaning | GSerial prefix/terminator position. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1c9–0x1ce（6B） |
| Semantic type | CString |
| Meaning | GSerial tail or post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_terminator · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1cf–0x1cf（1B） |
| Semantic type | Scalar |
| Meaning | Dedicated GSerial NUL. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_head · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1d0–0x1d0（1B） |
| Semantic type | CString |
| Meaning | BeiZhu first byte/empty NUL. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1d1–0x1de（14B） |
| Semantic type | CString |
| Meaning | BeiZhu tail or post-NUL backing. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_terminator · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1df–0x1df（1B） |
| Semantic type | Scalar |
| Meaning | Dedicated BeiZhu NUL. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.mbr_underlay · lba6_mbr_underlay / legacy-mbr-snapshot

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1e0–0x1ed（14B） |
| Semantic type | Snapshot |
| Meaning | Surviving MBR entry3 bytes tied to LBA12 type4 geometry. |
| Ownership | owner |
| Decode rule | Decode fixed CHS/type prefix plus LE start/count formula. |
| Encode rule | Preserve existing snapshot; do not synthesize in current writes. |
| Profile | lba6_mbr_underlay / legacy-mbr-snapshot |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| Physical evidence | P-GOLD-ENC;P-EESI-NETAC |
| Implementation provenance | Historical nonzero copy site remains provenance only. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.mbr_underlay · lba6_mbr_underlay / zero-underlay

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1e0–0x1ed（14B） |
| Semantic type | UnownedBacking |
| Meaning | Post-NUL zero backing. |
| Ownership | owner |
| Decode rule | No independent business consumer. |
| Encode rule | Keep zero unless preserving legacy snapshot. |
| Profile | lba6_mbr_underlay / zero-underlay |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD;P-EESI-NETAC |
| Implementation provenance | Historical nonzero selector remains provenance only. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.compat_1ee_1ef · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1ee–0x1ef（2B） |
| Semantic type | CompatibilityField |
| Meaning | Zero compatibility tail. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.encrypt_generation_flag · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1f0–0x1f3（4B） |
| Semantic type | Scalar |
| Meaning | Write-only !SAFE m_encrypt metadata. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.zero_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1f4–0x1fb（8B） |
| Semantic type | CompatibilityField |
| Meaning | Static zero tail. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.checksum · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x1fc–0x1ff（4B） |
| Semantic type | Checksum |
| Meaning | SAFE6 checksum over first 508 bytes. |
| Ownership | owner |
| Decode rule | Decode field; C-strings stop at first NUL. |
| Encode rule | Serialize canonical field and preserve documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba6::parse_lba6 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**dept_layout**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| short | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join59 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join60 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |

| Region（含首尾） | short | join59 | join60 |
| --- | --- | --- | --- |
| 0x000–0x03e | lba6.dept_prefix: Short Dept C-string/backing. | lba6.dept_prefix: Marker plus Dept[0..58]. | lba6.dept_prefix: Marker plus Dept[0..58]. |
| 0x03f–0x03f | lba6.dept_seam: Byte63 post-NUL backing. | lba6.dept_seam: Byte63 NUL seam. | lba6.dept_seam: Byte63 stores Dept[59]. |

**lba6_mbr_underlay**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| zero-underlay | owner | 6:1e0-1ed | OwnershipChanged | Post-NUL LBA6 slice is zero backing or surviving MBR entry3 snapshot. | S-MBR-SNAPSHOT-SEMANTIC;P-GOLD-ENC |
| legacy-mbr-snapshot | owner | 6:1e0-1ed | OwnershipChanged | Post-NUL LBA6 slice is zero backing or surviving MBR entry3 snapshot. | S-MBR-SNAPSHOT-SEMANTIC;P-GOLD-ENC |

| Region（含首尾） | zero-underlay | legacy-mbr-snapshot |
| --- | --- | --- |
| 0x1e0–0x1ed | lba6.mbr_underlay: Post-NUL zero backing. | lba6.mbr_underlay: Surviving MBR entry3 bytes tied to LBA12 type4 geometry. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA7

### 用途与 profile

- 14-byte LBA7 pass-info current-v0206.
- 14-byte LBA7 pass-info legacy-v0064.
- Absent/zero entry2.
- Packed 64-byte legacy EDPF entries 0 and 1.
- Present packed EDPF entry2.
- Writer-owned zero region after table/pass-info.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x07f | 128 | lba7.entries_0_1 | base / all | PackedStruct | owner |
| 0x080–0x0bf | 64 | lba7.entry2 | lba7_entry_count / three-entry | PackedStruct | owner |
| 0x080–0x0bf | 64 | lba7.entry2 | lba7_entry_count / two-entry | CompatibilityField | owner |
| 0x0c0–0x0cd | 14 | lba7.pass_info | lba7_passinfo_version / current-v0206 | PackedStruct | owner |
| 0x0c0–0x0cd | 14 | lba7.pass_info | lba7_passinfo_version / legacy-v0064 | PackedStruct | owner |
| 0x0ce–0x1ff | 306 | lba7.post_table_zero | base / all | CompatibilityField | owner |

### 逐字段规则与证据

#### lba7.entries_0_1 · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x07f（128B） |
| Semantic type | PackedStruct |
| Meaning | Packed 64-byte legacy EDPF entries 0 and 1. |
| Ownership | owner |
| Decode rule | Decode 0x40-stride entries. |
| Encode rule | Serialize packed old-table form. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::entries_0_1 |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.entry2 · lba7_entry_count / three-entry

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0bf（64B） |
| Semantic type | PackedStruct |
| Meaning | Present packed EDPF entry2. |
| Ownership | owner |
| Decode rule | Decode packed 64-byte entry2. |
| Encode rule | Serialize entry2. |
| Profile | lba7_entry_count / three-entry |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Entry count is independent from pass-info version. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::entry2 |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.entry2 · lba7_entry_count / two-entry

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0bf（64B） |
| Semantic type | CompatibilityField |
| Meaning | Absent/zero entry2. |
| Ownership | owner |
| Decode rule | Treat entry2 as absent. |
| Encode rule | Keep entry2 unused. |
| Profile | lba7_entry_count / two-entry |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Entry count is independent from pass-info version. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::entry2 |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.pass_info · lba7_passinfo_version / current-v0206

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x0c0–0x0cd（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte LBA7 pass-info current-v0206. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields; dormant period units remain uninterpreted. |
| Encode rule | Serialize selected pass-info version and preserve named dormant bytes. |
| Profile | lba7_passinfo_version / current-v0206 |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Independent from entry count. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.pass_info · lba7_passinfo_version / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x0c0–0x0cd（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte LBA7 pass-info legacy-v0064. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields; dormant period units remain uninterpreted. |
| Encode rule | Serialize selected pass-info version and preserve named dormant bytes. |
| Profile | lba7_passinfo_version / legacy-v0064 |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Independent from entry count. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.post_table_zero · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x0ce–0x1ff（306B） |
| Semantic type | CompatibilityField |
| Meaning | Writer-owned zero region after table/pass-info. |
| Ownership | owner |
| Decode rule | No semantic fields. |
| Encode rule | Initialize to zero. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba7::post_table_zero |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba7_entry_count**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| two-entry | owner | 7:080-0bf | AddedRemoved | Third packed legacy EDPF entry is independently absent or present. | S-WIN-CURRENT;P-GOLD-ENC;P-GOLD-NOPWD |
| three-entry | owner | 7:080-0bf | AddedRemoved | Third packed legacy EDPF entry is independently absent or present. | S-WIN-CURRENT;P-GOLD-ENC;P-GOLD-NOPWD |

| Region（含首尾） | two-entry | three-entry |
| --- | --- | --- |
| 0x080–0x0bf | lba7.entry2: Absent/zero entry2. | lba7.entry2: Present packed EDPF entry2. |

**lba7_passinfo_version**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| legacy-v0064 | owner | 7:0c0-0cd | ProducerChanged | LBA7 pass-info version is independent from packed entry count. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| current-v0206 | owner | 7:0c0-0cd | ProducerChanged | LBA7 pass-info version is independent from packed entry count. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| Region（含首尾） | legacy-v0064 | current-v0206 |
| --- | --- | --- |
| 0x0c0–0x0cd | lba7.pass_info: 14-byte LBA7 pass-info legacy-v0064. | lba7.pass_info: 14-byte LBA7 pass-info current-v0206. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA8

### 用途与 profile

- Current host-hardinfo is zero.
- Current main-onlyid text + zero DWORD.
- Dynamic ELABEL body/backing/tail
- ELABEL offset
- HDSerialInfo host identity
- Historical host DiskNumber/DeviceNumber-derived identity.
- LLGB magic
- Labversion
- Logical encrypted-prefix length
- MacInfo[6] reserved slot
- Monotonic-millisecond writeTime
- Reserved zero header
- Strict legacy absent/zero slot.
- ToolVersion[4]
- Transitional text + host-hardinfo DWORD.
- UsbOnlyInfo suffix

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x003 | 4 | lba8.magic | base / all | Scalar | owner |
| 0x004–0x007 | 4 | lba8.logical_length | base / all | Scalar | owner |
| 0x008–0x00b | 4 | lba8.tool_version | base / all | CompatibilityField | owner |
| 0x00c–0x00f | 4 | lba8.lab_version | base / all | Scalar | owner |
| 0x010–0x013 | 4 | lba8.write_time | base / all | Scalar | owner |
| 0x014–0x017 | 4 | overlay.host_hardinfo.lba8 | host_hardinfo_source / current-zero | CompatibilityField | overlay |
| 0x014–0x017 | 4 | overlay.host_hardinfo.lba8 | host_hardinfo_source / legacy-host-identity | CompatibilityField | overlay |
| 0x014–0x017 | 4 | lba8.host_hardinfo | base / all | CompatibilityField | owner |
| 0x018–0x01d | 6 | lba8.mac_info | base / all | CompatibilityField | owner |
| 0x01e–0x02d | 16 | lba8.usb_only_info | lba8_usb_only_info / current | CompatibilityField | owner |
| 0x01e–0x02d | 16 | lba8.usb_only_info | lba8_usb_only_info / strict-legacy-absent | CompatibilityField | owner |
| 0x01e–0x02d | 16 | lba8.usb_only_info | lba8_usb_only_info / transitional-2019 | CompatibilityField | owner |
| 0x02e–0x03d | 16 | lba8.usb_only_suffix | base / all | CompatibilityField | owner |
| 0x03e–0x03f | 2 | lba8.elab_offset | base / all | Scalar | owner |
| 0x040–0x07f | 64 | lba8.reserved_040_07f | base / all | CompatibilityField | owner |
| 0x080–0x1ff | 384 | lba8.elabel_body | base / all | EncryptedRegion | owner |

### 逐字段规则与证据

#### lba8.magic · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x003（4B） |
| Semantic type | Scalar |
| Meaning | LLGB magic |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.logical_length · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x004–0x007（4B） |
| Semantic type | Scalar |
| Meaning | Logical encrypted-prefix length |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::logical_length |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.tool_version · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x008–0x00b（4B） |
| Semantic type | CompatibilityField |
| Meaning | ToolVersion[4] |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::tool_version |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.lab_version · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x00c–0x00f（4B） |
| Semantic type | Scalar |
| Meaning | Labversion |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::lab_version |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.write_time · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x010–0x013（4B） |
| Semantic type | Scalar |
| Meaning | Monotonic-millisecond writeTime |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::write_time |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba8 · host_hardinfo_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x014–0x017（4B） |
| Semantic type | CompatibilityField |
| Meaning | Current host-hardinfo is zero. |
| Ownership | overlay |
| Decode rule | Overlay annotation; semantic owner is base/LBA4 encoding. |
| Encode rule | Producer axis is independent of LBA4 encoding and UsbOnlyInfo. |
| Profile | host_hardinfo_source / current-zero |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Cross-LBA mirror is independent. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::profile::host_hardinfo_source |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba8 · host_hardinfo_source / legacy-host-identity

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x014–0x017（4B） |
| Semantic type | CompatibilityField |
| Meaning | Historical host DiskNumber/DeviceNumber-derived identity. |
| Ownership | overlay |
| Decode rule | Overlay annotation; semantic owner is base/LBA4 encoding. |
| Encode rule | Producer axis is independent of LBA4 encoding and UsbOnlyInfo. |
| Profile | host_hardinfo_source / legacy-host-identity |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Cross-LBA mirror is independent. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::profile::host_hardinfo_source |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.host_hardinfo · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x014–0x017（4B） |
| Semantic type | CompatibilityField |
| Meaning | HDSerialInfo host identity |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::host_hardinfo |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.mac_info · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x018–0x01d（6B） |
| Semantic type | CompatibilityField |
| Meaning | MacInfo[6] reserved slot |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::mac_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / current

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01e–0x02d（16B） |
| Semantic type | CompatibilityField |
| Meaning | Current main-onlyid text + zero DWORD. |
| Ownership | owner |
| Decode rule | Decode selected compatibility representation; no behavior branch. |
| Encode rule | Serialize selected representation without coupling other axes. |
| Profile | lba8_usb_only_info / current |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Producer generation kept separate from representation. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::usb_only_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / strict-legacy-absent

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01e–0x02d（16B） |
| Semantic type | CompatibilityField |
| Meaning | Strict legacy absent/zero slot. |
| Ownership | owner |
| Decode rule | Decode selected compatibility representation; no behavior branch. |
| Encode rule | Serialize selected representation without coupling other axes. |
| Profile | lba8_usb_only_info / strict-legacy-absent |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Producer generation kept separate from representation. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::usb_only_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / transitional-2019

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x01e–0x02d（16B） |
| Semantic type | CompatibilityField |
| Meaning | Transitional text + host-hardinfo DWORD. |
| Ownership | owner |
| Decode rule | Decode selected compatibility representation; no behavior branch. |
| Encode rule | Serialize selected representation without coupling other axes. |
| Profile | lba8_usb_only_info / transitional-2019 |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-WIN-191141 |
| Consumer evidence | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Producer generation kept separate from representation. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::usb_only_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_suffix · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x02e–0x03d（16B） |
| Semantic type | CompatibilityField |
| Meaning | UsbOnlyInfo suffix |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::usb_only_suffix |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.elab_offset · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x03e–0x03f（2B） |
| Semantic type | Scalar |
| Meaning | ELABEL offset |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::elab_offset |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.reserved_040_07f · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x040–0x07f（64B） |
| Semantic type | CompatibilityField |
| Meaning | Reserved zero header |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::reserved_040_07f |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.elabel_body · base / all

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x1ff（384B） |
| Semantic type | EncryptedRegion |
| Meaning | Dynamic ELABEL body/backing/tail |
| Ownership | owner |
| Decode rule | Decode with LBA8 logical-length/ELABEL rules. |
| Encode rule | Serialize current field while preserving documented backing. |
| Profile | base / all |
| Evolution kind | Stable |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba8::elabel_body |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**host_hardinfo_source**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo mirrors share an independent producer axis. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-host-identity | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo mirrors share an independent producer axis. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| Region（含首尾） | current-zero | legacy-host-identity |
| --- | --- | --- |
| 0x014–0x017 | overlay.host_hardinfo.lba8: Current host-hardinfo is zero. | overlay.host_hardinfo.lba8: Historical host DiskNumber/DeviceNumber-derived identity. |

**lba8_usb_only_info**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| current | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo has current, transitional, and absent producer states. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| transitional-2019 | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo has current, transitional, and absent producer states. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| strict-legacy-absent | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo has current, transitional, and absent producer states. | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| Region（含首尾） | current | transitional-2019 | strict-legacy-absent |
| --- | --- | --- | --- |
| 0x01e–0x02d | lba8.usb_only_info: Current main-onlyid text + zero DWORD. | lba8.usb_only_info: Transitional text + host-hardinfo DWORD. | lba8.usb_only_info: Strict legacy absent/zero slot. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA9

### 用途与 profile

- Absent EETU/zero state.
- Backing before EPPE payload.
- Backing outside SAPF payload.
- Backing outside long-User payload.
- Continuation begins Dept[59].
- Continuation begins Dept[60].
- EETU time/useCount structure plus reverse backing.
- EPPE magic.
- EPPE writer-owned zero tail.
- Long User continuation User[28..NUL].
- Minimum password length.
- No active upper-half payload.
- SAPF restore metadata.
- Unused continuation backing.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x07f | 128 | lba9.eetu | lba9_eetu / absent-zero | CompatibilityField | owner |
| 0x000–0x07f | 128 | lba9.eetu | lba9_eetu / eetu | PackedStruct | owner |
| 0x080–0x0ff | 128 | lba9.dept_continuation | dept_layout / join59 | CString | owner |
| 0x080–0x0ff | 128 | lba9.dept_continuation | dept_layout / join60 | CString | owner |
| 0x080–0x0ff | 128 | lba9.dept_continuation | dept_layout / short | UnownedBacking | owner |
| 0x100–0x17f | 128 | lba9.eppe_pre | lba9_overlay / eppe | UnownedBacking | owner |
| 0x100–0x11f | 32 | lba9.sapf | lba9_overlay / sapf | PackedStruct | owner |
| 0x100–0x1ff | 256 | lba9.upper_zero | lba9_overlay / zero | UnownedBacking | owner |
| 0x100–0x17f | 128 | lba9.user_continuation | lba9_overlay / long-user | CString | owner |
| 0x120–0x1ff | 224 | lba9.sapf_tail | lba9_overlay / sapf | UnownedBacking | owner |
| 0x180–0x183 | 4 | lba9.eppe_magic | lba9_overlay / eppe | Scalar | owner |
| 0x180–0x1ff | 128 | lba9.long_user_tail | lba9_overlay / long-user | UnownedBacking | owner |
| 0x184–0x187 | 4 | lba9.eppe_min_len | lba9_overlay / eppe | Scalar | owner |
| 0x188–0x1ff | 120 | lba9.eppe_zero_tail | lba9_overlay / eppe | CompatibilityField | owner |

### 逐字段规则与证据

#### lba9.eetu · lba9_eetu / absent-zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x07f（128B） |
| Semantic type | CompatibilityField |
| Meaning | Absent EETU/zero state. |
| Ownership | owner |
| Decode rule | No temp-use semantics without EETU magic. |
| Encode rule | Preserve absent state. |
| Profile | lba9_eetu / absent-zero |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Independent from upper overlays. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eetu · lba9_eetu / eetu

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x07f（128B） |
| Semantic type | PackedStruct |
| Meaning | EETU time/useCount structure plus reverse backing. |
| Ownership | owner |
| Decode rule | Validate EETU; consume bounds/useCount; preserve reverse backing. |
| Encode rule | Serialize controls; preserve backing; explicit tail stays zero. |
| Profile | lba9_eetu / eetu |
| Evolution kind | AddedRemoved |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Reverse bytes are backing, not hidden fields. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / join59

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0ff（128B） |
| Semantic type | CString |
| Meaning | Continuation begins Dept[59]. |
| Ownership | owner |
| Decode rule | Zero seam selects join59; overlay continuation at Dept[59]. |
| Encode rule | Write NUL seam and continuation from Dept[59]. |
| Profile | dept_layout / join59 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Post-NUL backing is not normalized. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0ff（128B） |
| Semantic type | CString |
| Meaning | Continuation begins Dept[60]. |
| Ownership | owner |
| Decode rule | Nonzero seam selects join60; append at Dept[60]. |
| Encode rule | Write Dept[59] inline and continuation from Dept[60]. |
| Profile | dept_layout / join60 |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Post-NUL backing is not normalized. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x0ff（128B） |
| Semantic type | UnownedBacking |
| Meaning | Unused continuation backing. |
| Ownership | owner |
| Decode rule | Decode Dept only from LBA6 C-string. |
| Encode rule | Write short Dept; preserve LBA9 backing. |
| Profile | dept_layout / short |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| Consumer evidence | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Post-NUL backing is not normalized. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_pre · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x17f（128B） |
| Semantic type | UnownedBacking |
| Meaning | Backing before EPPE payload. |
| Ownership | owner |
| Decode rule | Ignore semantic content. |
| Encode rule | Preserve backing. |
| Profile | lba9_overlay / eppe |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | EPPE starts at +0x180. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.sapf · lba9_overlay / sapf

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x11f（32B） |
| Semantic type | PackedStruct |
| Meaning | SAPF restore metadata. |
| Ownership | owner |
| Decode rule | Decode SAPF only through +0x11F. |
| Encode rule | Serialize SAPF restore fields. |
| Profile | lba9_overlay / sapf |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Profile avoids User/EPPE reinterpretation. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.upper_zero · lba9_overlay / zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x1ff（256B） |
| Semantic type | UnownedBacking |
| Meaning | No active upper-half payload. |
| Ownership | owner |
| Decode rule | Ignore/preserve backing. |
| Encode rule | Preserve bytes. |
| Profile | lba9_overlay / zero |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Explicit ownership state. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.user_continuation · lba9_overlay / long-user

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x17f（128B） |
| Semantic type | CString |
| Meaning | Long User continuation User[28..NUL]. |
| Ownership | owner |
| Decode rule | Join User continuation and ignore post-NUL backing. |
| Encode rule | Write remaining User bytes through NUL. |
| Profile | lba9_overlay / long-user |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Official virtual positive. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.sapf_tail · lba9_overlay / sapf

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x120–0x1ff（224B） |
| Semantic type | UnownedBacking |
| Meaning | Backing outside SAPF payload. |
| Ownership | owner |
| Decode rule | Ignore semantic content. |
| Encode rule | Preserve backing. |
| Profile | lba9_overlay / sapf |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | SAPF consumer stops at +0x11F. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_magic · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x180–0x183（4B） |
| Semantic type | Scalar |
| Meaning | EPPE magic. |
| Ownership | owner |
| Decode rule | Validate EPPE magic. |
| Encode rule | Write EPPE magic. |
| Profile | lba9_overlay / eppe |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.long_user_tail · lba9_overlay / long-user

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x180–0x1ff（128B） |
| Semantic type | UnownedBacking |
| Meaning | Backing outside long-User payload. |
| Ownership | owner |
| Decode rule | Ignore semantic content. |
| Encode rule | Preserve backing. |
| Profile | lba9_overlay / long-user |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT;V-LBA6 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Physical preserve state plus virtual active User. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_min_len · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x184–0x187（4B） |
| Semantic type | Scalar |
| Meaning | Minimum password length. |
| Ownership | owner |
| Decode rule | Decode LE DWORD. |
| Encode rule | Write validated 6..19 value. |
| Profile | lba9_overlay / eppe |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_zero_tail · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x188–0x1ff（120B） |
| Semantic type | CompatibilityField |
| Meaning | EPPE writer-owned zero tail. |
| Ownership | owner |
| Decode rule | No current semantic consumer. |
| Encode rule | Current producer initializes zero. |
| Profile | lba9_overlay / eppe |
| Evolution kind | OwnershipChanged |
| Producer evidence | S-WIN-CURRENT |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Canonical COMPLETE semantics; provenance limits remain documented. |
| Semantic status | COMPLETE |
| Implementation status | COMPLETE |
| Behavior-test status | COMPLETE |
| Code symbol | edpcli::protocol::lba9::parse_lba9 |
| Test symbol | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**dept_layout**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| short | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join59 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join60 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept seam is an independent serialization axis spanning LBA6 and LBA9. | S-JOIN59-SEMANTIC;P-GOLD-ENC |

| Region（含首尾） | short | join59 | join60 |
| --- | --- | --- | --- |
| 0x080–0x0ff | lba9.dept_continuation: Unused continuation backing. | lba9.dept_continuation: Continuation begins Dept[59]. | lba9.dept_continuation: Continuation begins Dept[60]. |

**lba9_eetu**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| absent-zero | owner | 9:000-07f | AddedRemoved | EETU is optional and independent from Dept/User upper overlays. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| eetu | owner | 9:000-07f | AddedRemoved | EETU is optional and independent from Dept/User upper overlays. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| Region（含首尾） | absent-zero | eetu |
| --- | --- | --- |
| 0x000–0x07f | lba9.eetu: Absent EETU/zero state. | lba9.eetu: EETU time/useCount structure plus reverse backing. |

**lba9_overlay**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 9:100-1ff | OwnershipChanged | Upper half is multiplexed among restore, User continuation, password policy, and backing. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| sapf | owner | 9:100-1ff | OwnershipChanged | Upper half is multiplexed among restore, User continuation, password policy, and backing. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| long-user | owner | 9:100-1ff | OwnershipChanged | Upper half is multiplexed among restore, User continuation, password policy, and backing. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| eppe | owner | 9:100-1ff | OwnershipChanged | Upper half is multiplexed among restore, User continuation, password policy, and backing. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |

| Region（含首尾） | zero | sapf | long-user | eppe |
| --- | --- | --- | --- | --- |
| 0x100–0x11f | lba9.upper_zero: No active upper-half payload. | lba9.sapf: SAPF restore metadata. | lba9.user_continuation: Long User continuation User[28..NUL]. | lba9.eppe_pre: Backing before EPPE payload. |
| 0x120–0x17f | lba9.upper_zero: No active upper-half payload. | lba9.sapf_tail: Backing outside SAPF payload. | lba9.user_continuation: Long User continuation User[28..NUL]. | lba9.eppe_pre: Backing before EPPE payload. |
| 0x180–0x183 | lba9.upper_zero: No active upper-half payload. | lba9.sapf_tail: Backing outside SAPF payload. | lba9.long_user_tail: Backing outside long-User payload. | lba9.eppe_magic: EPPE magic. |
| 0x184–0x187 | lba9.upper_zero: No active upper-half payload. | lba9.sapf_tail: Backing outside SAPF payload. | lba9.long_user_tail: Backing outside long-User payload. | lba9.eppe_min_len: Minimum password length. |
| 0x188–0x1ff | lba9.upper_zero: No active upper-half payload. | lba9.sapf_tail: Backing outside SAPF payload. | lba9.long_user_tail: Backing outside long-User payload. | lba9.eppe_zero_tail: EPPE writer-owned zero tail. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA10

### 用途与 profile

- Caller extension
- EESI absent-zero profile.
- EESI magic
- Preserve/ignore tail
- Type2 label
- Type4 label
- UsbSuspensionWnd flag

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba10.absent | lba10_eesi / absent-zero | CompatibilityField | owner |
| 0x000–0x003 | 4 | lba10.eesi_magic | lba10_eesi / eesi-enabled | Scalar | owner |
| 0x004–0x007 | 4 | lba10.suspension_flag | lba10_eesi / eesi-enabled | Scalar | owner |
| 0x008–0x017 | 16 | lba10.share_label | lba10_eesi / eesi-enabled | CString | owner |
| 0x018–0x027 | 16 | lba10.encrypt_label | lba10_eesi / eesi-enabled | CString | owner |
| 0x028–0x07f | 88 | lba10.extension | lba10_eesi / eesi-enabled | CompatibilityField | owner |
| 0x080–0x1ff | 384 | lba10.tail | lba10_eesi / eesi-enabled | UnownedBacking | owner |

### 逐字段规则与证据

#### lba10.absent · lba10_eesi / absent-zero

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x1ff（512B） |
| Semantic type | CompatibilityField |
| Meaning | EESI absent-zero profile. |
| Ownership | owner |
| Decode rule | Do not interpret EESI without magic. |
| Encode rule | Preserve absent profile. |
| Profile | lba10_eesi / absent-zero |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | General census covers absent state. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::absent |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.eesi_magic · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x003（4B） |
| Semantic type | Scalar |
| Meaning | EESI magic |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::eesi_magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.suspension_flag · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x004–0x007（4B） |
| Semantic type | Scalar |
| Meaning | UsbSuspensionWnd flag |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::suspension_flag |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.share_label · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x008–0x017（16B） |
| Semantic type | CString |
| Meaning | Type2 label |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::share_label |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.encrypt_label · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x018–0x027（16B） |
| Semantic type | CString |
| Meaning | Type4 label |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::encrypt_label |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.extension · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x028–0x07f（88B） |
| Semantic type | CompatibilityField |
| Meaning | Caller extension |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::extension |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.tail · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x080–0x1ff（384B） |
| Semantic type | UnownedBacking |
| Meaning | Preserve/ignore tail |
| Ownership | owner |
| Decode rule | Decrypt/interpret first 0x80; preserve tail. |
| Encode rule | SetEESI owns first 0x80 and preserves tail. |
| Profile | lba10_eesi / eesi-enabled |
| Evolution kind | AddedRemoved |
| Producer evidence | S-EESI-361018 |
| Consumer evidence | S-EESI-361018 |
| Physical evidence | P-EESI-NETAC |
| Implementation provenance | Authentic pre-write enabled capture. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba10::tail |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba10_eesi**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| absent-zero | owner | 10:000-1ff | AddedRemoved | EESI is optional; enabled payload owns only its defined prefix and preserves tail. | S-EESI-361018;P-EESI-NETAC;P-GOLD-ENC |
| eesi-enabled | owner | 10:000-1ff | AddedRemoved | EESI is optional; enabled payload owns only its defined prefix and preserves tail. | S-EESI-361018;P-EESI-NETAC;P-GOLD-ENC |

| Region（含首尾） | absent-zero | eesi-enabled |
| --- | --- | --- |
| 0x000–0x003 | lba10.absent: EESI absent-zero profile. | lba10.eesi_magic: EESI magic |
| 0x004–0x007 | lba10.absent: EESI absent-zero profile. | lba10.suspension_flag: UsbSuspensionWnd flag |
| 0x008–0x017 | lba10.absent: EESI absent-zero profile. | lba10.share_label: Type2 label |
| 0x018–0x027 | lba10.absent: EESI absent-zero profile. | lba10.encrypt_label: Type4 label |
| 0x028–0x07f | lba10.absent: EESI absent-zero profile. | lba10.extension: Caller extension |
| 0x080–0x1ff | lba10.absent: EESI absent-zero profile. | lba10.tail: Preserve/ignore tail |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA11

### 用途与 profile

- 252-byte random key material
- DRKB magic
- Encrypted UID + zero fill
- PDKB magic

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x003 | 4 | lba11.drkb_magic | lba11_capacity / disk-size | Scalar | owner |
| 0x000–0x003 | 4 | lba11.drkb_magic | lba11_capacity / repair-chs | Scalar | owner |
| 0x004–0x0ff | 252 | lba11.random252 | lba11_capacity / disk-size | EncryptedRegion | owner |
| 0x004–0x0ff | 252 | lba11.random252 | lba11_capacity / repair-chs | EncryptedRegion | owner |
| 0x100–0x103 | 4 | lba11.pdkb_magic | lba11_capacity / disk-size | Scalar | owner |
| 0x100–0x103 | 4 | lba11.pdkb_magic | lba11_capacity / repair-chs | Scalar | owner |
| 0x104–0x1ff | 252 | lba11.uid_payload | lba11_capacity / disk-size | EncryptedRegion | owner |
| 0x104–0x1ff | 252 | lba11.uid_payload | lba11_capacity / repair-chs | EncryptedRegion | owner |

### 逐字段规则与证据

#### lba11.drkb_magic · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x003（4B） |
| Semantic type | Scalar |
| Meaning | DRKB magic |
| Ownership | owner |
| Decode rule | Derive crypto key using DISK_GEOMETRY_EX.DiskSize; decode canonical field. |
| Encode rule | Build sector using DISK_GEOMETRY_EX.DiskSize. |
| Profile | lba11_capacity / disk-size |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::drkb_magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.drkb_magic · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x003（4B） |
| Semantic type | Scalar |
| Meaning | DRKB magic |
| Ownership | owner |
| Decode rule | Derive crypto key using CHS capacity product; decode canonical field. |
| Encode rule | Build sector using CHS capacity product. |
| Profile | lba11_capacity / repair-chs |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::drkb_magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.random252 · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x004–0x0ff（252B） |
| Semantic type | EncryptedRegion |
| Meaning | 252-byte random key material |
| Ownership | owner |
| Decode rule | Derive crypto key using DISK_GEOMETRY_EX.DiskSize; decode canonical field. |
| Encode rule | Build sector using DISK_GEOMETRY_EX.DiskSize. |
| Profile | lba11_capacity / disk-size |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::random252 |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.random252 · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x004–0x0ff（252B） |
| Semantic type | EncryptedRegion |
| Meaning | 252-byte random key material |
| Ownership | owner |
| Decode rule | Derive crypto key using CHS capacity product; decode canonical field. |
| Encode rule | Build sector using CHS capacity product. |
| Profile | lba11_capacity / repair-chs |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::random252 |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.pdkb_magic · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x103（4B） |
| Semantic type | Scalar |
| Meaning | PDKB magic |
| Ownership | owner |
| Decode rule | Derive crypto key using DISK_GEOMETRY_EX.DiskSize; decode canonical field. |
| Encode rule | Build sector using DISK_GEOMETRY_EX.DiskSize. |
| Profile | lba11_capacity / disk-size |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::pdkb_magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.pdkb_magic · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x100–0x103（4B） |
| Semantic type | Scalar |
| Meaning | PDKB magic |
| Ownership | owner |
| Decode rule | Derive crypto key using CHS capacity product; decode canonical field. |
| Encode rule | Build sector using CHS capacity product. |
| Profile | lba11_capacity / repair-chs |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::pdkb_magic |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.uid_payload · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x104–0x1ff（252B） |
| Semantic type | EncryptedRegion |
| Meaning | Encrypted UID + zero fill |
| Ownership | owner |
| Decode rule | Derive crypto key using DISK_GEOMETRY_EX.DiskSize; decode canonical field. |
| Encode rule | Build sector using DISK_GEOMETRY_EX.DiskSize. |
| Profile | lba11_capacity / disk-size |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::uid_payload |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.uid_payload · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x104–0x1ff（252B） |
| Semantic type | EncryptedRegion |
| Meaning | Encrypted UID + zero fill |
| Ownership | owner |
| Decode rule | Derive crypto key using CHS capacity product; decode canonical field. |
| Encode rule | Build sector using CHS capacity product. |
| Profile | lba11_capacity / repair-chs |
| Evolution kind | ProducerChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC |
| Implementation provenance | Writer path selects capacity algorithm. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba11::uid_payload |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba11_capacity**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| disk-size | owner | 11:000-1ff | ProducerChanged | LBA11 capacity key input differs by writer path, not hardware identity. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| repair-chs | owner | 11:000-1ff | ProducerChanged | LBA11 capacity key input differs by writer path, not hardware identity. | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| Region（含首尾） | disk-size | repair-chs |
| --- | --- | --- |
| 0x000–0x003 | lba11.drkb_magic: DRKB magic | lba11.drkb_magic: DRKB magic |
| 0x004–0x0ff | lba11.random252: 252-byte random key material | lba11.random252: 252-byte random key material |
| 0x100–0x103 | lba11.pdkb_magic: PDKB magic | lba11.pdkb_magic: PDKB magic |
| 0x104–0x1ff | lba11.uid_payload: Encrypted UID + zero fill | lba11.uid_payload: Encrypted UID + zero fill |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。

## LBA12

### 用途与 profile

- 14-byte pass-info.
- 3x96B EDPF table; key mode A7F0/A6B0.
- 3x96B EDPF table; key mode AES-128-ECB.
- 3x96B EDPF table; key mode SM4-ECB.
- 3x96B EDPF table; key mode legacy-v0064 compatibility.
- Post-table zero plaintext inside cipher stream.

### 512B 布局与字段索引

每行标出范围和 profile；overlay 行是对应 owner 范围的注解。

| Offset（含首尾） | Length | Field ID | Axis / state | Type | Ownership |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x11f | 288 | lba12.edpf_table | lba12_mode / legacy-v0064 | EncryptedRegion | owner |
| 0x000–0x11f | 288 | lba12.edpf_table | lba12_mode / mode1 | EncryptedRegion | owner |
| 0x000–0x11f | 288 | lba12.edpf_table | lba12_mode / mode2 | EncryptedRegion | owner |
| 0x000–0x11f | 288 | lba12.edpf_table | lba12_mode / mode3 | EncryptedRegion | owner |
| 0x120–0x12d | 14 | lba12.pass_info | lba12_mode / legacy-v0064 | PackedStruct | owner |
| 0x120–0x12d | 14 | lba12.pass_info | lba12_mode / mode1 | PackedStruct | owner |
| 0x120–0x12d | 14 | lba12.pass_info | lba12_mode / mode2 | PackedStruct | owner |
| 0x120–0x12d | 14 | lba12.pass_info | lba12_mode / mode3 | PackedStruct | owner |
| 0x12e–0x1ff | 210 | lba12.zero_padding | lba12_mode / legacy-v0064 | CompatibilityField | owner |
| 0x12e–0x1ff | 210 | lba12.zero_padding | lba12_mode / mode1 | CompatibilityField | owner |
| 0x12e–0x1ff | 210 | lba12.zero_padding | lba12_mode / mode2 | CompatibilityField | owner |
| 0x12e–0x1ff | 210 | lba12.zero_padding | lba12_mode / mode3 | CompatibilityField | owner |

### 逐字段规则与证据

#### lba12.edpf_table · lba12_mode / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x11f（288B） |
| Semantic type | EncryptedRegion |
| Meaning | 3x96B EDPF table; key mode legacy-v0064 compatibility. |
| Ownership | owner |
| Decode rule | Decrypt table under legacy-v0064. |
| Encode rule | Serialize table under legacy-v0064. |
| Profile | lba12_mode / legacy-v0064 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Virtual mode1/mode3 remains explicitly virtual. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::edpf_table |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x11f（288B） |
| Semantic type | EncryptedRegion |
| Meaning | 3x96B EDPF table; key mode A7F0/A6B0. |
| Ownership | owner |
| Decode rule | Decrypt table under mode1. |
| Encode rule | Serialize table under mode1. |
| Profile | lba12_mode / mode1 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Virtual mode1/mode3 remains explicitly virtual. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::edpf_table |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x11f（288B） |
| Semantic type | EncryptedRegion |
| Meaning | 3x96B EDPF table; key mode SM4-ECB. |
| Ownership | owner |
| Decode rule | Decrypt table under mode2. |
| Encode rule | Serialize table under mode2. |
| Profile | lba12_mode / mode2 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Virtual mode1/mode3 remains explicitly virtual. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::edpf_table |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x000–0x11f（288B） |
| Semantic type | EncryptedRegion |
| Meaning | 3x96B EDPF table; key mode AES-128-ECB. |
| Ownership | owner |
| Decode rule | Decrypt table under mode3. |
| Encode rule | Serialize table under mode3. |
| Profile | lba12_mode / mode3 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Virtual mode1/mode3 remains explicitly virtual. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::edpf_table |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x120–0x12d（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte pass-info. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields. |
| Encode rule | Serialize selected mode/version pass-info. |
| Profile | lba12_mode / legacy-v0064 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Pass-info is inside continuous sector encryption. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x120–0x12d（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte pass-info. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields. |
| Encode rule | Serialize selected mode/version pass-info. |
| Profile | lba12_mode / mode1 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Pass-info is inside continuous sector encryption. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x120–0x12d（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte pass-info. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields. |
| Encode rule | Serialize selected mode/version pass-info. |
| Profile | lba12_mode / mode2 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Pass-info is inside continuous sector encryption. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x120–0x12d（14B） |
| Semantic type | PackedStruct |
| Meaning | 14-byte pass-info. |
| Ownership | owner |
| Decode rule | Decode named pass-info fields. |
| Encode rule | Serialize selected mode/version pass-info. |
| Profile | lba12_mode / mode3 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Pass-info is inside continuous sector encryption. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::pass_info |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x12e–0x1ff（210B） |
| Semantic type | CompatibilityField |
| Meaning | Post-table zero plaintext inside cipher stream. |
| Ownership | owner |
| Decode rule | Decrypt then require writer-zero compatibility region. |
| Encode rule | Zero plaintext before whole-sector encryption. |
| Profile | lba12_mode / legacy-v0064 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Tail is encrypted, never raw. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::zero_padding |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x12e–0x1ff（210B） |
| Semantic type | CompatibilityField |
| Meaning | Post-table zero plaintext inside cipher stream. |
| Ownership | owner |
| Decode rule | Decrypt then require writer-zero compatibility region. |
| Encode rule | Zero plaintext before whole-sector encryption. |
| Profile | lba12_mode / mode1 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Tail is encrypted, never raw. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::zero_padding |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x12e–0x1ff（210B） |
| Semantic type | CompatibilityField |
| Meaning | Post-table zero plaintext inside cipher stream. |
| Ownership | owner |
| Decode rule | Decrypt then require writer-zero compatibility region. |
| Encode rule | Zero plaintext before whole-sector encryption. |
| Profile | lba12_mode / mode2 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | P-GOLD-ENC;P-GOLD-NOPWD |
| Implementation provenance | Tail is encrypted, never raw. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::zero_padding |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| Offset | 0x12e–0x1ff（210B） |
| Semantic type | CompatibilityField |
| Meaning | Post-table zero plaintext inside cipher stream. |
| Ownership | owner |
| Decode rule | Decrypt then require writer-zero compatibility region. |
| Encode rule | Zero plaintext before whole-sector encryption. |
| Profile | lba12_mode / mode3 |
| Evolution kind | EncodingChanged |
| Producer evidence | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| Consumer evidence | S-WIN-CURRENT;S-LINUX-DWARF |
| Physical evidence | MISSING_PHYSICAL |
| Implementation provenance | Tail is encrypted, never raw. |
| Semantic status | COMPLETE |
| Implementation status | PLANNED |
| Behavior-test status | PLANNED |
| Code symbol | planned:protocol::lba12::zero_padding |
| Test symbol | UNIMPLEMENTED |
| Ownership test | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进 matrix

**lba12_mode**

| State | Role | 完整 axis 范围（可跨 LBA） | Evolution | 差异说明 | Evidence |
| --- | --- | --- | --- | --- | --- |
| legacy-v0064 | owner | 12:000-1ff | EncodingChanged | LBA12 wrapped-key mode is independent from all other profile axes. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12;P-GOLD-ENC |
| mode1 | owner | 12:000-1ff | EncodingChanged | LBA12 wrapped-key mode is independent from all other profile axes. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12;P-GOLD-ENC |
| mode2 | owner | 12:000-1ff | EncodingChanged | LBA12 wrapped-key mode is independent from all other profile axes. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12;P-GOLD-ENC |
| mode3 | owner | 12:000-1ff | EncodingChanged | LBA12 wrapped-key mode is independent from all other profile axes. | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12;P-GOLD-ENC |

| Region（含首尾） | legacy-v0064 | mode1 | mode2 | mode3 |
| --- | --- | --- | --- | --- |
| 0x000–0x11f | lba12.edpf_table: 3x96B EDPF table; key mode legacy-v0064 compatibility. | lba12.edpf_table: 3x96B EDPF table; key mode A7F0/A6B0. | lba12.edpf_table: 3x96B EDPF table; key mode SM4-ECB. | lba12.edpf_table: 3x96B EDPF table; key mode AES-128-ECB. |
| 0x120–0x12d | lba12.pass_info: 14-byte pass-info. | lba12.pass_info: 14-byte pass-info. | lba12.pass_info: 14-byte pass-info. | lba12.pass_info: 14-byte pass-info. |
| 0x12e–0x1ff | lba12.zero_padding: Post-table zero plaintext inside cipher stream. | lba12.zero_padding: Post-table zero plaintext inside cipher stream. | lba12.zero_padding: Post-table zero plaintext inside cipher stream. | lba12.zero_padding: Post-table zero plaintext inside cipher stream. |

### 实现与测试入口

正式 parser 与行为测试以本节各字段的 code/test symbol 及状态为准；未实现链接不代表可调用 API。profile detector 尚未在目录中登记。

- Ownership：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../tests/protocol_field_catalog.rs)）。
- Fixture 定位：通过各行 physical/producer/consumer evidence ID 查询 [证据清单](../audit/protocol/evidence_manifest.tsv)，保留其 modality 和 limitations。
