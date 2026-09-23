# EDP LBA0–LBA12 字段手册

> 自动生成：请修改标准 TSV 后重新生成，勿直接编辑本文件。

事实源：[field_catalog.tsv](../../audit/protocol/field_catalog.tsv)、[profile_axes.tsv](../../audit/protocol/profile_axes.tsv)。
证据 ID 的类型、定位与限制见 [evidence_manifest.tsv](../../audit/protocol/evidence_manifest.tsv)。
生成器：[generate_field_guide.rs](../../scripts/protocol/generate_field_guide.rs)；
运行 `rustc --edition=2021 scripts/protocol/generate_field_guide.rs -o target/generate-field-guide`，
再运行 `target/generate-field-guide`；加 `--check` 只校验，不写文件。

## 阅读约定

“偏移”为扇区内十六进制位置，区间含首尾；“长度”为字节数。每扇区 512B，整段 6656B。
同一范围可因配置类型有多行：`base/all` 与各所有权轴选中的一个状态共同覆盖盘面；
覆盖项只补充独立的来源或取值差异，不重复声明字节所有权。不同轴独立组合，不能按软件年代整体绑定。

`semantic_status` 记录语义闭环；`implementation_status` 与 `behavior_test_status` 分别记录正式代码和行为测试。
`planned:` 是设计目标，`UNIMPLEMENTED` 表示尚无正式链接。通用所有权测试不代表字段行为测试已经完成。
`MISSING_PHYSICAL` 表示缺物理证据引用，不降低语义状态；写入端/消费端中的虚拟证据不能作为物理采集。

`preserve` 表示按该字段编码规则保留原字节，不能擅自清零；`opaque` 表示在 EDP 层不解释内部负载；
`backing` 表示当前字段语义不消费的存储内容，不能仅因样本为零就认定为必须为零的常量。
具体写入、加密表示和 NUL 后行为以逐字段规则为准；这些区域仍有正式所有权。
`PackedStruct`/`EncryptedRegion` 行按目录现有粒度展示，不补造目录未列出的内部字段或算法。

目录包含 142 个字段 × 配置类型行、18 个正交轴、44 个状态。

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

### 用途与配置类型

- 跨 配置类型 原样保留的兼容性底层字节。
- 显式全零的缺失 引导代码。
- 四个标准 MBR 分区条目。
- 旧版 MBR 错误消息指针字节。
- MBR 55AA 签名。
- Netac 内嵌 MBR 引导代码。
- 官方 UsbMainBSec MBR 引导代码。
- 可选 SectorSize 之后原样保留的兼容性底层字节。
- SectorSize 覆盖项 缺失。
- SectorSize 覆盖项 保存小端 512。
- 标准 MBR 磁盘签名。
- 标准 MBR 保留字。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x18f（400B） |
| 语义类型 | PackedStruct |
| 含义 | Netac 内嵌 MBR 引导代码。 |
| 所有权 | owner |
| 解码规则 | 选择该 配置类型 级 引导代码 镜像。 |
| 编码规则 | 写入精确的一方镜像或显式全零。 |
| 配置类型 | lba0_bootstrap / netac-mbr |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-NETAC-MBR |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 选择器来源与 盘面 闭环分开。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.bootstrap · lba0_bootstrap / usb-main-bsec

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x18f（400B） |
| 语义类型 | PackedStruct |
| 含义 | 官方 UsbMainBSec MBR 引导代码。 |
| 所有权 | owner |
| 解码规则 | 选择该 配置类型 级 引导代码 镜像。 |
| 编码规则 | 写入精确的一方镜像或显式全零。 |
| 配置类型 | lba0_bootstrap / usb-main-bsec |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 选择器来源与 盘面 闭环分开。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.bootstrap · lba0_bootstrap / zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x18f（400B） |
| 语义类型 | CompatibilityField |
| 含义 | 显式全零的缺失 引导代码。 |
| 所有权 | owner |
| 解码规则 | 选择该 配置类型 级 引导代码 镜像。 |
| 编码规则 | 写入精确的一方镜像或显式全零。 |
| 配置类型 | lba0_bootstrap / zero |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 选择器来源与 盘面 闭环分开。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.compat_190_19f · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x190–0x19f（16B） |
| 语义类型 | UnownedBacking |
| 含义 | 跨 配置类型 原样保留的兼容性底层字节。 |
| 所有权 | owner |
| 解码规则 | 没有 EDP 负载语义。 |
| 编码规则 | 原样保留字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-NETAC-MBR |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.sector_size_overlay · lba0_sector_size_overlay / absent

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1a0–0x1a3（4B） |
| 语义类型 | Scalar |
| 含义 | SectorSize 覆盖项 缺失。 |
| 所有权 | owner |
| 解码规则 | 解码小端 DWORD；0 表示缺失，512 表示已填充。 |
| 编码规则 | 保留缺失状态或写入 512。 |
| 配置类型 | lba0_sector_size_overlay / absent |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 与 引导代码 家族相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.sector_size_overlay · lba0_sector_size_overlay / sector-size-512

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1a0–0x1a3（4B） |
| 语义类型 | Scalar |
| 含义 | SectorSize 覆盖项 保存小端 512。 |
| 所有权 | owner |
| 解码规则 | 解码小端 DWORD；0 表示缺失，512 表示已填充。 |
| 编码规则 | 保留缺失状态或写入 512。 |
| 配置类型 | lba0_sector_size_overlay / sector-size-512 |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 与 引导代码 家族相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.compat_1a4_1b4 · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1a4–0x1b4（17B） |
| 语义类型 | UnownedBacking |
| 含义 | 可选 SectorSize 之后原样保留的兼容性底层字节。 |
| 所有权 | owner |
| 解码规则 | 没有 EDP 负载语义。 |
| 编码规则 | 原样保留字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-NETAC-MBR |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.legacy_message_ptrs · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1b5–0x1b7（3B） |
| 语义类型 | CompatibilityField |
| 含义 | 旧版 MBR 错误消息指针字节。 |
| 所有权 | owner |
| 解码规则 | 仅在 UsbMainBSec 引导代码 下解释。 |
| 编码规则 | 保持模板字节或全零状态。 |
| 配置类型 | base / all |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.mbr_disk_signature · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1b8–0x1bb（4B） |
| 语义类型 | Scalar |
| 含义 | 标准 MBR 磁盘签名。 |
| 所有权 | owner |
| 解码规则 | 解码小端 u32。 |
| 编码规则 | 保留标准签名。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.mbr_reserved · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1bc–0x1bd（2B） |
| 语义类型 | UnownedBacking |
| 含义 | 标准 MBR 保留字。 |
| 所有权 | owner |
| 解码规则 | 没有 EDP 业务含义。 |
| 编码规则 | 原样保留字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-NETAC-MBR |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.partition_table · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1be–0x1fd（64B） |
| 语义类型 | PackedStruct |
| 含义 | 四个标准 MBR 分区条目。 |
| 所有权 | owner |
| 解码规则 | 解码四个 16 字节条目。 |
| 编码规则 | 序列化/恢复 MBR 条目。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-NETAC-MBR |
| 消费端证据 | S-REPAIR-CURRENT |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba0.signature_55aa · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1fe–0x1ff（2B） |
| 语义类型 | Scalar |
| 含义 | MBR 55AA 签名。 |
| 所有权 | owner |
| 解码规则 | 校验 55 AA。 |
| 编码规则 | 写入 55 AA。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-NETAC-MBR |
| 消费端证据 | S-REPAIR-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba0::parse_lba0 |
| 测试符号 | basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba0_bootstrap**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 0:000-18f | ProducerChanged | LBA0 引导代码 家族；上层选择器来源与 盘面 语义分开。 | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |
| usb-main-bsec | owner | 0:000-18f | ProducerChanged | LBA0 引导代码 家族；上层选择器来源与 盘面 语义分开。 | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |
| netac-mbr | owner | 0:000-18f | ProducerChanged | LBA0 引导代码 家族；上层选择器来源与 盘面 语义分开。 | S-WIN-CURRENT;S-NETAC-MBR;P-GOLD-ENC |

| 区域（含首尾） | zero | usb-main-bsec | netac-mbr |
| --- | --- | --- | --- |
| 0x000–0x18f | lba0.bootstrap: 显式全零的缺失 引导代码。 | lba0.bootstrap: 官方 UsbMainBSec MBR 引导代码。 | lba0.bootstrap: Netac 内嵌 MBR 引导代码。 |

**lba0_sector_size_overlay**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 0:1a0-1a3 | AddedRemoved | 可选 SAFE1/旧版 SectorSize 覆盖项 与 引导代码 家族相互独立。 | S-WIN-CURRENT;P-GOLD-ENC |
| sector-size-512 | owner | 0:1a0-1a3 | AddedRemoved | 可选 SAFE1/旧版 SectorSize 覆盖项 与 引导代码 家族相互独立。 | S-WIN-CURRENT;P-GOLD-ENC |

| 区域（含首尾） | absent | sector-size-512 |
| --- | --- | --- |
| 0x1a0–0x1a3 | lba0.sector_size_overlay: SectorSize 覆盖项 缺失。 | lba0.sector_size_overlay: SectorSize 覆盖项 保存小端 512。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA1

### 用途与配置类型

