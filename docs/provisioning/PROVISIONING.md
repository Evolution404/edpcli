# 制盘：当前实现与后续扩展

本文是制盘功能的唯一长期规划文档。它记录**当前已实现能力**、已验证协议边界与后续扩展方向；历史阶段计划已删除，后续不得再创建平行的制盘计划作为第二事实源。

## 1. 当前已经实现的能力

当前 `src/provision/` 是纯领域、纯内存能力：

`TargetIdentity + ProvisionMetadata + ProvisionProfile + ProvisionEntropy -> ProvisionImage(6656B) -> ProvisionValidator`

代码位于：

- `src/provision/spec.rs`
- `src/provision/profile.rs`
- `src/provision/generate.rs`
- `src/provision/filesystem.rs`
- `src/provision/validate.rs`

该模块禁止打开设备、执行系统命令或提权。硬件发现和真实写盘属于应用层/平台层。

当前 `ProvisionProfile::canonical_v1()` 的配置类型 ID 为 `jiangsu-safe6-nopwd`。当前构造器生成 LBA0、4、6、7、8、11、12，并由 `ProvisionValidator` 做离线一致性验证。

现有 `generate_image()` 继续保留标准 v1 二合一兼容输出；新增 `generate_official_image()` + `OfficialProvisionPlan` 已能纯内存生成四种官方分区模式的 LBA0/LBA7/LBA12，并由独立 `OfficialProvisionValidator` 反向校验 MBR、EDPF 类型/标志、逻辑几何与 LCE。LBA12 当前封装密钥轴已独立实现为 `ProvisionKeyMaterial`：mode1=A7F0、mode2=SM4-ECB、mode3=AES-128-ECB，三种输出均与官方虚拟写入端逐字节夹具一致；`0000aaaa` 的 v0x0206 隐式有效密码替换也已编码。LBA7 旧版 8B 封装密钥则独立建模为 `LegacyLba7KeyMaterial`，按 `fold32(password)` 对两个 32 位半字异或封装，真实 Netac 原盘向量已逐字节回归；LBA7 与 LBA12 的明文文件密钥来源仍作为两个独立输入，不建立未经证明的派生关系。

当前官方四模式制盘链共用协议元数据、分区几何、物理加密属性、文件系统配置和 LCE 六扇区负载。新盘实际写入分为两阶段：先对协议/LCE 做事务写入与读回验证，再对用户勾选的分区逐个格式化并验证。协议阶段失败会回滚；格式化阶段失败保留已验证的协议结构，并分别报告分区结果。未勾选的分区不会写入文件系统扇区。

## 2. 已闭环的官方协议事实

协议层已经通过官方写入端/消费端闭环以下事实，未来制盘功能必须直接复用，不得重新猜测：

| 模式 | 官方名称 | `PartionType[]` |
| ---: | --- | --- |
| 0 | 缺省三分区 | `[1,2,4]` |
| 1 | 启动区和交换区二合一 | `[2,4]` |
| 2 | 整盘加密 | `[1,4]` |
| 3 | 内外网通用双分区 | `[1,2]` |

`PartionType`：`1=Boot`、`2=Share`、`4=Encrypt`。

当前一方 `cemsusbregsiter.dll::sub_10041A80` 的原始 MBR 类型选择分支也已逐指令闭合：模式0→`0x0E`、模式1→`0x07`、模式2→`0x0B`、模式3→`0x0E`。模式1命中“type2 位于 entry0”分支；模式2命中“type1 位于 entry0 且 type4 位于 entry1”分支。该证据映射由 `official_mbr_partition_type` 保留。新盘计划还根据可见前部分区选定的文件系统派生最终 MBR 类型；这一步只改变系统可见的 MBR 字节，不改变 LBA7/LBA12 的 EDP `PartionType`。

同一一方写入端 `sub_10046D20` 还逐分区类型固定了 `NeedEncrypt`：type1=`0`、type2=`1`、type4=`1`；`CreatePartitions` 的位置规则为 entry0/entry1 的 `NeedDisturb=1`、entry2=`0`。四模式生成器必须按这两条写入端规则序列化，不能沿用旧二合一生成器“所有条目均为1”的简化值。

真实 Netac 当前原盘进一步确认：LBA12 中 type1/`NeedEncrypt=0` 条目的 `UserKeyCRC`、`FileKeyCRC`、16B 封装文件密钥和 `EncryptMode` 全部为0；type2/type4 才写密码/文件密钥材料。当前构造器和独立校验器已按该规则实现“无法确认即拒绝继续”。

同一真实 Netac 原盘的 LBA7 也给出完全对应的门禁：type1/`NeedEncrypt=0` 的 `UserKeyCRC`、`FileKeyCRC` 和 8B 旧版封装文件密钥全部为0；type2/type4 共用同一组非零旧版材料。当前 `wrap_legacy_lba7_file_key()` 已用 `0000aaaa` 与该原盘 8B 明文文件密钥复算出完全一致的 16B 条目材料。

当前一方 `CUsbRegsiter::FormatDisk` 的文件系统配置轴也已闭合：从 `usbtoolCfg.ini` 的 `[GLOBAL] fType` 读取格式类型，缺省值为 `exfat`，并把 `ntfs`、`exfat`、`fat32` 分别规范化为传给 `fmifs.dll!FormatEx` 的 `NTFS`、`exFat`、`fat32`；`version.ver` 中的 `[information] FormatImageDisk` 属于独立图像盘分支，命中时强制走 `FAT`，不能与普通 U 盘配置混为一谈。

