# EDP 备份容器 v1

## 1. 目标

EDPB 是 edpcli 的单文件设备备份容器，扩展名为 `.edpb`。

设计目标：

1. 一个文件携带设备身份、原始证据、解析结果和完整性信息。
2. 原始盘面字节始终是事实源，解析或解密结果只能作为派生数据。
3. 能表达当前已知区域，也能在不升级容器格式的前提下容纳未来发现的其他厂商保留区。
4. 能区分可恢复数据、只用于取证的数据和纯派生数据。
5. 正式运行时不读取旧 `.bin`，不依赖 `.sha256` 旁挂文件。
6. 旧 `.bin` 仅在一次性离线迁移阶段读取；迁移完成后不保留永久兼容路径。

## 2. 四个核心抽象

### 2.1 `DeviceSnapshot`

一次对一块物理 U 盘在某个时间点的完整观测。

包含稳定身份、容量、采集时间、采集级别，以及 `Region`、`Extent`、`Artifact` 的集合。

### 2.2 `Region`

源设备上的一个有语义的逻辑区域，例如：

- `protocol`：LBA0-LBA12 协议区
- `partition.type1`
- `partition.type2`
- `partition.type4`
- `lba7_compatibility_extent`
- `device.tail_metadata_mirror` / `device.tail_restore_node`
- `vendor.unknown.N`

`Region` 描述“这是什么区域”，不表示该区域被全量读取。

### 2.3 `Extent`

一次实际读取的连续物理范围，以 `start_lba + sector_count` 表示。

`Extent` 只描述源设备坐标，不保存解析语义。一个 `Region` 可以包含多个不连续 `Extent`。

### 2.4 `Artifact`

最终存入 EDPB 的一份内容。

`Artifact` 可以是：

- 原始扇区
- 解密后的扇区
- 文件系统元数据
- 空间占用统计
- 文件列表
- 协议解析结果

一个 `Artifact` 可以来源于一个或多个 `Extent`，也可以由其他 `Artifact` 派生。

## 3. 文件总体布局

所有固定整数均使用小端序。

顺序为：

1. 96B 固定头
2. 一个或多个 64B 块头 + 块负载
3. UTF-8 JSON 清单
4. 80B 固定尾

清单位于尾部，固定头和固定尾都保存清单的位置、长度和 SHA-256。

## 4. 固定头

固定长度 96B。

| 偏移 | 大小 | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | 魔数：EDPB + CR LF + 0x1A + LF |
| 0x08 | 2 | 主版本，当前 1 |
| 0x0A | 2 | 次版本，当前 0 |
| 0x0C | 4 | 头长度，当前 96 |
| 0x10 | 8 | 清单偏移 |
| 0x18 | 8 | 清单长度 |
| 0x20 | 8 | 固定尾偏移 |
| 0x28 | 8 | 采集时间戳（秒） |
| 0x30 | 32 | 清单 SHA-256 |
| 0x50 | 16 | 保留区，v1 必须为 0 |

## 5. 块头

每个有实体内容的 `Artifact` 对应一个带帧的块。

固定长度 64B。

| 偏移 | 大小 | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | 魔数：EDPCHNK + LF |
| 0x08 | 2 | 块帧版本，当前 1 |
| 0x0A | 2 | 编码方式，0 = none |
| 0x0C | 4 | 保留 |
| 0x10 | 8 | 存储长度 |
| 0x18 | 8 | 原始长度 |
| 0x20 | 32 | 解码后原始负载的 SHA-256 |

v1 当前只写 `codec=none`。未来可以增加压缩编码，而不改变外层容器结构。

## 6. 固定尾

固定长度 80B。

固定尾重复最关键的定位信息，使固定头局部损坏时仍有恢复清单的可能。

| 偏移 | 大小 | 含义 |
| --- | ---: | --- |
| 0x00 | 8 | 魔数：EDPBFTR + LF |
| 0x08 | 2 | 主版本 |
| 0x0A | 2 | 次版本 |
| 0x0C | 4 | 保留 |
| 0x10 | 8 | 清单偏移 |
| 0x18 | 8 | 清单长度 |
| 0x20 | 8 | 完整文件大小 |
| 0x28 | 32 | 清单 SHA-256 |
| 0x48 | 8 | 保留 |

## 7. 清单

新建备份使用 `edpb.manifest.v3`，`backup_purpose=metadata_only`，`capture_level=metadata`。读取器继续接受历史 v1/v2 清单；历史 `core`、`deep` 和 `legacy_migrated` 是来源分类，不是新建备份的产品等级。v1/v2 不能伪装成 v3 合约。

### 7.1 快照与恢复合约

`snapshot` 记录 `snapshot_id`、`created_epoch`、`capture_level` 和 `device_state`。v3 的 `restore_contract` 明确表示恢复分区结构、按盘型恢复 EDP 协议、不恢复文件系统、不恢复用户数据，并要求恢复后只读评估。`partitions` 逐项记录索引、角色、分区类型、精确起点与扇区数；文件系统和卷标字段只是提示，不能替代盘面验证。

### 7.2 设备身份

`identity.hardware` 保存可用的 VID/PID、USB 序列号原文、序列号质量、厂商/产品/修订、传输方式及硬件几何。v3 不生成或持久化新的 `serial_sha256`；v1/v2 中已有的历史摘要仍可读取并用于兼容匹配。序列号原文仅在备份容器和必要的内存身份对象中使用，不能写入普通日志或提权参数。`identity.protocol` 保存可验证的 `device_id`、`onlyid`、制盘类型和 LBA4 身份摘要；文件名与系统临时 `diskN` 均不构成可信身份。

### 7.3 几何与观测