- GPT 主头扇区缺失状态。
- GPT 主头及由模板负责的剩余区域。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba1.gpt_primary | gpt_layout / absent | CompatibilityField | owner |
| 0x000–0x1ff | 512 | lba1.gpt_primary | gpt_layout / enabled | PackedStruct | owner |

### 逐字段规则与证据

#### lba1.gpt_primary · gpt_layout / absent

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | CompatibilityField |
| 含义 | GPT 主头扇区缺失状态。 |
| 所有权 | owner |
| 解码规则 | 没有 GPT 负载。 |
| 编码规则 | 保留缺失全零状态。 |
| 配置类型 | gpt_layout / absent |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 物理样本集覆盖缺失 配置类型。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba1::parse_lba1 |
| 测试符号 | gpt_behavior::gpt_absent_is_explicit_and_unknown_never_defaults_to_absent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba1.gpt_primary · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | PackedStruct |
| 含义 | GPT 主头及由模板负责的剩余区域。 |
| 所有权 | owner |
| 解码规则 | 解码 EFI PART 头、LBA、GUID 和 CRC。 |
| 编码规则 | 构造官方 GPT 头和 CRC。 |
| 配置类型 | gpt_layout / enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | V-GPT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 官方虚拟正例，不是物理证据。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba1::parse_lba1 |
| 测试符号 | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**gpt_layout**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 是否存在与 SAFE6 元数据 配置类型 相互独立。 | V-GPT;P-GOLD-ENC |
| enabled | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 是否存在与 SAFE6 元数据 配置类型 相互独立。 | V-GPT;P-GOLD-ENC |

| 区域（含首尾） | absent | enabled |
| --- | --- | --- |
| 0x000–0x1ff | lba1.gpt_primary: GPT 主头扇区缺失状态。 | lba1.gpt_primary: GPT 主头及由模板负责的剩余区域。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA2

### 用途与配置类型

- GPT 条目扇区缺失状态。
- 有效的 GPT entry0。
- 未使用的 GPT entries1..3。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba2.gpt_entries | gpt_layout / absent | CompatibilityField | owner |
| 0x000–0x07f | 128 | lba2.gpt_entry0 | gpt_layout / enabled | PackedStruct | owner |
| 0x080–0x1ff | 384 | lba2.gpt_unused | gpt_layout / enabled | UnownedBacking | owner |

### 逐字段规则与证据

#### lba2.gpt_entries · gpt_layout / absent

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | CompatibilityField |
| 含义 | GPT 条目扇区缺失状态。 |
| 所有权 | owner |
| 解码规则 | 没有 GPT 条目。 |
| 编码规则 | 保留缺失全零状态。 |
| 配置类型 | gpt_layout / absent |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 物理样本集覆盖缺失 配置类型。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba2::parse_lba2 |
| 测试符号 | gpt_behavior::gpt_absent_is_explicit_and_unknown_never_defaults_to_absent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba2.gpt_entry0 · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x07f（128B） |
| 语义类型 | PackedStruct |
| 含义 | 有效的 GPT entry0。 |
| 所有权 | owner |
| 解码规则 | 解码 128 字节条目。 |
| 编码规则 | 序列化官方 entry0。 |
| 配置类型 | gpt_layout / enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | V-GPT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 官方虚拟正例。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba2::parse_lba2 |
| 测试符号 | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba2.gpt_unused · gpt_layout / enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x1ff（384B） |
| 语义类型 | UnownedBacking |
| 含义 | 未使用的 GPT entries1..3。 |
| 所有权 | owner |
| 解码规则 | TypeGUID 为零时忽略剩余内容。 |
| 编码规则 | 保持未使用条目无所有者。 |
| 配置类型 | gpt_layout / enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | V-GPT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | TypeGUID 为零后，剩余内容在语义上忽略。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba2::parse_lba2 |
| 测试符号 | gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**gpt_layout**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| absent | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 是否存在与 SAFE6 元数据 配置类型 相互独立。 | V-GPT;P-GOLD-ENC |
| enabled | owner | 1:000-1ff;2:000-1ff | AddedRemoved | GPT LBA1/LBA2 是否存在与 SAFE6 元数据 配置类型 相互独立。 | V-GPT;P-GOLD-ENC |

| 区域（含首尾） | absent | enabled |
| --- | --- | --- |
| 0x000–0x07f | lba2.gpt_entries: GPT 条目扇区缺失状态。 | lba2.gpt_entry0: 有效的 GPT entry0。 |
| 0x080–0x1ff | lba2.gpt_entries: GPT 条目扇区缺失状态。 | lba2.gpt_unused: 未使用的 GPT entries1..3。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA3

### 用途与配置类型

- 全零的制造商元数据。
- 独立的历史 MP 配置类型 B。
- 制造商 MP 配置类型 A。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / historical-mp-b | OpaquePreserve | owner |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / kingston-mp-a | OpaquePreserve | owner |
| 0x000–0x1ff | 512 | lba3.manufacturer_metadata | lba3_metadata / zero | OpaquePreserve | owner |

### 逐字段规则与证据

#### lba3.manufacturer_metadata · lba3_metadata / historical-mp-b

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | OpaquePreserve |
| 含义 | 独立的历史 MP 配置类型 B。 |
| 所有权 | owner |
| 解码规则 | EDP 不解码该负载。 |
| 编码规则 | 原样保留完整扇区；禁止归一化。 |
| 配置类型 | lba3_metadata / historical-mp-b |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| 消费端证据 | S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 制造商序列化器位于 EDP 边界之外。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba3::parse_lba3 |
| 测试符号 | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba3.manufacturer_metadata · lba3_metadata / kingston-mp-a

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | OpaquePreserve |
| 含义 | 制造商 MP 配置类型 A。 |
| 所有权 | owner |
| 解码规则 | EDP 不解码该负载。 |
| 编码规则 | 原样保留完整扇区；禁止归一化。 |
| 配置类型 | lba3_metadata / kingston-mp-a |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| 消费端证据 | S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 制造商序列化器位于 EDP 边界之外。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba3::parse_lba3 |
| 测试符号 | basic_behavior::basic_parsers_replay_all_committed_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba3.manufacturer_metadata · lba3_metadata / zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | OpaquePreserve |
| 含义 | 全零的制造商元数据。 |
| 所有权 | owner |
| 解码规则 | EDP 不解码该负载。 |
| 编码规则 | 原样保留完整扇区；禁止归一化。 |
| 配置类型 | lba3_metadata / zero |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141-LBA3 |
| 消费端证据 | S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 制造商序列化器位于 EDP 边界之外。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba3::parse_lba3 |
| 测试符号 | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba3_metadata**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 3:000-1ff | ProducerChanged | 制造商 MP 元数据状态共享 EDP 仅保留生命周期。 | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |
| kingston-mp-a | owner | 3:000-1ff | ProducerChanged | 制造商 MP 元数据状态共享 EDP 仅保留生命周期。 | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |
| historical-mp-b | owner | 3:000-1ff | ProducerChanged | 制造商 MP 元数据状态共享 EDP 仅保留生命周期。 | S-WIN-CURRENT;S-WIN-191141-LBA3;P-GOLD-ENC |

| 区域（含首尾） | zero | kingston-mp-a | historical-mp-b |
| --- | --- | --- | --- |
| 0x000–0x1ff | lba3.manufacturer_metadata: 全零的制造商元数据。 | lba3.manufacturer_metadata: 制造商 MP 配置类型 A。 | lba3.manufacturer_metadata: 独立的历史 MP 配置类型 B。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA4

### 用途与配置类型