`exfat` 路线已实现为稀疏元数据构造器，只生成启动区、FAT、分配位图、大小写表和根目录等必要扇区。现有深度解析器可完整反向解析；另外已用 `hdiutil` 临时虚拟块设备做本机硬件在环验证，系统原生识别为 `ExFAT`、成功挂载并完成文件写回/读回，验证过程只使用 `/tmp` 虚拟镜像，没有访问物理 U 盘。

四模式物理文件系统规则当前固定为：模式0 `[明文 type1, 加密 type2, 加密 type4]`；模式1 `[明文 type2, 加密 type4]`；模式2 的 `0x7E00` type1 仅为兼容保留项、不创建文件系统，type4 加密；模式3 `[明文 type1, 加密 type2]`。其中模式1 的 type2 虽然 `NeedEncrypt=1`，但 MBR 直接暴露路径已经由真实免密 SanDisk 验证为物理明文，不能机械按该标志加密。当前跨平台数据区写入只对已验证的 `mode2` 扇区级 `SM4-ECB` 路线开放，其他封装模式无法确认时拒绝继续。

LBA7 旧表中条目0 与后续条目的几何规则不同；后续条目可保留各自 `PartionType` 并共同指向 LCE（LBA7 兼容扩展区）。因此制盘功能不得把 LCE 当成 type4 专属区域。详细证据见 [`../protocol/LCE.md`](../protocol/LCE.md)。

LCE 实际 3072B 物理负载也已纳入构造器：使用已闭环的固定六扇区 FAT16 兼容明文模板，以 zero8 密钥和 `StartSector*512` 的完整 64 位物理字节偏移执行 EDPSECDISK 变换。`build_lce_ciphertext()` 对真实 Lexar 物理密文实现 3072B 逐字节一致回归，并对非标准几何拒绝生成。

## 3. 当前实现 A：官方四模式新盘制盘

四种官方布局共用同一 `ProvisionSpec` 和正交配置轴，没有增加四份复制粘贴模板。

### 3.1 正交轴

至少拆分以下轴：

1. `OfficialPartitionMode`：0/1/2/3 四种官方布局；
2. LBA12 封装文件密钥 / `EncryptMode` 配置类型；
3. `OfficialFilesystemFormat`：`fat16` / `exfat` / `ntfs` / `fat32` 文件系统配置轴，当前实际写入仅开放 FAT16 与 exFAT；
4. 旧版/当前 LBA4 表示配置类型；
5. LBA6 Dept 续段/快照配置类型；
6. LBA8 身份/保留底层字节配置类型；
7. GPT/MBR 配置类型；
8. 写入端代际/兼容版本来源。

软件版本只作为来源信息，不直接代替盘面配置类型。

### 3.2 四模式构造器

当前构造器从同一 `ProvisionSpec`/类型化协议模型构造四种模式，不从供体盘复制 EDP 元数据；真实写盘仅对制造商负责的 LBA3 执行目标盘原样保留。

每种模式都必须验证：

- LBA0 分区表；
- LBA7 旧版 `EDP_PARTION_INFO`；
- LBA12 当前 `tagNewEdpPartionInfo`；
- LCE 指针/几何；
- type1/type2/type4 逻辑几何；
- `UserKeyCRC`/`FileKeyCRC`/封装文件密钥/`EncryptMode`；
- LBA6/LBA8/LBA11 跨 LBA 身份不变量；
- 文件系统可见性和挂载预期。

### 3.3 模式 2 特殊规则

官方“整盘加密”写入端仍生成 `[1,4]`，其中 type1 被强制为 `0x7E00` 字节的兼容/保留几何。当前构造器精确复刻该行为，没有把界面描述“只含保密区”简化成单 `[4]` 条目。

### 3.4 可选格式化、MBR 类型与原生文件系统验证

EDP `PartionType`、模式、物理加密属性和文件系统类型是四个独立轴。当前默认矩阵：

| 模式 | 可见前部 | 后续分区 | 前部 MBR 类型 |
| ---: | --- | --- | --- |
| 0 | type1 明文 FAT16 | type2 加密 exFAT；type4 加密 exFAT | `0x0E` |
| 1 | type2 启动/交换二合一，明文 exFAT | type4 加密 exFAT | `0x07` |
| 2 | type1 `0x7E00` 兼容保留区，不可格式化 | type4 加密 exFAT | `0x0B` |
| 3 | type1 明文 FAT16 | type2 加密 exFAT | `0x0E` |

模式0默认启动区严格从 LBA63 开始，长度 20417 扇区，下一分区从 LBA20480 开始。FAT16 构造器生成完整 BPB、两份 FAT、固定根目录和卷标；exFAT 构造器保持原有实现。每个可格式化分区在计划阶段独立选 FAT16 或 exFAT，默认都不勾选格式化。选 exFAT 作为可见前部文件系统时，计划从一开始写 `0x07`；选 FAT16 时写 `0x0E`。FAT32/NTFS 仍只有类型模型，当前 CLI/TUI 不开放其写入，构建时拒绝静默回退。