`geometry` 记录逻辑扇区大小、可知时的物理扇区大小、总扇区数和容量。`observation` 记录本次盘号、平台和程序版本。恢复前后均以当前物理盘重新观测的几何为准。

### 7.4 `Region`、`Extent`、`Artifact`

`Region` 描述物理区域和已识别/未知语义；`Extent` 使用起点、扇区数和用途引用存在的 `Region`；`Artifact` 使用来源 `Extent`、派生关系、完整性、存储位置和 SHA-256 描述容器负载。`restore_policy=restorable` 才能进入恢复写盘计划；`evidence_only` 与 `derived_only` 不得由普通恢复自动写回。未知字节可以保留为证据，但不得伪造结构语义。

## 8. 新建元数据备份的固定范围

Plain MBR 盘保存原始 LBA0 和类型化分区几何；Plain GPT 盘保存 protective MBR、主 header/entry array、备份 entry array/header，并校验主备 header CRC、分区数组 CRC、disk GUID 与几何一致性。Plain 不要求也不保存 EDP LBA0～12 协议工件。

EDP 盘保存原始 LBA0～12、由 LBA7 指针确认的 6 扇区 LCE，以及已识别的盘尾历史镜像与恢复节点。LBA0～12 内的 PassInfo、NeedEncrypt、EncryptMode、wrapped FileKey 与 CRC 保持原始字节；创建备份不要求用户密码，也不解包 FileKey。

新建元数据备份不读取或保存数据分区文件系统引导扇区、FAT、分配位图、目录或普通文件负载。不能把 `filesystem_hint` 当作文件系统已备份的证据。

## 9. 元数据采集与缺失处理

采集器只读访问源盘，并从已验证的 MBR/GPT 或 EDPF/LBA12 结构生成类型化分区几何。读取的原始 `Artifact` 必须有完整范围、字节长度和哈希；已识别但读取失败的范围必须记录采集问题，不能用零字节冒充成功。恢复计划只使用已验证且声明可恢复的原始扇区工件。

制盘前强制备份与手工 `backup create` 使用同一元数据语义；备份创建、容器校验和目标身份确认成功以后，制盘才允许进入卸载锁卷与写盘阶段。

## 10. 历史深度分析

旧版 `capture_level=deep` 仍可读；CLI `--deep` 已弃用，仅保留历史只读分析兼容，TUI 不再提供新建深度备份。其文件系统元数据与派生清单规则见 [DEEP_BACKUP_V1.md](DEEP_BACKUP_V1.md)。历史深度工件不改变新建备份只保存元数据的产品定义，也不使用户文件负载自动成为可恢复内容。

## 11. LCE 与已识别盘尾结构

LCE 是 LBA7 后续条目指向的固定 6 扇区兼容物理块。LBA7 指针是位置事实源；CHS 公式只作一致性交叉验证。type2/type4 可指向同一物理 LCE，但仍保留各自逻辑角色。新 v3 EDP 元数据备份把原始 LCE 标记为可恢复，恢复前复核 LBA7 指针和清单范围。

盘尾仅保存已确认位置和语义的两个独立对象：`total_sectors - 1024` 起连续 9 扇区的历史 LBA4/LBA12 镜像，以及 `total_sectors - 4` 的单扇区历史恢复节点。新 v3 EDP 将二者作为可恢复原始协议工件。相邻未知空间不能伪装成协议结构；IIR 也不能与 LCE 混同。

## 12. 完整性规则

校验器检查固定头/尾版本与长度、清单定位和 SHA-256、引用图、块边界与编码、每个负载的 SHA-256、存储范围不重叠，以及 v3 的备份目的、恢复合约和分区几何。Plain 必须符合 MBR/GPT 工件规则；EDP 必须符合 LBA0～12、LCE 与已识别盘尾规则。旧版容器使用各自 schema 的校验条件，不能通过伪装字段绕过 v3 约束。

容器校验成功不代表目标可写；恢复还要独立验证备份与当前物理盘身份、精确扇区大小和总几何。

## 13. 恢复与后续操作安全

恢复按 `restore_policy=restorable` 的原始扇区 `Artifact` 生成事务计划。Plain 写回分区表元数据；EDP 写回 LBA0～12、确认的 LCE 和盘尾协议对象。LBA0 最后提交，事务同步、逐扇区读回，失败时回滚。显式备份路径不能绕过强物理身份和精确几何；卸载锁盘后重新打开并再次观察身份与盘面。

元数据恢复成功仅表示元数据事务及读回成功。恢复后的只读评估单独区分 `Usable`、`NeedsFormat`、`PasswordRequired`、`CryptoMetadataInvalid` 与 `Unsupported`。评估或后续格式化失败不能反转已验证的元数据恢复结果；元数据备份不保证原文件系统或用户数据可挂载。

格式化需要用户逐分区选择并独立确认，应用层再次核对身份和几何；只写所选分区。加密分区沿用原密钥域时必须先验证密码、wrapped FileKey 与 FileKeyCRC。清空并重建加密分区另需新密码双输入、独立破坏性确认、全新 FileKey 与 LBA7/LBA12 记录，并通过事务读回及新密码验证；`--yes` 不跳过该确认。当前真实文件系统写入只开放已验证的 FAT16/exFAT 和 SM4 加密路径。

## 14. 单文件要求

EDPB 不生成 `.sha256` 旁挂文件；完整性信息在 `.edpb` 内部。用户重命名文件不改变容器内的身份、校验和恢复授权。

## 15. 旧 `.bin` 规则

正式运行时，`backup list/verify/restore` 与 `info/inspect` 均不把旧 `.bin` 当作 EDPB。旧 `.bin` 只由一次性迁移工具读取。迁移结果使用 `capture_level=legacy_migrated`，不存在的分区、盘尾、文件系统统计和文件列表必须标记 `not_captured`，不能猜测或填零冒充真实采集。