- 明文 $$$onlyid$$$ 头。
- 当前 HSerial 向量为全零。
- 当前 host-hardinfo 为全零。
- HSerialCRC[5] 向量
- 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。
- 旧版 ABI 携带调用方提供的 HSerial[5]。
- MyHardinfo 镜像
- NewLabFlag LLGB
- OnllyID2Nd 激活密钥种子
- OnlyIdXor8 校验值
- 表示载体/保留底层字节
- 第二密钥来自独立 GUID CRC。
- 第二密钥复用 main onlyid。
- 扇区元组 08 04 0C 01
- SingleUsbFlg
- 尾部 LLGB
- 版本 1
- bConnetServer
- bDataToServer

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x017（24B） |
| 语义类型 | CString |
| 含义 | 明文 $$$onlyid$$$ 头。 |
| 所有权 | owner |
| 解码规则 | 解析 main onlyid。 |
| 编码规则 | 格式化标准头。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.onlyid_xor8 · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x018–0x01b（4B） |
| 语义类型 | Scalar |
| 含义 | OnlyIdXor8 校验值 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.onlyid_xor8 · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x018–0x01b（4B） |
| 语义类型 | Scalar |
| 含义 | OnlyIdXor8 校验值 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.second_key_source · lba4_second_key_source / current-main-onlyid

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01c–0x01f（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 第二密钥复用 main onlyid。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；所有者为 lba4_编码。 |
| 编码规则 | 仅写入端来源发生变化。 |
| 配置类型 | lba4_second_key_source / current-main-onlyid |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 可与任一编码方式组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.second_key_source · lba4_second_key_source / legacy-guid-crc

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01c–0x01f（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 第二密钥来自独立 GUID CRC。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；所有者为 lba4_编码。 |
| 编码规则 | 仅写入端来源发生变化。 |
| 配置类型 | lba4_second_key_source / legacy-guid-crc |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 可与任一编码方式组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.second_key · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01c–0x01f（4B） |
| 语义类型 | Scalar |
| 含义 | OnllyID2Nd 激活密钥种子 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.second_key · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01c–0x01f（4B） |
| 语义类型 | Scalar |
| 含义 | OnllyID2Nd 激活密钥种子 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.hserial_source · lba4_hserial_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x020–0x033（20B） |
| 语义类型 | CompatibilityField |
| 含义 | 当前 HSerial 向量为全零。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；所有者为 lba4_编码。 |
| 编码规则 | 全零/调用方写入模式独立变化。 |
| 配置类型 | lba4_hserial_source / current-zero |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-191141;S-BUS-2020 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 不臆造调用方的值生成算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.lba4.hserial_source · lba4_hserial_source / legacy-caller-vector

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x020–0x033（20B） |
| 语义类型 | CompatibilityField |
| 含义 | 旧版 ABI 携带调用方提供的 HSerial[5]。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；所有者为 lba4_编码。 |
| 编码规则 | 全零/调用方写入模式独立变化。 |
| 配置类型 | lba4_hserial_source / legacy-caller-vector |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-191141;S-BUS-2020 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 不臆造调用方的值生成算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.hserial · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x020–0x033（20B） |
| 语义类型 | CompatibilityField |
| 含义 | HSerialCRC[5] 向量 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.hserial · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x020–0x033（20B） |
| 语义类型 | CompatibilityField |
| 含义 | HSerialCRC[5] 向量 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.single_usb · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x034–0x034（1B） |
| 语义类型 | CompatibilityField |
| 含义 | SingleUsbFlg |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.single_usb · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x034–0x034（1B） |
| 语义类型 | CompatibilityField |
| 含义 | SingleUsbFlg |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba4 · host_hardinfo_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x035–0x038（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 当前 host-hardinfo 为全零。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；语义所有者为 base/LBA4 编码。 |
| 编码规则 | 写入端轴与 LBA4 编码、UsbOnlyInfo 相互独立。 |
| 配置类型 | host_hardinfo_source / current-zero |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 跨 LBA 镜像关系相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba4 · host_hardinfo_source / legacy-host-identity

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x035–0x038（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；语义所有者为 base/LBA4 编码。 |
| 编码规则 | 写入端轴与 LBA4 编码、UsbOnlyInfo 相互独立。 |
| 配置类型 | host_hardinfo_source / legacy-host-identity |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 跨 LBA 镜像关系相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.my_hardinfo · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x035–0x038（4B） |
| 语义类型 | CompatibilityField |
| 含义 | MyHardinfo 镜像 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.my_hardinfo · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x035–0x038（4B） |
| 语义类型 | CompatibilityField |
| 含义 | MyHardinfo 镜像 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.new_lab_flag · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x039–0x03c（4B） |
| 语义类型 | Scalar |
| 含义 | NewLabFlag LLGB |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.new_lab_flag · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x039–0x03c（4B） |
| 语义类型 | Scalar |
| 含义 | NewLabFlag LLGB |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.version · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03d–0x040（4B） |
| 语义类型 | Scalar |
| 含义 | 版本 1 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.version · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03d–0x040（4B） |
| 语义类型 | Scalar |
| 含义 | 版本 1 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.sector_tuple · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x041–0x044（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 扇区元组 08 04 0C 01 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.sector_tuple · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x041–0x044（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 扇区元组 08 04 0C 01 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.data_to_server · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x045–0x045（1B） |
| 语义类型 | CompatibilityField |
| 含义 | bDataToServer |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.data_to_server · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x045–0x045（1B） |
| 语义类型 | CompatibilityField |
| 含义 | bDataToServer |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.connect_server · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x046–0x046（1B） |
| 语义类型 | CompatibilityField |
| 含义 | bConnetServer |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.connect_server · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x046–0x046（1B） |
| 语义类型 | CompatibilityField |
| 含义 | bConnetServer |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.representation_backing · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x047–0x1fb（437B） |
| 语义类型 | UnownedBacking |
| 含义 | 表示载体/保留底层字节 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.representation_backing · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x047–0x1fb（437B） |
| 语义类型 | UnownedBacking |
| 含义 | 表示载体/保留底层字节 |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.trailing_llgb · lba4_encoding / ordinary-rolling

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1fc–0x1ff（4B） |
| 语义类型 | Scalar |
| 含义 | 尾部 LLGB |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；逻辑标志以 rolling 解码后的视图为准。 |
| 编码规则 | 通过普通 rolling 序列化，不对 post-XOR 标志重新解释。 |
| 配置类型 | lba4_encoding / ordinary-rolling |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba4.trailing_llgb · lba4_encoding / post-xor

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1fc–0x1ff（4B） |
| 语义类型 | Scalar |
| 含义 | 尾部 LLGB |
| 所有权 | owner |
| 解码规则 | 应用官方 滚动读取端；盘面 与 读取端 的标志视图必须分开保留。 |
| 编码规则 | 先执行完整 rolling，再用 node 标志对 +0x45/+0x46 做 post-XOR 覆盖。 |
| 配置类型 | lba4_encoding / post-xor |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-REPAIR-2021 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 编码可与写入端 覆盖项 独立组合。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba4::parse_lba4 |
| 测试符号 | lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**host_hardinfo_source**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo 镜像共享一条独立的写入端轴。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-host-identity | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo 镜像共享一条独立的写入端轴。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| 区域（含首尾） | current-zero | legacy-host-identity |
| --- | --- | --- |
| 0x035–0x038 | overlay.host_hardinfo.lba4: 当前 host-hardinfo 为全零。 | overlay.host_hardinfo.lba4: 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。 |

**lba4_encoding**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| post-xor | owner | 4:018-1ff | EncodingChanged | 恢复节点 逻辑字段稳定，但 盘面 表示不同。 | S-WIN-CURRENT;S-WIN-191141;V-LBA4;V-LBA4-V19 |
| ordinary-rolling | owner | 4:018-1ff | EncodingChanged | 恢复节点 逻辑字段稳定，但 盘面 表示不同。 | S-WIN-CURRENT;S-WIN-191141;V-LBA4;V-LBA4-V19 |

| 区域（含首尾） | post-xor | ordinary-rolling |
| --- | --- | --- |
| 0x018–0x01b | lba4.onlyid_xor8: OnlyIdXor8 校验值 | lba4.onlyid_xor8: OnlyIdXor8 校验值 |
| 0x01c–0x01f | lba4.second_key: OnllyID2Nd 激活密钥种子 | lba4.second_key: OnllyID2Nd 激活密钥种子 |
| 0x020–0x033 | lba4.hserial: HSerialCRC[5] 向量 | lba4.hserial: HSerialCRC[5] 向量 |
| 0x034–0x034 | lba4.single_usb: SingleUsbFlg | lba4.single_usb: SingleUsbFlg |
| 0x035–0x038 | lba4.my_hardinfo: MyHardinfo 镜像 | lba4.my_hardinfo: MyHardinfo 镜像 |
| 0x039–0x03c | lba4.new_lab_flag: NewLabFlag LLGB | lba4.new_lab_flag: NewLabFlag LLGB |
| 0x03d–0x040 | lba4.version: 版本 1 | lba4.version: 版本 1 |
| 0x041–0x044 | lba4.sector_tuple: 扇区元组 08 04 0C 01 | lba4.sector_tuple: 扇区元组 08 04 0C 01 |
| 0x045–0x045 | lba4.data_to_server: bDataToServer | lba4.data_to_server: bDataToServer |
| 0x046–0x046 | lba4.connect_server: bConnetServer | lba4.connect_server: bConnetServer |
| 0x047–0x1fb | lba4.representation_backing: 表示载体/保留底层字节 | lba4.representation_backing: 表示载体/保留底层字节 |
| 0x1fc–0x1ff | lba4.trailing_llgb: 尾部 LLGB | lba4.trailing_llgb: 尾部 LLGB |

**lba4_hserial_source**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:020-033 | ProducerChanged | HSerialCRC 向量可独立处于缺失状态或由调用方提供。 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19;P-GOLD-ENC |
| legacy-caller-vector | overlay | 4:020-033 | ProducerChanged | HSerialCRC 向量可独立处于缺失状态或由调用方提供。 | S-WIN-CURRENT;S-WIN-191141;V-LBA4-V19;P-GOLD-ENC |

| 区域（含首尾） | current-zero | legacy-caller-vector |
| --- | --- | --- |
| 0x020–0x033 | overlay.lba4.hserial_source: 当前 HSerial 向量为全零。 | overlay.lba4.hserial_source: 旧版 ABI 携带调用方提供的 HSerial[5]。 |

**lba4_second_key_source**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| current-main-onlyid | overlay | 4:01c-01f | ProducerChanged | OnllyID2Nd 含义稳定，但写入端种子来源会变化。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-guid-crc | overlay | 4:01c-01f | ProducerChanged | OnllyID2Nd 含义稳定，但写入端种子来源会变化。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| 区域（含首尾） | current-main-onlyid | legacy-guid-crc |
| --- | --- | --- |
| 0x01c–0x01f | overlay.lba4.second_key_source: 第二密钥复用 main onlyid。 | overlay.lba4.second_key_source: 第二密钥来自独立 GUID CRC。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA5

