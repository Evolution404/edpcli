# EDP Backup Container v1

## 1. 目标

EDPB 是 edpcli 的单文件设备备份容器，扩展名为 .edpb。

设计目标：

1. 一个文件携带设备身份、原始证据、解析结果和完整性信息。
2. 原始盘面字节始终是事实源，解析或解密结果只能作为派生数据。
3. 能表达当前已知区域，也能无格式升级地容纳未来发现的区域 A、区域 B 或其他厂商保留区。
4. 能区分可恢复数据、只用于取证的数据和纯派生数据。
5. 正式运行时不读取旧 .bin，不依赖 .sha256 sidecar。
6. 旧 .bin 仅在一次性离线迁移阶段读取；迁移完成后不保留永久兼容路径。

## 2. 四个核心抽象

### 2.1 DeviceSnapshot

一次对一块物理 U 盘在某个时间点的完整观测。

包含稳定身份、容量、采集时间、采集级别以及 Region、Extent、Artifact 的集合。

### 2.2 Region

源设备上的一个有语义的逻辑区域，例如：

- protocol：LBA0-12 协议区
- partition.type1
- partition.type2
- partition.type4
- tail.A
- device.tail_window
- vendor.unknown.N

Region 描述“这是什么区域”，不表示该区域被全量读取。

### 2.3 Extent

一次实际读取的连续物理范围，以 start_lba + sector_count 表示。

Extent 只描述源设备坐标，不保存解析语义。一个 Region 可以包含多个不连续 Extent。

### 2.4 Artifact

最终存入 EDPB 的一份内容。

Artifact 可以是：

- 原始扇区
- 解密后的扇区
- 文件系统元数据
- 空间占用统计
- 文件列表
- 协议解析结果

一个 Artifact 可以来源于一个或多个 Extent，也可以由其他 Artifact 派生。

## 3. 文件总体布局

所有固定整数均使用 little-endian。

顺序为：

1. 96B Fixed Header
2. 一个或多个 64B Chunk Header + Chunk Payload
3. UTF-8 JSON Manifest
4. 80B Fixed Footer

Manifest 位于尾部，Header 和 Footer 都保存 Manifest 的位置、长度和 SHA-256。

## 4. Fixed Header

固定长度 96B。

| Offset | Size | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | Magic: EDPB + CR LF + 0x1A + LF |
| 0x08 | 2 | Major version，当前 1 |
| 0x0A | 2 | Minor version，当前 0 |
| 0x0C | 4 | Header size，当前 96 |
| 0x10 | 8 | Manifest offset |
| 0x18 | 8 | Manifest length |
| 0x20 | 8 | Footer offset |
| 0x28 | 8 | Capture epoch seconds |
| 0x30 | 32 | Manifest SHA-256 |
| 0x50 | 16 | Reserved，v1 必须为 0 |

## 5. Chunk Header

每个有实体内容的 Artifact 对应一个 framed chunk。

固定长度 64B。

| Offset | Size | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | Magic: EDPCHNK + LF |
| 0x08 | 2 | Chunk frame version，当前 1 |
| 0x0A | 2 | Codec，0 = none |
| 0x0C | 4 | Reserved |
| 0x10 | 8 | Stored length |
| 0x18 | 8 | Original length |
| 0x20 | 32 | 解码后原始 payload 的 SHA-256 |

v1 当前只写 codec=none。未来可以增加压缩 codec，而不改变外层容器结构。

## 6. Fixed Footer

固定长度 80B。

Footer 重复最关键的定位信息，使 Header 局部损坏时仍有恢复 Manifest 的可能。

| Offset | Size | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | Magic: EDPBFTR + LF |
| 0x08 | 2 | Major version |
| 0x0A | 2 | Minor version |
| 0x0C | 4 | Reserved |
| 0x10 | 8 | Manifest offset |
| 0x18 | 8 | Manifest length |
| 0x20 | 8 | Complete file size |
| 0x28 | 32 | Manifest SHA-256 |
| 0x48 | 8 | Reserved |

## 7. Manifest

Manifest schema 名为 edpb.manifest.v1。

### 7.1 Snapshot

必须记录：

- snapshot_id
- created_epoch
- capture_level
- device_state

capture_level：

- core
- metadata
- deep
- legacy_migrated

### 7.2 Device identity

必须记录：

- VID
- PID
- device_id
- onlyid

这些字段位于容器内部，文件名不参与可信身份判断。

### 7.3 Geometry

记录：

- logical_sector_size
- physical_sector_size（可未知）
- total_sectors
- capacity_bytes

### 7.4 Observation

记录本次观测值：

- disk_number
- platform
- edpcli_version

diskN 只代表本次系统枚举，不是稳定设备身份。

### 7.5 Region

每项至少包含：

- id
- role
- start_lba，可未知
- sector_count，可未知
- semantic_status

semantic_status：

- identified
- unknown