## 16. 一次性本机迁移验收

历史 `.bin` 迁移须校验旧文件和旁挂校验值，逐字节保存 6656B LBA0～12，填入可验证的身份，明确标记缺失字段，并对新 EDPB 完整校验。记录旧/新文件映射和 SHA-256；全部验证成功前不得删除旧文件。正式运行代码不依赖该迁移工具。

## 17. 深度分析兼容状态

[DEEP_BACKUP_V1.md](DEEP_BACKUP_V1.md) 记录历史深度采集、只读文件系统分析和离线重放的边界。该能力仅作兼容/分析使用；新备份入口只有元数据备份，历史 v1/v2/深度 EDPB 仍可由读取器按各自清单结构校验。

## 18. 元数据备份重构实施记录与约束

> 本节保留 2026-09-28 起的需求、实施顺序和验收记录。第 1～17 节描述当前格式与行为；本节各阶段以对应实施状态为准。真实盘验收仍按 18.18～18.19 单独执行。

### 18.1 重构目标

备份职责收敛为一种：**元数据备份**。

它只负责：

1. 保存物理介质身份与几何；
2. 保存磁盘分区结构；
3. 对 EDP 盘保存可验证的 EDP 协议元数据和已识别协议扩展区；
4. 在恢复时把上述元数据原样恢复；
5. 恢复完成后检查各数据分区是否需要格式化，并引导用户执行后续格式化。

它明确**不负责**：

- 恢复文件系统；
- 恢复目录；
- 恢复文件名；
- 恢复 FAT / allocation bitmap / MFT 等用户文件分配关系；
- 恢复普通用户文件数据；
- 承诺恢复后数据分区立即可挂载；
- 承诺恢复前后的数据分区内容一致。

产品语义固定为：

```text
backup create = 创建元数据备份
backup restore = 恢复磁盘/EDP 元数据
```

文件系统和用户数据不属于备份恢复职责；恢复后需要时由用户明确确认后格式化。

### 18.2 已验证问题与重构原因

2026-09-28 对真实 `disk5` 的 Plain → EDP → Plain 恢复实验证明旧版 UI/恢复语义存在误导：

- 原 Plain MBR 指向 `LBA2048`；
- 当时的 Plain EDPB 只采集 `LBA0～12`；
- 制盘后，原 `LBA2048` 文件系统已不存在；
- 当时的 `backup restore` 只恢复 `LBA0～12`，因此 MBR 再次指向 `LBA2048`，但该位置没有有效 exFAT；
- `diskutil` 仍能看到分区，`fsck_exfat` 报主/备用 boot region 无效，卷不能挂载；
- 原 EDP 制盘阶段创建的 exFAT 仍残留在 `LBA63`，但恢复后的 MBR 不再指向它。

因此旧提示“恢复成功，请拔出重插”不准确。当前实现区分：

- **元数据恢复成功**；
- **文件系统是否可用**；
- **是否需要格式化**。

元数据恢复成功不能再等价于“卷已经可以挂载”。

### 18.3 单一备份类型

重构完成后，新创建的 EDPB 只对用户暴露一种备份：

```text
元数据备份
```

用户命令保持：

```text
edpcli backup create
edpcli backup restore
edpcli backup list
edpcli backup verify
```

新产品默认入口只创建元数据备份；历史分析兼容策略：

1. TUI 移除 `Deep` 入口；
2. CLI `--deep` 已标为弃用，但暂保留旧的只读深度采集路径；
3. 不允许把 `--deep` 静默映射为另一种语义后仍显示为 `Deep`；
4. 历史 `capture_level=deep` EDPB 继续支持 `list / verify / info / inspect`；
5. 历史 `Deep` 容器中的目录/统计派生数据只作为兼容只读信息，新的 `restore` 不依赖这些派生数据。

### 18.4 新备份保存内容

#### 18.4.1 所有盘型共用内容

所有新元数据备份至少保存：

- typed media identity：VID / PID、USB serial 原始字符串和 `SerialQuality`、vendor / `product` / revision、`transport`；
- 设备几何：`logical_sector_size`、可选 `physical_sector_size`、`total_sectors`、`capacity_bytes`；
- 采集平台、时间、edpcli 版本；
- 磁盘分区方案；
- 每个分区的序号、`start_lba`、`sector_count`、partition type / role；
- 可识别时保存 `filesystem_hint` 和 `volume_label_hint`，仅用于恢复后的格式化建议。

USB 序列号在新 `manifest` 中**直接保存原始字符串，不做 SHA-256 哈希**。`SerialQuality` 继续用于区分 `Usable / Suspicious / Missing`；当质量为 `Missing` 时 `serial = null`。恢复授权优先比较可用序列号的直接值，不能再依赖不可逆摘要。历史 v1/v2 中已有 `serial_sha256` 继续只读兼容，但新 v3 不再写 `serial_sha256`。

原始扇区和派生描述必须继续分层：可恢复的原始协议/分区表字节是事实源，`filesystem_hint` 只能作为提示，不能反向生成协议字节。

#### 18.4.2 Plain 盘

Plain 盘保存的恢复事实源是**分区元数据**。

MBR 盘至少保存：

- LBA0 原始 MBR；
- 解析后的分区条目作为交叉验证；
- 当前磁盘几何和身份。

GPT 盘至少保存：

- protective MBR；
- primary GPT header；
- primary partition entry array；
- backup GPT header；
- backup partition entry array；
- 两端 CRC / 几何一致性结果。

Plain 盘不再把“固定 LBA0～12”定义成完整恢复范围。`LBA1～12` 如果不是分区方案本身需要的元数据，只作为历史兼容/证据处理。