### 用途与配置类型

- 不透明的写保护探测临时扇区。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
| --- | --- | --- | --- | --- | --- |
| 0x000–0x1ff | 512 | lba5.write_probe_scratch | base / all | OpaquePreserve | owner |

### 逐字段规则与证据

#### lba5.write_probe_scratch · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | OpaquePreserve |
| 含义 | 不透明的写保护探测临时扇区。 |
| 所有权 | owner |
| 解码规则 | 将负载作为不透明数据处理。 |
| 编码规则 | 原样保留完整扇区。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba5::parse_lba5 |
| 测试符号 | basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

目录未为本扇区声明独立 配置类型轴；逐字段演进类型见上表。

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA6

### 用途与配置类型

- 2*CRC32(device_id) 兼容性校验值。
- Autonum 槽；NUL 后为保留底层字节。
- BeiZhu 首字节/空值 NUL。
- BeiZhu 尾部或 NUL 后保留底层字节。
- 第 63 字节为 NUL 拼接边界。
- 第 63 字节为 NUL 后保留底层字节。
- 第 63 字节保存 Dept[59]。
- CRC32(device_id)。
- BeiZhu 专用 NUL。
- GSerial 专用 NUL。
- GSerial 前缀/终止符位置。
- GSerial 尾部或 NUL 后保留底层字节。
- 带保留底层字节的 Label 槽。
- 标记 + Dept[0..58]。
- Office 槽；NUL 后为保留底层字节。
- NUL 后全零保留底层字节。
- SAFE6 对前 508 字节计算的校验和。
- 短 Dept C 字符串/保留底层字节。
- 静态 引导代码/消息模板。
- 静态模板材料。
- 静态全零尾部。
- 与 LBA12 type4 几何信息绑定的残留 MBR entry3 字节。
- UsbMainBSec 静态模板。
- User 槽；NUL 后为保留底层字节。
- 仅写的 !SAFE m_encrypt 元数据。
- 全零兼容性尾部。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x03e（63B） |
| 语义类型 | CString |
| 含义 | 标记 + Dept[0..58]。 |
| 所有权 | owner |
| 解码规则 | 全零拼接边界选择 join59；在 Dept[59] 叠加续段。 |
| 编码规则 | 写入 NUL 拼接边界，续段从 Dept[59] 开始。 |
| 配置类型 | dept_layout / join59 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 历史 join59 可执行文件来源不是 盘面 语义闭环的必要条件。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_prefix · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x03e（63B） |
| 语义类型 | CString |
| 含义 | 标记 + Dept[0..58]。 |
| 所有权 | owner |
| 解码规则 | 非零拼接边界选择 join60；从 Dept[60] 追加。 |
| 编码规则 | 内联写入 Dept[59]，续段从 Dept[60] 开始。 |
| 配置类型 | dept_layout / join60 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 历史 join59 可执行文件来源不是 盘面 语义闭环的必要条件。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_prefix · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x03e（63B） |
| 语义类型 | CString |
| 含义 | 短 Dept C 字符串/保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 仅从 LBA6 C 字符串解码 Dept。 |
| 编码规则 | 写入短 Dept；原样保留 LBA9 底层字节。 |
| 配置类型 | dept_layout / short |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 历史 join59 可执行文件来源不是 盘面 语义闭环的必要条件。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / join59

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03f–0x03f（1B） |
| 语义类型 | ProfileSelector |
| 含义 | 第 63 字节为 NUL 拼接边界。 |
| 所有权 | owner |
| 解码规则 | 全零拼接边界选择 join59；在 Dept[59] 叠加续段。 |
| 编码规则 | 写入 NUL 拼接边界，续段从 Dept[59] 开始。 |
| 配置类型 | dept_layout / join59 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 各状态的精确字节角色已闭环。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03f–0x03f（1B） |
| 语义类型 | ProfileSelector |
| 含义 | 第 63 字节保存 Dept[59]。 |
| 所有权 | owner |
| 解码规则 | 非零拼接边界选择 join60；从 Dept[60] 追加。 |
| 编码规则 | 内联写入 Dept[59]，续段从 Dept[60] 开始。 |
| 配置类型 | dept_layout / join60 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 各状态的精确字节角色已闭环。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.dept_seam · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03f–0x03f（1B） |
| 语义类型 | ProfileSelector |
| 含义 | 第 63 字节为 NUL 后保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 仅从 LBA6 C 字符串解码 Dept。 |
| 编码规则 | 写入短 Dept；原样保留 LBA9 底层字节。 |
| 配置类型 | dept_layout / short |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 各状态的精确字节角色已闭环。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_040_04f · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x040–0x04f（16B） |
| 语义类型 | CompatibilityField |
| 含义 | UsbMainBSec 静态模板。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.user_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x050–0x06f（32B） |
| 语义类型 | CString |
| 含义 | User 槽；NUL 后为保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.autonum_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x070–0x07f（16B） |
| 语义类型 | CString |
| 含义 | Autonum 槽；NUL 后为保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.office_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0bf（64B） |
| 语义类型 | CString |
| 含义 | Office 槽；NUL 后为保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_0c0_0ff · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x0c0–0x0ff（64B） |
| 语义类型 | CompatibilityField |
| 含义 | 静态模板材料。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.device_crc · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x103（4B） |
| 语义类型 | Scalar |
| 含义 | CRC32(device_id)。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.device_crc_guard · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x104–0x107（4B） |
| 语义类型 | Scalar |
| 含义 | 2*CRC32(device_id) 兼容性校验值。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.template_108_187 · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x108–0x187（128B） |
| 语义类型 | CompatibilityField |
| 含义 | 静态 引导代码/消息模板。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.label_slot · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x188–0x1bf（56B） |
| 语义类型 | CString |
| 含义 | 带保留底层字节的 Label 槽。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_prefix · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1c0–0x1c8（9B） |
| 语义类型 | CString |
| 含义 | GSerial 前缀/终止符位置。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1c9–0x1ce（6B） |
| 语义类型 | CString |
| 含义 | GSerial 尾部或 NUL 后保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.gserial_terminator · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1cf–0x1cf（1B） |
| 语义类型 | Scalar |
| 含义 | GSerial 专用 NUL。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_head · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1d0–0x1d0（1B） |
| 语义类型 | CString |
| 含义 | BeiZhu 首字节/空值 NUL。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1d1–0x1de（14B） |
| 语义类型 | CString |
| 含义 | BeiZhu 尾部或 NUL 后保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.beizhu_terminator · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1df–0x1df（1B） |
| 语义类型 | Scalar |
| 含义 | BeiZhu 专用 NUL。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.mbr_underlay · lba6_mbr_underlay / legacy-mbr-snapshot

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1e0–0x1ed（14B） |
| 语义类型 | Snapshot |
| 含义 | 与 LBA12 type4 几何信息绑定的残留 MBR entry3 字节。 |
| 所有权 | owner |
| 解码规则 | 解码固定 CHS/type 前缀及小端 start/count 公式。 |
| 编码规则 | 保留现有快照；当前写入不得自行生成。 |
| 配置类型 | lba6_mbr_underlay / legacy-mbr-snapshot |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| 物理证据 | P-GOLD-ENC;P-EESI-NETAC |
| 实现来源 | 历史非零复制位置仅属于来源信息。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.mbr_underlay · lba6_mbr_underlay / zero-underlay

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1e0–0x1ed（14B） |
| 语义类型 | UnownedBacking |
| 含义 | NUL 后全零保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 没有独立业务消费者。 |
| 编码规则 | 除非保留旧版快照，否则保持为零。 |
| 配置类型 | lba6_mbr_underlay / zero-underlay |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141-LBA6;S-MBR-SNAPSHOT-SEMANTIC |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD;P-EESI-NETAC |
| 实现来源 | 历史非零选择器仅属于来源信息。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.compat_1ee_1ef · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1ee–0x1ef（2B） |
| 语义类型 | CompatibilityField |
| 含义 | 全零兼容性尾部。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.encrypt_generation_flag · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1f0–0x1f3（4B） |
| 语义类型 | Scalar |
| 含义 | 仅写的 !SAFE m_encrypt 元数据。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.zero_tail · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1f4–0x1fb（8B） |
| 语义类型 | CompatibilityField |
| 含义 | 静态全零尾部。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba6.checksum · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x1fc–0x1ff（4B） |
| 语义类型 | Checksum |
| 含义 | SAFE6 对前 508 字节计算的校验和。 |
| 所有权 | owner |
| 解码规则 | 解码字段；C 字符串在第一个 NUL 处停止。 |
| 编码规则 | 序列化标准字段并原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba6::parse_lba6 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**dept_layout**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| short | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join59 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join60 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |

| 区域（含首尾） | short | join59 | join60 |
| --- | --- | --- | --- |
| 0x000–0x03e | lba6.dept_prefix: 短 Dept C 字符串/保留底层字节。 | lba6.dept_prefix: 标记 + Dept[0..58]。 | lba6.dept_prefix: 标记 + Dept[0..58]。 |
| 0x03f–0x03f | lba6.dept_seam: 第 63 字节为 NUL 后保留底层字节。 | lba6.dept_seam: 第 63 字节为 NUL 拼接边界。 | lba6.dept_seam: 第 63 字节保存 Dept[59]。 |