2026-09-24 真实物理盘验证：mode0 启动区 LBA63、20417 扇区上的现有 exFAT 镜像，经 `fsck_exfat -n /dev/rdisk4s1` 完整检查，主/备用引导区、系统文件、大小写表、层次结构和分配位图均通过，卷名为“启动区”，退出码为0。仅把 MBR entry0 的 `0x1C2` 字节由 `0x0E` 改为 `0x07`，复核 LBA0 只有这一字节变化，macOS 随即识别为 ExFAT、卷名“启动区”并自动挂载。此证据把原有不挂载现象定位到 MBR 类型与 exFAT 不匹配；不能据此修改已验证的 exFAT 构造器。

原生 FAT16 验证入口为 `scripts/hil-fat16-macos.sh`。脚本只创建临时普通镜像，经 `hdiutil` 暴露虚拟盘，运行 `fsck_msdos -n`，检查 `diskutil` 的 FAT16 类型与卷名，完成挂载、测试文件写入/读回、卸载、再次挂载与读回。2026-09-24 在本机运行通过，系统将该分区识别为 `MS-DOS FAT16`，卷名为“启动区”。真实 USB 盘的 HIL 仍需专门设备和人工确认；不能把临时虚拟盘测试表述为真实盘验证。

## 4. 已审核实施方案：通用四模式制盘与数据保留

2026-09-24 已确认新的产品方向：物理制盘只保留官方 mode0/1/2/3 四种目标模式，删除第五个“改造”模式及其专用 conversion 计划/writer。普通盘和已注册盘最终都进入同一套目标制盘计划；来源盘只提供默认值、可复用的分区记录和数据保留证据，不能再按 source mode 选择专用 writer。

### 4.1 总体数据流

```text
进入“制盘”工作区
    ↓
先显示可用 USB 设备列表
    ↓
用户明确选择目标盘
    ↓
显示该盘当前盘型 + 历史 EDPB 保存情况
    ↓
用户选择“先保存当前盘”或“不保存继续”
    ↓
只读分析来源盘
    ↓
普通盘 / mode0 / mode1 / mode2 / mode3
    ↓
选择目标 mode0/1/2/3
    ↓
根据来源盘与目标模式生成可编辑默认表单
    ↓
用户自由修改
    ↓
计算精确 sector 几何
    ↓
逐分区判定 PreserveExact / PreserveWithRewrap / Rebuild
    ↓
TargetProvisionPlan
    ↓
统一协议生成 + 统一事务 writer
```

从 `TargetProvisionPlan` 开始，writer 不允许再依据来源是 mode0/1/2/3 做分支；它只能依据目标模式、最终分区几何和每个分区的 `PartitionAction` 工作。

### 4.1.1 统一“盘型”命名与展示

设备页、备份页、制盘页统一使用**盘型**描述当前盘/备份对应的 EDP 制盘类型，不再把“免密状态 / 加密原盘”作为主分类。五种盘型固定为：

| 盘型枚举 | UI 完整名称 | UI 短名称 |
| --- | --- | --- |
| `Plain` | 普通盘 | 普通盘 |
| `Mode0` | 模式0 · 缺省三分区 | mode0 · 缺省三分区 |
| `Mode1` | 模式1 · 启动区和交换区二合一 | mode1 · 二合一 |
| `Mode2` | 模式2 · 整盘加密 | mode2 · 整盘加密 |
| `Mode3` | 模式3 · 内外网通用双分区 | mode3 · 内外网双分区 |

mode0～mode3 的完整名称沿用当前已验证官方命名；协议/文档中不得用自创名称替代官方名称。建议领域层统一抽象为：

```rust
enum DiskProvisionKind {
    Plain,
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}
```

其中 `Plain` 表示未识别到有效 EDP mode0/1/2/3 注册结构。盘型与“是否存在备份”“备份创建时间”“是否是某次历史快照”是不同维度，禁止继续用“免密快照/加密原盘”混合作为盘型。

设备页的每个 USB 设备必须显示盘型；备份页每条备份也必须通过备份中的协议镜像识别并显示盘型。空间不足时用短名称，详情区显示完整官方名称。

### 4.2 来源盘模型

新增统一只读来源模型 `ExistingProvisionProfile`，至少保存：

- 来源是否为已注册 EDP 盘以及 `OfficialPartitionMode`；
- `onlyid`、User、Dept、SAFE6 label，以及可可靠解析的完整 PassInfo 策略：初始化密码强制修改、取消密码复杂性验证、交换区密码最大错误次数、保密区密码最大错误次数；
- 每个来源分区的语义角色、`PartionType`、`start_lba`、`sector_count`、物理加密属性和可确认文件系统；
- LBA7/LBA12 对应条目的原始/类型化记录；
- `UserKeyCRC`、`FileKeyCRC`、wrapped FileKey、`EncryptMode` 以及能够可靠取得的 FileKey；
- 设备总扇区数和 LCE 前的可用边界。

该模型只描述“当前盘是什么”，不能包含“如何改造”的动作。

### 4.3 分区语义角色

不能用 `PartionType` 数字相同代替语义兼容。统一使用：

- `Boot`
- `Share`
- `BootShareCombined`
- `Encrypt`
- `CompatibilityReserve`

当前语义矩阵：