Plain 盘不保存分区 boot region、FAT、allocation bitmap、root directory、目录项和普通文件数据。

#### 18.4.3 EDP 盘

EDP 盘继续保存所有已经由协议事实源确认、且恢复所需的原始元数据：

- LBA0～12 原始协议区；
- LCE，按真实 LBA7 盘内指针采集；
- 已识别的盘尾协议/恢复结构；
- EDP 模式和分区几何；
- `onlyid`、`device_id`；
- PassInfo；
- `NeedEncrypt / EncryptMode`；
- `UserKeyCRC / FileKeyCRC`；
- wrapped FileKey / 旧版兼容密钥材料；
- 其他已经在协议总文档和机器账本中闭环、且恢复确实需要的原始字节。

EDP 元数据备份**不要求知道用户密码**。备份阶段不提示密码、不猜密码、不要求解出 FileKey；wrapped key material 按盘面原样保存。无法解密数据分区不能被解释成“备份不完整”。

### 18.5 明确不采集的内容

新的元数据备份禁止扩大成文件系统备份。默认不采集：

- 普通文件系统 boot region；
- FAT16/FAT32 FAT；
- exFAT 的 FAT、分配位图和大小写转换表；
- NTFS MFT / Bitmap；
- 文件系统目录元数据；
- 文件名、路径、时间戳列表；
- 用户文件 `payload`；
- 空闲区、slack、删除文件残留。

`DEEP_BACKUP_V1.md` 中的目录遍历、文件数量、空间统计等能力不再属于新的默认备份主路径。

### 18.6 EDPB `manifest` 演进

外层 EDPB 容器二进制格式继续保持 v1；新的 `writer` 建议升级为 `edpb.manifest.v3`，明确新的恢复契约，避免旧程序把新备份误解成旧的“恢复 LBA0～12”语义。

v3 至少新增：

```text
backup_purpose = metadata_only

identity.hardware:
  serial = "设备真实序列号" | null
  serial_quality = usable | suspicious | missing
  # v3 不再写 serial_sha256

restore_contract:
  partition_metadata = true
  edp_protocol_metadata = true/false
  filesystem = false
  directory_metadata = false
  user_file_data = false
  post_restore_format_may_be_required = true

partitions[]:
  index
  role
  partition_type
  start_lba
  sector_count
  encrypted
  filesystem_hint
  volume_label_hint
```

兼容要求：

- 新 `writer` 只写 `manifest` v3；
- v3 直接保存 `identity.hardware.serial`；不生成新的 `serial_sha256`；
- `reader` 继续读取 v1 / v2；
- v1 / v2 不得被伪装成 v3；
- 旧 `core` Plain 备份在 UI 中显示为“旧版核心元数据”，不能显示成“完整元数据备份”；
- 历史 `Deep` 容器继续保留原始 `capture_level`；
- 旧文件不就地修改。

### 18.7 恢复主流程

新的 `restore` 分为两个独立阶段：

```text
A. 元数据恢复
B. 恢复后分区处理
```

A 成功后，即使用户取消 B，也必须报告“元数据恢复成功”。

#### 18.7.1 A：元数据恢复安全链

恢复前继续执行当前严格安全链：

1. 验证 EDPB 完整性；
2. `canonical` media identity 判断备份与目标盘关系；
3. usable serial、VID/PID、`geometry` 硬冲突必须拒绝；
4. 精确检查总扇区数和逻辑扇区大小；
5. 用户确认；
6. 卸载/锁定前再次检查；
7. reopen 后重新观察目标盘并再次授权；
8. 构建 typed `WriteTransactionPlan`；
9. 原子写、sync、读回校验；
10. 失败按现有 transaction 规则回滚。

Plain 只恢复 MBR/GPT 等分区元数据，不写文件系统区域。

EDP 恢复 LBA0～12及本次协议事实源明确标记为 `restorable` 的 LCE / 尾部协议对象，不写普通数据分区文件系统内容，也不因为密码未知而拒绝元数据恢复。

A 完成后的事件统一为：

```text
元数据恢复成功
```

禁止继续只显示“已恢复，请拔出重插”。

### 18.8 恢复后的分区状态

元数据读回成功后进入只读评估，不把评估失败回算成元数据恢复失败。

每个数据分区归类为：

```text
Usable
NeedsFormat
PasswordRequired
CryptoMetadataInvalid
Unsupported
```

- `Usable`：当前分区可按恢复后的结构正常识别，不强迫格式化；
- `NeedsFormat`：结构已恢复，但没有可用文件系统，建议用户格式化；
- `PasswordRequired`：EDP 加密域元数据存在，但没有可验证的原密码/FileKey；
- `CryptoMetadataInvalid`：key record / CRC / 协议结构本身不一致；
- `Unsupported`：当前版本无法安全判断，禁止猜测。

Plain 或 EDP 明文分区没有可识别文件系统时进入 `NeedsFormat`。EDP 加密分区必须先处理密钥域，不能把密文直接当普通文件系统检查。

### 18.9 格式化是恢复后的独立操作

格式化不属于 `backup restore` 的写入事务本身。

```text
元数据恢复成功
↓
只读检查分区状态
↓
显示恢复后处理
↓
用户逐分区决定是否格式化
```

格式化必须：

- 单独确认；
- 复用现有 provision/format 共享实现；
- 不另写第二套 FAT/exFAT 格式化器；
- 格式化失败不改变“元数据恢复成功”的结论；
- 每个分区分别报告成功、失败或跳过。

Plain 示例：

```text
✓ 元数据恢复成功

disk5s1
  原布局        LBA2048 + 245757952 sectors
  文件系统提示  exFAT
  当前状态      需要格式化

该备份不包含文件系统和用户数据。
是否现在格式化为空的 exFAT？
[格式化] [稍后处理]
```