**lba6_mbr_underlay**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| zero-underlay | owner | 6:1e0-1ed | OwnershipChanged | LBA6 的 NUL 后切片为全零底层字节或残留的 MBR entry3 快照。 | S-MBR-SNAPSHOT-SEMANTIC;P-GOLD-ENC |
| legacy-mbr-snapshot | owner | 6:1e0-1ed | OwnershipChanged | LBA6 的 NUL 后切片为全零底层字节或残留的 MBR entry3 快照。 | S-MBR-SNAPSHOT-SEMANTIC;P-GOLD-ENC |

| 区域（含首尾） | zero-underlay | legacy-mbr-snapshot |
| --- | --- | --- |
| 0x1e0–0x1ed | lba6.mbr_underlay: NUL 后全零保留底层字节。 | lba6.mbr_underlay: 与 LBA12 type4 几何信息绑定的残留 MBR entry3 字节。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA7

### 用途与配置类型

- 14 字节 LBA7 密码信息，v0206 当前版 版本。
- 14 字节 LBA7 密码信息，v0064 旧版 版本。
- entry2 缺失/全零状态。
- 打包的 64 字节旧版 EDPF entry0 和 entry1。
- 存在的打包 EDPF entry2。
- 由写入端负责的 table/密码信息 后全零区域。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x07f（128B） |
| 语义类型 | PackedStruct |
| 含义 | 打包的 64 字节旧版 EDPF entry0 和 entry1。 |
| 所有权 | owner |
| 解码规则 | 按 0x40 步长解码条目。 |
| 编码规则 | 序列化打包的旧表形式。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.entry2 · lba7_entry_count / three-entry

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0bf（64B） |
| 语义类型 | PackedStruct |
| 含义 | 存在的打包 EDPF entry2。 |
| 所有权 | owner |
| 解码规则 | 解码打包的 64 字节 entry2。 |
| 编码规则 | 序列化 entry2。 |
| 配置类型 | lba7_entry_count / three-entry |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 条目数量与 密码信息 版本相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.entry2 · lba7_entry_count / two-entry

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0bf（64B） |
| 语义类型 | CompatibilityField |
| 含义 | entry2 缺失/全零状态。 |
| 所有权 | owner |
| 解码规则 | 将 entry2 视为缺失。 |
| 编码规则 | 保持 entry2 未使用。 |
| 配置类型 | lba7_entry_count / two-entry |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 条目数量与 密码信息 版本相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.pass_info · lba7_passinfo_version / current-v0206

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x0c0–0x0cd（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 LBA7 密码信息，v0206 当前版 版本。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段；休眠 period 单位仍不解释。 |
| 编码规则 | 序列化所选 密码信息 版本，并原样保留已命名的休眠字节。 |
| 配置类型 | lba7_passinfo_version / current-v0206 |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 与条目数量相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.pass_info · lba7_passinfo_version / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x0c0–0x0cd（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 LBA7 密码信息，v0064 旧版 版本。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段；休眠 period 单位仍不解释。 |
| 编码规则 | 序列化所选 密码信息 版本，并原样保留已命名的休眠字节。 |
| 配置类型 | lba7_passinfo_version / legacy-v0064 |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 与条目数量相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba7.post_table_zero · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x0ce–0x1ff（306B） |
| 语义类型 | CompatibilityField |
| 含义 | 由写入端负责的 table/密码信息 后全零区域。 |
| 所有权 | owner |
| 解码规则 | 没有语义字段。 |
| 编码规则 | 初始化为零。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba7::parse_lba7 |
| 测试符号 | edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba7_entry_count**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| two-entry | owner | 7:080-0bf | AddedRemoved | 第三个打包旧版 EDPF 条目可独立缺失或存在。 | S-WIN-CURRENT;P-GOLD-ENC;P-GOLD-NOPWD |
| three-entry | owner | 7:080-0bf | AddedRemoved | 第三个打包旧版 EDPF 条目可独立缺失或存在。 | S-WIN-CURRENT;P-GOLD-ENC;P-GOLD-NOPWD |

| 区域（含首尾） | two-entry | three-entry |
| --- | --- | --- |
| 0x080–0x0bf | lba7.entry2: entry2 缺失/全零状态。 | lba7.entry2: 存在的打包 EDPF entry2。 |

**lba7_passinfo_version**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| legacy-v0064 | owner | 7:0c0-0cd | ProducerChanged | LBA7 密码信息 版本与打包条目数量相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| current-v0206 | owner | 7:0c0-0cd | ProducerChanged | LBA7 密码信息 版本与打包条目数量相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| 区域（含首尾） | legacy-v0064 | current-v0206 |
| --- | --- | --- |
| 0x0c0–0x0cd | lba7.pass_info: 14 字节 LBA7 密码信息，v0064 旧版 版本。 | lba7.pass_info: 14 字节 LBA7 密码信息，v0206 当前版 版本。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA8

### 用途与配置类型

- 当前 host-hardinfo 为全零。
- 当前 main-onlyid 文本 + 全零 DWORD。
- 动态 ELABEL 正文/保留底层字节/尾部
- ELABEL 偏移
- HDSerialInfo 主机身份信息
- 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。
- LLGB 魔数
- Labversion
- 逻辑加密前缀长度
- MacInfo[6] 保留槽
- 单调毫秒 writeTime
- 保留的全零头部
- 严格旧版缺失/全零槽。
- ToolVersion[4]
- 过渡期文本 + host-hardinfo DWORD。
- UsbOnlyInfo 后缀

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x003（4B） |
| 语义类型 | Scalar |
| 含义 | LLGB 魔数 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.logical_length · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x004–0x007（4B） |
| 语义类型 | Scalar |
| 含义 | 逻辑加密前缀长度 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.tool_version · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x008–0x00b（4B） |
| 语义类型 | CompatibilityField |
| 含义 | ToolVersion[4] |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.lab_version · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x00c–0x00f（4B） |
| 语义类型 | Scalar |
| 含义 | Labversion |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.write_time · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x010–0x013（4B） |
| 语义类型 | Scalar |
| 含义 | 单调毫秒 writeTime |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba8 · host_hardinfo_source / current-zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x014–0x017（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 当前 host-hardinfo 为全零。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；语义所有者为 base/LBA4 编码。 |
| 编码规则 | 写入端轴与 LBA4 编码、UsbOnlyInfo 相互独立。 |
| 配置类型 | host_hardinfo_source / current-zero |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 跨 LBA 镜像关系相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### overlay.host_hardinfo.lba8 · host_hardinfo_source / legacy-host-identity

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x014–0x017（4B） |
| 语义类型 | CompatibilityField |
| 含义 | 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。 |
| 所有权 | overlay |
| 解码规则 | 覆盖项 注解；语义所有者为 base/LBA4 编码。 |
| 编码规则 | 写入端轴与 LBA4 编码、UsbOnlyInfo 相互独立。 |
| 配置类型 | host_hardinfo_source / legacy-host-identity |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 跨 LBA 镜像关系相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.host_hardinfo · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x014–0x017（4B） |
| 语义类型 | CompatibilityField |
| 含义 | HDSerialInfo 主机身份信息 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.mac_info · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x018–0x01d（6B） |
| 语义类型 | CompatibilityField |
| 含义 | MacInfo[6] 保留槽 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / current

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01e–0x02d（16B） |
| 语义类型 | CompatibilityField |
| 含义 | 当前 main-onlyid 文本 + 全零 DWORD。 |
| 所有权 | owner |
| 解码规则 | 解码所选兼容表示；不产生行为分支。 |
| 编码规则 | 序列化所选表示，不与其他轴耦合。 |
| 配置类型 | lba8_usb_only_info / current |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入端代际与表示形式分开维护。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / strict-legacy-absent

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01e–0x02d（16B） |
| 语义类型 | CompatibilityField |
| 含义 | 严格旧版缺失/全零槽。 |
| 所有权 | owner |
| 解码规则 | 解码所选兼容表示；不产生行为分支。 |
| 编码规则 | 序列化所选表示，不与其他轴耦合。 |
| 配置类型 | lba8_usb_only_info / strict-legacy-absent |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入端代际与表示形式分开维护。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_info · lba8_usb_only_info / transitional-2019

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x01e–0x02d（16B） |
| 语义类型 | CompatibilityField |
| 含义 | 过渡期文本 + host-hardinfo DWORD。 |
| 所有权 | owner |
| 解码规则 | 解码所选兼容表示；不产生行为分支。 |
| 编码规则 | 序列化所选表示，不与其他轴耦合。 |
| 配置类型 | lba8_usb_only_info / transitional-2019 |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-WIN-191141 |
| 消费端证据 | S-WIN-CURRENT;S-WIN-191141;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入端代际与表示形式分开维护。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.usb_only_suffix · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x02e–0x03d（16B） |
| 语义类型 | CompatibilityField |
| 含义 | UsbOnlyInfo 后缀 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.elab_offset · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x03e–0x03f（2B） |
| 语义类型 | Scalar |
| 含义 | ELABEL 偏移 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.reserved_040_07f · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x040–0x07f（64B） |
| 语义类型 | CompatibilityField |
| 含义 | 保留的全零头部 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba8.elabel_body · base / all

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x1ff（384B） |
| 语义类型 | EncryptedRegion |
| 含义 | 动态 ELABEL 正文/保留底层字节/尾部 |
| 所有权 | owner |
| 解码规则 | 按照 LBA8 逻辑长度/ELABEL 规则解码。 |
| 编码规则 | 序列化当前字段，同时原样保留已记录的底层字节。 |
| 配置类型 | base / all |
| 演进类型 | Stable |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba8::parse_lba8 |
| 测试符号 | lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**host_hardinfo_source**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| current-zero | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo 镜像共享一条独立的写入端轴。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| legacy-host-identity | overlay | 4:035-038;8:014-017 | ProducerChanged | LBA4/LBA8 host-hardinfo 镜像共享一条独立的写入端轴。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| 区域（含首尾） | current-zero | legacy-host-identity |
| --- | --- | --- |
| 0x014–0x017 | overlay.host_hardinfo.lba8: 当前 host-hardinfo 为全零。 | overlay.host_hardinfo.lba8: 历史主机端由 DiskNumber/DeviceNumber 派生的身份信息。 |