| 模式 | 分区语义 |
| --- | --- |
| mode0 | Boot(type1, 明文) + Share(type2, 加密) + Encrypt(type4, 加密) |
| mode1 | BootShareCombined(type2, 明文) + Encrypt(type4, 加密) |
| mode2 | CompatibilityReserve(type1) + Encrypt(type4, 加密) |
| mode3 | Boot(type1, 明文) + Share(type2, 加密) |

例如 mode0 的 type2 与 mode1 的 type2 虽然数值同为2，但物理语义分别是加密 Share 与明文 BootShareCombined，禁止直接 Preserve。

### 4.4 容量 canonical 单位与新 UI

所有容量和位置的 canonical 单位统一为 sector：`start_lba`、`sector_count`、`end_lba`。MiB/GiB 仅为 UI 快捷输入和显示单位，禁止将已注册盘的精确 sector 值先四舍五入为 MiB 再反算。

每个可编辑分区独立支持两种输入方式：

1. **快速输入**：MiB/GiB，适合普通新盘和快速调整；
2. **精确输入**：sector，适合已注册盘互转和需要保持旧几何的场景。

普通未注册盘默认快速输入；已注册盘中能直接继承或由旧边界精确推导的容量默认精确输入。一个表单允许混合输入，例如 Boot 使用官方默认快速值、Share/Encrypt 使用精确 sector。

快速→精确为无损整数转换。精确→快速若不是整 MiB/GiB，必须明确提示舍入将改变几何，用户主动确认后才能改变 canonical sector 值。

“剩余全部”属于一次性快捷填入：点击时计算并写入当前字段，此后修改其它字段不得联动改值，只刷新最终 LBA、剩余/超出和 Preserve/Rebuild 状态。

### 4.5 普通盘默认值

普通未注册盘没有可保留数据，所有目标分区均为 Rebuild。

PassInfo 制盘策略使用完整 14B 结构生成并同步写入 LBA7/LBA12。TUI 表单统一显示“初始化密码强制修改”“取消密码复杂性验证”“交换区密码最大错误次数”“保密区密码最大错误次数”；两个最大错误次数使用逻辑值 `0..255`，存储层负责协议规定的 XOR `0x88` 变换，表单层不得直接填写线上的变换后字节。

- mode0：Boot=LBA63 起、默认20417 sectors、FAT16；Encrypt默认1024 MiB、exFAT；Share一次性填入扣除 Boot/Encrypt/LCE 后的最大可用值；
- mode1：Encrypt默认1024 MiB；BootShareCombined一次性填入其余可用空间；
- mode2：按 canonical 规则生成0x7E00 CompatibilityReserve，Encrypt占用 mode2 允许的数据空间；
- mode3：Boot 使用当前已验证官方默认，Share一次性填入其余可用空间。

### 4.6 已注册盘 Prefill 原则

已注册盘默认目标从“空间利用最大化”改为“可保留数据最大化”，优先级固定为：

1. 同语义来源分区存在时，优先继承精确 start/sector_count；
2. 可通过旧分区边界精确推导目标字段时，使用精确 sector；
3. 目标新增字段无来源时，使用该目标模式系统默认；
4. 目标删除字段不带入目标；
5. 若保留旧分区需要留下未分配空间，允许默认留下未分配空间，不得为了“自动占满”破坏原本可 Preserve 的分区。

身份字段在可可靠解析时默认带入 `onlyid`、User、Dept、SAFE6 label。PassInfo 策略只有在 LBA7/LBA12 两份记录版本有效且四项值完全一致时才继承：初始化密码强制修改、取消密码复杂性验证、交换区密码最大错误次数、保密区密码最大错误次数。普通未注册盘默认分别为“否、否、255、255”。任一副本不一致或字段值非法时不得猜测，回退到默认值。明文密码只有在系统实际掌握/可靠确认时才带入，不能从 CRC/wrapped key 假装恢复。

#### 4.6.1 Preserve 分区的位置锚定与未分配空间

已注册盘重制时，凡来源中已经判定为 Preserve candidate 的分区，默认具有**位置锚定**语义：其 `start_lba` 与 `sector_count` 不会因为用户修改其它分区容量而被自动联动改变。用户只修改前一个分区时，系统优先保留后续可 Preserve 分区的原始几何，因此允许在两个目标分区之间出现未分配空间。

以 mode0→mode1 为例，默认 Combined 精确延伸到原 Encrypt(type4) 起点，Encrypt 保持原 `start_lba` 和 `sector_count`。若用户缩小 Combined：

```text
LBA63
├──── 新 Combined ────┤
                      ├── 未分配空间 ──┤
                                         old Encrypt start
                                         ├── 原 Encrypt ──┤
```

此时 Encrypt 的起点、大小、FileKey 和 data extent 均未变化，因此仍为 `PreserveExact`；UI 只显示新增未分配空间，不得自动把 Encrypt 向前移动。

若用户增大 Combined，但仍未触碰锚定 Encrypt，则只消耗原有未分配空间，Encrypt 继续 Preserve。若 Combined 的新末端达到或越过 Encrypt 起点，则当前布局进入**重叠无效状态**：UI 显示超出/重叠的精确 sector 数，禁止提交，但仍不得自动移动 Encrypt。

只有在用户明确修改 Encrypt 的起点/大小，或主动执行未来的“紧凑布局/重新排列后续分区”操作时，才允许解除该锚定；一旦最终 `start_lba` 或 `sector_count` 与来源不一致，Encrypt 立即转为 `Rebuild`，并明确提示原数据不能原样保留。