`filesystem_hint` 只用于建议；没有可靠提示时必须让用户选择，不能猜测。

### 18.10 EDP 加密分区与密码

#### 18.10.1 备份时

不知道密码也必须能完成 EDP 元数据备份，只要协议元数据本身可读取且完整。密码不是备份前置条件。

#### 18.10.2 恢复元数据时

恢复 wrapped key、CRC、PassInfo 等原始结构仍然不要求密码。

#### 18.10.3 恢复后准备格式化时

如果该分区 `NeedEncrypt != 0`：

1. 尝试已有证据允许的默认密码路径；
2. 若需要用户密码，提示输入原密码；
3. 使用 `UserKeyCRC` 验证密码；
4. 按 `EncryptMode` 解包 FileKey；
5. 使用 `FileKeyCRC` 验证 FileKey；
6. 只有全部通过，才能使用**原密钥域**创建新的空文件系统。

成功时原密码和原 FileKey 都保持有效，只重建空文件系统。

### 18.11 解密失败必须明确分类

禁止把所有失败统一显示为“解密失败”。至少区分：

- 未提供密码：`需要原密码`；
- `UserKeyCRC` 不通过：`原密码验证失败`；
- FileKey 解包后 CRC 不一致：`密钥记录校验失败`；
- EDP key record / 协议结构损坏：`加密元数据异常`。

UI 示例：

```text
保密区无法验证原加密密钥

元数据恢复已经成功，但当前不能安全格式化该加密分区。

请选择：
  输入原密码重试
  暂不处理
  清空并重建加密分区
```

密码未知/密码错误不能显示成磁盘损坏；协议结构损坏也不能显示成密码错误。

### 18.12 清空并重建加密分区

这是恢复后的独立破坏性操作，不属于 `restore`。

只有用户主动选择“清空并重建加密分区”才进入该流程。必须再次明确提示：

- 原密码关系将作废；
- 原 FileKey 将作废；
- 分区将变成新的空加密分区；
- 原残留数据以后不能通过新密钥访问。

然后**强制要求用户输入新密码两次**：

```text
新密码：
确认新密码：
```

两次一致并通过当前密码规则后，复用现有 provision key-domain 单一事实源：

1. 生成新的 FileKey；
2. 根据新密码生成 `UserKeyCRC`；
3. 使用当前协议规定的 password derivation / wrap 算法包装新 FileKey；
4. 生成新的 wrapped FileKey；
5. 计算 `FileKeyCRC`；
6. 更新 LBA7 / LBA12 等对应密钥记录；
7. 按原分区位置/容量创建新的空加密文件系统；
8. sync；
9. 读回并重新验证新密码 → FileKey → 文件系统链。

禁止自动生成一个用户不知道的密码；没有新密码不得重建；不得只改 CRC 不更新 FileKey；不得绕过加密写入链。

### 18.13 TUI 重构

#### 18.13.1 备份页

新备份只显示“元数据备份”。历史文件可显示“旧版核心元数据 / 历史 `Metadata` / 历史 `Deep`”，这些标签只描述来源，不代表新产品有多个备份等级。

备份详情固定显示：

```text
恢复内容
  ✓ 物理身份/几何
  ✓ 分区结构
  ✓ EDP 协议（如适用）
  ✗ 文件系统
  ✗ 目录
  ✗ 用户文件
```

#### 18.13.2 制盘进度

当前“制盘前备份”改为“制盘前元数据备份”：

```text
制盘前元数据备份
  读取介质身份
  保存分区/协议元数据
  校验 EDPB
  校验备份身份
```

只有备份创建和身份校验成功后才允许进入 `LockAndReopen`。

#### 18.13.3 恢复完成页

Plain：

```text
✓ 元数据恢复成功

分区        状态
disk5s1     需要格式化

该备份不包含文件系统和用户数据。
Enter 处理选中分区
Esc   稍后处理
```

EDP：

```text
✓ 元数据恢复成功

区域             状态
启动/交换区      需要格式化
保密区           需要原密码
```

正常页面仍遵守现有 UI 规则，不堆叠完整快捷键说明；详细键位放 `?` 帮助。

### 18.14 CLI 重构

`backup create`：

```text
✓ 元数据备份完成
  分区结构：已保存
  EDP 协议：已保存 / 不适用
  文件系统：不包含
  用户数据：不包含
```

`backup restore`：

```text
✓ 元数据恢复完成
⚠ 数据分区未恢复文件系统；部分分区可能需要格式化
```

CLI 默认不自动格式化。后续格式化必须显式执行或交互确认。

### 18.15 Application/domain 拆分

建议引入 application-owned typed 结果，避免 TUI/CLI 自己解析错误字符串：

```text
MetadataBackupReport
MetadataRestoreReport
PostRestorePartitionState
PostRestoreAssessment
PartitionFormatRequest
EncryptedPartitionReinitializeRequest
```

恢复服务只返回已写回的 `metadata` `artifact`、读回结果、fresh identity、分区列表和后续状态。前端不得根据错误字符串猜测 `NeedsFormat / PasswordRequired`。

### 18.16 安全边界

以下边界保持硬门槛：

1. `backup restore` 仍要求强物理身份和精确 `geometry`；
2. protocol identity 不能覆盖 serial/VID/PID hard conflict；
3. explicit backup path 不能绕过 `restore` authorization；
4. reopen 后必须 fresh observe；
5. `restore` 只写 `manifest` 声明 `restorable` 的 `metadata` `artifact`；
6. 分区格式化必须在 `restore` transaction 完成以后单独开始；
7. EDP 加密格式化必须先验证 FileKey；
8. FileKey 无法验证时禁止试探性写入；
9. 加密重建必须新密码二次输入并生成全新 FileKey；
10. post-`restore` format/reinitialize 失败不得回滚已经成功并读回验证的 `metadata` `restore`；
11. format/reinitialize 自己仍需原子写和读回安全门槛；
12. 系统盘保护、USB 目标固定、sudo 边界继续沿用现有实现。