未知区域允许完整保存原始字节，但不得伪造含义。

### 7.6 Extent

每项包含：

- id
- region_id
- start_lba
- sector_count
- purpose

Extent 必须引用存在的 Region。

### 7.7 Artifact

每项包含：

- id
- kind
- media_type
- source_extent_ids
- derivation
- restore_policy
- completeness
- storage

restore_policy：

- restorable：允许显式恢复流程使用
- evidence_only：只用于证据和分析，默认禁止写回
- derived_only：派生结果，禁止写回

completeness：

- complete
- partial
- not_captured

storage 包含：

- frame_offset
- data_offset
- stored_length
- original_length
- codec
- sha256

## 8. Core 级强制内容

所有原生创建的 EDPB 至少必须包含：

- Region: region.protocol
- Extent: extent.protocol.lba0_12
- start_lba = 0
- sector_count = 13
- Artifact: raw.protocol.lba0_12
- 原始长度 = 13 × logical_sector_size
- restore_policy = restorable

恢复时必须读取 raw.protocol.lba0_12 的原始 payload。

严禁从解析 JSON 重新拼装 LBA0-12 后写盘。

## 9. Metadata 级规划

Metadata 级在 Core 之上增加关键盘面证据，目标包括：

- type1/type2/type4 的几何信息
- 各数据分区文件系统头和必要元数据 Extent
- FAT / allocation bitmap / root directory 等关键元数据
- 区域 A
- 设备尾部窗口
- 未来发现的其他未知 Region

Metadata 不要求复制整个数据分区。

## 10. Deep 级规划

Deep 在 Metadata 之上增加可派生信息：

- 可用时的解密视图
- 文件系统类型
- 总空间、已用空间、空闲空间
- 文件数量、目录数量
- 完整文件列表
- 文件属性、大小、时间、可选哈希

Deep 默认不备份所有用户文件内容。

原始密文 Artifact 与解密后的派生 Artifact 必须分别保存。

## 11. 区域 A 和未知尾部

未知区域首先按 evidence_only 原始证据保存。

例如：

- Region id = tail.A
- semantic_status = unknown
- start_lba / sector_count 使用实际探测结果
- Artifact kind = raw_sectors
- restore_policy = evidence_only

以后即使识别出其语义，也只需增加新的解析器或派生 Artifact，不需要改变 EDPB 文件结构。

除了明确的区域 A，建议 Metadata 级另保存一个可配置的 device.tail_window，便于未来重新研究尚未识别的尾部结构。

## 12. 完整性规则

EDPB verify 必须至少检查：

1. Header magic/version/长度
2. Footer magic/version/文件长度
3. Header 与 Footer 的 Manifest locator 一致
4. Manifest SHA-256
5. Region/Extent/Artifact 引用图
6. Chunk magic/version/codec
7. Chunk offset 和长度边界
8. 每个 Artifact payload SHA-256
9. 必需 Core Artifact 存在且长度正确
10. 不允许 Artifact storage 非法重叠

Manifest 中的身份字段还应与 raw LBA0-12 能验证的字段做交叉一致性检查。

## 13. 恢复安全

EDPB 是备份容器，不代表每个 Artifact 都可恢复。

只有 restore_policy=restorable 的 Artifact 才能进入写盘计划。

evidence_only 和 derived_only 不得被普通 restore 自动写回。

区域 A、未知尾部区域和未知厂商区默认 evidence_only。

恢复到物理盘前仍必须执行现有的目标盘身份复核、reopen 后复核、原子写、sync/readback/rollback 安全链。

## 14. 单文件要求

EDPB 不生成 .sha256 sidecar。

所有完整性信息都在 .edpb 内部。

用户重命名 .edpb 不影响设备身份、校验、列表归组和恢复判断。

## 15. 旧 .bin 规则

正式运行时：

- backup list 不列出 .bin
- backup verify 不接受 .bin
- backup restore 不接受 .bin
- info/inspect 不把 .bin 当备份源

旧 .bin 仅由一次性迁移工具读取。

迁移后的容器使用 capture_level=legacy_migrated，并明确标记原文件中不存在的分区元数据、尾部区域、文件系统统计和文件列表为 not_captured。

迁移过程中不得猜测或填零冒充真实采集数据。

## 16. 一次性本机迁移验收

新格式实现完成后，对本机旧备份执行一次性迁移：

1. 扫描现有 .bin 和其历史 sidecar。
2. 校验历史文件。
3. 把 6656B 原始 LBA0-12 逐字节写入 EDPB raw Artifact。
4. 从可验证来源填入设备身份。
5. 缺失的新字段标记 not_captured。
6. 对新 EDPB 重新执行完整 verify。
7. 输出旧文件到新文件的映射、旧 SHA-256、新 EDPB SHA-256和成功/失败统计。
8. 全部验证成功前不得删除旧文件。

正式发行代码不依赖该迁移器。