因此新布局模型不得继续假设所有目标分区必须首尾连续排列。`TargetPartitionGeometry` 必须独立保存每个分区的 `start_lba` 和 `sector_count`，并允许合法 gap；合法性检查分别处理 overlap、设备边界、LCE 边界和未分配空间。普通未注册盘默认仍采用紧凑连续布局，只有已注册盘的 Preserve 优先 Prefill 才默认锚定来源分区。

### 4.7 4×4 模式互转默认规则

同模式重制（0→0、1→1、2→2、3→3）优先精确继承所有同语义分区；用户不改时应尽可能全部 Preserve。

关键交叉转换默认：

- **0→1**：Encrypt 精确继承原 type4；Combined 精确取 `old_encrypt_start - 63`，默认保持 type4 起点，因此 Encrypt 可成为 Preserve candidate；前部 Combined 必须 Rebuild。
- **1→0**：Encrypt 精确继承；Boot 使用 mode0 官方20417 sectors；Share 精确反推到原 Encrypt 起点，使 Encrypt 默认保持原位；Boot/Share Rebuild。
- **0→3**：Boot/Share 若目标几何保持一致均可 Preserve；原 Encrypt 在目标模式中不存在。为了保留 Share，允许默认留下原 Encrypt 区域为未分配空间，只有用户主动“占满剩余”后 Share 才转为 Rebuild。
- **3→0**：Boot 可精确继承；若新增 Encrypt 需要缩小 Share，则 Share Rebuild；若来源盘原本存在足够未分配空间，则可继续 Preserve Share。
- **0/1/2 之间的 Encrypt(type4)**：只要最终语义、start、sector_count、物理加密、文件系统解释和 crypto profile 全部兼容，均可作为 Preserve candidate；否则 Rebuild。
- mode1 Combined 与 mode0/mode3 Share 语义不同，禁止直接 Preserve。
- mode2 CompatibilityReserve 不作为普通用户数据分区 Preserve；它按目标 mode2 canonical 规则重新生成。

最终是否 Preserve 不由 source/target mode 组合直接决定，而由来源分区与最终目标分区逐项兼容判断决定。

### 4.8 PartitionAction

统一动作模型：

```text
PreserveExact
PreserveWithRewrap
Rebuild
```

**PreserveExact** 必须同时满足：语义角色一致、`PartionType`一致、start LBA 完全一致、sector_count 完全一致、物理加密语义一致、文件系统解释兼容、crypto profile 兼容、原记录可可靠解析。行为是：数据 extent 不写、文件系统不重建、FileKey/加密参数复用，目标协议重新组装时复用属于该分区的 key material。

**PreserveWithRewrap** 用于几何/数据完全保持但用户修改密码的场景：FileKey 和 ciphertext 不变，仅重新包装认证/key record。只有旧 FileKey 可可靠取得、认证状态可验证且目标 crypto profile 兼容时才允许。第一阶段实现证据不足时可暂缓，仅开放 PreserveExact + Rebuild。

**Rebuild**：任意 Preserve 条件不满足时采用。重新生成 FileKey、分区记录和文件系统，必要时重新加密并写数据 extent。UI 必须明确提示原数据不能原样保留。

### 4.9 FileKey 与协议记录复用

Preserve 分区必须继续使用原 FileKey，否则旧 ciphertext 无法正常读取。目标 LBA7/LBA12 的表头、entry count、entry order 和目标模式专属字段仍按目标模式重新生成。

不能先假设“来源 entry 整条 byte-for-byte 搬到新 slot”总是安全。实现 Preserve 前必须通过测试证明哪些字节属于 partition-owned material、哪些与 slot/index/header 绑定。特别要验证 mode0 的 type4 entry2 移到 mode1 的 type4 entry1 时，`UserKeyCRC`、`FileKeyCRC`、wrapped FileKey、`EncryptMode` 等数据解密相关字段的复用边界。

### 4.10 制盘选盘与制盘前保存

“制盘”工作区不能直接默认当前设备或依赖其它页面的临时选择。进入制盘后第一步必须显示可用 USB 设备列表，由用户明确选择目标盘；选盘后固定该 `diskN`/硬件身份作为本次制盘目标，再显示该盘当前盘型和历史 EDPB 保存状态。随后才进入保存提示和目标模式选择。

选盘页面每条设备至少显示：disk 编号、容量、USB 身份摘要、盘型；已注册盘还可显示 `onlyid`。系统盘/非 USB 整盘继续按现有安全门禁排除或明确标记不可选。

目标盘固定后显示历史 EDPB 保存状态：

- 没有保存记录；
- 或已保存 N 份及最近保存时间。

用户明确选择“先保存当前盘”或“不保存，继续制盘”。选择保存则保存成功后进入模式菜单；选择跳过则直接进入模式菜单。后续统一 writer 不允许再偷偷强制创建第二份备份。

UI 还必须说明 EDPB 的实际覆盖范围，不能把元数据/快照备份描述成必然包含普通分区所有用户文件。

设备页和备份页也必须同步改造盘型显示：