### 18.17 实施顺序 B0 → B10

B0～B10 按阶段实施并小步提交；以下保留每阶段的验收证据。

**B0 — 冻结真实故障回归**

**实施状态（2026-09-28）：COMPLETE。** 已增加 disk5 根因夹具，固定“恢复 MBR/分区起点后，LBA2048 文件系统仍可能无效”的事实；CLI/TUI 完成事件改为“元数据恢复成功 + 文件系统未恢复”，不再提示“拔出重插”；制盘 mandatory backup 的 application 顺序测试确认备份完成后才进入 commit/`LockAndReopen`。Focused `tui_suite chapter_18_b0` 3/3 PASS，`cargo fmt --all`、`git diff --check` PASS。

- 固定 disk5 Plain → EDP → `restore` 后 MBR 正确但文件系统不可挂载的真实场景；
- `restore` 完成事件不能再声称卷已经可用；
- 制盘前 backup 必须发生在 `LockAndReopen` 前；
- 备份失败必须 0 destructive writes。

**B1 — `Manifest` v3 / typed `restore` contract**

**实施状态（2026-09-28）：COMPLETE。** 新 `writer` 已切换到 `edpb.manifest.v3`，写入 `backup_purpose=metadata_only`、typed `restore_contract` 与 partition typed 容器；v3 `identity.hardware.serial` 保存可用 USB serial 原始字符串且不生成/持久化新的 `serial_sha256`。运行时 raw serial 仅保留于内存并从通用 serde/Debug 中隐藏，避免进入 elevation argv/lineage/log；`restore` identity matcher 直接比较 v3 usable raw serial，v1/v2 继续只读兼容历史 digest。v1/v2 明确拒绝 v3 contract/partition 字段，v3 明确拒绝 `legacy` serial digest，防止 schema 伪装。Focused tests：EDPB 71/71、media identity 16/16、CLI write safety 31/31 PASS；raw-serial mismatch / VID:PID / `geometry` / reopen-swap 等 0-write 授权测试保持通过。

- 增加 `edpb.manifest.v3`；
- 新 `writer` 只创建 `metadata_only`；
- 保持 v1/v2 `reader` compatibility；
- 增加 partition typed `metadata` 和 `restore` contract；
- 增加旧备份 UI 分类。

**B2 — Plain `metadata` `capture`**

**实施状态（2026-09-28）：COMPLETE。** Plain 新建备份已切换为 `Metadata` 级 v3：MBR 仅保存原始 LBA0 + typed partition entries；GPT 保存 protective MBR、主 header/entry array、备份 entry array/header，并在采集时校验主/备 header CRC、partition-array CRC、disk GUID 与 `geometry` 一致性。采集器不会读取分区起点的 exFAT/FAT/NTFS boot region，也不会保存 FAT/bitmap/root directory/user `payload`；v3 Plain `validator` 明确禁止固定 LBA0～12 protocol `artifact` 混入。Focused tests：B2 MBR/GPT 2/2、实际 Plain `backup create` 1/1、完整 `backup_suite` 73/73 PASS。

- MBR typed `capture`；
- GPT 主/备表 `capture`；
- `filesystem`/label 仅作 hint；
- 删除“Plain 核心备份等于完整恢复”的产品假设。

**B3 — EDP `metadata` `capture` 收敛**

**实施状态（2026-09-28）：COMPLETE。** 新默认 EDP `Metadata` 采集不再读取 partition prefix/suffix、`filesystem` boot、`filesystem` key sectors 或目录/文件分析证据；typed partition `geometry` 从已确认 EDPF/LBA12 结构生成，LBA0～12 继续作为原始协议事实源（其中包含 PassInfo/NeedEncrypt/EncryptMode/UserKeyCRC/FileKeyCRC/wrapped FileKey/历史 key material），无需密码或 FileKey unwrap 即可备份。LBA7 指针指向的 6-sector LCE 与已确认的盘尾 `metadata` mirror / `restore` node 保留原始字节并标为 Restorable。旧 `Deep` 所需 `filesystem` evidence 已移动到 `Deep` 专用兼容采集 helper，新默认路径不依赖它。Focused tests：B3 1/1、`Deep` 兼容 1/1、实际 EDP `backup create` 1/1；完整 `backup_suite` 74/74 PASS。

- LBA0～12；
- LCE；
- 已识别尾部协议对象；
- key records / PassInfo 继续以原始协议 `Artifact` 为事实源；
- backup 不请求密码；
- 新默认路径不依赖 `deep` `filesystem` analyzer。

**B4 — `Metadata` `restore`**

**实施状态（2026-09-28）：COMPLETE。** `Restore` planner 已改为从已验证 EDPB 中枚举 `RestorePolicy::Restorable` 的 raw-sector `Artifact`，逐 `Extent` 生成 typed `WriteTransactionPlan`；LBA0 仍为最后提交的 `Commit`，其余元数据经同一 atomic write/sync/readback/rollback 事务执行。v3 Plain 不再要求不存在的固定 LBA0～12 protocol `Artifact`，可恢复 MBR/GPT 分区元数据；EDP/historical v1/v2 继续恢复 LBA0～12，并对 LCE 与备份 LBA7 指针复核，同时恢复已确认的两类盘尾结构。强物理身份、精确 `geometry`、prepare/unmount/lock、reopen fresh identity recheck 均未降低；Plain backup 缺 onlyid 时恢复到同一块已制成 EDP 的物理盘继续允许。Focused：v3 Plain 1/1；EDP protocol+LCE+tail 1/1；CLI 写安全 32/32 PASS。