**lba8_usb_only_info**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| current | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo 存在当前、过渡和缺失三种写入端状态。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| transitional-2019 | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo 存在当前、过渡和缺失三种写入端状态。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |
| strict-legacy-absent | owner | 8:01e-02d | ProducerChanged | UsbOnlyInfo 存在当前、过渡和缺失三种写入端状态。 | S-WIN-CURRENT;S-WIN-191141;P-GOLD-ENC |

| 区域（含首尾） | current | transitional-2019 | strict-legacy-absent |
| --- | --- | --- | --- |
| 0x01e–0x02d | lba8.usb_only_info: 当前 main-onlyid 文本 + 全零 DWORD。 | lba8.usb_only_info: 过渡期文本 + host-hardinfo DWORD。 | lba8.usb_only_info: 严格旧版缺失/全零槽。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA9

### 用途与配置类型

- EETU 缺失/全零状态。
- EPPE 负载之前的保留底层字节。
- SAPF 负载之外的保留底层字节。
- 长 User 负载之外的保留底层字节。
- 续段从 Dept[59] 开始。
- 续段从 Dept[60] 开始。
- EETU 时间/useCount 结构及反向保留底层字节。
- EPPE 魔数。
- 由 EPPE 写入端负责的全零尾部。
- 长 User 续段 User[28..NUL]。
- 最小密码长度。
- 上半区没有有效负载。
- SAPF 恢复元数据。
- 未使用的续段保留底层字节。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x07f（128B） |
| 语义类型 | CompatibilityField |
| 含义 | EETU 缺失/全零状态。 |
| 所有权 | owner |
| 解码规则 | 没有 EETU 魔数时不解释临时使用语义。 |
| 编码规则 | 保留缺失状态。 |
| 配置类型 | lba9_eetu / absent-zero |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 与上层 覆盖项 相互独立。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eetu · lba9_eetu / eetu

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x07f（128B） |
| 语义类型 | PackedStruct |
| 含义 | EETU 时间/useCount 结构及反向保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 校验 EETU；消费 bounds/useCount；原样保留反向底层字节。 |
| 编码规则 | 序列化控制字段；原样保留底层字节；显式尾部保持全零。 |
| 配置类型 | lba9_eetu / eetu |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 反向字节属于底层保留内容，不是隐藏字段。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / join59

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0ff（128B） |
| 语义类型 | CString |
| 含义 | 续段从 Dept[59] 开始。 |
| 所有权 | owner |
| 解码规则 | 全零拼接边界选择 join59；在 Dept[59] 叠加续段。 |
| 编码规则 | 写入 NUL 拼接边界，续段从 Dept[59] 开始。 |
| 配置类型 | dept_layout / join59 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | NUL 后底层字节不做归一化。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / join60

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0ff（128B） |
| 语义类型 | CString |
| 含义 | 续段从 Dept[60] 开始。 |
| 所有权 | owner |
| 解码规则 | 非零拼接边界选择 join60；从 Dept[60] 追加。 |
| 编码规则 | 内联写入 Dept[59]，续段从 Dept[60] 开始。 |
| 配置类型 | dept_layout / join60 |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | NUL 后底层字节不做归一化。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.dept_continuation · dept_layout / short

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x0ff（128B） |
| 语义类型 | UnownedBacking |
| 含义 | 未使用的续段保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 仅从 LBA6 C 字符串解码 Dept。 |
| 编码规则 | 写入短 Dept；原样保留 LBA9 底层字节。 |
| 配置类型 | dept_layout / short |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;S-JOIN59-SEMANTIC |
| 消费端证据 | S-WIN-CURRENT;S-FILEOPHOOK-2022;S-JOIN59-SEMANTIC |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | NUL 后底层字节不做归一化。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::dept_join_seams_and_post_nul_backing_are_independent |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_pre · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x17f（128B） |
| 语义类型 | UnownedBacking |
| 含义 | EPPE 负载之前的保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 忽略语义内容。 |
| 编码规则 | 原样保留底层字节。 |
| 配置类型 | lba9_overlay / eppe |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | EPPE 从 +0x180 开始。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.sapf · lba9_overlay / sapf

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x11f（32B） |
| 语义类型 | PackedStruct |
| 含义 | SAPF 恢复元数据。 |
| 所有权 | owner |
| 解码规则 | SAPF 只解码到 +0x11F。 |
| 编码规则 | 序列化 SAPF 恢复字段。 |
| 配置类型 | lba9_overlay / sapf |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 该 配置类型 避免对 User/EPPE 重新解释。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.upper_zero · lba9_overlay / zero

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x1ff（256B） |
| 语义类型 | UnownedBacking |
| 含义 | 上半区没有有效负载。 |
| 所有权 | owner |
| 解码规则 | 忽略/原样保留底层字节。 |
| 编码规则 | 原样保留字节。 |
| 配置类型 | lba9_overlay / zero |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 显式所有权状态。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.user_continuation · lba9_overlay / long-user

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x17f（128B） |
| 语义类型 | CString |
| 含义 | 长 User 续段 User[28..NUL]。 |
| 所有权 | owner |
| 解码规则 | 拼接 User 续段并忽略 NUL 后保留底层字节。 |
| 编码规则 | 写入剩余 User 字节直到 NUL。 |
| 配置类型 | lba9_overlay / long-user |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 官方虚拟正例。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.sapf_tail · lba9_overlay / sapf

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x120–0x1ff（224B） |
| 语义类型 | UnownedBacking |
| 含义 | SAPF 负载之外的保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 忽略语义内容。 |
| 编码规则 | 原样保留底层字节。 |
| 配置类型 | lba9_overlay / sapf |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | SAPF 消费端在 +0x11F 停止。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_magic · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x180–0x183（4B） |
| 语义类型 | Scalar |
| 含义 | EPPE 魔数。 |
| 所有权 | owner |
| 解码规则 | 校验 EPPE 魔数。 |
| 编码规则 | 写入 EPPE 魔数。 |
| 配置类型 | lba9_overlay / eppe |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.long_user_tail · lba9_overlay / long-user

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x180–0x1ff（128B） |
| 语义类型 | UnownedBacking |
| 含义 | 长 User 负载之外的保留底层字节。 |
| 所有权 | owner |
| 解码规则 | 忽略语义内容。 |
| 编码规则 | 原样保留底层字节。 |
| 配置类型 | lba9_overlay / long-user |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT;V-LBA6 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 物理保留状态 + 虚拟激活 User。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_min_len · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x184–0x187（4B） |
| 语义类型 | Scalar |
| 含义 | 最小密码长度。 |
| 所有权 | owner |
| 解码规则 | 解码小端 DWORD。 |
| 编码规则 | 写入已校验的 6..19 值。 |
| 配置类型 | lba9_overlay / eppe |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba9.eppe_zero_tail · lba9_overlay / eppe

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x188–0x1ff（120B） |
| 语义类型 | CompatibilityField |
| 含义 | 由 EPPE 写入端负责的全零尾部。 |
| 所有权 | owner |
| 解码规则 | 当前没有语义消费者。 |
| 编码规则 | 当前写入端初始化为零。 |
| 配置类型 | lba9_overlay / eppe |
| 演进类型 | OwnershipChanged |
| 写入端证据 | S-WIN-CURRENT |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 标准 COMPLETE 语义；来源边界继续保留记录。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba9::parse_lba9 |
| 测试符号 | dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**dept_layout**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| short | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join59 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |
| join60 | owner | 6:000-03f;9:080-0ff | OwnershipChanged | Dept 拼接边界是跨 LBA6/LBA9 的独立序列化轴。 | S-JOIN59-SEMANTIC;P-GOLD-ENC |

| 区域（含首尾） | short | join59 | join60 |
| --- | --- | --- | --- |
| 0x080–0x0ff | lba9.dept_continuation: 未使用的续段保留底层字节。 | lba9.dept_continuation: 续段从 Dept[59] 开始。 | lba9.dept_continuation: 续段从 Dept[60] 开始。 |

**lba9_eetu**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| absent-zero | owner | 9:000-07f | AddedRemoved | EETU 为可选项，并与 Dept/User 上层 覆盖项 相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| eetu | owner | 9:000-07f | AddedRemoved | EETU 为可选项，并与 Dept/User 上层 覆盖项 相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| 区域（含首尾） | absent-zero | eetu |
| --- | --- | --- |
| 0x000–0x07f | lba9.eetu: EETU 缺失/全零状态。 | lba9.eetu: EETU 时间/useCount 结构及反向保留底层字节。 |