- 设备页原“非 cems 盘 / cems盘 / cems · 免密”等状态改为统一盘型；
- 备份页原“免密状态 / 加密原盘”列改为“盘型”，值为普通盘或 mode0/1/2/3；
- 备份详情页使用同一盘型名称；
- 搜索关键字应支持 `普通盘`、`mode0`、`mode1`、`mode2`、`mode3` 及官方模式名称；
- 旧 `is_nopwd` 可在兼容/迁移阶段保留为内部历史字段，但不得继续驱动主 UI 分类，最终应由统一盘型识别替代。

### 4.11 实时 UI 状态与 Review

每个分区卡片显示：

- 语义角色和状态 badge（原数据可保留 / 将重建）；
- 快速/精确输入方式；
- canonical sector_count 及约等 MiB/GiB；
- start/end LBA；
- 默认值来源（系统默认、旧分区、旧边界推导、用户编辑）；
- Preserve/Rebuild 原因；
- Preserve 时是否复用 FileKey、是否禁止写 data extent。

用户编辑任何容量后立即重新计算布局、gap/overlap 和 `PartitionAction`。修改前置分区容量时不得自动移动后续锚定的 Preserve 分区：缩小前置分区产生 gap，后续分区继续 Preserve；增大到发生 overlap 时布局禁止提交。只有用户明确改变后续分区自身的 `start_lba` 或 `sector_count`，该分区才从 PreserveExact 转为 Rebuild；改回原精确几何后状态可恢复。

Final Review 必须逐分区列出 Preserve/Rebuild、最终精确 LBA、FileKey 处理和预计数据损失，真实写盘仍需输入 YES。

### 4.12 TargetProvisionPlan 与统一 writer

最终新增统一 `TargetProvisionPlan` / `TargetPartitionPlan`，每个目标分区携带 `PartitionAction` 与 key material 策略。writer 只消费该计划：

- PreserveExact：禁止写对应 data extent；
- PreserveWithRewrap：禁止写 data extent，只允许必要认证/key record 更新；
- Rebuild：生成文件系统、必要时加密、写 data extent 并读回。

协议/LCE/MBR 仍按目标模式统一生成，事务层统一执行目标复核、写前镜像、写入、sync、readback 和 rollback。不得再出现 mode0→mode1 writer、conversion writer 等 source-mode 专用实现。

## 5. 实施顺序与测试门禁

本重构采用“测试规格先行，再实现”的顺序。

### 5.1 Phase 1：领域模型与红灯测试

先新增测试并锁定：

- sector-backed `CapacityInput`，Quick/Exact 双输入及无损/有损切换；
- `PartitionSemanticRole`；
- `ExistingProvisionProfile` / `ExistingPartition`；
- `PartitionAction` 兼容判定；
- 普通盘→4模式默认值；
- 4×4 来源/目标 Prefill 矩阵；
- mode0→mode1 默认 Encrypt Preserve；缩小/增大 Combined 只产生 gap 或 overlap，不得自动移动 Encrypt；明确改变 Encrypt 起点或大小1 sector即 Rebuild，改回恢复 Preserve；
- 同模式不改时 Preserve；
- “剩余全部”只执行一次且不联动。
- 已注册盘允许合法未分配 gap；前置分区缩小时后续 Preserve 分区保持锚定；前置分区扩大撞到锚定分区时必须报 overlap 并禁止提交；
- “紧凑布局/重新排列”若未来开放，必须是显式用户动作，并在会导致 Preserve→Rebuild 时先显示数据丢失警告。
- `DiskProvisionKind` 五分类：普通盘、mode0、mode1、mode2、mode3；设备页和备份页必须显示一致盘型；
- 制盘工作区进入后必须先选盘，固定目标后再显示盘型/备份状态，再进入目标模式选择；不得静默复用其它工作区当前选中的设备。

### 5.2 Phase 2：来源盘通用解析

将旧 conversion 模块中仍有价值的 LBA7/LBA12 几何与 key material 解析迁入通用来源分析。解析失败/证据不足必须 fail-closed，不得把未知盘误判为已注册盘。

### 5.3 Phase 3：双输入 TUI/CLI

TUI 每个分区独立支持 Quick/Exact；CLI 增加 `--boot-sectors`、`--share-sectors`、`--encrypt-sectors` 与对应 MiB 参数互斥。底层最终请求转为 sector canonical。

### 5.4 Phase 4：Prefill engine 与 Preserve planner

实现通用 `prefill_for_target_mode()` 和逐分区兼容 planner；来源模式只能影响 Prefill 与 source partition 候选，不得直接进入 writer 分支。

### 5.5 Phase 5：协议 producer 支持 per-partition key material

当前 `OfficialProvisionPlan` 对加密分区共用一组 LBA7/LBA12 key material，需重构为每个目标分区可选择“复用来源材料”或“生成新材料”。同时以测试确定 slot/index 相关字段的重新编码边界。

### 5.6 Phase 6：统一 writer

扩展现有 `commit_new_provision()` 或替换为统一目标计划 writer，使 Preserve 分区 data extent 永不进入 write set，Rebuild 分区按现有格式化/加密路径生成并写入。

### 5.7 Phase 7：删除旧 conversion

通用 planner/writer 覆盖后，彻底删除第五模式、`provision convert`、`PasswordlessConversion*`、`build/prepare/commit_passwordless_conversion`、`atomic_write_passwordless_conversion_sectors` 以及专用测试和过时文档。