- 按盘型生成 `metadata`-only `WriteTransactionPlan`；
- Plain MBR/GPT；
- EDP protocol + restorable protocol extents；
- `restore` 完成事件改为 `metadata` 语义；
- 真实 disk5 回归。

**B5 — Post-`restore` assessment**

**实施状态（2026-09-28）：COMPLETE。** 新增 application-owned `PostRestorePartitionState`、`PostRestoreAssessment`、`MetadataBackupReport`、`MetadataRestoreReport`、`PartitionFormatRequest`、`EncryptedPartitionReinitializeRequest`。恢复事务成功后立即进入只读 assessment；assessment 失败只降级为 typed `Unsupported`/`issue`，不会反转 `metadata` `restore` `success`。Plain 严格校验 boot sector 后区分 `Usable / NeedsFormat`；EDP 明文同样只读判断，加密域在默认密码可验证时检查 FileKeyCRC/解密 boot，否则明确 `PasswordRequired`，key record/FileKeyCRC 异常为 `CryptoMetadataInvalid`。CLI/TUI 通过 typed `WriteEvent::PostRestoreAssessment` 消费状态，不解析错误字符串。Focused tests：B5 状态矩阵 5/5、CLI write safety 32/32、B0 3/3、Clippy `-D warnings` PASS。

- 实现 `Usable / NeedsFormat / PasswordRequired / CryptoMetadataInvalid / Unsupported`；
- 全部只读，不写盘。

后续审计补齐生产返回值：创建备份时返回含分区数和协议区保存标记的 `MetadataBackupReport`；元数据恢复主服务返回 `MetadataRestoreOutcome`，其中 `MetadataRestoreReport` 只记录事务写入与读回验证，恢复后评估另列。旧 CLI 整数退出码仅作外层适配。

**B6 — 格式化引导**

**实施状态（2026-09-28）：COMPLETE。** 新增显式 `PartitionFormatRequest` / `PostRestoreFormatResult`，只有 assessment 为 `NeedsFormat` 的目标分区才能进入格式化；构造 assessment/request 本身不会写盘。Plain 与 EDP 明文分区均复用 provision 的 `build_empty_fat16/build_empty_exfat`，并抽取共享 sparse-`filesystem` write/sync/readback executor，原 provision 格式化路径也改为调用同一执行器，未引入第二套 FAT/exFAT `writer`。无 `filesystem` hint 时必须由调用方显式选择已验证格式；当前 portable `writer` 对 FAT32/NTFS 继续 fail-closed。格式化结果按分区返回，格式化失败不会修改已经成功的 `MetadataRestoreReport`。Focused tests：B6 3/3、provision `formatter` 18/18、Clippy `-D warnings` PASS。

- Plain / EDP 明文分区；
- 复用现有格式化器；
- 用户逐分区确认；
- 无可靠 `filesystem` hint 时由用户选择；
- format 结果与 `restore` 结果分离。

后续审计补齐独立生产入口：CLI 恢复后由用户输入分区编号与空文件系统类型，再单独确认。应用层在卸载锁盘前和重开后复核固定介质身份、精确磁盘几何及所选分区几何；只写该分区，随后同步、读回并重新评估为 `Usable`。加密分区仍拒绝明文格式化，原密钥域路径见 B7。新增安全测试 11/11 通过，覆盖取消、状态不符、身份冲突、几何冲突、重开换盘、仅所选分区写入、写入失败与成功后的重新评估。

后续事务审计将共享稀疏文件系统执行器改为镜像、写入、同步、读回和失败回滚；恢复后格式化与制盘共用该执行器。一次性中途写失败测试确认已触碰扇区恢复原始字节，已成功的元数据恢复报告保持不变。

**B7 — EDP 密钥域恢复后处理**

**实施状态（2026-09-29）：已完成当前验证过的 SM4 模式路径。** `ExistingPartitionRecord::verified_file_key` 统一按 `NeedEncrypt`、`EncryptMode`、`UserKeyCRC`、解包、`FileKeyCRC` 验证原密钥，并返回 typed `ExistingFileKeyError`；A7F0、SM4、AES-128 ECB 三种包装模式共用现有原语。恢复后加密分区可输入原密码，使用原 FileKey 和共享空文件系统构造器写入所选分区，再同步、读回并用原密码重新评估；协议密钥记录保持原样。默认密码先尝试已知路径。当前加密文件系统写入链只验证过 SM4 模式；其他包装模式虽可验原 FileKey，仍拒绝格式化。缺密码、密码不符、密钥记录异常和 CRC 错误均为独立的类型化结果。

- 原密码验证；
- unwrap FileKey；
- FileKeyCRC；
- 使用原密钥域格式化空分区；
- 失败原因分类。

**B8 — 清空并重建加密分区**

**实施状态（2026-09-29）：COMPLETE（已验证的 SM4 数据写入模式）。** CLI 必须显式选择 `reinitialize`，新密码二次输入且不得与原记录密码相同，然后再次确认原密码、原 FileKey 与残留数据访问关系作废。应用层在卸载锁盘前及重开后核对物理身份、精确磁盘与分区几何；复用现有制盘密钥包装原语生成新 LBA7/LBA12 密钥记录、全新随机 FileKey，以及现有 FAT16/exFAT 空文件系统构造器。新文件系统与两个密钥记录进入同一扇区事务，执行镜像、同步、读回和失败回滚；完成后用新密码验证 FileKey 与解密后的文件系统引导扇区，重新评估必须为 `Usable`。仅修改所选分区和 LBA7/LBA12 对应槽位，原元数据恢复报告不变。FAT32/NTFS 及未验证的数据加密写入模式继续拒绝写入。专项测试：B8 4/4、恢复后操作 15/15 通过。