**lba9_overlay**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| zero | owner | 9:100-1ff | OwnershipChanged | 上半区在 恢复、User 续段、密码策略和保留底层字节之间复用。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| sapf | owner | 9:100-1ff | OwnershipChanged | 上半区在 恢复、User 续段、密码策略和保留底层字节之间复用。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| long-user | owner | 9:100-1ff | OwnershipChanged | 上半区在 恢复、User 续段、密码策略和保留底层字节之间复用。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |
| eppe | owner | 9:100-1ff | OwnershipChanged | 上半区在 恢复、User 续段、密码策略和保留底层字节之间复用。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA6;P-GOLD-ENC |

| 区域（含首尾） | zero | sapf | long-user | eppe |
| --- | --- | --- | --- | --- |
| 0x100–0x11f | lba9.upper_zero: 上半区没有有效负载。 | lba9.sapf: SAPF 恢复元数据。 | lba9.user_continuation: 长 User 续段 User[28..NUL]。 | lba9.eppe_pre: EPPE 负载之前的保留底层字节。 |
| 0x120–0x17f | lba9.upper_zero: 上半区没有有效负载。 | lba9.sapf_tail: SAPF 负载之外的保留底层字节。 | lba9.user_continuation: 长 User 续段 User[28..NUL]。 | lba9.eppe_pre: EPPE 负载之前的保留底层字节。 |
| 0x180–0x183 | lba9.upper_zero: 上半区没有有效负载。 | lba9.sapf_tail: SAPF 负载之外的保留底层字节。 | lba9.long_user_tail: 长 User 负载之外的保留底层字节。 | lba9.eppe_magic: EPPE 魔数。 |
| 0x184–0x187 | lba9.upper_zero: 上半区没有有效负载。 | lba9.sapf_tail: SAPF 负载之外的保留底层字节。 | lba9.long_user_tail: 长 User 负载之外的保留底层字节。 | lba9.eppe_min_len: 最小密码长度。 |
| 0x188–0x1ff | lba9.upper_zero: 上半区没有有效负载。 | lba9.sapf_tail: SAPF 负载之外的保留底层字节。 | lba9.long_user_tail: 长 User 负载之外的保留底层字节。 | lba9.eppe_zero_tail: 由 EPPE 写入端负责的全零尾部。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA10

### 用途与配置类型

- 调用方扩展区
- EESI 缺失全零 配置类型。
- EESI 魔数
- 原样保留/忽略的尾部
- type2 标签
- type4 标签
- UsbSuspensionWnd 标志

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x1ff（512B） |
| 语义类型 | CompatibilityField |
| 含义 | EESI 缺失全零 配置类型。 |
| 所有权 | owner |
| 解码规则 | 没有魔数时不解释 EESI。 |
| 编码规则 | 保留缺失 配置类型。 |
| 配置类型 | lba10_eesi / absent-zero |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 通用样本集覆盖缺失状态。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.eesi_magic · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x003（4B） |
| 语义类型 | Scalar |
| 含义 | EESI 魔数 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.suspension_flag · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x004–0x007（4B） |
| 语义类型 | Scalar |
| 含义 | UsbSuspensionWnd 标志 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.share_label · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x008–0x017（16B） |
| 语义类型 | CString |
| 含义 | type2 标签 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.encrypt_label · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x018–0x027（16B） |
| 语义类型 | CString |
| 含义 | type4 标签 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.extension · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x028–0x07f（88B） |
| 语义类型 | CompatibilityField |
| 含义 | 调用方扩展区 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba10.tail · lba10_eesi / eesi-enabled

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x080–0x1ff（384B） |
| 语义类型 | UnownedBacking |
| 含义 | 原样保留/忽略的尾部 |
| 所有权 | owner |
| 解码规则 | 解密并解释前 0x80 字节；尾部原样保留。 |
| 编码规则 | SetEESI 负责前 0x80 字节并原样保留尾部。 |
| 配置类型 | lba10_eesi / eesi-enabled |
| 演进类型 | AddedRemoved |
| 写入端证据 | S-EESI-361018 |
| 消费端证据 | S-EESI-361018 |
| 物理证据 | P-EESI-NETAC |
| 实现来源 | 真实的写入前启用状态采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba10::parse_lba10 |
| 测试符号 | lba10_behavior::lba10_absent_and_eesi_enabled_profiles_round_trip_without_touching_tail |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba10_eesi**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| absent-zero | owner | 10:000-1ff | AddedRemoved | EESI 为可选项；启用负载只负责已定义前缀，并原样保留尾部。 | S-EESI-361018;P-EESI-NETAC;P-GOLD-ENC |
| eesi-enabled | owner | 10:000-1ff | AddedRemoved | EESI 为可选项；启用负载只负责已定义前缀，并原样保留尾部。 | S-EESI-361018;P-EESI-NETAC;P-GOLD-ENC |

| 区域（含首尾） | absent-zero | eesi-enabled |
| --- | --- | --- |
| 0x000–0x003 | lba10.absent: EESI 缺失全零 配置类型。 | lba10.eesi_magic: EESI 魔数 |
| 0x004–0x007 | lba10.absent: EESI 缺失全零 配置类型。 | lba10.suspension_flag: UsbSuspensionWnd 标志 |
| 0x008–0x017 | lba10.absent: EESI 缺失全零 配置类型。 | lba10.share_label: type2 标签 |
| 0x018–0x027 | lba10.absent: EESI 缺失全零 配置类型。 | lba10.encrypt_label: type4 标签 |
| 0x028–0x07f | lba10.absent: EESI 缺失全零 配置类型。 | lba10.extension: 调用方扩展区 |
| 0x080–0x1ff | lba10.absent: EESI 缺失全零 配置类型。 | lba10.tail: 原样保留/忽略的尾部 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA11

### 用途与配置类型

- 252 字节随机密钥材料
- DRKB 魔数
- 加密 UID + 全零填充
- PDKB 魔数

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x003（4B） |
| 语义类型 | Scalar |
| 含义 | DRKB 魔数 |
| 所有权 | owner |
| 解码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 构造扇区。 |
| 配置类型 | lba11_capacity / disk-size |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.drkb_magic · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x003（4B） |
| 语义类型 | Scalar |
| 含义 | DRKB 魔数 |
| 所有权 | owner |
| 解码规则 | 使用 CHS 容量乘积派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 CHS 容量乘积构造扇区。 |
| 配置类型 | lba11_capacity / repair-chs |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.random252 · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x004–0x0ff（252B） |
| 语义类型 | EncryptedRegion |
| 含义 | 252 字节随机密钥材料 |
| 所有权 | owner |
| 解码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 构造扇区。 |
| 配置类型 | lba11_capacity / disk-size |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.random252 · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x004–0x0ff（252B） |
| 语义类型 | EncryptedRegion |
| 含义 | 252 字节随机密钥材料 |
| 所有权 | owner |
| 解码规则 | 使用 CHS 容量乘积派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 CHS 容量乘积构造扇区。 |
| 配置类型 | lba11_capacity / repair-chs |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.pdkb_magic · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x103（4B） |
| 语义类型 | Scalar |
| 含义 | PDKB 魔数 |
| 所有权 | owner |
| 解码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 构造扇区。 |
| 配置类型 | lba11_capacity / disk-size |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.pdkb_magic · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x100–0x103（4B） |
| 语义类型 | Scalar |
| 含义 | PDKB 魔数 |
| 所有权 | owner |
| 解码规则 | 使用 CHS 容量乘积派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 CHS 容量乘积构造扇区。 |
| 配置类型 | lba11_capacity / repair-chs |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.uid_payload · lba11_capacity / disk-size

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x104–0x1ff（252B） |
| 语义类型 | EncryptedRegion |
| 含义 | 加密 UID + 全零填充 |
| 所有权 | owner |
| 解码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 DISK_GEOMETRY_EX.DiskSize 构造扇区。 |
| 配置类型 | lba11_capacity / disk-size |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba11.uid_payload · lba11_capacity / repair-chs

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x104–0x1ff（252B） |
| 语义类型 | EncryptedRegion |
| 含义 | 加密 UID + 全零填充 |
| 所有权 | owner |
| 解码规则 | 使用 CHS 容量乘积派生加密密钥；解码标准字段。 |
| 编码规则 | 使用 CHS 容量乘积构造扇区。 |
| 配置类型 | lba11_capacity / repair-chs |
| 演进类型 | ProducerChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC |
| 实现来源 | 写入路径决定容量算法。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba11::parse_lba11 |
| 测试符号 | lba11_behavior::lba11_disk_size_and_repair_chs_profiles_replay_physical_gold |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba11_capacity**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| disk-size | owner | 11:000-1ff | ProducerChanged | LBA11 容量密钥输入取决于写入路径，而不是硬件身份。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |
| repair-chs | owner | 11:000-1ff | ProducerChanged | LBA11 容量密钥输入取决于写入路径，而不是硬件身份。 | S-WIN-CURRENT;S-LINUX-DWARF;P-GOLD-ENC |