### 5.8 Phase 8：完整回归与真实盘验收

至少通过：

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --all-targets`
- 完整 `cargo test --all-targets`（Runner 有时限时分批覆盖全部 target）
- 普通盘→mode0/1/2/3
- 关键互转 mode0→mode1：默认精确值下 type4 FileKey/数据保持可读；修改几何后明确转 Rebuild。

## 6. 完成标准

只有同时满足以下条件才算完成：

1. TUI/CLI 不存在第五物理改造模式；
2. 不存在 conversion-specific writer；
3. 普通盘可制 mode0/1/2/3；
4. 四种已注册模式可选择任意目标模式；
5. 已注册盘自动带入可可靠继承的身份和精确分区参数；
6. 设备页、备份页、制盘页统一使用五种“盘型”：普通盘、模式0·缺省三分区、模式1·启动区和交换区二合一、模式2·整盘加密、模式3·内外网通用双分区；
7. 制盘工作区第一步必须由用户明确选盘，选盘后才显示当前盘型/历史备份并进入模式选择；
8. 容量支持 Quick(MiB/GiB) + Exact(sector)，sector 为 canonical；
9. 用户编辑一个字段不联动修改其它容量或移动其它锚定分区；
10. 已注册盘允许合法 gap，overlap 必须 fail-closed 且不得自动重排；
11. UI 实时显示最终 LBA、gap/overlap、剩余/超出和 Preserve/Rebuild；
12. PreserveExact 分区复用原 FileKey，data extent 不写；
13. Rebuild 分区明确提示原数据会丢失；
14. 制盘前显示历史保存情况并由用户明确选择是否先保存；
15. 所有目标通过同一 `TargetProvisionPlan` 和统一 writer；
16. 4×4 Prefill/Preserve 测试矩阵和完整回归全绿；
17. 关键真实盘互转验收通过。

## 7. 当前实施状态与交接（2026-09-24）

### 7.1 仓库基线

- 仓库：`/Users/zhangyuxi/Desktop/edpcli`
- 分支：`main`
- 基线 HEAD：`d8efb2b4f6654aa837f80f720ba5fb5add11af8a`
- `origin/main` 与该基线一致。
- 当前工作区**有意保留未提交 WIP**，约 16 个已修改文件 + 新增 `tests/provision_reprovision.rs`。禁止 `git reset --hard`、`git clean` 或覆盖这些修改；必须先审阅 diff 后继续。

当前主要修改文件包括：

- `docs/provisioning/PROVISIONING.md`
- `README.md` / `docs/user/USAGE.md`
- `src/application/provision.rs`
- `src/cli.rs` / `src/cli_args.rs`
- `src/provision/conversion.rs` / `src/provision/mod.rs`
- `src/tui/state.rs` / `src/tui/task.rs` / `src/tui/mod.rs` / `src/tui/render.rs`
- `tests/cli_v2_parser.rs`
- `tests/provision_conversion.rs`
- `tests/tui_lifecycle.rs`
- `tests/tui_state.rs`
- `tests/provision_reprovision.rs`

### 7.2 已完成且应保留的方向

1. 独立第五物理“改造”入口已经开始从 TUI/CLI surface 删除，`provision convert` 的用户入口不应恢复。
2. 制盘前保存提示已经开始接入：进入物理制盘前显示目标盘历史 EDPB 保存情况，并让用户选择“先保存”或“跳过”。
3. 本文第4～6节已经写入并作为**唯一正式设计**：四模式通用 Prefill、Quick/Exact、sector canonical、Preserve/Rebuild、位置锚定、合法 gap、overlap fail-closed、统一 TargetProvisionPlan/writer。
4. 已新增 `tests/provision_reprovision.rs`，开始锁定：
   - Quick/Exact 的 sector canonical；
   - PreserveExact 对语义/位置/大小变化的严格判断；
   - 普通盘 mode0 默认；
   - mode0→mode1 非整 MiB 精确 Encrypt 带入；
   - mode1→mode0 通过精确 Share 反推保持 Encrypt 起点；
   - 同模式精确容量继承。
5. 最新补充规则：已注册盘可 Preserve 分区默认**位置锚定**。mode0→mode1 缩小 Combined 只产生 gap，不移动 type4；增大撞到 type4 时报告 overlap 并禁止提交；只有用户显式改变 type4 几何/执行重新排列才转 Rebuild。
6. 最新 UI 需求：统一引入五种“盘型”分类（普通盘、mode0/1/2/3，mode 名称沿用官方命名）；设备页和备份页都显示盘型，替代现有“免密状态/加密原盘/cems·免密”等主分类。
7. 制盘流程必须改为“进入制盘 → 先选盘 → 显示该盘盘型和历史备份 → 选择是否先保存 → 选择目标 mode0/1/2/3”，不得默认继承其它工作区当前选盘。

### 7.3 当前 WIP 中需要纠正的旧方向

在用户最终确认“删除旧改造算法、改为通用 Preserve planner”之前，WIP 曾临时把旧 conversion 内嵌进 mode1，因此目前代码仍残留：

- `Mode1Preserved`
- `Mode1NeedsForm`
- `try_prepare_mode1_from_existing_mode0()`
- `is_mode0_source()`
- `build_passwordless_conversion()`
- `commit_passwordless_conversion()`
- conversion-specific writer / `PasswordlessConversion*`

这些**不是最终设计**。不要继续修补这条专用 mode1 路线。应把其中有价值的“读取旧盘几何/key material”能力迁入通用 `ExistingProvisionProfile`，之后删除 conversion-specific 模型和 writer。

同样，当前 `README.md`、`docs/user/USAGE.md`、部分测试里可能仍有“mode1 自动走旧保留重制”的过渡文案；最终应按本文统一方案重写。

### 7.4 当前验证状态

当前 WIP **尚未达到可提交状态**。

最近执行：

```text
cargo test --test provision_reprovision -- --nocapture
```

编译阶段先被前一版 WIP 阻塞：

```text
src/tui/render.rs:
non-exhaustive patterns:
&ProvisionPrepared::Mode1NeedsForm not covered
```

这不是新设计需要补一个 match arm 的问题，而是提示专用 `Mode1NeedsForm/Mode1Preserved` 方向应被移除/重构。不要为了过编译继续扩大这些临时枚举。

在引入该临时枚举之前，曾有一轮 `cargo check --all-targets` 通过；但**当前最新工作区不能视为 green**，必须重新验证。

### 7.5 下一步严格执行顺序

1. 先执行 `git status --short --branch`、`git rev-parse HEAD`、`git log -6 --oneline --decorate`，确认仍在上述基线，禁止 reset/clean。
2. 审阅当前 diff，区分“应保留的新方案修改”和“旧 mode1 conversion 临时分支”。
3. 清除 `Mode1Preserved/Mode1NeedsForm` 及 mode1 专用 conversion 调用，使 TUI/CLI 重新只有通用制盘计划入口；保留制盘前备份提示。
4. 不要立刻删除 `conversion.rs`：先把其中可靠的旧盘 LBA7/LBA12 几何/key material 解析迁移成通用 `ExistingProvisionProfile`，测试覆盖后再删除原文件。
5. 实现纯领域模型，优先让 `tests/provision_reprovision.rs` 通过：
   - `CapacityInputMode::{Quick, Exact}`
   - `QuickCapacityUnit::{MiB, GiB}`
   - `CapacitySource`
   - `ExistingProvisionProfile` / `ExistingPartition`
   - `TargetPartitionGeometry`
   - `PartitionAction::{PreserveExact, Rebuild}`（Rewrap 可后置）
   - `prefill_for_target_mode()`
6. 将现有 `OfficialPartitionSizes`/layout 扩展到 Boot/Share/Encrypt 都能接受 exact-sector override；旧 MiB API 可兼容，但新 canonical 必须是 sector。
7. Prefill/planner 必须支持独立 `start_lba`，不能继续假设目标分区自动连续排列；已注册盘允许 gap，overlap 必须 fail-closed。
8. 补完整 20 条基础 Prefill 矩阵（plain→4 + 4×4），再接 TUI Quick/Exact UI。
9. 增加统一盘型识别 `DiskProvisionKind`，复用 canonical LBA12/LBA7 模式识别；把 `disk_scan`、设备页、备份目录/详情、搜索和制盘选盘页统一迁移到五种盘型显示。旧 `is_nopwd` 仅作为历史兼容信息，不再作为 UI 主分类。
10. 重构 TUI 制盘入口为显式选盘 stage：先列 USB 盘并显示盘型，用户确认目标后再进入 BackupPrompt/Mode Menu；目标固定后继续沿用 reopen 身份复核。
11. 在实现 FileKey Preserve 前，单独测试/审计 LBA12/LBA7 entry 从不同 slot 移动时哪些字段可复用，不能假设整条 entry byte-for-byte 可搬。
12. 通用 `TargetProvisionPlan` 和 writer 覆盖 Preserve/Rebuild 后，再删除全部 `PasswordlessConversion*` / conversion-specific writer / 旧专用测试。
13. 每个阶段小步提交；最终必须 `cargo fmt --all -- --check`、`git diff --check`、`cargo check --all-targets`、完整 tests 全绿，再 push。

### 7.6 不可违反的实现约束

- 不因修改一个容量字段自动修改其它容量字段。
- 不因前置分区缩小自动前移后续 Preserve 分区；允许 gap。
- 不因前置分区扩大自动后移 Preserve 分区；发生 overlap 就禁止提交。
- Preserve 必须 fail-closed：语义、start、sector_count、物理加密、filesystem/crypto compatibility 有一项不确定即 Rebuild。
- Preserve 数据区必须不进入 write set，并复用原 FileKey/key material。
- Rebuild 必须在 UI/Review 明确说明原数据不可原样保留。
- 盘型识别必须统一来源：设备页、备份页、制盘页不得各自维护不同的“免密/加密”判断逻辑。
- 制盘页必须先选盘；未固定目标盘前不得进入目标模式表单，也不得开始备份或写盘准备。
- 不新增第二份并行制盘计划文档；本文是唯一长期实施事实源。

最终产品定义：**普通盘使用目标模式系统默认值；已注册盘依据原盘精确参数和目标模式生成可编辑默认表单。用户确定最终布局后，系统逐分区判断是否与原分区在语义、位置、大小、物理加密、文件系统和 crypto profile 上完全兼容；兼容则复用原 key material/FileKey 和原数据，不兼容则重建并明确提示数据损失。所有模式统一由同一套目标计划和 writer 完成。**