`--yes` 不能跳过密钥域重建的独立确认。

- 新密码双输入；
- 新 FileKey；
- 重写对应 key-domain `metadata`；
- 新空文件系统；
- 读回验证；
- 独立破坏性确认。

**B9 — CLI/TUI 清理**

**实施状态（2026-09-29）：COMPLETE。** TUI 新建备份只提供元数据备份及固定覆盖范围说明，移除了深度备份命令、向导和运行分支；恢复结果按分区显示只读评估状态，并明确提示后续分区处理需独立确认。制盘演示进度统一称“制盘前元数据备份”。CLI 默认备份完成时列明分区结构、协议与未包含的文件系统/用户数据；`--deep` 仍可调用历史分析路径，但帮助与运行时均标为弃用。历史 EDPB 的读取兼容保持。

- 移除 TUI `Deep`；
- CLI `--deep` 弃用；
- 更新备份页、恢复完成页、`?` 帮助；
- 制盘进度改“制盘前元数据备份”；
- 删除新产品路径中已经没有用途的 `deep` 入口和文案，但保留历史 EDPB 读取兼容。

**B10 — 文档收口**

**实施状态（2026-09-29）：COMPLETE。** 第 7～17 节已按 v3 元数据备份、Plain/EDP 不同采集范围、独立后处理与历史兼容重写；用户手册、架构文档同步更新，深度分析文档标为历史兼容。第 18 节保留实施记录，真实盘验收仍需按 18.18～18.19 单独执行。

- 重写本文件第 7～17 节为新当前行为；
- 更新 `docs/user/USAGE.md`；
- 对 `DEEP_BACKUP_V1.md` 标记历史兼容/分析用途或按审计结果移除；
- 更新 `docs/architecture/ARCHITECTURE.md`；
- 删除本节“待审核/未实施”措辞，使标准文档只描述真实实现。

### 18.18 测试矩阵

**备份**

- Plain MBR；
- Plain GPT；
- EDP mode1/mode2/mode3/mode4；
- EDP 密码未知仍能备份；
- corrupt protocol fail closed；
- backup 路径不写目标盘；
- 不读取用户文件 `payload`；
- 不遍历目录；
- 新 EDPB 明确 `metadata_only`。
- v3 EDPB 直接保存可用 USB serial，`verify`/readback 后值与采集时一致；
- v3 不产生 `serial_sha256`；
- v1/v2 的历史 `serial_sha256` 仍能读取和用于兼容匹配。

**恢复**

- Plain 元数据恢复后分区几何一致；
- Plain 无文件系统时返回 `NeedsFormat`；
- 元数据恢复成功但格式化取消，`restore` 仍为 `success`；
- wrong serial / wrong VID:PID / wrong `geometry` 全部 0 writes；
- reopen swap 继续拒绝；
- EDP 协议原始字节读回一致；
- EDP 密码未知不阻止 `metadata` `restore`。

**加密后处理**

- default password verified；
- 正确原密码；
- 错误原密码得到明确 password mismatch；
- wrapped FileKey 损坏得到 `CryptoMetadataInvalid`；
- FileKeyCRC mismatch 得到 `CryptoMetadataInvalid`；
- 未输入新密码不能 reinitialize；
- 两次新密码不一致不能 reinitialize；
- reinitialize 后旧密码失效、新密码可验证；
- reinitialize 不改变分区 start/size/role，除非另有明确用户请求。

**真实盘验收**

至少重新执行 disk5：

```text
Plain 格式化
→ backup create
→ 制 EDP
→ backup restore
→ 验证 MBR/分区几何恢复
→ 明确显示 NeedsFormat
→ 用户确认格式化
→ 新空卷可挂载
```

并执行一块真实 EDP 盘：

```text
EDP metadata backup
→ 改变/清理布局
→ metadata restore
→ 验证协议/分区恢复
→ 加密区密码未知时明确 PasswordRequired
→ 输入正确原密码后可格式化
```

### 18.19 验收标准

本轮重构只有同时满足以下条件才能宣布完成：

1. 用户只需要理解一种“元数据备份”；
2. 新 backup 不采集目录和用户文件；
3. 不知道 EDP 密码也能完整创建元数据备份；
4. `restore` 文案不再暗示文件系统或用户数据已恢复；
5. Plain 恢复后若没有文件系统，明确进入 `NeedsFormat`；
6. EDP 加密区无法验证密钥时给出明确原因，不自动格式化；
7. “清空并重建加密分区”必须要求新密码双输入；
8. 制盘前强制备份仍发生在任何破坏性写入之前；
9. 所有 `restore` authorization / reopen / transaction / readback 安全门槛保持；
10. v1/v2 历史 EDPB 仍可读；
11. 新 v3 EDPB 直接保存 USB serial，不再哈希；
12. disk5 真实回归闭环；
13. `scripts/test-fast.sh`、full gate、受影响的平台专项门禁全部通过。

### 18.20 审核后的最终产品定义

```text
备份
= 保存磁盘身份、分区结构和 EDP 协议元数据

恢复
= 恢复上述元数据

恢复后
= 检查各分区；需要时由用户确认格式化

EDP 加密分区
= 能验证原密码则沿用原密钥域格式化
= 无法验证则明确提醒
= 用户可选择暂不处理，或输入新密码“清空并重建加密分区”
```

> **最终定位：edpcli 的备份是元数据恢复工具，不是用户文件备份工具。**