| 区域（含首尾） | disk-size | repair-chs |
| --- | --- | --- |
| 0x000–0x003 | lba11.drkb_magic: DRKB 魔数 | lba11.drkb_magic: DRKB 魔数 |
| 0x004–0x0ff | lba11.random252: 252 字节随机密钥材料 | lba11.random252: 252 字节随机密钥材料 |
| 0x100–0x103 | lba11.pdkb_magic: PDKB 魔数 | lba11.pdkb_magic: PDKB 魔数 |
| 0x104–0x1ff | lba11.uid_payload: 加密 UID + 全零填充 | lba11.uid_payload: 加密 UID + 全零填充 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。

## LBA12

### 用途与配置类型

- 14 字节 密码信息。
- 3*96B EDPF 表；密钥模式为 A7F0/A6B0。
- 3*96B EDPF 表；密钥模式为 AES-128-ECB。
- 3*96B EDPF 表；密钥模式为 SM4-ECB。
- 3*96B EDPF 表；密钥模式为 v0064 旧版 兼容模式。
- 密文流中表结构之后的全零明文区。

### 512B 布局与字段索引

每行标出范围和配置类型；覆盖项行是对应所有者范围的注解。

| 偏移（含首尾） | 长度 | 字段 ID | 轴 / 状态 | 类型 | 所有权 |
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
| 偏移 | 0x000–0x11f（288B） |
| 语义类型 | EncryptedRegion |
| 含义 | 3*96B EDPF 表；密钥模式为 v0064 旧版 兼容模式。 |
| 所有权 | owner |
| 解码规则 | 使用设备 CRC 解密外层扇区；校验 NeedEncrypt 条目采用 旧版模式0封装密钥 表示。 |
| 编码规则 | 序列化 旧版模式0封装密钥 条目，再使用设备 CRC 加密整个扇区。 |
| 配置类型 | lba12_mode / legacy-v0064 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 旧版 模式0 兼容语义已闭环，但尚无已提交的 LBA12 正向物理采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x11f（288B） |
| 语义类型 | EncryptedRegion |
| 含义 | 3*96B EDPF 表；密钥模式为 A7F0/A6B0。 |
| 所有权 | owner |
| 解码规则 | 按 模式1 解密表。 |
| 编码规则 | 按 模式1 序列化表。 |
| 配置类型 | lba12_mode / mode1 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 模式1/模式3 的正例明确保持为虚拟证据。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x11f（288B） |
| 语义类型 | EncryptedRegion |
| 含义 | 3*96B EDPF 表；密钥模式为 SM4-ECB。 |
| 所有权 | owner |
| 解码规则 | 按 模式2 解密表。 |
| 编码规则 | 按 模式2 序列化表。 |
| 配置类型 | lba12_mode / mode2 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 模式1/模式3 的正例明确保持为虚拟证据。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.edpf_table · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x000–0x11f（288B） |
| 语义类型 | EncryptedRegion |
| 含义 | 3*96B EDPF 表；密钥模式为 AES-128-ECB。 |
| 所有权 | owner |
| 解码规则 | 按 模式3 解密表。 |
| 编码规则 | 按 模式3 序列化表。 |
| 配置类型 | lba12_mode / mode3 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 模式1/模式3 的正例明确保持为虚拟证据。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x120–0x12d（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 密码信息。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段。 |
| 编码规则 | 序列化所选 mode/version 的 密码信息。 |
| 配置类型 | lba12_mode / legacy-v0064 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 旧版 模式0 兼容语义已闭环，但尚无已提交的 LBA12 正向物理采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x120–0x12d（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 密码信息。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段。 |
| 编码规则 | 序列化所选 mode/version 的 密码信息。 |
| 配置类型 | lba12_mode / mode1 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 密码信息 位于连续的扇区加密流内。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x120–0x12d（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 密码信息。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段。 |
| 编码规则 | 序列化所选 mode/version 的 密码信息。 |
| 配置类型 | lba12_mode / mode2 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 密码信息 位于连续的扇区加密流内。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.pass_info · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x120–0x12d（14B） |
| 语义类型 | PackedStruct |
| 含义 | 14 字节 密码信息。 |
| 所有权 | owner |
| 解码规则 | 解码已命名的 密码信息 字段。 |
| 编码规则 | 序列化所选 mode/version 的 密码信息。 |
| 配置类型 | lba12_mode / mode3 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 密码信息 位于连续的扇区加密流内。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / legacy-v0064

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x12e–0x1ff（210B） |
| 语义类型 | CompatibilityField |
| 含义 | 密文流中表结构之后的全零明文区。 |
| 所有权 | owner |
| 解码规则 | 解密后要求兼容区域为写入端规定的全零状态。 |
| 编码规则 | 整扇区加密前先将明文置零。 |
| 配置类型 | lba12_mode / legacy-v0064 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 旧版 模式0 兼容语义已闭环，但尚无已提交的 LBA12 正向物理采集。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode1

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x12e–0x1ff（210B） |
| 语义类型 | CompatibilityField |
| 含义 | 密文流中表结构之后的全零明文区。 |
| 所有权 | owner |
| 解码规则 | 解密后要求兼容区域为写入端规定的全零状态。 |
| 编码规则 | 整扇区加密前先将明文置零。 |
| 配置类型 | lba12_mode / mode1 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 尾部始终加密，不是原始明文。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode2

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x12e–0x1ff（210B） |
| 语义类型 | CompatibilityField |
| 含义 | 密文流中表结构之后的全零明文区。 |
| 所有权 | owner |
| 解码规则 | 解密后要求兼容区域为写入端规定的全零状态。 |
| 编码规则 | 整扇区加密前先将明文置零。 |
| 配置类型 | lba12_mode / mode2 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | P-GOLD-ENC;P-GOLD-NOPWD |
| 实现来源 | 尾部始终加密，不是原始明文。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

#### lba12.zero_padding · lba12_mode / mode3

| 属性 | 目录值 |
| --- | --- |
| 偏移 | 0x12e–0x1ff（210B） |
| 语义类型 | CompatibilityField |
| 含义 | 密文流中表结构之后的全零明文区。 |
| 所有权 | owner |
| 解码规则 | 解密后要求兼容区域为写入端规定的全零状态。 |
| 编码规则 | 整扇区加密前先将明文置零。 |
| 配置类型 | lba12_mode / mode3 |
| 演进类型 | EncodingChanged |
| 写入端证据 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| 消费端证据 | S-WIN-CURRENT;S-LINUX-DWARF |
| 物理证据 | MISSING_PHYSICAL |
| 实现来源 | 尾部始终加密，不是原始明文。 |
| 语义状态 | COMPLETE |
| 实现状态 | COMPLETE |
| 行为测试状态 | COMPLETE |
| 代码符号 | edpcli::protocol::lba12::parse_lba12 |
| 测试符号 | edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct |
| 所有权测试 | protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state |

### 历史演进矩阵

**lba12_mode**

| 状态 | 角色 | 完整轴范围（可跨 LBA） | 演进类型 | 差异说明 | 证据 |
| --- | --- | --- | --- | --- | --- |
| legacy-v0064 | owner | 12:000-1ff | EncodingChanged | LBA12 封装密钥 mode 与所有其他 配置类型 轴相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF |
| mode1 | owner | 12:000-1ff | EncodingChanged | LBA12 封装密钥 mode 与所有其他 配置类型 轴相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |
| mode2 | owner | 12:000-1ff | EncodingChanged | LBA12 封装密钥 mode 与所有其他 配置类型 轴相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12;P-GOLD-ENC;P-GOLD-NOPWD |
| mode3 | owner | 12:000-1ff | EncodingChanged | LBA12 封装密钥 mode 与所有其他 配置类型 轴相互独立。 | S-WIN-CURRENT;S-LINUX-DWARF;V-LBA12 |

| 区域（含首尾） | legacy-v0064 | mode1 | mode2 | mode3 |
| --- | --- | --- | --- | --- |
| 0x000–0x11f | lba12.edpf_table: 3*96B EDPF 表；密钥模式为 v0064 旧版 兼容模式。 | lba12.edpf_table: 3*96B EDPF 表；密钥模式为 A7F0/A6B0。 | lba12.edpf_table: 3*96B EDPF 表；密钥模式为 SM4-ECB。 | lba12.edpf_table: 3*96B EDPF 表；密钥模式为 AES-128-ECB。 |
| 0x120–0x12d | lba12.pass_info: 14 字节 密码信息。 | lba12.pass_info: 14 字节 密码信息。 | lba12.pass_info: 14 字节 密码信息。 | lba12.pass_info: 14 字节 密码信息。 |
| 0x12e–0x1ff | lba12.zero_padding: 密文流中表结构之后的全零明文区。 | lba12.zero_padding: 密文流中表结构之后的全零明文区。 | lba12.zero_padding: 密文流中表结构之后的全零明文区。 | lba12.zero_padding: 密文流中表结构之后的全零明文区。 |

### 实现与测试入口

正式解析器与行为测试以本节各字段的代码/测试符号及状态为准；未实现链接不代表可调用接口。配置类型检测器尚未在目录中登记。

- 所有权：`protocol_field_catalog::field_catalog_has_exact_byte_ownership_for_every_profile_state`（[测试文件](../../tests/protocol_field_catalog.rs)）。
- 测试夹具定位：通过各行物理/写入端/消费端证据 ID 查询 [证据清单](../../audit/protocol/evidence_manifest.tsv)，保留其证据类型和限制。
