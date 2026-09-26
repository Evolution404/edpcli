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

2026-09-25 回归修复：格式化 checkbox 重新成为唯一用户意图来源，`PartitionAction::Rebuild` 不再静默把“否”覆盖成“是”。文件系统计划的 key 校验也改为严格按**当前分区**处理：type1/`NeedEncrypt=0` 明文分区不要求 FileKeyCRC；type2/type4 等物理加密分区使用该分区自己的 `partition_lba12_material[index]` 验证 FileKeyCRC 和 wrap mode，validator 对 LBA7/LBA12 同样按分区级 material 验证，不再混用全局 key material。

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

从 `TargetProvisionPlan` 开始，writer 不允许再依据来源盘型做专用分支；它只能依据统一目标 `ProvisionTarget`、最终分区几何、最终 sector write-set 和每个分区的 `PartitionAction` 工作。

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

其中 `Plain` 表示普通、非 EDP mode0/1/2/3 的磁盘状态。它既是设备分类之一，也是新的“恢复普通盘”目标状态，但**不是 mode4，也不是第五个官方 EDP 模式**。盘型与“是否存在备份”“备份创建时间”“是否是某次历史快照”是不同维度，禁止继续用“免密快照/加密原盘”混合作为盘型。

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

### 4.4 统一目标模型：四个官方模式 + Plain

制盘目标不再只用 `OfficialPartitionMode` 表达，统一引入：

```rust
enum ProvisionTarget {
    Plain,
    Official(OfficialPartitionMode),
}
```

固定规则：

- `OfficialPartitionMode` 仍然只有 mode0～mode3；
- `Plain` 是非 EDP 目标状态，**不得命名为 mode4**；
- TUI 制盘菜单最终固定为：mode0、mode1、mode2、mode3、恢复普通盘；
- source/target 逻辑按五种盘型统一建模，但四个 EDP 官方模式的协议事实与命名保持不变；
- writer 只消费最终 `ProvisionTarget` / `TargetProvisionPlan`，不得出现 source-mode 专用 writer。

### 4.5 容量 canonical、单位与通用 `f` 填满

所有容量和位置的 canonical 单位统一为 sector：`start_lba`、`sector_count`、`end_lba`。MiB/GiB 仅为 UI 输入/显示层，任何单位切换都不得损失 canonical sector 精度。

所有容量字段统一支持：

- `MiB`；
- `GiB`；
- `sector`；
- `Space` 循环切换单位；
- MiB/GiB 显示固定保留 3 位小数并四舍五入；
- sector 显示精确整数。

容量提示只保留：

```text
Space 切换 MiB / GiB / sector · f 填满
```

`f` 是所有制盘目标共同能力，mode0～mode3 与 Plain 都支持。只有当前焦点位于“容量字段”时 `f` 才表示“填满当前最大可设范围”；在用户名、部门、标签、卷标、密码等文本字段中，字符 `f` 必须正常输入。

`f` 必须与实时布局显示的 `max_sectors` 使用同一算法：

- 当前分区后方存在锚定分区时，只填到下一锚定分区起点之前；
- 当前分区后方只有连续空闲空间时，填满该连续空闲空间；
- 最后一个分区可填到目标可用区末端；
- 不得为了“填满”自动移动其它锚定分区；
- Plain 中不得跨越后续分区去吞并更远的 gap。

`f` 是一次性修改当前 `sector_count` 的动作，不建立后续联动关系。

### 4.6 字段级输入约束与编辑行为

当前 `provision_push_char()` 不能继续把任意非控制字符写入任意字段。新增字段级输入策略（例如 `ProvisionInputPolicy`），由字段类型决定合法输入。

约束固定如下：

- Quick 容量（MiB/GiB）：只允许 `0-9` 和一个 `.`；允许编辑中的 `12.`，禁止第二个小数点、负号、字母和空格；
- Exact sector / `start_lba` / sector count：只允许 `0-9`；
- `0..255` 逻辑字段：只允许数字，并在输入阶段阻止大于255的值；
- `onlyid`：按现有协议解析语义允许无符号 `u32` 或负 `i32` 文本，因此只允许数字，以及首字符位置唯一的 `-`；
- 用户名、部门、SAFE6 标签、卷标、密码等文本字段：允许对应安全可打印字符，同时继续执行既有协议长度/编码限制；
- 文件系统、布尔开关、单位等枚举字段不作为自由文本输入框，继续用 `Space` 或明确快捷键切换。

非法字符不得进入字段；UI 给出短提示，例如“起始 LBA 仅允许输入整数”，而不是等用户提交后才发现解析失败。

输入框获得焦点后：

- `←/→` 只移动输入光标；
- `Home/End` 到行首/行尾；
- 不能再用 `←/→` 切换 workspace/tab；
- 非编辑状态下 `←/→` 才可承担页面/选项切换。

### 4.7 Plain / 恢复普通盘

#### 4.7.1 产品语义

“恢复普通盘”表示将目标 USB 重建为普通、非 EDP 管理的磁盘布局。它是破坏性重新分区/格式化操作，但**不是安全擦除**：未被最终 write-set 覆盖的历史扇区可能仍残留旧内容，文档和 Review 不得宣称数据已安全抹除。

当前磁盘已经是 Plain 时，UI 应明确显示“当前已是普通盘”；如仍允许进入该目标，语义是重新分区/格式化，而不是重复“解除 EDP”。

#### 4.7.2 第一阶段分区表范围

第一阶段只实现：

- MBR；
- 最多4个主分区；
- 暂不实现 GPT、扩展分区和逻辑分区。

领域模型应预留未来 `PartitionTableKind::{Mbr, Gpt}`，但 GPT 不在本轮验收范围。实现前额外审计真实/官方 EDP 来源盘是否可能残留 GPT primary/backup header 或 partition entry array；只有证据证明存在 OS 混淆风险时才扩展清理范围，禁止凭猜测擦盘尾。

#### 4.7.3 默认布局

第一次进入 Plain 表单时自动创建 P1：

```text
P1.start_lba    = 2048
P1.sector_count = total_sectors - 2048
P1.filesystem   = exFAT
```

即默认从 LBA2048 开始占满剩余磁盘。`2048` 只是默认值，不是强制对齐规则；用户可自行修改起点和容量，程序不得偷偷改回默认值。

底层新增动态 Plain 表单模型，例如：

```rust
struct PlainProvisionForm {
    partitions: Vec<PlainPartitionForm>,
}

struct PlainPartitionForm {
    start_lba: u64,
    sector_count: u64,
    unit: QuickCapacityUnit,
    filesystem: PlainFilesystem,
    volume_label: String,
}
```

canonical 只保存 `start_lba + sector_count`；MiB/GiB 只属于编辑视图。

#### 4.7.4 多分区规则

Plain 支持1～4个 MBR 主分区。默认 P1 已占满整个可用区，因此第一次“添加分区”如果没有连续空闲空间，必须提示用户先缩小已有分区，不能自动缩 P1。

存在空闲区后，“添加分区”默认使用当前布局中可用的连续空闲区，初始容量占满该连续空闲区。用户随后可自行缩小或修改起点。

核心不变量：**任何普通编辑动作都不得自动移动其它分区。**

- 缩小 P1：产生或扩大 gap，P2/P3/P4 起点不变；
- 扩大 P1 撞到 P2：立即显示 overlap，禁止生成计划；
- 删除 P2：原区域变为未分配，不扩大 P1、不前移 P3；
- 修改任一分区起点/容量：只改变该分区本身，实时布局负责显示 gap/overlap/越界。

删除采用统一键盘交互：当前焦点属于 Plain 某分区字段时，`D` 请求删除该分区并进入明确确认；不得做隐式删除。

#### 4.7.5 Plain 文件系统

第一阶段默认使用项目已有、可验证的 first-party sparse exFAT builder，不依赖 `diskutil eraseDisk/partitionDisk` 完成核心格式化。Plain 分区不生成 FileKey、LBA7/LBA12 partition entry 或 EDP 加密材料。

未来可扩展 FAT32/NTFS 等，但未完成 first-party writer + analyzer + HIL 前不得提前暴露为可选能力。

### 4.8 已注册盘 Prefill、锚定与五状态转换

已注册 EDP 盘仍坚持“可保留数据最大化”：

1. 同语义来源分区存在时，优先继承精确 `start_lba/sector_count`；
2. 可通过旧边界精确推导时使用 exact sector；
3. 目标新增字段无来源时使用目标模式系统默认；
4. 目标删除字段不带入目标；
5. 为 Preserve 留下 gap 是合法布局，不能为了“自动占满”破坏可 Preserve 分区。

Preserve candidate 默认具有位置锚定语义：修改前置分区不得自动移动后续 Preserve 分区；前置分区缩小产生 gap，增大撞到锚定分区时进入 overlap 无效状态。只有用户明确修改该分区自身起点/大小，才解除其原几何并转为 Rebuild。

总体 source/target 盘型是五种：Plain、mode0、mode1、mode2、mode3。Plain 作为 target 时使用本节 Plain planner；四个官方模式之间的默认规则继续遵守现有已验证结论：

- 0→1：Encrypt 精确继承，Combined 重建；
- 1→0：Encrypt 精确继承，Boot/Share 重建；
- 0→3：Boot/Share 几何兼容时可 Preserve；
- 3→0：Boot 可继承，新增 Encrypt 是否迫使 Share Rebuild 由最终几何决定；
- mode0/1/2 间 type4 Encrypt 只有在语义、位置、大小、物理加密、filesystem/crypto profile 全兼容时才是 Preserve candidate；
- mode1 Combined 与 mode0/mode3 Share 语义不同，禁止直接 Preserve；
- mode2 CompatibilityReserve 按 canonical 规则重建。

最终 Preserve/Rebuild 由逐分区兼容判定决定，不由 source/target mode 组合直接决定。

### 4.9 PartitionAction 与 key material

统一动作模型：

```text
PreserveExact
PreserveWithRewrap
Rebuild
```

`PreserveExact` 必须同时满足：语义、PartionType、start LBA、sector_count、物理加密、文件系统解释和 crypto profile 全兼容；对应 data extent 禁止进入 write-set，并复用原 FileKey/key material。

`PreserveWithRewrap` 只用于几何/数据完全保持但用户改变认证包装的场景；证据不足时可继续后置，不得通过猜测提前开放。

`Rebuild`：任一 Preserve 条件不满足即采用；重新生成必要协议/文件系统/加密材料，并在 UI/Review 明确提示原数据不能原样保留。

### 4.10 制盘选盘与制盘前保存

制盘 workspace 第一阶段必须显式选择物理 USB，不得静默复用其它页面当前选择。流程固定为：

```text
选择 USB
→ 显示当前盘型和历史备份状态
→ 选择是否先保存
→ 选择 mode0 / mode1 / mode2 / mode3 / 恢复普通盘
→ 表单
→ Planning
→ Review
→ YES
→ 安全事务写盘
```

设备页、备份页、制盘页统一显示 `DiskProvisionKind::{Plain,Mode0,Mode1,Mode2,Mode3}`；旧“免密状态/加密原盘/cems·免密”不得继续作为主分类。

### 4.11 统一 TUI 表单与视觉规范

#### 4.11.1 复用现有布局

Plain 不另起一套卡片式编辑器。所有目标继续复用当前 `ProvisionStage::Form`：

- 宽屏：左56%“参数” + 右44%“实时布局”；
- 窄屏：自动切换上下布局；
- mode0～mode3 提供固定语义分区字段；
- Plain 提供动态 P1～P4 字段；
- Review/Confirm/Running/Result 共用现有流程。

#### 4.11.2 分组内部对齐

字段只在当前 section 内对齐，不能跨“身份信息 / 分区布局 / 格式化 / 密码策略”做全表单统一列宽。Plain 的 P1～P4 同样只在所属 section 内对齐。

#### 4.11.3 输入高亮范围

当前输入框不能把用于布局对齐的 padding 一起套 `selected()`。输入渲染拆成：

```text
selected(actual visible content)
+ unstyled padding
```

`input_value_window()` 只负责内容窗口、左右溢出标记、光标和 secret mask，不负责把字符串补齐到整列宽度。静态枚举/checkbox 同样只高亮实际值，不能把背景色拖到行尾。

#### 4.11.4 `▶` 统一表示键盘焦点

全 TUI 固定语义：

```text
▶ = 当前键盘焦点
```

所有可用 `↑/↓` 或 `j/k` 纵向移动焦点的 table/list/menu/form 都必须显示 `▶`，包括：Devices、Backups、Provision SelectDisk、BackupPrompt、Provision Menu、Provision Form、Advanced Inspect Form/列表等。

以下不使用 `▶`：Tabs、静态状态、Review/Result、持久 checkbox。备份页必须能明确表达：

```text
▶ ☑ backup1
  ☑ backup2
  ☐ backup3
```

其中 `▶` 是 focus，`☑` 是持久勾选，两种状态不能混用。实现应抽公共 `FOCUS_MARKER = "▶ "` / empty marker helper，避免各页面手写。

#### 4.11.5 实时布局与比例条

继续使用现有右侧“实时布局”与细粒度 `━` 比例条，比例来自真实 sector 几何。mode0～mode3 图例保持启动/交换(二合一)/保密/兼容/空闲；Plain 图例变为 P1/P2/P3/P4/空闲，不得借用 EDP role 颜色语义冒充普通分区。

右侧必须显示：

- 整盘容量和 sector；
- 可分区范围；
- 已分配/未分配；
- 中间 gap，而不只显示总未分配；
- 每分区 start/end LBA、容量；
- 当前分区最大可设值与还能增加；
- overlap/越界精确错误；
- Preserve/Rebuild 状态（官方模式目标）。

### 4.12 Plain 写盘语义与通用事务 writer

#### 4.12.1 Plain 最终内容

EDP→Plain 至少需要：

- LBA0 写入目标 MBR；
- LBA1、LBA2、LBA4～LBA12 不再构成有效 EDP 协议状态；
- LBA3 **byte-for-byte 保留**，继续视为 opaque manufacturer metadata；
- 从当前来源实际解析得到的 LCE 六扇区不再保留旧 EDP 有效内容；
- 建立用户最终确定的1～4个普通 MBR 分区；
- 每个选择格式化的 Plain 分区建立可验证 exFAT。

MBR exFAT 普通主分区使用标准 type `0x07`。

若旧 LCE 落在新 Plain 分区范围内，transaction planner 必须先归一化同一 LBA 的最终 ownership：新文件系统实际需要的 sector 内容优先；未被新文件系统占用但仍含旧 LCE 内容的 sector 可清零。禁止依赖“先写A后写B碰巧覆盖”决定最终结果。

#### 4.12.2 通用 sector transaction

当前 official writer 需要泛化为统一 write transaction，例如：

```text
SectorWrite { lba, bytes, stage, owner }
WriteTransactionPlan { touched_sectors, ordered_writes }
```

统一顺序：

1. snapshot 全部 touched sectors；
2. 数据/文件系统/cleanup extent；
3. 非 MBR metadata；
4. LBA0/MBR 最后 commit；
5. sync；
6. exact readback；
7. verify；
8. 任意失败对 touched set 精确 rollback。

mode0～mode3 与 Plain 必须共用该事务设施，不允许新增弱化安全链的 Plain writer。

现有安全链继续保持：system disk guard → USB whole-disk guard → selector pinning → 可选备份 → unmount/lock → reopen rdwr → identity/serial/capacity/LBA3 复核 → atomic write → sync/readback → rollback。

### 4.13 Review 与 post-write 验证

Plain Review 至少列出：目标=普通盘、MBR、分区数、每个 Pn 的精确 LBA/容量/filesystem/卷标、将移除 EDP 协议布局、LBA3 保留、旧 LCE 失去 EDP 语义，以及“不是安全擦除”警告。真实写盘继续要求精确输入 `YES`。

Plain 成功必须通过：

1. LBA0 精确等于计划 MBR；
2. MBR entries 与计划一致；
3. LBA3 与写前完全一致；
4. LBA1/2/4～12 不再形成有效 EDP state；
5. 来源 LCE 不再保持旧 EDP 有效内容；
6. 重新 scan 分类为 `Plain`；
7. P1～Pn geometry 精确；
8. 每个格式化分区通过 filesystem analyzer；
9. exFAT boot/FAT/bitmap/root 验证；
10. Virtual-HIL 可 mount、create file、readback、unmount。

## 5. 实施顺序

本轮按“先通用交互和门禁，再 Plain，再 writer/HIL”的顺序推进，禁止在已知 UI 基础问题未收口时先堆 Plain 特殊代码。

### 5.1 Phase 0：计划落盘

- 本文作为唯一长期制盘实施事实源；
- 刷新实际 HEAD/worktree 状态；
- 单独 commit/push 文档计划，不与产品代码混合。

### 5.2 Phase 1：通用 TUI 交互修复

测试先行完成：

1. selected 高亮只覆盖实际值，不覆盖 padding；
2. 全 TUI `▶ = focus` 统一；
3. 字段级 `ProvisionInputPolicy`；
4. mode0～mode3 所有容量字段加入 `f` 填满；
5. 输入框 `←/→` 只移动光标；
6. MiB/GiB 三位小数显示、sector canonical 无损；
7. capacity hint 精简；
8. section 内对齐回归；
9. 实时比例条/最大可设算法门禁。

### 5.3 Phase 2：彻底删除 Offline Convert

完整删除 Offline Convert 产品路径：TUI menu/stage/form/result、palette aliases、task/worker、CLI `convert`、`src/application/offline_convert.rs`、conversion-only `sectors.rs` 函数/类型、专用测试和 README/USAGE 文案。

删除前审计调用关系，只保留仍被通用协议解析/crypto/source-profile 使用的公共能力。

### 5.4 Phase 3：目标领域模型

引入 `ProvisionTarget::{Plain,Official(...)}`，清除把 `OfficialPartitionMode` 当作全部目标域的假设。保持 `DiskProvisionKind` 五分类与设备/备份/制盘识别同源。

### 5.5 Phase 4：Plain UI/Planner（只读）

先实现 `PlainProvisionForm` / `Vec<PlainPartitionForm>`、P1默认、添加/删除、1～4分区、gap、`f`、自定义起点/容量、overlap/overflow validation 和实时布局；此阶段不写物理盘。

### 5.6 Phase 5：Plain MBR + filesystem plan

生成最终 MBR、partition entries、filesystem plans、EDP metadata cleanup、LBA3 preserve、LCE cleanup 和 overlap-normalized sector ownership。先完成 plan/review/read-only tests。

### 5.7 Phase 6：通用 transaction writer

把现有 official transaction 泛化为目标无关的 touched-sector transaction；保持官方四模式全回归，并让 Plain 使用同一安全链。

### 5.8 Phase 7：Virtual-HIL

覆盖：1/2/3/4分区、gap、非2048起点；验证 OS 解析、mount、文件写入/readback、unmount、重新 scan 为 Plain。

### 5.9 Phase 8：真实 USB 验收

Virtual-HIL 全绿后再做真实盘，至少覆盖：真实 mode0→Plain、mode1→Plain、Plain→mode0、Plain 多分区。真实盘仍遵循写前备份提示和全部设备身份复核。

## 6. 测试门禁

### 6.1 Offline removal

- `ProvisionKind::ALL` 不再包含 Offline；
- CLI/palette/help/completion 不再接受旧 convert/offline 入口；
- 不再存在 Offline worker/stage/service。

### 6.2 Plain planner

- 默认 P1 `start_lba=2048` 且占满剩余盘；
- 1～4 MBR primary；
- 支持 gap 和非2048起点；
- 删除分区不移动其它分区；
- overlap/overflow fail-closed；
- 第5个分区拒绝；
- Plain 不是 Official mode4。

### 6.3 `f` 填满

mode0、mode1、mode2、mode3、Plain 全覆盖：容量字段 `f`→max；文本字段 `f`→普通字符；不跨锚定/后续分区；canonical sector 精确。

### 6.4 输入过滤

覆盖 Quick decimal、Exact integer、LBA、0..255、onlyid、文本字段；非法字符不进入字段，并有短提示。

### 6.5 高亮与 focus marker

渲染测试保证 selected background 不覆盖布局 padding；Devices、Backups、Provision 各 stage、Advanced Inspect 等所有纵向导航控件都有统一 `▶`，Tabs/checkbox/static state 不误用箭头。

### 6.6 transaction

覆盖：MBR last write、snapshot all touched sectors、任一失败 rollback、readback mismatch rollback、reopen target swap/serial/capacity/LBA3 变化拒绝、system disk/non-USB 拒绝。

### 6.7 状态转换与真实语义

至少：mode0→Plain、mode1→Plain、mode2→Plain、mode3→Plain、Plain→Plain；以及现有 Plain→mode0/1/2/3 与关键官方模式互转继续全绿。

### 6.8 最终仓库门禁

```text
cargo fmt --all -- --check
git diff --check
cargo check --all-targets
cargo test --all-targets
Virtual-HIL
Real USB acceptance
```

## 7. 完成标准

只有同时满足以下条件才算完成：

1. Offline Convert 从 TUI/CLI/service/tests/docs 产品路径彻底消失；
2. 制盘目标固定为 mode0、mode1、mode2、mode3、恢复普通盘；
3. Plain 明确不是 mode4；
4. Plain 默认 LBA2048 开始并占满剩余盘；
5. Plain 支持1～4个 MBR 主分区和合法 gap；
6. 用户可精确修改每个 Plain 分区 start_lba/sector_count；
7. 编辑一个分区不得自动移动其它分区；
8. 所有目标模式容量字段统一支持 `f` 填满；
9. 所有自由输入字段执行字段级字符/范围约束；
10. 输入框 `←/→` 只移动光标；
11. MiB/GiB 只显示3位小数，内部 sector 精度不丢失；
12. 输入值高亮不再拖到列尾；
13. `▶` 全 TUI 统一只表示键盘焦点；
14. section 只做内部对齐；
15. 实时布局正确显示比例、每个 gap、最大可设、overlap/越界；
16. 设备页、备份页、制盘页五种盘型识别同源；
17. LBA3 byte-for-byte preserve；
18. 来源 LCE cleanup 与新 filesystem sector ownership 无冲突；
19. Plain 与官方四模式共用统一 atomic transaction/readback/rollback；
20. Plain post-write 重新识别为 `Plain`；
21. Virtual-HIL 全绿；
22. 完整 tests/fmt/check/diff-check 全绿；
23. 关键真实 USB 验收通过；
24. README/USAGE/本文件与真实代码一致。

## 8. 当前实施状态与交接（2026-09-24）

### 8.1 仓库基线

- 仓库：`/Users/zhangyuxi/Desktop/edpcli`
- 分支：`main`
- 当前计划编写前 HEAD：`e38e1dc317609cc7a558a7f604d968cfe6bf6dfc`
- `origin/main` 与该 HEAD 一致；
- 计划编写前工作区 clean；
- 禁止后续 AI 使用 `git reset/clean` 覆盖其它工作；每次开始必须重新核对实际 status/HEAD/log。

### 8.2 已存在、应直接复用的实现

当前代码已经具备并应保留：

- 制盘显式选盘 → BackupPrompt → Menu → Form → Planning → Review → Confirm → Running → Result；
- Form 宽屏 56/44“参数 + 实时布局”、窄屏上下布局；
- sector-backed Quick/Exact 容量编辑、MiB/GiB/sector 单位切换基础；
- 独立 start LBA、合法 gap、overlap fail-closed；
- 右侧实时 layout bar、最大可设/grow/limiter 计算；
- `DiskProvisionKind::{Plain,Mode0,Mode1,Mode2,Mode3}`；
- 已注册盘 Prefill/Preserve candidate 基础；
- system-disk/USB guard、unmount/lock、reopen 身份复核、LBA3 保护；
- official atomic write：snapshot touched sectors、data/LCE first、LBA1–12 next、MBR last、readback/rollback；
- first-party sparse exFAT writer/analyzer 与已有 Virtual-HIL 经验。

### 8.3 已完成阶段（2026-09-25）

- Phase 0 已完成：本计划作为长期事实源独立提交并推送；
- Phase 1 已完成：字段级输入过滤、容量 `f` 填满、输入框光标、三位小数显示、selected 高亮、section 对齐、最大可设/比例门禁与统一 focus marker 已落地；
- Phase 2 已完成：Offline Convert 已从 TUI、palette、TaskHub、application service、CLI grammar/help/completion、`sectors.rs` conversion-only 写入 API、专用测试和 README/USAGE 产品文案中删除；
- Phase 3 已完成：新增 `ProvisionTarget::{Plain, Official(...)}`；`NewProvisionRequest` 与 TUI→worker 边界携带目标类型而不是裸 `u8`/额外 UI kind，`OfficialPartitionMode` 只在官方 layout/generator 内部使用；
- Phase 4 已完成：Plain planner 与动态 1～4 分区 TUI 已落地，默认 P1 从 LBA2048 占满剩余盘；每个分区独立 start/capacity/filesystem/label，支持 gap、`f` 填满、Insert/Delete，编辑一个分区绝不自动移动其它分区；overlap/overflow/MBR 32-bit 边界 fail-closed；该阶段曾用三层只读挡板隔离未完成 writer，现已在 Phase 5～7 后移除；
- Phase 5 已完成：新增纯 `PlainProvisionWritePlan`，生成1～4个 MBR primary entries、MBR `0x55AA`、LBA1/2/4～12 EDP metadata cleanup、LBA3 preserve 集合、来源 LCE cleanup 与 sparse filesystem sector；所有同 LBA 候选先归一化到唯一 `PlainSectorOwner`，filesystem 实际扇区覆盖旧 LCE cleanup，不依赖写入先后决定最终内容；任何 Plain 分区覆盖 LBA3 时 fail-closed。
- Phase 6 已完成：新增目标无关 `WriteTransactionPlan` / `SectorWriteStage` / `execute_write_transaction()`；唯一事务执行器统一负责写前 sync、snapshot 全部 touched sectors、Data→Metadata→Commit 排序、exact readback、失败后 exact rollback。旧 `atomic_write_sectors` 与 official writer 均迁移到该核心，删除两套重复 snapshot/readback/rollback；`PlainProvisionWritePlan` 可无歧义映射到同一事务模型，MBR=Commit、EDP metadata cleanup=Metadata、filesystem/LCE cleanup=Data。
- Phase 7 已完成：新增默认可运行的 `plain_virtual_hil`，覆盖1～4分区、显式 gap、非2048起点、统一事务写入、LBA3 preserve、LCE cleanup、MBR entries、exFAT analyzer 与 post-write `Plain` 识别；新增 macOS disposable raw-disk HIL，只有 WholeDisk+Virtual+Disk Image 才允许进入测试写链，实际完成 raw image 制盘→eject/reattach→OS 识别 exFAT→mount→create/readback file→unmount。现有 Linux/Windows loop/VHD HIL 继续保留。
- Plain 产品写入链已启用：TUI Plain Form 不再在 `AppState` 本地伪造只读 Review，而是通过 TaskHub 调用 application `prepare_plain_provision()`，固定 USB hardware probe/容量/LBA3，并从同一 LBA0～12 快照按 LBA7 实际 entry pointer 解析来源 LCE；Review→YES→critical worker 调用 `commit_plain_provision()`，复用 system/USB guard、unmount/lock、reopen、probe/容量/LBA3 复核和统一 transaction，写后再次验证 MBR/LBA3/Plain 分类。
- 原先依赖旧 `sectors::convert` 合成免密盘的测试夹具已迁移到正式 `provision::generate_image()`，识别/备份测试继续覆盖 mode1 免密样本；
- `src/sectors.rs` 现在只保留只读盘状态识别与 LBA12 EDPF 解析，元数据写入只有 `provision` 一套正式实现。

### 8.4 Phase 1～7 验证结果

- `cargo check --all-targets` 通过；
- Phase 2 定向 CLI/TUI/sector/write-event 测试 97/97 通过；
- 夹具迁移后的 `backup` 27/27、`identify_list` 4/4 通过；
- 顶层 `convert` 与 palette `offline-convert` 都由回归测试明确保持不可用；
- README/USAGE 不再把 Offline Convert 描述为产品能力；协议文档只保留其历史验证背景；
- Phase 4 `cargo check --all-targets` 通过；Plain planner 6/6、Plain/TUI/写盘安全定向回归 71/71 通过。
- Phase 5 `cargo fmt --all -- --check`、`cargo check --all-targets` 通过；`plain_provision` 10/10 通过，覆盖 Plain MBR、EDP metadata cleanup、LBA3 preserve、LCE↔filesystem ownership 归一化、多分区 entries 与 LBA3 冲突拒绝。
- Phase 6 `atomic_write` + `provision_transaction_write` 16/16 通过；覆盖通用 stage 顺序、重复/越界拒绝、exact touched-set rollback、旧 metadata writer 回归、official writer 回归、Plain→通用事务映射。
- Phase 7 `plain_virtual_hil` 1/1 通过；macOS OS-level Plain virtual-disk HIL 1/1 通过，并实测 `/dev/rdiskN` 写盘后重新 attach 可被 macOS 识别为可挂载 exFAT，文件创建/readback/unmount 成功。
- Plain 产品链定向验证：application provision 8/8、TUI state 37/37、`plain_virtual_hil` 1/1、`provision_transaction_write` 7/7 全绿；`cargo check --all-targets` 通过。

### 8.5 下一步执行顺序

- Phase 8 已部分完成（2026-09-25）：真实 `/dev/disk4` 上 **mode0 → Plain** 已通过完整写盘验收。写前自动创建 EDPB 备份，随后走正式 `prepare_plain_provision()/commit_plain_provision()` application 安全链，结果为 `commit=PASS`；写后 LBA3 byte-for-byte preserve 通过，目标重新识别为 Plain。证据目录：`~/edpcli-phase8-hil/20260925_124458_disk4_plain`。
- 尚未完成的真实写盘场景：**Plain → mode0、mode0 → mode1、mode1 → Plain、Plain 多分区**。当前 ChatGPT Mac 执行环境在第一项完成后开始统一拦截后续制盘链命令，因此这些场景不得写成已通过；需要在允许真实写盘的终端/执行环境继续。
- Inspect 全盘结构化浏览器 I1～I9 的实现与真实只读数据链已完成；CLI 自身 raw-device sudo re-exec HIL 仍与执行环境权限限制分开记录。

1. 完成剩余 Phase 8 真实 USB 场景；
2. 并行按第 9 节实施 Inspect 全盘结构化浏览器，但不得复制 CLI/TUI 两套解析后端。

最终产品定义：**edpcli 制盘中心统一面向五种磁盘目标状态，其中 mode0～mode3 是官方 EDP 模式，Plain 是非 EDP 普通盘目标而不是 mode4。所有目标共用同一套选盘、表单、实时布局、Review 和安全事务基础；容量以 sector 为唯一精确真相，UI 提供 MiB/GiB/sector、`f` 填满、字段级输入约束和统一焦点视觉。Plain 复用现有制盘界面并支持1～4个 MBR 普通分区，不自动移动其它分区，不宣称安全擦除。**

## 9. Inspect 全盘结构化浏览器重构计划（2026-09-24）

### 9.1 背景与目标

当前 TUI Inspect 仍以 LBA0～12 为核心视图，这已经不能覆盖项目现有能力和后续协议分析需求。Inspect 必须从“查看 EDP LBA0～12”重构为：

> **查看整个物理盘，并在已知区域叠加 EDP 协议语义。**

核心目标：

1. TUI Inspect 支持全盘任意 LBA / byte offset 浏览，不再限制 LBA0～12；
2. LBA0～12 只是全盘结构树中的一个已解析区域；
3. LCE、分区表、普通分区、数据区、尾部区域、未分配区等都能作为树节点浏览；
4. 未知区域仍可完整查看 raw bytes，不因没有 decoder 而不可达；
5. 单扇区查看升级为真正的 Sector Inspector：Hex、ASCII、字段、byte、bit、raw/decoded 三者联动；
6. 已知字段与 Hex byte 双向高亮；
7. 字段颜色与整套 TUI Theme 使用统一语义 token，不在 Inspect 内独立硬编码配色；
8. 树节点统一使用 `o` 展开/折叠；
9. 全盘浏览必须 lazy / virtualized，不允许为整盘预生成 sector 节点；
10. CLI `inspect raw/decode/meta` 与 TUI 使用同一套 reader / decoder / metadata backend，不允许双轨解析。

### 9.2 产品结构：三层浏览模型

Inspect 统一分为三层：

```text
Disk
└─ Region / Structure
   └─ Sector
      └─ Group / Field
         └─ Byte / Bit
```

用户可以从整盘结构逐层进入，也可以通过跳转直接到任意扇区。

#### 9.2.1 Disk 层

负责显示：

- 设备路径；
- 容量；
- sector size；
- VID/PID/deviceid；
- 当前识别状态：mode0 / mode1 / mode2 / mode3 / Plain；
- 分区布局；
- 已知协议区域；
- 未知/未分配区域；
- 尾部区域。

#### 9.2.2 Region / Structure 层

结构树建议：

```text
diskN
├─ Device
│  ├─ Identity
│  └─ Geometry
├─ EDP Metadata
│  ├─ LBA0
│  ├─ LBA1
│  ├─ ...
│  └─ LBA12
├─ LCE
│  ├─ extent
│  └─ decoded fields
├─ Partition Table
│  ├─ Entry 0
│  ├─ Entry 1
│  └─ ...
├─ Partitions
│  ├─ Partition 1
│  │  ├─ Boot sector
│  │  └─ Data
│  └─ Partition 2
├─ Unallocated / Raw Range
└─ Tail Area
```

未知区域不能伪造语义，统一显示为 `Unknown`、`Raw Range` 或已经验证过的其它状态。

#### 9.2.3 Sector 层

任意 sector 都可以进入单扇区查看器。已知 sector 叠加 decoder；未知 sector 仍显示完整 512B raw data。

### 9.3 TUI 主布局

宽屏默认采用三栏：

```text
┌ Inspect ────────────────────────────────────────────────────────────────────┐
│ Disk: /dev/diskN   119.2 GiB   mode1   512 B/sector          [RO]          │
├──────────────────────┬──────────────────────────────┬───────────────────────┤
│ STRUCTURE            │ HEX / RAW                    │ FIELD DETAILS         │
│                      │                              │                       │
│ ▼ diskN              │ LBA 4                        │ ▼ Identity            │
│   ├─ ▼ EDP Metadata  │ 0000  45 44 50 00 ...       │   onlyid   ...        │
│   │   ├─ LBA0        │ 0010  01 00 00 00 ...       │   VID      0781       │
│   │   ├─ ...         │ ...                          │   PID      5583       │
│   │   └─ LBA12       │                              │                       │
│   ├─ ▶ LCE           │                              │ ▼ Flags               │
│   ├─ ▼ Partitions    │                              │   ...                 │
│   ├─ ▶ Data Area     │                              │                       │
│   └─ ▶ Tail Area     │                              │                       │
├──────────────────────┴──────────────────────────────┴───────────────────────┤
│ o 展开/折叠  Enter查看  gl 跳转  / 搜索  Tab面板  ? 帮助  q返回            │
└─────────────────────────────────────────────────────────────────────────────┘
```

窄屏不复制另一套逻辑，只重排同一状态模型：

1. Structure；
2. Hex；
3. Fields/Details；

通过 Tab/Shift+Tab 切换焦点或视图。

### 9.4 树状导航规则

统一交互：

```text
j / ↓          下一项
k / ↑          上一项
o              展开 / 折叠当前树节点
Enter          查看 / 进入当前节点
gg             树顶部
G              树底部
Tab            下一个面板
Shift+Tab      上一个面板
Esc / q        返回
```

约束：

- `o` 是唯一主展开/折叠快捷键；
- 不再依赖左右箭头展开树；
- `←/→` 保留给文本光标、Hex byte 光标和需要水平移动的内容；
- `▶` 继续遵守全 TUI 规则：仅表示当前键盘焦点，不表示普通状态。

### 9.5 全盘浏览必须 lazy / virtualized

禁止建立：

```text
LBA0
LBA1
...
LBA268435455
```

这样的全量节点。

大范围节点只记录：

```text
start_lba
sector_count
kind
decoder/metadata optional
```

例如：

```text
▶ Data Area
  LBA 2048 – 243621887
```

进入后按屏幕窗口只实例化当前附近的少量 sector。Sector cache 应有明确上限，不因持续滚动无限增长。

### 9.6 跳转能力

`g` 打开统一 Jump 面板：

```text
Jump to

LBA / offset:
┌───────────────────────┐
│ 243623933             │
└───────────────────────┘

Unit: LBA
Space: LBA / byte offset
```

至少支持：

- 十进制 LBA；
- 十六进制 LBA；
- 十进制 byte offset；
- 十六进制 byte offset。

跳转后：

1. 结构树尽可能定位到该位置所属 Region；
2. 打开对应 sector；
3. Hex 光标定位到 offset 对应 byte；
4. 若属于已知字段，字段树自动联动定位。

### 9.7 单扇区 Sector Inspector

#### 9.7.1 顶部位置元数据

单扇区页面必须明确告诉用户“当前 512B 在整盘哪里”：

```text
LBA:             4
Sector size:     512 B
Absolute offset: 0x00000800
Range:           0x800..0x9FF
Region:          EDP Metadata
Parser:          LBA4
```

未知扇区：

```text
Region:          Data Area
Parser:          none
```

不允许用户只看到 Hex 而不知道磁盘位置和区域归属。

#### 9.7.2 Hex 基础布局

512B sector 固定按 16B/行显示，共 32 行：

```text
OFFSET  00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F   ASCII

0000    45 44 50 00 01 00 00 00 37 41 46 32 31 39 00 00   EDP.....7AF219..
0010    ...
...
01F0    ...
```

默认 offset 是 sector-relative：

- `0000` = 当前 LBA + 0x000；
- `0046` = 当前 LBA + 0x046；
- `01FF` = 当前 LBA + 0x1FF。

#### 9.7.3 相对/绝对 offset

必须同时具备 sector-relative 与 disk-absolute 概念。

例如 LBA4：

```text
absolute sector start = 4 * 512 = 0x800
sector +0x046
disk   +0x00000846
```

UI 可以默认显示 relative，提供切换或在 detail/status 中同时展示两者。内部定位始终使用无损 integer offset，不用浮点换算。

#### 9.7.4 Byte 光标

Hex panel 获得焦点后：

```text
←     前一个 byte
→     后一个 byte
↑     上一行同列
↓     下一行同列
```

当前 byte detail 至少包括：

```text
offset       +0x045
absolute     0x845
hex          01
decimal      1
binary       00000001
ASCII        .
```

移动 byte 光标不能触发 Tab 切换或树导航。

#### 9.7.5 Hex → Field 联动

若当前 byte 落在已知字段范围内，字段面板自动选中对应字段：

```text
Field
name          bConnectServer
range         +0x046
size          1 byte
raw           0x01
decoded       true
group         Flags
```

字段树同步定位：

```text
▼ Flags
  > bConnectServer
```

#### 9.7.6 Field → Hex 联动

选择一个字段后，Hex 必须高亮其完整 byte range。

例如：

```text
onlyid
offset +0x008..+0x027
length 32 B
```

则 Hex 中对应 32B 连续高亮；状态栏同步显示：

```text
onlyid  +0x008..+0x027  32 B
```

如果字段跨行，跨行连续高亮；不能只高亮首 byte。

#### 9.7.7 多字节字段解释

已知 typed field 可以显示：

```text
VID
offset       +0x018..+0x019
length       2 B
raw          81 07
u16 LE       0x0781
decimal      1921
decoded      SanDisk VID
```

支持的数据表示可包括：

- u8；
- u16/u32/u64 LE；
- 必要时 BE；
- ASCII；
- UTF-8；
- hex；
- bit field。

但只显示 decoder 明确声明的主解释，不默认把所有可能的整数编码全部铺出来。

#### 9.7.8 Bit 展开

对 flag 字段，`o` 可继续展开到 bit：

```text
▼ flags +0x046 = 0x05
  ├─ enable         bit0 = true
  ├─ reserved       bit1 = false
  └─ connectServer  bit2 = true
```

未知 bit 只能标 unknown/reserved，不能猜含义。

#### 9.7.9 字段树层级

统一支持：

```text
Sector
└─ Group
   └─ Field
      ├─ range
      ├─ raw
      ├─ decoded
      └─ bits / members
```

例如：

```text
▼ LBA4
  ▼ Identity
    ▶ onlyid
    ▶ HSerial
    ▶ HDSerial
  ▼ Flags
    ▶ Flags0
    ▶ Flags1
```

`o` 始终作用于当前可展开节点。

#### 9.7.10 Unknown / Reserved / Preserved 必须区分

Inspect 不允许因为某段 bytes 当前全 0 就标记为 padding。

状态至少区分：

- `Unknown`：语义未闭环；
- `Reserved`：已验证为保留字段；
- `Preserved`：跨版本/跨代要求原样保留；
- `Padding`：只有有明确证据时才允许使用。

这些状态必须来自协议真相源/decoder metadata，不由 TUI 自行推断。

#### 9.7.11 前后扇区

单 sector 浏览至少支持：

```text
PageUp      previous sector
PageDown    next sector
```

切换时：

- 保持当前面板；
- 尽量保持 byte column；
- 更新 region/parser/offset；
- 已知字段重新解析；
- 不重新加载整盘。

### 9.8 Raw / Decode / Mixed 三种查看模式

保持与 CLI 概念一致：

#### Raw

最大化 Hex/ASCII，适合完全未知区域。

#### Decode

以结构树和字段语义为主，适合正常用户快速阅读。

#### Mixed

默认模式：

```text
Hex + Field Tree + Detail
```

适合协议分析，是 TUI Inspect 的主工作模式。

`r` 可以在 Raw → Decode → Mixed 之间循环；若现有全局快捷键冲突，以统一 shortcut registry 为准，不允许页面私自覆盖。

### 9.9 搜索

`/` 打开 Inspect Search。

第一阶段至少支持：

- 字段名；
- group 名；
- LBA；
- decoded value；
- region 名。

例如：

```text
/ onlyid
/ bConnectServer
/ LBA12
/ type4
```

后续可增加：

```text
/hex DE AD BE EF
```

全盘 raw byte pattern 搜索。

全盘 raw 搜索必须是流式、可取消、有进度的独立阶段，不能阻塞 TUI 主线程，也不能在第一版为了“功能齐全”直接同步扫整盘。

### 9.10 颜色与整体风格

Inspect 不再定义自己的“彩虹配色”，全部走全局 Theme semantic token。

建议语义：

| 语义 | 用途 |
| --- | --- |
| primary/accent | 字段名、关键结构 |
| foreground | 普通 value |
| secondary accent | enum / typed value |
| success | valid / true / checksum pass |
| warning | 可疑但可继续 |
| error | invalid / checksum fail |
| muted | unknown / reserved /辅助 offset |
| selection | 当前 focus / 当前 byte / 当前 field |

Hex 字段范围允许弱背景或下划线帮助区分 Identity / Flags / Checksum / Reserved 等，但必须：

- 低饱和；
- 不遮盖 raw byte 文本；
- 当前 selection 的视觉优先级最高；
- 同一主题 token 在 Devices / Backups / Provision / Inspect 含义一致；
- 不在业务组件里写死 ANSI/RGB 颜色。

### 9.11 数据模型

禁止继续把 TUI Inspect 写成：

```rust
if lba <= 12 {
    ...
}
```

引入统一只读浏览节点模型，概念上至少包含：

```text
InspectNode
├─ id
├─ label
├─ kind
├─ range
├─ children
├─ decoder
└─ status
```

建议 `kind` 支持：

- Device；
- Region；
- Extent；
- Sector；
- Structure；
- Group；
- Field；
- Partition；
- UnknownRange。

`range` 至少使用：

```text
start_lba
sector_count
optional byte subrange
```

与备份系统现有 `DeviceSnapshot / Region / Extent / Artifact` 概念尽可能复用公共磁盘区域抽象，但不要为了复用强行把 UI 状态塞入备份领域模型。

### 9.12 Reader / Decoder / UI 分层

统一数据流：

```text
Disk
 │
SectorReader
 │
raw bytes
 ├──────────────→ Hex View
 │
 └──────────────→ Decoder Registry
                    ├─ LBA0
                    ├─ LBA4
                    ├─ LBA12
                    ├─ LCE
                    └─ future decoders
```

原则：

1. raw bytes 是唯一底层事实；
2. decoder 是 raw bytes 上的语义 overlay；
3. TUI 不重复实现 parser；
4. CLI `inspect --lba ... --raw/--decode/--meta` 与 TUI 共用 backend；
5. 已知 decoder 失败时必须 fail closed：仍可显示 raw，但不能显示伪 decoded 值；
6. decoder 返回字段必须携带 range，才能实现 Hex↔Field 双向联动。

### 9.13 缓存与性能

第一版即纳入以下约束：

- 当前 sector 必须按需读取；
- 可预取少量前后 sector，但 cache 有硬上限；
- 大 Region 不能展开成全量 child vector；
- 树滚动只 materialize viewport 邻近节点；
- decoder 仅对当前/预取 sector 执行；
- 全盘 search 与 checksum 等重操作独立为可取消任务；
- 切换 sector 不允许触发整盘重新扫描。

### 9.14 复制能力

协议分析高频需要复制。

建议：

```text
y    copy current value/raw byte
Y    copy full location + raw + decoded description
```

例如：

```text
LBA4 +0x018..+0x019 = 81 07 -> 0x0781
```

若全局已有 yank/copy shortcut，按统一快捷键体系实现，不在 Inspect 另造冲突键。

### 9.15 跨扇区字段

数据模型必须从第一版就允许字段跨 sector，即使 UI 第一阶段只做基础展示。

例如：

```text
start = LBA100 +0x1F0
length = 64 B
```

实际覆盖：

```text
LBA100 +0x1F0..+0x1FF
LBA101 +0x000..+0x02F
```

Field range 必须使用绝对 byte range 或等价无损结构表达，不能假设所有字段都局限在单一 512B sector。

### 9.16 快捷键目标方案

最终目标：

```text
j / ↓          下一项
k / ↑          上一项
o              展开 / 折叠
Enter          查看 / 进入
Tab            下一个面板
Shift+Tab      上一个面板

gl             跳转 LBA / offset
/              搜索

PageUp         上一个 sector
PageDown       下一个 sector
Home           当前 region 开头
End            当前 region 末尾

← / →          Hex byte 光标 / 输入框文本光标
v              Raw / Decode / Mixed
?              当前上下文快捷键帮助
Esc / q        返回
```

底部状态栏不一次性列出所有快捷键，只显示当前上下文最有用的一组，例如：

```text
o 展开  Enter查看  gl 跳转  / 搜索  Tab面板  ? 帮助
```

### 9.17 分阶段实施顺序

#### Phase I1：现状审计与测试基线

- 审计当前 CLI Inspect/TUI Advanced Inspect/backend；
- 找出所有 LBA0～12 hard-coded 边界；
- 找出现有 raw/decode/meta reader/decoder 的重复实现；
- 记录现有 Theme token / focus marker / tabs / scrolling primitives；
- 测试先行锁住当前已验证协议解析结果。

#### Phase I2：统一 Inspect backend

- 建立 `SectorReader` / range reader；
- 建立 decoder registry；
- Field 带 byte range/type/raw/decoded/status；
- CLI raw/decode/meta 迁移到统一 backend；
- 不改协议含义，只改变组织方式。

#### Phase I3：全盘 InspectNode / Region 模型

- Device → Region → Extent → Sector → Field；
- LBA0～12、LCE、partition table、partitions、data、tail、unknown range 全部可表达；
- 大区域 lazy children；
- 任意 LBA 能映射到所属 region。

#### Phase I4：树状 TUI

- 三栏宽屏 / 同状态窄屏；
- `o` 展开折叠；
- j/k；
- Enter；
- Tab/Shift+Tab；
- viewport virtualization；
- selection 保持可见。

#### Phase I5：Sector Inspector

- 32×16 Hex；
- ASCII；
- relative / absolute offset；
- byte cursor；
- PageUp/PageDown；
- Raw/Decode/Mixed；
- typed value；
- bit 展开。

#### Phase I6：Hex ↔ Field 双向联动

- byte → field；
- field → byte range；
- 多行 field range；
- unknown/reserved/preserved 状态；
- 跨 sector field range 数据模型；
- copy/yank。

#### Phase I7：Jump / Search

- `g` LBA/offset；
- `/` 结构化字段搜索；
- 自动树定位；
- 第二阶段再增加 raw full-disk pattern search。

#### Phase I8：视觉统一与窄屏

- Theme semantic token；
- selected 高亮不铺满无关 padding；
- `▶` 统一；
- tabs 与其它页面一致；
- 窄终端降级规则；
- 不硬编码颜色。

#### Phase I9：回归与真实盘只读验收

- mock/sparse disk；
- 大容量虚拟盘；
- mode0～mode3；
- Plain；
- LCE 存在/不存在；
- 未知 sector；
- 真实 USB 只读 Inspect；
- 验证不会写盘。

### 9.18 测试门禁

至少增加以下回归测试：

1. TUI Inspect 不再限制 LBA0～12；
2. 任意合法 LBA 可跳转；
3. 超出设备范围的 LBA fail closed；
4. 大磁盘不会建立与 sector_count 等量的节点；
5. lazy range 滚动不会无限增长内存；
6. `o` 只展开/折叠当前节点；
7. `←/→` 在 Hex 内只移动 byte 光标；
8. `←/→` 在文本输入框内只移动文本光标；
9. Field → Hex 完整 range 高亮；
10. Hex byte → Field 正确反查；
11. 跨行字段高亮范围正确；
12. multi-byte typed decode endian 正确；
13. unknown byte 不被误标为 known/padding；
14. Unknown / Reserved / Preserved 分类不混淆；
15. PageUp/PageDown 正确切换 sector；
16. relative/absolute offset 换算精确；
17. CLI 与 TUI 对同一 sector 使用同一个 decoder 结果；
18. decoder error 时仍能查看 raw；
19. Theme 中不出现 Inspect 私有硬编码业务颜色；
20. 窄屏布局不丢失选中节点/byte；
21. 全盘 Inspect 所有常规路径保持只读；
22. 真实 USB Inspect 不触发写盘、mount 改写或制盘事务。

### 9.19 单扇区第一阶段完成标准

单扇区查看器第一阶段只有同时满足以下条件才算完成：

1. 任意合法 LBA 均能打开；
2. 512B 完整显示为 32 行 × 16B；
3. 同时具备 sector-relative 与 disk-absolute offset；
4. byte 光标可以逐 byte 移动；
5. 已知字段和 Hex 双向联动；
6. 字段可用 `o` 展开 raw/decoded/range/bit；
7. 未知区域明确标 Unknown，不猜；
8. PageUp/PageDown 无缝切换前后 sector；
9. 单 sector 浏览只读取当前和有界邻近 cache；
10. CLI/TUI 共用 reader/decoder；
11. 颜色、selection、focus marker 与全局 Theme 一致；
12. 不改变任何已闭环协议字段含义。

### 9.20 Inspect 整体完成标准

只有同时满足以下条件才算新版 Inspect 完成：

1. 全盘任意位置可浏览；
2. LBA0～12 不再是 UI 上限，只是 EDP Metadata 节点；
3. LCE、partition table、partitions、data、tail、unknown range 可被统一表达；
4. 大区域 lazy/virtualized；
5. Sector Inspector 完成；
6. Hex↔Field 双向联动完成；
7. `o` 树折叠/展开完成；
8. `g` 任意 LBA/offset 跳转完成；
9. 结构化搜索完成；
10. Raw/Decode/Mixed 完成；
11. Theme/焦点/Tab 与其它 TUI 页面统一；
12. CLI/TUI decoder 同源；
13. 所有新增门禁全绿；
14. 真实 USB 只读验收通过；
15. README/USAGE/本文件与真实实现保持一致。

最终产品定义：**新版 Inspect 是 edpcli 的只读全盘结构化浏览器。树负责定位磁盘结构，Sector Inspector 负责研究具体数据，decoder 只在 raw bytes 上叠加经过验证的协议语义；无 decoder 的区域仍然可看，未知含义绝不猜测。**

### 9.21 当前实施状态（2026-09-25）

- Phase I1 审计已完成：CLI 已支持任意 `u64` LBA/range/count，TUI 全盘 Inspect backend 也具备任意合法 LBA 读取能力；确认历史主要双轨来自旧 `InspectWorkspace` 固定 LBA0～12 路径，以及 CLI/TUI 各自维护的 protocol/non-protocol decode 与 meta 决策。
- Phase I2 已完成：application 层统一 `SectorReader` / checked range reader、`Protocol / LCE / Partition` decoder registry、`decode_sector()` / `sector_meta_text()`；CLI `raw/decode/meta` 已删除自己的 EDPB/物理盘 reader、partition boot 读取、decode/meta/export 循环，只负责来源选择、参数转换和渲染统一 `AdvancedInspectWorkspace`。旧 `InspectWorkspace` 已删除；TUI 现行产品只暴露一个 Inspect，全盘树直接复用该 backend。
- 统一 Field 模型已落地：协议 parser 原有字段语义不改，由 application materialize 为绝对 `[start,end_exclusive)` byte range、Field type、raw bytes、decoded bytes、status、label/value/group/children；range 模型从第一版即可无损表达跨 sector 字段，unknown/reserved/preserved 状态枚举已预留，当前已验证协议字段标记为 Known。
- Phase I3 已完成模型层：新增 UI-neutral 的 `InspectTopology / InspectNode / InspectNodeRange / InspectChildren`，形成 Device → Region → Extent → lazy Sector → Field 结构；LBA0～12、LCE、各 partition/data、盘尾取证窗口、盘尾 9-sector mirror、end-4 restore-node 与 unknown complement 均可表达。LBA0 sector stub 显式包含 MBR partition-table Structure；任意 LBA 可映射到一个或多个所属 Region。
- 大 Region/Partition/unknown range 不生成全量 Sector vector，只保存 `LazySectors { start_lba, sector_count }`，按 offset+limit materialize sector page；tail mirror/end-4 嵌套在 tail region 内，避免顶层重复区域。拓扑复用 EDPB 的 `SemanticStatus`，但不把 TUI 展开/焦点状态塞入备份领域模型。
- `AdvancedInspectWorkspace` 已直接携带同源 topology，因此 CLI/TUI 后续不需要重新推导磁盘区域。I3 定向门禁：tree 5/5、application inspect 11/11、CLI 6/6、TUI workspace 5/5、TUI lifecycle 13/13，`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I4 已完成：旧“高级检查参数表单 → 平铺 LBA 结果”双轨已删除，选择物理盘/EDPB 后直接后台建立 topology 并进入统一 Browser。宽屏采用 Tree / Overview / Detail 三栏，中等宽度为左树 + 右侧上下两栏，窄屏复用同一 Browser 状态纵向排列，不维护第二套窄屏状态机。
- 树交互已落地：`j/k` 与上下键移动，`h/l` 收起/展开，`o` 切换折叠，Enter 查看/进入，`gl` 跳转，`/` 搜索、`n/N` 循环匹配，Tab 正向 Tree→Overview→Detail、Shift+Tab 反向切换，Esc 返回；selection 通过 `visible_window()` 始终保持在可视窗口内。旧 `AdvancedInspectStage::Form/Result`、`AdvancedInspectForm`、旧 result navigation 与参数表单文案均已清零。
- viewport virtualization 已落地：每个 lazy extent 独立维护 sector window offset，每页最多 materialize 64 个 Sector，并通过“上一页/下一页”控制行翻窗；200-sector 回归样本验证第一页仅 LBA2048..2111、第二页仅 LBA2112..2175，旧页节点立即退出树，树规模不随分区总容量线性增长。I4 定向门禁：TUI lifecycle 13/13、TUI state 38/38、Inspect workspace 5/5、Inspect scroll 3/3，`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I5 已完成：Sector 行 Enter 后通过独立 generation + single-flight worker 按需读取，不把 I/O 放进 TUI state；同一 application backend 新增 Sector Inspector 专用 fail-soft decode，decoder 不适用时仍返回 512B raw 并携带 `decode_error`，CLI `decode` 仍保持严格 fail-closed。
- Sector Inspector 固定逻辑视图为 32×16B Hex + ASCII，显示 sector-relative offset 与 disk-absolute byte offset，支持 byte cursor（←/→ ±1B、j/k 或 ↑/↓ ±16B）、`0/$` 当前 16B 行首尾、`gg/G` 当前 Sector 首尾、`Ctrl-u/Ctrl-d` 半页、PageUp/PageDown 跨 Sector且保留 cursor、`v` 循环 Raw/Decode/Mixed；小终端仅滚动可视行，不改变 32×16 数据模型。
- 当前 byte 会映射同源 `InspectField`，详情显示 typed value、field type/status、绝对 byte range；无已知 field 时明确显示 Unknown，不推测语义。按 `o` 展开当前 byte bit 与已验证 Field child。metadata LBA0～12 常驻，非 metadata 按需缓存最多 5 个 Sector，避免全盘浏览退化为无界内存增长。
- I5 专项门禁：application Inspect 12/12、CLI Inspect 6/6、Sector Inspector 2/2、TUI lifecycle 13/13、TUI state 39/39 通过；完整 `cargo fmt --all -- --check`、`cargo check --all-targets`、`cargo test --all-targets`、`git diff --check` 均通过。下一步进入 Phase I6：完成 Hex ↔ Field 双向定位、多行 range、Unknown/Reserved/Preserved 可视语义和 copy/yank。
- Phase I6 已完成：Hex byte → Field 继续直接使用 canonical `InspectField` 绝对 byte range 反查；树中 Field → Hex 新增反向入口，Enter Field 后直接打开 Sector Inspector、跳到字段起始 byte，并 pin 当前 Field。完整 Field range 使用绝对 byte range 高亮，可跨 16B 行；跨 sector Field 通过 PageUp/PageDown 保留 pin 并在相邻 sector 定位到 range 交集起点，手动移动 byte 后自动退出 pinned Field，恢复 cursor-driven 反查。
- `Known / Unknown / Reserved / Preserved` 保持独立状态并使用现有 Theme semantic style 渲染，不引入 Inspect 私有硬编码颜色；Unknown byte 继续明确显示未分类，不自动降级为 padding/Reserved。`y` 使用 TUI 内部 yank register 复制当前 Field 语义值或当前 byte，`Y` 复制当前 Field raw range；该 register 跨平台、可测试，不在 TUI state 中引入 `pbcopy`/`xclip` 等平台命令依赖。
- I6 专项门禁：Sector Inspector 4/4 通过，覆盖 Field→Hex、Hex→Field、跨行/跨 sector range、Unknown/Reserved/Preserved、yank register；完整 `cargo test --all-targets` 通过（最终长时 Runner Job `exit_code=0`），`cargo fmt --all -- --check`、`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I7 已完成：Browser 中 `gl` 打开统一 Jump 输入，支持十进制/0x 十六进制 LBA 与绝对 byte offset，并使用 checked arithmetic 将 offset 无损映射为 LBA + sector-relative byte；越界、非法和 u64 overflow 均 fail closed，不做 silent clamp。Jump 命中 lazy extent 时仅切换该 extent 的 64-sector window、展开必要祖先并选中目标 Sector；byte offset 继续复用现有 Sector worker，Inspector cursor 精确落到目标 byte，event loop/state 不直接执行磁盘 I/O。
- `/` 结构化搜索已完成：搜索 canonical topology 的 Region / Extent / Structure / Group / Field label，以及当前已知/cached `InspectField.value`；纯 topology 路径查找和 cached sector 结构匹配位于 `application::inspect_tree`，TUI 不复制 decoder/parser。命中会自动展开 Tree 路径并定位目标；`n/N` 在当前匹配集合中循环下一个/上一个，Field 命中可直接继续既有 Field→Hex。搜索不会为大分区 materialize 全量 Sector，也不会执行同步 full-disk raw scan；raw pattern search 仍明确留在第二阶段。
- I7 已完成全部门禁：Sector Inspector 9/9（含 LBA jump、hex LBA、absolute byte offset 精确 cursor、lazy extent 自动翻页、invalid/overflow/out-of-range、Field label/typed value 搜索、40×10/80×24/120×36 prompt 渲染与 cache/virtualization 不退化）、TUI state 39/39、TUI lifecycle 13/13、Inspect tree 5/5、documentation layout 5/5 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、680/680 tests passed、0 failed。下一步进入 Phase I8：视觉统一与窄屏收口。
- Phase I8 已完成实现：全局渲染入口新增 `ThemeToken` 语义层，原有 accent/secondary/success/warning/error/muted/selection 全部经统一 token 映射；Advanced Inspect 区域不再直接引用 `Color::*`。Unknown/Reserved 使用 muted 语义，Preserved 使用 success，当前 byte/field/panel selection 继续使用统一 selection token。
- Advanced Inspect 新增与其它页面一致的 `结构树 / 节点概览 / 节点详情` Panel；I8 当时曾用 `Tab/Shift+Tab` 切换 `AdvancedInspectPanel`，该历史键位已被第 10 章最终规则取代，现行只允许 `Ctrl-w*` 切换 Panel，`Tab/Shift+Tab` 专用于顶层 Devices/Backups 标签。Tree 的 `▶` 仅表示当前键盘焦点且全屏唯一，展开/折叠改用 `−/+`，避免与 focus marker 混淆；selected 背景只覆盖箭头和实际 row 内容，不涂满到 panel padding/border。
- I8 窄屏采用同状态单面板降级：宽屏仍为三栏，中屏仍为左树 + 右侧上下两栏；当宽度 <92 或有效高度 <14 时只渲染当前 active panel，Tree selection、detail scroll、Sector byte cursor 都不被响应式布局改写。Sector Inspector 在 60×18 下仍会自动滚到 cursor 所在 16B 行，并保留精确 byte selection。
- I8 已完成全部门禁：TUI contract 8/8、TUI lifecycle 13/13、Sector Inspector 10/10、Backup workspace 5/5、旧 Inspect workspace 5/5、Search/Command 4/4、documentation layout 5/5 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets`、`git diff --check` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、682/682 tests passed、0 failed。下一步进入 Phase I9：回归与真实盘只读验收。
- Phase I9 自动验收已完成：新增 `inspect_full_disk_acceptance`，用正式 `generate_official_image()` 生成 mode0～mode3 四种官方协议形态，逐一验证 LBA12 logical partitions 与 LCE 同时进入统一 full-disk topology，且 LBA0 MBR 对同一官方分区的可见映射不会再生成重复 Region；4,000,000,000-sector sparse disk 的高 LBA Jump 仍只 materialize 至多 64 个 Sector，Unknown decode 保持 fail-closed。
- I9 审计补齐 Plain 盘拓扑：LBA0 中签名有效、范围合法且未被 LBA12 同范围覆盖的 MBR primary entry 现在以 `MBR Pn` lazy Partition Region 进入全盘树；decoder 保持 `None`，因此普通分区 raw 可达但不会猜测 EDP 加密语义。越界 MBR entry 直接忽略，不 silently clip；与 EDP partition 同范围时不重复。对应 Inspect tree 门禁 8/8 通过。
- I9 只读边界新增 application 门禁：`SectorReader` audit fixture 证明 Raw Inspect 只读取显式请求的 sector，不触发额外数据区读取，更没有写接口；application Inspect advanced tests 8/8、full-disk acceptance 3/3、TUI state 39/39、Sector Inspector 10/10 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets`、`git diff --check` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、689/689 tests passed、0 failed。
- 真实盘只读验收已取得实盘证据：2026-09-25 对 `/dev/disk4`（8.1GB 外置物理 USB，15,728,640 × 512B）使用独立只读 raw-sector helper 成功读取 LBA0～12、LCE 6 sectors、三个真实分区起始/相邻 sector、未知 LBA20 与盘尾 16 sectors；未执行 mount/unmount/eject/provision/write。LBA0～12 首次/末次物理读取均为 6656B，SHA-256 同为 `55438b4b0fc04eb8e0b323d49807eea96a56f776f4cc04a3ebbaf05ed9c2c95b`，`cmp` byte-for-byte IDENTICAL，证明本轮测试未改写主协议区。
- 真实盘生产解析链验证通过：`SysRunner + identify()` 从真实 LBA7 与硬件探测得到 `disk&ven_aigo&prod_u335&rev_1100`，VID/PID=`3535/6300`，容量与物理盘一致；生产 `decode_sector()` 对 LBA0、4、7、11、12 分别进入 canonical `lba0/lba4/lba7/lba11/lba12`，其中 LBA7=rolling XOR、LBA11=DRKB+PDKB、LBA12=A6B0 512B，均未回退 RAW。真实 `InspectDiskContext` 解出 3 个 partition 与 LCE：type1 从 LBA63 开始并识别为 FAT16 明文；type2 从 LBA20480 开始、type4 从 LBA13627392 开始，两者均 `SM4-ECB + FileKeyCRC=PASS`，解密后识别为 exFAT；LCE 位于 LBA15725843～15725848，6/6 sectors 均按 EDPSECDISK zero8 + 64-bit physical-offset tweak 解码；未知 LBA20 明确 fail-closed。
- 仍需区分的一层是 CLI 自身的 raw-device 二次提权启动：当前非交互 Runner 中 `edpcli inspect --disk 4` 会重新执行 `sudo`，该 sudo 会话不能复用外部 helper 的认证票据，因此 CLI 进程级 raw-device 入口尚未直接跑通。这个限制不影响上述“真实物理扇区读取 + 当前生产 reader/context/decoder 语义”验收结果，但在获得可供 Runner 复用的受限 raw-device 权限前，不把“CLI 自身 sudo re-exec HIL”写成已通过。

- 2026-09-25 收口：Inspect 双轨已从运行时删除，而不只是隐藏入口。`NavCommand` 只保留单一 `OpenInspect`，`:advanced-inspect` / `:inspect-advanced` / `:ai` 不再作为命令面板别名；旧 `InspectState`、`inspect_data/inspect_pending`、旧 LBA0～12 worker/queue、旧 flat renderer 与对应旧滚动测试均已删除，原有两个有价值的 EDPB backend 测试迁入 `inspect_suite`。`:inspect`、设备/备份页单键 `i` 和备份页 Enter 均直接进入同一全盘结构树。Sector Inspector 补齐此前文档声明但事件循环未接线的 `Space`、`0/$`、`gg/G`、`Ctrl-u/Ctrl-d`，并新增真实 `n/N` 循环搜索状态。专项回归 `provision_suite` 178/178、`inspect_suite` 55/55、`tui_suite` 147/147；正式 fast gate 6 suites / 8 artifacts、0 failures，正式 full gate 8 suites / 10 artifacts / doctest、0 failures。

---

## 10. TUI 现代视觉与 Vim 键位统一重构计划（2026-09-25）

本节是后续 TUI 视觉与快捷键实现的**全局事实源**。第 9 节中已经完成的 Inspect 功能、数据模型、只读边界和验收结论继续有效；但第 9.16、9.17 以及各历史实施状态中涉及 `g`、`o`、`Tab`、`r/d/m` 等具体按键的描述仅代表当时实现。**凡与本节冲突的视觉和键位规则，一律以本节为准。**

本节只调整 TUI 表现层和输入映射，不改变 LBA0～12/LCE 已闭环协议语义，不改变 CLI 业务语义，也不得降低 system-disk guard、整盘确认、写前备份、卸载/锁卷、reopen identity、事务写入、readback、rollback 等写盘安全门槛。

### 10.1 总体目标

TUI 从当前“高饱和 ANSI 基础色 + 页面局部硬编码快捷键”重构为：

1. 支持现代终端 24-bit TrueColor，默认采用低饱和深色主题；
2. 不使用纯黑/纯白和高饱和 Cyan/Magenta/Green/Yellow/Red 作为大面积常规 UI；
3. 主色只有一种低饱和蓝青色，成功/警告/危险只表达语义，不承担装饰；
4. Selection、Focus、Panel、Input、Status 使用统一视觉层级；
5. 页面代码只表达语义 token，不直接决定 RGB；
6. 快捷键采用统一 Vim 交互语言，形成稳定肌肉记忆；
7. 明确 Normal / Insert / Search / Command / Confirm 输入状态，文本编辑时快捷键不得抢字符；
8. Workspace、Panel、List、Tree、Form、Hex、Search 使用一致的导航语义；
9. 所有快捷键由统一 keymap/action 层解释，禁止 workspace 在 event loop 中私自增加冲突的 `KeyCode::Char(...)`；
10. 帮助栏只显示当前上下文最重要的 4～6 个动作，完整映射统一由 `?` 展示。

### 10.2 现代低饱和 TrueColor 视觉系统

#### 10.2.1 默认 Dark Palette

TrueColor 默认主题固定从下面的语义调色板起步，后续如需微调必须保持相同层级关系，不重新引入高饱和基础色：

| 语义 | RGB | 用途 |
| --- | --- | --- |
| Background | `#11161C` | 应用背景 |
| Surface | `#171D24` | 普通面板/输入区域 |
| SurfaceActive | `#1D2530` | 当前行、当前区域 |
| Selection | `#263442` | 低对比选中背景 |
| Border | `#303945` | 普通边框 |
| BorderFocus | `#58758D` | 当前焦点面板边框 |
| TextPrimary | `#D7DCE2` | 正文 |
| TextSecondary | `#9BA7B3` | 次级信息 |
| TextMuted | `#687481` | hint/disabled/unknown |
| Accent | `#78A9C1` | 唯一主强调色 |
| AccentSoft | `#52758A` | 次级强调 |
| Success | `#7FA68A` | 成功 |
| Warning | `#B49A68` | 注意 |
| Danger | `#B77C7C` | 错误/危险 |
| Violet | `#9687A8` | 少量第二语义色 |

要求：

- 普通正文禁止 `Color::White`，背景禁止 `Color::Black` 作为设计目标；
- 常规选中态禁止“黑字 + Cyan 大色块”；
- `Success/Warning/Danger` 只在需要表达状态时出现；
- 不因为 mode、workspace、panel 不同而随意增加新 hue；
- 色彩差异不足以单独承载关键含义，仍需文字、符号或位置共同表达。

#### 10.2.2 Selection 与 Focus

当前选中行改为“低亮度背景 + 左侧 Accent 标记 + 正常文本”，例如：

```text
  SanDisk Ultra              disk5    58.4 GiB
▌ Kingston DataTraveler      disk6    29.1 GiB
  Generic Flash Disk         disk7    14.8 GiB
```

规则：

- Selection 背景只覆盖真实 row/content，不延伸到 panel padding 或行尾；
- `▌` 仅表示当前 selection，不替代全局 focus marker 语义；
- Panel focus 主要通过 `BorderFocus` 和标题 Accent 表达；
- 当前可编辑输入框通过 SurfaceActive + BorderFocus 表达，不把 label 一起高亮；
- disabled 使用 TextMuted，不使用高饱和颜色。

#### 10.2.3 Panel 与边框

减少“所有内容都套框”的视觉噪声：

- 顶层页面依靠留白、分组标题和文本层级组织；
- 真正独立的区域才使用边框；
- 非焦点 panel 使用 `Border`；
- 当前 panel 使用 `BorderFocus`；
- 标题默认 TextSecondary，焦点标题才使用 Accent；
- 不允许多个相邻 panel 同时使用高亮边框。

#### 10.2.4 Tabs

Tabs 不再使用高亮大背景块。建议：

```text
 Devices    Backups    Provision    Inspect
                       ─────────
```

当前 Tab：

- Accent 前景；
- Bold；
- 可配合短下划线/细分隔；
- 只使用微弱 SurfaceActive，不使用 Cyan/Magenta 高饱和背景。

#### 10.2.5 Provision 容量条

容量布局仍需要颜色区分不同区域，但全部改为低饱和语义色：

| 区域 | RGB |
| --- | --- |
| Plain | `#7693AE` |
| Boot | `#6E9CA5` |
| Share | `#789782` |
| Encrypt | `#8F819E` |
| Compatibility | `#A08D68` |
| Free | `#46515C` |

要求：

- 禁止继续直接使用 `LightBlue/Cyan/Green/Magenta/Yellow`；
- legend、bar、detail 使用同一 token；
- 保持 sector 为精确真相，视觉颜色不影响几何计算；
- 空闲区域必须比有效分区更弱，不抢焦点。

#### 10.2.6 状态颜色

普通状态默认 TextSecondary，而不是“一切 Ready 都绿色”：

```text
Status    Ready
Mode      mode1
Device    disk5
```

只有明确结果才使用状态色：

```text
✓ Backup verified
! Partition table will be overwritten
```

危险提示不默认整行红底；只对符号和关键短语使用 Danger。

#### 10.2.7 动画

`animation.rs` 不再维护独立的 Green/Yellow/Red/Cyan/Magenta 体系。普通动画主要依靠同一 hue 的亮度变化：

| 动画语义 | RGB |
| --- | --- |
| Dim | `#46515C` |
| Accent | `#6F91A5` |
| Core | `#8CB1C3` |
| Guard | `#B77C7C` |

正常动画禁止不断切换 hue；只有真实 Guard/Error 才使用 Danger。

#### 10.2.8 TrueColor 能力与回退

新增主题能力层，默认 `Auto`：

```text
Auto
 ├─ 24-bit terminal -> TrueColorDark
 ├─ ANSI256 terminal -> Ansi256Dark
 └─ fallback         -> Ansi16
```

实现要求：

- TrueColor 使用 `Color::Rgb(r,g,b)`；
- 可利用 `COLORTERM=truecolor/24bit` 及终端能力信息判断；
- 检测失败必须安全回退，不影响交互功能；
- ANSI256/ANSI16 是视觉降级，不得改变语义；
- 允许通过环境变量显式覆盖，例如 `EDPCLI_TUI_THEME=dark|ansi256|ansi16`；
- 第一阶段只要求高质量 Dark theme，不为“完整 light theme”扩大范围。

### 10.3 Theme 架构

新增或收敛到：

```text
src/tui/theme.rs
```

建议核心结构：

```rust
struct Palette {
    background: Color,
    surface: Color,
    surface_active: Color,
    selection: Color,
    border: Color,
    border_focus: Color,
    text_primary: Color,
    text_secondary: Color,
    text_muted: Color,
    accent: Color,
    accent_soft: Color,
    success: Color,
    warning: Color,
    danger: Color,
    violet: Color,
}
```

业务页面只调用语义接口，例如：

```text
theme.text()
theme.secondary()
theme.muted()
theme.selection()
theme.panel()
theme.focused_panel()
theme.input()
theme.input_focused()
theme.success()
theme.warning()
theme.danger()
theme.partition(kind)
theme.animation(kind)
```

硬规则：

- `src/tui/theme.rs` 是 TUI 颜色单一事实源；
- workspace render 禁止新增 `Color::Cyan/Red/Green/Magenta/Yellow/Light*` 等直接业务颜色；
- 已有 `ThemeToken` 可以迁移/扩展，但不能形成第二套 palette；
- Inspect、Provision、Backup、Devices、动画全部复用同一 Theme。

### 10.4 Vim 风格统一输入模型

#### 10.4.1 输入模式

TUI 至少明确以下输入状态：

```text
Normal
Insert
Search
Command
Confirm
```

语义：

- Normal：导航、打开、选择、执行动作；
- Insert：编辑表单字段；
- Search：输入搜索条件；
- Command：输入 `:` 命令；
- Confirm：确认/取消当前动作。

**Insert/Search/Command 中，字母必须首先作为文本输入，不允许 Normal 快捷键抢占。**

`Esc` 始终只退出当前最内层 mode/modal，不跨层级跳跃。

#### 10.4.2 全局导航基础

所有列表、树和可滚动选择区域统一：

| 动作 | 主键 | 辅助键 |
| --- | --- | --- |
| 下一项 | `j` | `↓` |
| 上一项 | `k` | `↑` |
| 左/折叠/父级 | `h` | `←`（仅非文本/非 Hex） |
| 右/展开/子级 | `l` | `→`（仅非文本/非 Hex） |
| 顶部 | `gg` | `Home` 可作为 alias |
| 底部 | `G` | `End` 可作为 alias |
| 半页上 | `Ctrl-u` | - |
| 半页下 | `Ctrl-d` | - |
| 打开/默认动作 | `Enter` / `o` | - |
| 返回 | `Esc` / `q` | - |
| 帮助 | `?` | - |
| 命令 | `:` | - |

`h/l` **不得再用于切换 Workspace**。

#### 10.4.3 顶层标签与 Panel

当前顶层 UI **只有两个真实标签：Devices 与 Backups**。Provision 是从设备页进入的功能流程，Inspect 是从设备/备份进入的检查界面，都不是顶层标签。

| 动作 | 键 |
| --- | --- |
| 下一个顶层标签 | `Tab` |
| 上一个顶层标签 | `Shift+Tab` |
| Devices ↔ Backups | `Tab / Shift+Tab` 循环 |

页面内部 Panel 继续使用 Vim window 类比：

| 动作 | 键 |
| --- | --- |
| 左 Panel | `Ctrl-w h` |
| 下 Panel | `Ctrl-w j` |
| 上 Panel | `Ctrl-w k` |
| 右 Panel | `Ctrl-w l` |
| 下一 Panel | `Ctrl-w w` |
| 上一 Panel | `Ctrl-w W` |

硬规则：`Tab/Shift+Tab` 不再承担 Panel focus；Panel 只由 `Ctrl-w*` 管理。`gt/gT`、`gd/gb/gp/gi` 全部删除，不保留第二套入口。

#### 10.4.4 `g` 仅保留必要的导航前缀

`g` 不允许作为按一次即执行的动作，也不再承载页面/功能跳转。只保留：

```text
gg      top
gl      Inspect: go to LBA/absolute byte offset
```

`gt/gT/gd/gb/gp/gi` 均为无效组合。无效或超时 prefix 应取消 pending 状态，不触发其它动作。

#### 10.4.5 搜索

所有 Workspace 统一：

```text
/       进入 Search
n       下一个匹配
N       上一个匹配
Enter   确认搜索
Esc     退出搜索输入
```

进入 Search 后按 Insert-like 文本输入处理。

#### 10.4.6 表单与 Insert mode

Provision 等表单在 Normal mode：

```text
j/k       上下字段
h/l       枚举/选项左移右移
Space     toggle
i         编辑当前文本/数字字段
Enter     生成只读计划
p         生成/预览计划
f         填满剩余容量
Esc/q     返回
```

进入 Insert：

```text
-- INSERT --
←/→        文本光标
Home/End   首尾
Backspace
Delete
Enter      完成编辑并回 Normal
Esc        完成编辑并回 Normal
```

硬规则：

- Insert 中 `h/j/k/l/g/d/r/f` 等都是普通字符；
- 数字字段继续由 `ProvisionInputPolicy` 拒绝非法字符；
- 输入框内 `←/→` 永远只移动文本光标，不切 Workspace/Panel；
- 不再通过“当前字段是否 editable”决定左右箭头是否突然切页面。

#### 10.4.7 通用动作语义

| 动作 | 键 | 规则 |
| --- | --- | --- |
| Refresh | `r` | 所有 workspace 一致 |
| Select | `Space` | 多选/toggle |
| Delete | `d` | 当前或 selected；必须进入确认 |
| Yank | `y` | 当前语义值/byte |
| Yank raw/full | `Y` | 更完整表示 |
| Help | `?` | 当前上下文完整帮助 |
| Command | `:` | 低频动作入口 |

禁止某页面私自把 `r` 改成 view mode、把 `d` 改成 decode 等其它含义。

### 10.5 各 Workspace 键位

#### 10.5.1 Devices

```text
Tab/Shift-Tab  设备 / 备份标签切换
j/k            移动
gg/G           首尾
p / Enter      进入 Provision
i              进入 Inspect
a              新建备份
r              重新扫描
/ n N          搜索
?              帮助
q/Esc          返回/退出
```

#### 10.5.2 Backups

```text
Tab/Shift-Tab  设备 / 备份标签切换
j/k            移动
gg/G           首尾
Space          选中/取消
i / Enter      进入 Inspect
a              创建备份，类型在 modal 中选择 Metadata/Deep
v              Verify
R              进入 Restore 安全向导
d              删除当前或已选
r              刷新 catalog
/ n N          搜索
?              帮助
q/Esc          返回
```

删除单条/批量不再使用 `D/X` 两套按键；`d` 根据 selection 自动决定对象，并进入统一确认。

#### 10.5.3 Provision

Normal：

```text
j/k       字段移动
gg/G      第一/最后字段
h/l       修改枚举/选项
Space     toggle
i         编辑
f         fill remaining capacity
a         添加 Plain partition（允许时）
d         删除当前 Plain partition（允许时）
p         plan/preview
w         write
e         export
?         帮助
Esc/q     返回
```

`w` 只触发进入既有写盘安全流程，不能绕过 whole-disk confirmation、backup、lock、identity、transaction/readback/rollback。

#### 10.5.4 Inspect Tree

```text
j/k       上下节点
h         折叠；已折叠时去 parent
l         展开；已展开时可进入 child
o         toggle alias
Enter     打开/查看
gg/G      首尾
Ctrl-u/d  半页
/ n N     搜索
gl        Jump LBA / absolute byte offset
Ctrl-w*   Panel focus
?         帮助
Esc/q     返回
```

树的主折叠/展开语义由 `h/l` 承担，`o` 仅作为兼容/可发现 alias，不再是唯一主键。

#### 10.5.5 Sector Inspector / Hex

```text
h/l       byte -1 / +1
j/k       byte -16 / +16
0         当前行首
$         当前行尾
gg        sector 首 byte
G         sector 末 byte
PageUp    上一 sector
PageDown  下一 sector
Ctrl-u/d  详情/viewport 半页滚动
Space     field/bit 展示 toggle
v         Raw / Decode / Mixed 循环
y         yank 当前 byte/field
Y         yank 当前 field raw / sector full raw
/ n N     搜索
Esc/q     回 Tree
```

当前 `r/d/m` 切 Raw/Decode/Mixed 的局部映射废止，避免与全局 refresh/delete 冲突，由 `v` 统一循环 view mode。

### 10.6 Confirm 统一规则

普通确认 modal：

```text
y       confirm
n       cancel
Esc     cancel
```

但真实 destructive disk write 的确认强度继续由现有安全模型决定；不得为了 Vim 风格把高风险整盘确认简化成单个 `y`。

### 10.7 Command Palette

`:` 作为低频动作入口，第一阶段至少支持可发现的 command list，不要求一次实现完整 Vim Ex。

建议逐步支持：

```text
:q
:quit
:devices
:backups
:provision
:inspect
:refresh
:help
```

低频的 prune、restore、export 等功能优先进入 command palette 或上下文 modal，不为了每个动作占用顶层单字母。

### 10.8 Keymap 架构

新增：

```text
src/tui/keymap.rs
```

键盘事件统一转换为 UI-neutral action：

```rust
enum TuiAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Top,
    Bottom,
    HalfPageUp,
    HalfPageDown,
    Open,
    Back,
    Edit,
    Select,
    Search,
    SearchNext,
    SearchPrevious,
    WorkspaceNext,
    WorkspacePrevious,
    Plan,
    Insert,
    Restore,
    PanelLeft,
    PanelDown,
    PanelUp,
    PanelRight,
    PanelNext,
    PanelPrevious,
    Refresh,
    Delete,
    Yank,
    Help,
    Command,
}
```

数据流：

```text
KeyEvent
   ↓
KeyMap(mode + pending prefix)
   ↓
TuiAction
   ↓
Workspace handler
```

硬规则：

- `src/tui/event.rs` / `src/tui/mod.rs` 不再作为大量业务字母键的散落事实源；
- workspace handler 消费 `TuiAction`，只处理本 workspace 是否支持该 action；
- Insert/Search/Command 优先消费文本；
- `g` 与 `Ctrl-w` 是有状态 prefix，由 keymap 统一管理；其中 `g` 只接受 `gg/gl`，`Ctrl-w` 只管理 Panel；
- 新快捷键必须先登记 keymap + help registry + tests，不允许页面私加；
- Help/footer 从同一 key binding metadata 生成或校验，避免文案与行为漂移。

### 10.9 底部帮助栏

底部不展示整张键表，只显示当前上下文 4～6 个高频动作。

普通列表：

```text
j/k Move   Enter Open   / Search   r Refresh   ? Help
```

Inspect Tree：

```text
j/k Move   h/l Fold   Enter Open   / Search   gl Goto   ? Help
```

Insert：

```text
-- INSERT --   Esc Done   ←/→ Cursor
```

完整帮助统一由 `?` 打开，内容按当前 mode/workspace 分组显示。

### 10.10 实施阶段

#### Phase T0：基线与冲突清单

- 测试先行锁住现有 TUI 业务行为；
- 全仓统计 `src/tui/**` 的直接 `Color::*`；
- 全仓统计 `KeyCode::Char`、Tab、Left/Right、PageUp/PageDown；
- 生成“现有键 → 当前语义 → 新键”迁移表；
- 明确历史 shortcut 测试哪些需要更新，不能通过删除测试逃避冲突。

#### Phase T1：Theme 基础层

- 新增/重构 `theme.rs`；
- 实现 TrueColorDark + ANSI256 + ANSI16 fallback；
- 建立 Palette + semantic style；
- 迁移全局 Selection/Focus/Input/Panel/Tabs；
- 增加静态门禁，业务 workspace 不得新增高饱和直接 `Color::*`。

#### Phase T2：视觉 Workspace 收口

- Devices；
- Backups；
- Provision；
- Inspect；
- animation；
- Provision capacity bar；
- 统一 footer/help/modal。

每迁移一个 workspace 都要有 buffer/snapshot 或等价渲染回归测试。

#### Phase T3：Keymap / Mode 架构

- 新增 `keymap.rs`；
- `Normal/Insert/Search/Command/Confirm`；
- `g` prefix；
- `Ctrl-w` prefix；
- 全局动作 enum；
- help registry 同源；
- 先写键位契约测试再迁移 workspace。

#### Phase T4：Workspace Vim 迁移

依次迁移：

1. Devices/Backups；
2. Provision 表单与 Insert；
3. Inspect Tree/Panel；
4. Sector Inspector/Hex；
5. Search/Command/Confirm。

每个阶段删除对应旧分支，不长期保留“双键盘状态机”。

#### Phase T5：冲突清理与 UX 收口

- 删除旧 `g=Jump` 单键；
- 删除 `r/d/m=Raw/Decode/Mixed`；
- 删除 `h/l/Left/Right=Workspace`；
- 删除 Backup `D/X` 双删除；
- 删除 `i/I` 作为 Inspect 打开动作，`i` 回归 Insert；
- Tab/Shift+Tab 只切顶层 Devices/Backups 标签；Panel 仅保留 `Ctrl-w*`；
- 更新 README/USAGE/Help/本文件；
- 审计所有 footer 文案与实际 keymap 一致。

### 10.10.1 实施状态（2026-09-25）

第 10 章已全部完成，T0～T5 与 10.11/10.12 验收项均已收口：

- T0：完成现状盘点、旧键冲突清单和迁移基线；
- T1：统一主题层已落地，支持 24-bit TrueColor、ANSI256 与 ANSI16 自动回退；
- T2：Devices、Backups、Provision、Inspect、动画、容量条、footer/help/modal 已使用统一视觉语义；
- T3：集中式 `keymap.rs`、Normal/Insert/Search/Command/Confirm、`g` prefix、`Ctrl-w` prefix 与帮助元数据已经建立；
- T4：Devices/Backups、Provision Insert、Inspect Tree/Panel、Sector Inspector/Hex、Search/Command 以及备份批量删除、清理、单条删除、恢复/写盘确认弹窗均通过统一 KeyMapper 分发；事件循环不再直接解析业务字符输入；
- T5：旧 `g=Jump`、`r/d/m` view mode、`h/l/Left/Right=Workspace`、Backup `D/X` 双状态机已经移除；旧平铺 Inspect 的 state、worker、renderer 和专用滚动测试均已删除，所有入口统一进入全盘结构树。最终交互进一步收敛为：`Tab/Shift-Tab` 只切 Devices/Backups 两个顶层标签，Panel 只由 `Ctrl-w*` 管理，删除 `gt/gT/gd/gb/gp/gi`，设备/备份 Normal 模式使用 `p/i/a/R` 单键执行对应功能；README、USAGE、Help、footer 与第 9 章仍具现行含义的快捷键示例已同步。

2026-09-25 本轮回归补充后的专项验收结果：`tui_suite` 148/148；`provision_suite` 178/178；`inspect_suite` 55/55；`cargo fmt --all -- --check` 与 `git diff --check` 通过。此前 macOS Plain Virtual-HIL 1/1 的 detach/reattach、exFAT 挂载与文件读回结论继续有效。本轮新增门禁覆盖：Form Enter 生成计划、Normal/Insert 可视区分与输入框冗余宽度、逐分区 FileKeyCRC（含 mode2 兼容保留区后的槽位索引）、用户格式化选择不被 Rebuild 覆盖、单一全盘 Inspect 入口、旧 flat Inspect 运行时删除、Sector Inspector 完整 Vim 导航及 `n/N` 循环搜索，以及最终快捷键模型：`Tab/Shift-Tab` 切顶层标签、`Ctrl-w*` 切 Panel、`p/i/a/R` 仅在无输入的对应页面执行单键功能、输入模式优先消费文本。正式 fast gate 4 suites / 6 artifacts、0 failures，正式 full gate 8 suites / 10 artifacts / doctest、0 failures。

重构未修改 LBA0～12/LCE 协议语义，也未降低任何真实写盘安全门槛：系统盘保护、USB 整盘确认、写前备份、卸载/锁卷、reopen 身份复核、atomic write、readback、rollback 均保持原有边界；真实制盘仍必须精确输入大写 `YES`。

### 10.11 回归门禁

至少增加以下自动门禁：

1. TrueColor palette 的 RGB 值稳定；
2. ANSI256/ANSI16 fallback 可创建且不 panic；
3. workspace render 不直接新增高饱和 ANSI 业务色；
4. Selection 不再使用 Black-on-Cyan；
5. selected 背景不铺到 row padding；
6. focus panel 与 non-focus panel 使用不同语义 token；
7. Provision bar 六种区域使用 Theme partition token；
8. animation 不再私有硬编码 Green/Yellow/Red/Cyan/Magenta；
9. Normal 下 `j/k/h/l` 语义统一；
10. Insert 下 `h/j/k/l/g/d/r/f` 被作为文本字符；
11. 输入框 Left/Right 只移动光标；
12. `Tab/Shift+Tab` 只在 Devices / Backups 两个顶层标签间切换；
13. `p/i/a/R` 在对应无输入页面直接执行 Provision / Inspect / Backup / Restore 功能；
14. 单按 `g` 不触发 Jump；
15. `gl` 才进入 Inspect Jump，`gt/gT/gd/gb/gp/gi` 均无效；
16. `Ctrl-w h/j/k/l/w/W` 只改变 Panel focus；
17. Tab/Shift+Tab 不再改变 Panel focus；
18. `gg/G/Ctrl-u/Ctrl-d` 在列表/树语义一致；
19. `/ n N` 在支持搜索的 workspace 一致；
20. `r` 不再被页面重定义为 view mode；
21. `d` 删除必须进入确认；
22. Sector Inspector `v` 正确循环 Raw/Decode/Mixed；
23. `?` 展示的绑定与实际 keymap 同源或有契约测试；
24. 40×10、60×18、80×24、120×36 典型终端尺寸不出现布局回退；
25. 窄屏切 panel 不丢 selection/cursor；
26. Theme/keymap 重构不改变协议 parse、ProvisionRequest、write transaction、安全 guard。

### 10.12 完成标准

只有同时满足以下条件才算 TUI 视觉与快捷键重构完成：

1. 默认现代终端使用低饱和 24-bit TrueColor；
2. 不支持 TrueColor 时自动回退且功能完整；
3. 全 TUI 颜色来自统一 Theme；
4. Cyan/Magenta/Green/Yellow/Red 不再作为 workspace 私有高饱和装饰色；
5. Selection、Focus、Input、Tabs、Modal、Status 视觉一致；
6. Devices/Backups/Provision/Inspect/animation 使用同一设计语言；
7. Normal/Insert/Search/Command/Confirm 模式明确；
8. `Tab/Shift+Tab` 与 `Ctrl-w*` 分别统一顶层标签/Panel；
9. `gt/gT/gd/gb/gp/gi` 已删除，`h/l` 不再切顶层标签；
10. 输入框方向键永不切 Tab/Panel/Workspace；
11. `g` 统一为 prefix，Inspect Jump 迁移为 `gl`；
12. 搜索、列表导航、树导航、删除、刷新、帮助语义全局一致；
13. Sector Inspector 不再用 `r/d/m` 占用全局动作键；
14. workspace 内散落的业务 `KeyCode::Char` 显著收敛到 keymap；
15. Footer/Help 与真实绑定同源；
16. 所有新增专项测试通过；
17. `cargo fmt --all -- --check`、`git diff --check`、正式 fast/full gate 全绿；
18. 如触及 Provision/Inspect 行为边界，Virtual-HIL/只读验收按影响范围补跑；
19. 不改变 LBA0～12/LCE 协议语义；
20. 不降低任何真实写盘安全门槛。

最终产品定义：**edpcli TUI 使用克制的低饱和 TrueColor 视觉系统，以统一语义 Theme 提供现代终端观感；交互以 Vim 的 tab/window/navigation 思路组织 Workspace、Panel、列表、树、表单和 Hex，文本编辑通过明确 Insert mode 与导航彻底隔离。用户不需要记忆“每个页面自己的快捷键”，同一动作在整个应用中始终使用同一种交互语言。**

---

## 11. TUI 导航、Inspect 信息架构与统一表格系统重构计划（2026-09-25）

### 11.1 背景与覆盖关系

本章来自 2026-09-25 实机验收反馈，目标不是继续修补单个快捷键或单个渲染问题，而是一次性收口 **导航语义、Inspect 信息架构、磁盘布局表达、区间显示和表格布局**。本章实施完成后，凡与第 10 章快捷键/Panel 规则冲突的地方，**以本章为最终产品定义**；第 10 章保留为上一轮已完成重构的历史记录。

本轮必须解决以下十项用户验收问题：

1. `b` 是 Backup 快捷键，废止当前 `a=Backup`；
2. 设备页 Enter 进入 Provision，删除 `p=Provision`；Provision Form 也只用 Enter 生成计划，不再保留 `p` 预览/生成别名；
3. `q` 是全局退出意图，`Esc` 只负责返回上一级；
4. 所有 Sector/LBA 范围只显示一次，并统一为闭区间 `[start..end]`；
5. Inspect 选中 LBA12 等已知节点时，详情区立即显示该节点的结构化解析数据，而不是只显示 SHA-256 再要求 Enter；
6. Inspect 顶部直接显示当前磁盘布局、各区域比例和布局条，并与 Provision 共用同一视觉/计算模型；
7. Inspect 根结构树严格按物理 LBA 起点排序，Unknown 必须插回真实位置，不允许统一堆到末尾；
8. Inspect 内 `Tab/Shift-Tab` 必须有效，循环切换“结构树 / 节点概览 / 节点详情”子工作区；
9. 增加面包屑和明确的 `Esc 返回：<目标>` 提示，用户必须知道当前层级和返回目标；
10. 所有 Table 使用统一自适应列宽算法，并支持 `h/l` 横向滚动；长“部门”等字段不能挤掉更重要的“盘型”等右侧列。

硬约束继续保持：

- 不改变 LBA0～12/LCE 协议语义；
- 不猜测 Unknown 区域用途，Unknown 不得伪装成 free space；
- 不降低系统盘保护、USB 整盘确认、写前备份、卸载/锁卷、reopen 身份复核、atomic write、readback、rollback、精确大写 `YES` 等写盘安全门槛；
- Inspect 本轮仍是只读能力；
- 所有区间内部计算继续使用 half-open `[start, end_exclusive)`，闭区间只属于 UI formatter，禁止因为显示规则修改底层边界数学。

### 11.2 最终导航与快捷键契约

#### 11.2.1 顶层标签

顶层只有两个真实标签：

```text
Devices  |  Backups
```

Provision 和 Inspect 都是从当前对象进入的子工作区，不属于顶层 Tab。

- 顶层 Devices/Backups：`Tab` = 下一个标签，`Shift-Tab` = 上一个标签；两个标签循环切换；
- 进入 Inspect 后：`Tab/Shift-Tab` 不离开 Inspect，而是切 Inspect 内部子工作区，见 11.7；
- 进入其它 modal/Confirm/Input 时，由当前模式优先消费键盘，不能误触顶层切换。

#### 11.2.2 单键动作

无文本输入的 Normal 页面统一遵循“一个常用动作一个键”：

| 场景 | 键 | 行为 |
| --- | --- | --- |
| Devices | `Enter` | 对当前设备进入 Provision |
| Devices | `i` | Inspect 当前设备 |
| Devices | `b` | 对当前设备进入既有 Backup 创建流程 |
| Backups | `Enter` / `i` | Inspect 当前备份 |
| Backups | `b` | 进入既有 Backup 创建流程（按当前产品上下文选择目标盘，不另造旁路） |
| Backups | `v` | Verify 当前备份 |
| Backups | `R` | 进入 Restore 既有安全向导 |
| Backups | `d` | 删除当前/已选备份并进入确认 |
| 全局 | `q` | 发出退出整个 App 的全局退出意图 |
| 全局 | `Esc` | 返回上一级，不承担退出 App |

删除/禁止：

- `p=Provision`；
- `a=Backup`；
- `gt/gT/gd/gb/gp/gi`；
- 任何与以上动作重复的第二套用户可见快捷键。

Provision Form：

- Normal：`Enter` = 生成只读计划；
- `i` = 编辑当前可编辑字段；
- Insert：Enter/Esc 完成当前字段编辑并回 Normal；
- 不再保留 `p` 作为计划生成/预览别名。

`q` 的优先级必须高于 workspace 业务动作，不能再有页面把 `q` 重定义成“返回”。如果处于不可安全中断的真实写盘 critical section，仍必须服从既有 fail-closed transaction/shutdown 保护，不能通过 `q` 绕过写盘安全边界；其它状态下退出意图应立即结束 App。

### 11.3 NavigationStack 与面包屑

新增统一导航抽象，建议命名：

```text
NavigationStack
NavigationFrame
BreadcrumbModel
```

不要继续让 Esc 行为散落在各 workspace 的临时 `match` 中。

基本规则：

- `Esc` = `navigation.pop()`；
- 每个 frame 保存来源、selection、scroll、panel focus 等恢复所需状态；
- 从 Devices 进入 Inspect，Inspect 根的返回目标必须是原 Devices selection；
- 从 Backups 进入 Inspect，Inspect 根的返回目标必须是原 Backups selection；
- 从 LBA12 打开 Sector Inspector，Esc 返回 LBA12 所在 Inspect 状态，而不是重新加载/丢 selection；
- 从 Provision 子步骤 Esc 按现有业务层级逐层返回，不能直接退出 App。

Inspect 顶部至少显示两类信息：

```text
设备 > disk4 > Inspect > EDP 主协议区 > LBA12
Esc 返回：设备列表
```

注意：树上“当前选中节点路径”与“真实导航栈”不是同一个概念。仅用 `j/k` 移动选择时，不应为了每个节点都 push navigation frame；breadcrumb 可以显示 selected node path，但必须单独显示真实 `Esc 返回：...`，避免用户误以为 Esc 会回树父节点。

### 11.4 Sector/LBA 区间统一 formatter

新增一个单一事实源 formatter，例如：

```text
format_lba_closed_range(start, end_exclusive) -> "[start..end]"
```

UI 禁止自己拼 `LBAxxx..yyy`、`[x..y)` 或同时显示两套范围。

显示规范：

```text
LBA12 [12..12]
未知区域 [13..62]
分区[0] type1 [63..20479]
LCE [15725843..15725848]
```

内部数据仍保持：

```text
start
end_exclusive
length = end_exclusive - start
```

显示端唯一转换：

```text
end_inclusive = end_exclusive - 1
```

必须覆盖空区间/溢出防护，禁止 off-by-one 回归。

### 11.5 Inspect topology：按物理 LBA 顺序构建完整空间图

根节点下所有 extent 必须以 `start_lba ASC` 排序。Unknown 不能作为一个“类别”统一 append 到末尾，而必须作为已知 extent 的 complement 插入真实物理位置。

以当前实盘截图为例，根节点应按类似顺序展示：

```text
EDP 主协议区 [0..12]
未知区域 [13..62]
分区[0] type1 [63..20479]
分区[1] type2 [20480..13627391]
分区[2] type4 [13627392..15724543]
未知区域 [15724544..15725842]
LCE [15725843..15725848]
未知区域 [15725849..15726591]
盘尾区域 [15726592..15728639]
```

Topology builder 推荐流程：

1. 收集所有 canonical known extents；
2. 按 `start_lba` 排序；
3. 校验 overlap；
4. 计算 known extent 之间的 gap，并生成 Unknown extent；
5. 合并 known + Unknown，再按 start 输出；
6. 如存在尾部已知区域/盘尾证据，同样放回真实位置。

不变量测试至少包括：

- root extents 的 start LBA 单调递增；
- 不允许 overlap；
- gap 生成的 Unknown 边界正确；
- LCE 前后的 Unknown 不被错误合并跨过 LCE；
- UI formatter 显示为闭区间但内部仍是 half-open。

### 11.6 Inspect 顶部磁盘布局条

抽出 Provision 当前已经存在的布局视觉能力，形成共享抽象，建议：

```text
DiskLayoutModel
DiskLayoutSegment
DiskLayoutBar
```

Provision 和 Inspect 禁止各自维护两套比例/颜色/区间计算。

Inspect 顶部显示：

- 总容量/总 sector；
- 设备状态/盘型；
- 一条连续磁盘布局条；
- 各 segment 的名称、范围、sector 数、百分比；
- 颜色继续来自统一 Theme 的 partition tokens；
- 极小区域（例如 LCE 6 sectors）即使比例无法在 bar 中肉眼分辨，也必须在图例中保留精确数字。

示意：

```text
磁盘布局  8.05 GiB / 15728640 sectors
[EDP][未知][type1][type2................][type4......][未知][LCE][未知][Tail]

type1  [63..20479]              20417 sectors   0.13%
type2  [20480..13627391]        ...             ...
type4  [13627392..15724543]     ...             ...
LCE    [15725843..15725848]     6 sectors       <0.01%
```

Unknown 只能标为“未知区域/Unknown”，不能推断为 free/unused。

### 11.7 Inspect 三个子工作区与 Tab 行为

Inspect 保留三个并列子工作区：

```text
结构树  |  节点概览  |  节点详情
```

最终键位：

- Inspect 内 `Tab`：结构树 → 节点概览 → 节点详情 → 结构树；
- `Shift-Tab`：反向循环；
- 宽屏：三栏可同时存在，Tab 只改变 focus/highlight；
- 窄屏：允许只渲染当前 focus 子工作区，因此 Tab 变成实际视图切换；
- `Ctrl-w h/j/k/l/w/W` 可继续作为高级 Panel focus alias，但不能让 Tab 在 Inspect 中失效；
- 退出 Inspect 只能用 Esc 返回来源或 q 全局退出，Tab 不能跳回 Devices/Backups。

这条规则覆盖第 10 章“Tab 只切顶层、Panel 只用 Ctrl-w”的旧规则：**顶层页面 Tab 切顶层标签；Inspect 内 Tab 切 Inspect 子工作区，按当前 navigation context 解释。**

### 11.8 Inspect 选中节点即显示结构化解析

当前“选中 LBA12 → 右侧只显示 RAW SHA-256 → Enter 才进一步看”的 UX 必须改掉。

新规则：树 selection 改变时，中间/右侧立即消费 application/canonical decoder 已经产生的结构化数据，禁止 TUI 重写协议 parser。

例如选中 `LBA12 [12..12]` 后：

**节点概览**至少显示：

```text
名称：LBA12
类型：Sector
范围：[12..12]
大小：512 B
状态：identified
Decoder：Protocol / LBA12
```

**节点详情**直接显示 LBA12 的已解析 group/field/entry，例如 Header、partition entries、PartionType、start LBA、sector count、CRC/校验状态等已有 canonical 字段。字段具体名称和语义必须来自现有 decoder，不能在 TUI 猜写。

相同规则覆盖 LBA0～12、LCE、分区 boot sector、已知 Field/Structure 节点。

Enter 的职责重新定义为：

- Sector 节点：打开 Sector Inspector/Hex；
- Field/Group：根据现有结构进入更细视图或定位 Hex；
- 普通节点：展开/进入；

**Enter 不再是“查看正常解析数据”的前置条件。**

Sector Inspector 继续承担底层分析：Raw/Decode/Mixed、byte cursor、field range、`0/$`、`gg/G`、`Ctrl-u/d`、PageUp/PageDown、`v`、`Space/o`、`gl` 等。

### 11.9 统一 AdaptiveTableLayout

新增全局表格布局单一事实源，建议：

```text
AdaptiveTableLayout
AdaptiveColumnSpec
TableViewport
HorizontalScrollState
```

所有普通表格统一使用，不再每张表手写固定 `Constraint::Length(...)`。

每列至少声明：

```text
min_width
preferred_width
max_width
priority
weight
truncate_policy
```

算法要求：

1. 使用 terminal display width（CJK 宽字符必须按 cell width），禁止用 UTF-8 byte length；
2. 先满足所有可见列 `min_width`；
3. 剩余空间优先补到 `preferred_width`；
4. 再按 `weight` 向高弹性列分配；
5. 宽度不足时按 `priority` 压缩低优先级列；
6. 仍放不下时进入 horizontal viewport，而不是让最右侧列直接消失；
7. 可选固定第一标识列（如设备名/备份名），其余列水平滚动；
8. 单元格 truncate 必须有一致省略策略，不能破坏边框和列对齐。

设备表当前验收重点：

- “部门”允许弹性增长/截断；
- “盘型”必须有更高显示优先级，不能因部门过长完全看不到；
- 容量、总线、VID:PID 等稳定字段按合理 min/preferred 控制；
- `ven_prod`、onlyid、姓名、部门等按真实数据宽度参与计算。

统一迁移范围至少包括：

- Devices table；
- Backups table；
- Inspect field/partition entry table；
- Provision review/plan 中的真正 Table；
- 后续新增表格必须复用同一算法。

### 11.10 Table 的 h/l 横向滚动

普通 Table context：

- `h` = 向左滚；
- `l` = 向右滚；
- 滚动单位优先按“下一列边界/可读 viewport step”，不要每次只挪一个字符；
- footer 显示当前位置，例如 `h/l 横向滚动 · 3/9 列`；
- 到最左/最右必须 clamp，不 wrap。

上下文冲突按 widget role 解决：

- Tree：`h/l` = 折叠/展开；
- Table：`h/l` = 水平滚动；
- Input/Insert：`h/l` = 文本字符，不触发导航；
- Sector Inspector：继续使用其已有 byte/navigation 语义。

KeyMapper 需要能够结合当前 widget/context role 分发，不允许重新回到“所有页面同一个字符硬编码成同一业务动作”的做法。

### 11.11 推荐 Inspect 最终布局

宽屏目标：

```text
┌ Inspect · disk4 · 8.05 GiB · 模式0 ───────────────────────────────┐
│ 设备 > disk4 > Inspect > EDP 主协议区 > LBA12    Esc 返回：设备列表 │
├───────────────────────────────────────────────────────────────────┤
│ 磁盘布局                                                          │
│ [EDP][未知][type1][type2................][type4......][未知][LCE]… │
│ type1 0.13% · type2 ... · type4 ... · LCE 6 sectors ...          │
├结构树────────────────┬节点概览──────────────┬节点详情──────────────┤
│ EDP [0..12]          │ LBA12                │ LBA12 Header         │
│ ├ LBA0 [0..0]        │ 范围 [12..12]        │ Partition entries    │
│ ...                  │ Decoder: Protocol    │ Entry 0 ...          │
│ > LBA12 [12..12]     │ 512 B                │ PartionType ...      │
│ 未知 [13..62]        │ identified           │ Start LBA ...        │
│ type1 [...]          │                      │ Sector count ...     │
│ type2 [...]          │                      │ ...                  │
│ type4 [...]          │                      │                      │
│ 未知 [...]           │                      │                      │
│ LCE [...]            │                      │                      │
└───────────────────────────────────────────────────────────────────┘
Tab/Shift-Tab 子工作区 · Enter Sector Inspector · Esc 返回 · q 退出
```

窄屏目标：

- 顶部 breadcrumb + Esc target 不允许消失；
- layout bar 可降级为紧凑条 + 一行摘要；
- 三个 Inspect 子工作区一次只显示当前一个，由 Tab/Shift-Tab 切换；
- selection、scroll、cursor 在切换后必须保持。

### 11.12 分阶段实施

#### Phase U0：契约与失败测试

在改业务代码前先建立 red tests：

- `b` 是 Backup，`a` 不再触发 Backup；
- Devices `Enter` = Provision，`p` 不再触发 Provision；
- `q` 是全局退出意图，Esc 只返回上一级；
- range formatter 只输出一个闭区间；
- Inspect Tab/Shift-Tab 子工作区循环；
- topology root 物理顺序；
- LBA12 selection 自动出现结构化详情；
- AdaptiveTableLayout 的 CJK/窄屏/优先级；
- Table `h/l` 横滚。

#### Phase U1：Keymap + NavigationStack

- 收口 `b/Enter/i/q/Esc`；
- 删除 `p=Provision`、`a=Backup`；
- 建立 NavigationStack/BreadcrumbModel；
- 恢复来源 selection/scroll；
- 更新 footer/help 同源门禁。

#### Phase U2：Range formatter + topology ordering

- 建立统一 closed-range formatter；
- 删除树上重复区间；
- known extent 排序、gap→Unknown、全盘物理顺序；
- 加 overlap/gap/off-by-one 门禁。

#### Phase U3：Inspect immediate details

- tree selection 驱动 overview/detail；
- LBA0～12/LCE/partition 已知 decoder 结果直接可见；
- Enter 下沉到 Sector Inspector；
- 禁止 TUI 复制 protocol parser。

#### Phase U4：共享 DiskLayoutModel

- 抽取 Provision 现有布局算法/renderer；
- Inspect 复用；
- 增加精确 sector/占比图例；
- 小 segment 保留文本图例。

#### Phase U5：Inspect sub-workspace + breadcrumb UX

- Tab/Shift-Tab 三工作区循环；
- 宽屏 focus、窄屏单 pane；
- breadcrumb/`Esc 返回：...`；
- Ctrl-w 作为高级 alias，不再是唯一入口。

#### Phase U6：AdaptiveTableLayout + horizontal viewport

- 公共列规格和宽度算法；
- Devices/Backups/Inspect/Provision 表格迁移；
- `h/l` 横滚；
- CJK/超长部门/盘型可见性专项测试。

#### Phase U7：清理、文档与验收

- 删除所有旧 `p=Provision`、`a=Backup`、重复 range、无效 Tab/Panel 文案和死代码；
- 更新 README、USAGE、Help、footer、本文件；
- `cargo fmt --all -- --check`、`git diff --check`；
- `tui_suite` / `inspect_suite` / `provision_suite`；
- `scripts/test-fast.sh`；
- `python3 scripts/test-full.py --profile full`；
- Inspect 只读真实盘验收；本轮若未改写盘路径，不要求真实写盘 HIL。

### 11.13 必须新增的回归门禁

至少覆盖：

1. Devices：Enter Provision、`i` Inspect、`b` Backup；
2. `p` 在 Devices 不再触发 Provision；
3. `a` 不再触发 Backup；
4. Backups：Enter/i Inspect、b Backup、v Verify、R Restore、d Delete；
5. q 在非 critical 状态触发全局 ExitRequested，Esc 不退出 App；
6. critical write 状态下 q 不绕过 transaction safety；
7. `[start..end_exclusive)` 内部转换到 `[start..end]` UI 无 off-by-one；
8. UI 不出现 `LBAx..y [x..z)` 双份范围；
9. topology 根节点 start LBA 单调递增；
10. known extents 不 overlap；
11. Unknown gap 边界正确且位于真实位置；
12. 选择 LBA12 后无需 Enter 即可看到 canonical structured fields；
13. Enter LBA12 才进入 Sector Inspector；
14. Inspect Tab/Shift-Tab 按 Tree→Overview→Detail 循环，且不退出 Inspect；
15. 窄屏切子工作区不丢 selection/scroll/cursor；
16. breadcrumb 与真实 NavigationStack/返回目标一致；
17. Devices 长部门名时盘型仍可访问/显示；
18. display width 正确处理中文、ASCII、emoji；
19. 所有统一 Table 在窄屏可 h/l 横滚；
20. Tree h/l 仍折叠展开，不被 Table 横滚规则污染；
21. Insert 中 h/l 是文本字符；
22. 40×10、60×18、80×24、120×36、超宽终端无 panic/越界；
23. Provision/Inspect 共用 DiskLayoutModel 后 segment range/比例一致；
24. Help/footer 的快捷键与真实 KeyMapper 有契约测试；
25. 不改变 LBA0～12/LCE parser golden tests；
26. 不降低任何写盘安全 guard。

### 11.14 完成标准

只有以下条件同时满足，本章才允许标记 COMPLETE：

1. 用户从任意主要页面都能明确回答“我在哪里、Tab 会去哪、Esc 会去哪、q 做什么”；
2. Devices/Backups 常用动作均为最终单键模型，无重复别名；
3. Inspect 根树按物理 LBA 顺序完整表达整盘空间；
4. Sector/LBA 范围只显示一次且统一闭区间；
5. 选中 LBA12 等已知节点立即显示人类可读的 canonical 解析数据；
6. Provision/Inspect 使用同一磁盘布局组件；
7. Inspect Tab/Shift-Tab 可用且语义稳定；
8. breadcrumb 与 Esc 返回行为严格一致；
9. 所有表格使用统一自适应算法，长字段不会永久挤掉关键列；
10. 表格 h/l 横滚、树 h/l 折叠、输入 h/l 文本三种 context 不冲突；
11. 所有现行 Help/footer/USAGE/PROVISIONING 与实际键位一致；
12. 专项、fast、full 全绿；
13. 真实盘 Inspect 只读验收通过；
14. 工作区 clean，HEAD==origin/main；
15. 本地安装版与 release 构建 SHA-256 一致。

### 11.15 实施状态（2026-09-25）

**U0～U7 实现已完成，但本章暂不标记 COMPLETE。**

- U0～U6 的导航、区间格式、物理顺序拓扑、选中即解析、共享磁盘布局、检查子工作区、面包屑、自适应表格和横向滚动均已落地；U7 的旧快捷键/死代码清理、README/USAGE/Help/footer 同步也已完成。
- 专项门禁：`tui_suite` **161/161**、`inspect_suite` **55/55**、`provision_suite` **178/178**；`cargo fmt --all -- --check` 与 `git diff --check` 通过。
- 正式快速门禁：4 suites / 6 artifacts，**0 failures，9.97s**；正式完整门禁：8 suites / 10 artifacts + doctest，**0 failures，31.99s**。
- 代码提交 `af9abd9` 已推送至 `origin/main`；提交后工作区 clean，`HEAD == origin/main`。基于 clean HEAD 构建并安装的 macOS arm64 release 为 `edpcli 2.4.0`，构建产物与 `~/.local/bin/edpcli` SHA-256 均为 `90e2435499ae93da2835a5a04d0f5eb283788359a28dfc19ed8619d519e57674`。
- 已检测到真实外接物理盘 `/dev/disk4`（8.1 GB）。全盘检查只读命令能够正确进入管理员权限请求边界；本轮通过 macOS `SecurityAgent` 发起原生授权后等待 300 秒未获用户确认并超时退出，因此**未执行裸盘读取，也未发生任何写盘**。
- 因此 11.14 第 13 项“真实盘全盘检查只读验收通过”当前仍为 **未验收**。在该项实际成功前，本章不得写成 COMPLETE；其余完成标准均已达到或已具自动门禁证据。

最终产品原则：**顶层标签简单、常用动作单键、q 退出/Esc 返回语义固定；Inspect 首屏就是可读的磁盘空间图和协议解析器，而不是一个必须继续钻取才能理解的数据树；所有表格和布局由共享基础设施统一计算，避免同类 UI 在不同页面重复漂移。**

## 12. 后续计划：五状态互转、独立密码域与 FileKey 保留策略（2026-09-26）

> 状态：**PLAN ONLY / 暂不实施**。本章用于后续独立开发阶段；当前 worktree 只固化设计，不修改生产代码、协议构造器、TUI 或写盘链。后续实现必须在当时最新 `main` 上重新核对本章与协议真相源，禁止直接把本章中的“建议类型名”当成已经实现的事实。

本章补全 4.8/4.9 中尚未展开的五状态互转密码语义。核心变化不是增加 20 套 source→target 特例，而是把“盘型布局”“区域语义”“密码知识”“FileKey 处理”“数据处置”拆成正交轴，由统一 planner 对每个语义区域独立决策。

### 12.1 不变量与术语

五种 source/target 状态继续固定为：

| 状态 | 含义 |
| --- | --- |
| `Plain` | 普通盘；不是 mode4，不生成 EDP key material |
| `Mode0` | 模式0 · 缺省三分区 |
| `Mode1` | 模式1 · 启动区和交换区二合一 |
| `Mode2` | 模式2 · 整盘加密 |
| `Mode3` | 模式3 · 内外网通用双分区 |

已闭环的物理/协议事实继续优先于本章任何产品设计：

- mode0：type1 Boot 明文、type2 Share 加密、type4 Encrypt 加密；
- mode1：type2 BootShareCombined 物理明文、type4 Encrypt 加密；不能因为 type2 的协议字段含 `NeedEncrypt=1` 就把 Combined 数据区机械按密文处理；
- mode2：type1 CompatibilityReserve 为 canonical 兼容保留结构、不创建用户文件系统，type4 Encrypt 加密；
- mode3：type1 Boot 明文、type2 Share 加密；
- type1 / type2 / type4 的协议角色、物理加密、文件系统和 key material 是独立维度，不允许只看 `PartionType` 或 `NeedEncrypt` 推导数据写法。

**密码属于 key domain，而不是属于整块 U 盘。** mode0 中 Share 与 Encrypt 至少是两个独立密码域；两者密码可以相同，也可以完全不同。后续任何代码都禁止用一个全局 `request.password` 同时代表“所有来源旧密码 + 所有目标新密码”。

对于 mode1 等“协议存在 key material、但物理数据区语义特殊”的区域，也必须按 canonical profile 独立建模，不能把“有密码/key record”和“数据区一定逐扇区加密”画等号。

### 12.2 密码知识必须逐域探测

每个来源 key domain 独立保存密码知识状态，建议模型：

```rust
enum SourcePasswordKnowledge {
    DefaultVerified,
    UserVerified,
    Unknown,
}
```

默认密码探测逐域执行。当前 v0x0206 默认密码仍为 `0000aaaa`，但“默认密码通过”必须是完整密码学验证，不是只比较一个 CRC：

```text
读取该域 canonical LBA12 key record
        ↓
UserKeyCRC == crc32("0000aaaa") ?
        ↓ yes
按当前已验证规则得到 effective password
        ↓
按 EncryptMode unwrap FileKey
        ↓
FileKeyCRC == crc32(raw FileKey) ?
        ↓ yes
DefaultVerified
```

任一步失败只能得到：

```text
Unknown / 当前密码不是已验证的默认密码
```

UI 不得显示“密码错误”，因为系统只证明了默认密码未通过，并不知道真正密码是什么。

LBA12 当前 key record 作为密码/FileKey 验证的 canonical 主路径；LBA7 legacy material 只能按已验证协议规则做一致性校验或原样保留，不能因为 LBA12 验证失败而猜测另一套密码算法。

### 12.3 来源密码与目标密码彻底分离

后续请求模型至少要能表达“一个区域的旧密码”和“同一区域制盘后的新密码”不同，建议方向：

```rust
struct KeyDomainPlan {
    role: KeyDomainRole,
    source_password: SourcePasswordKnowledge,
    target_password: TargetPasswordPolicy,
    disposition: RegionDisposition,
}
```

其中目标密码策略建议至少能表达：

```rust
enum TargetPasswordPolicy {
    PreserveOpaque,      // 不知道旧密码，密码/key material 全部透传
    ReuseVerified,       // 已验证旧密码，目标继续使用同一密码
    ReplaceVerified,     // 已验证旧密码，目标改成新密码
    InitializeNew,       // 放弃旧数据/新区域，创建新 FileKey + 新密码
}
```

具体 Secret 类型以后按项目现有 secret/zeroize 设施落地。本章只要求以下语义：

- `source_password` 只用于验证/解开来源 FileKey；
- `target_password` 只决定目标 key record 的认证包装；
- Share 与 Encrypt 分别持有自己的 source/target password；
- 一个域验证失败不得把另一个已经验证或可透传的域降级成 Rebuild；
- 密码明文不得写入日志、备份元数据、Review 文本或进度日志；只在必要内存生命周期内存在并尽快清零。

### 12.4 区域数据处置统一为六类

4.9 当前的 `PreserveExact / PreserveWithRewrap / Rebuild` 继续是已实现基线。后续五态互转 planner 建议提升到更明确的区域处置模型；名称可在实施时调整，但语义必须覆盖：

```text
PreserveOpaque
PreserveVerified
RewrapVerified
Migrate
Rebuild
Drop
```

#### 12.4.1 PreserveOpaque：不知道密码也能原样透传

适用于“用户不知道旧密码，但目标仍然可以保持同一份密文和同一份 key material”的情况。

必须同时满足：

1. 来源/目标语义角色兼容；
2. `PartionType` / canonical crypto profile 兼容；
3. start LBA 完全一致；
4. sector_count 完全一致；
5. 物理加密属性一致；
6. 文件系统解释一致；
7. 对应数据 extent 完全不进入 write-set；
8. 对应来源 LBA7/LBA12 key material 只允许**逐字段原值搬运到目标对应语义槽位**，禁止 unwrap、重算 FileKey、改密码或随机生成新 key。

因此：

```text
旧密码未知
+ extent/key profile 完全可保留
→ PreserveOpaque
→ 原 ciphertext 不动
→ 原 wrapped FileKey / CRC / EncryptMode 不动
→ 制盘后继续使用原密码
```

如果目标 slot/index 发生变化，只能把同一份已验证结构的 key material 搬到新的对应语义 entry；“搬槽位”不等于“重新生成”。

#### 12.4.2 PreserveVerified：验证后保持原密码

用户输入旧密码并通过完整 FileKeyCRC 验证后，如果几何和数据完全可保留：

```text
P_old → unwrap K_old → CRC PASS
→ 数据区不动
→ 目标继续使用 P_old
```

可以继续复用原 key record，或按 canonical writer 重新序列化等价 material；无论实现选哪种，都必须证明 raw FileKey 仍为 `K_old` 且 data extent 零写入。

#### 12.4.3 RewrapVerified：只改密码，不改 FileKey

这是“保留数据但修改密码”的唯一正确模型：

```text
P_old
  ↓ unwrap
K_old
  ↓ wrap with P_new
new wrapped FileKey
```

要求：

- 旧密码必须验证成功；
- raw FileKey 必须保持 `K_old`；
- ciphertext/data extent 完全不写；
- 只更新认证/封装相关 key material；
- 不得因为“目标密码变化”而生成 `K_new`。

#### 12.4.4 Migrate：语义/几何改变但用户要求保数据

`Migrate` 表示必须读取来源文件系统/数据并写入目标文件系统，不能伪装成 Preserve。

来源为加密域时：

```text
验证 P_source
→ unwrap K_source
→ 读取/解密来源
→ 建立目标区域
→ 使用目标域自己的 K_target / P_target
→ 迁移数据
```

第一阶段如果未实现文件级/分区级迁移，planner 必须把 `Migrate` 标记为“不支持，需用户改选 Rebuild/Drop”，而不是静默降级成 Preserve。

#### 12.4.5 Rebuild：新 FileKey 必须伴随完整初始化

`Rebuild` 只在用户明确放弃该区域原数据、或目标区域本来就是新增区域时使用：

```text
随机 K_new
→ 用 P_target wrap
→ 写入新 key material
→ 完整初始化/格式化目标文件系统
```

硬门禁：

> **任何 `K_new` 都必须有对应的完整目标文件系统初始化；禁止 `K_new + 旧 ciphertext`。**

这条门禁要在 prepare 阶段和 commit/write-set 校验阶段同时存在，不能只靠 TUI checkbox。

#### 12.4.6 Drop：来源区域明确不进入目标

来源区域在目标模式中没有语义映射且用户不迁移数据时，使用 `Drop`。UI/Review 必须明确列出“该来源区域不会保留”，避免把“没有目标 entry”误解成已经备份或已经迁移。

### 12.5 TUI：每个密码域独立显示、验证和决策

制盘表单在来源分析完成后，对每个 key domain 独立显示状态。mode0 示例：

```text
交换区
  来源密码  [0000aaaa              ]  ✓ 默认密码已验证
  数据处理  ● 保留原数据
  目标密码  [0000aaaa              ]

保密区
  来源密码  [                      ]  ⚠ 当前密码不是已验证的默认密码
            [验证]
  数据处理  ● 直接透传原加密区域
            ○ 输入旧密码后保留/改密码
            ○ 放弃并重新初始化
  目标密码  —  （透传时禁用）
```

规则：

1. 默认密码完整验证通过后，来源密码字段自动显示/填入默认密码，目标密码默认继承同一值，但用户可改；
2. 默认密码验证失败时只标注“非默认密码/尚未验证”；
3. 用户可输入该域旧密码并显式触发验证；
4. 用户也可在满足 `PreserveOpaque` 条件时直接选择“透传”；
5. 透传时目标密码输入必须禁用，因为系统不知道 raw FileKey，不能安全换密码；
6. 用户选择“重新初始化”时才允许直接设置全新的目标密码，并必须显示数据丢失警告；
7. Share/Encrypt UI 状态完全独立，不做自动同步；
8. Review 页逐域显示：来源状态、密码验证状态、数据处置、FileKey 动作、是否写 data extent、最终风险。

### 12.6 五状态互转总矩阵

本表描述**默认规划方向**，最终仍由逐区域 compatibility 判定决定；同一个 source→target 中不同区域可以同时出现 PreserveOpaque、Rewrap、Rebuild 等不同动作。

| Source ↓ / Target → | Plain | Mode0 | Mode1 | Mode2 | Mode3 |
| --- | --- | --- | --- | --- | --- |
| Plain | 精确布局可保留，否则普通重分区/重建 | 新建 Boot/Share/Encrypt；Share/Encrypt 独立新密码域 | 新建 Combined/Encrypt；按 canonical profile 建 key material | 新建 CompatibilityReserve + Encrypt | 新建 Boot/Share |
| Mode0 | 需导出/迁移可用数据，否则恢复普通盘重建 | Boot/Share/Encrypt 逐域 Preserve/Rewrap/Rebuild | Encrypt 优先精确保留；Boot+Share→Combined 需 Migrate/Rebuild | type4 Encrypt 仅在全兼容时可 Preserve；CompatibilityReserve 重建；其余 Drop/Migrate | Boot/Share 逐域兼容时 Preserve；Encrypt Drop/Migrate |
| Mode1 | 需导出/迁移可用数据，否则重建 Plain | Encrypt 优先精确保留；Combined→Boot+Share 需 Migrate/Rebuild | Combined/Encrypt 逐域 Preserve/Rewrap/Rebuild | type4 Encrypt 全兼容时可 Preserve；CompatibilityReserve 重建；Combined Drop/Migrate | Combined 与 mode3 Share 语义不同，需 Migrate/Rebuild；目标 Boot 新建；Encrypt Drop/Migrate |
| Mode2 | type4 数据需解密迁移到 Plain，否则重建 | type4 Encrypt 全兼容时可 Preserve；Boot/Share 新建；CompatibilityReserve 丢弃 | type4 Encrypt 全兼容时可 Preserve；Combined 新建；CompatibilityReserve 丢弃 | CompatibilityReserve 按 canonical 规则重建；type4 Encrypt 可 Preserve/Rewrap/Rebuild | type4 Encrypt→type2 Share 语义不同，需 Migrate/Rebuild；Boot 新建 |
| Mode3 | 明文/加密数据按区域迁移，否则重建 Plain | Boot/Share 全兼容时可 Preserve；Encrypt 新建 | Boot+Share→Combined 语义变化，需 Migrate/Rebuild；Encrypt 新建 | Share(type2)→Encrypt(type4) 语义不同，需 Migrate/Rebuild；CompatibilityReserve 重建 | Boot/Share 逐域 Preserve/Rewrap/Rebuild |

“可 Preserve”永远不是仅看 mode 组合，而必须经过 12.7 的逐区域判定。

### 12.7 逐区域 mapping 决策算法

后续实现禁止新增 20 个 `modeX_to_modeY()` writer。模式只负责生成 canonical source/target region model，统一 planner 再做 mapping：

```text
Source Disk
   ↓ decode
SourceRegions

Target Kind + Form
   ↓ canonical layout
TargetRegions

SourceRegions × TargetRegions
   ↓ semantic mapper
RegionMappingPlan
   ↓ compatibility + password knowledge
RegionDisposition
```

建议区域模型至少包含：

```rust
struct SourceRegion {
    role: RegionRole,
    partion_type: Option<u8>,
    extent: Extent,
    physical_crypto: PhysicalCryptoProfile,
    filesystem: FilesystemProfile,
    key_domain: Option<SourceKeyDomain>,
}

struct TargetRegion {
    role: RegionRole,
    partion_type: Option<u8>,
    extent: Extent,
    physical_crypto: PhysicalCryptoProfile,
    filesystem: FilesystemProfile,
    key_domain: Option<TargetKeyDomain>,
}
```

Preserve compatibility 按顺序 fail-closed：

```text
semantic role compatible?
  ↓
PartionType / canonical target profile compatible?
  ↓
start_lba exact?
  ↓
sector_count exact?
  ↓
physical crypto exact?
  ↓
filesystem interpretation exact?
  ↓
key profile/wrap mode compatible?
  ↓
YES → 可以进入 Preserve family
NO  → Migrate / Rebuild / Drop
```

其中“密码未知”不在前六项中自动否决 Preserve；它只决定 Preserve family 里能否做 `RewrapVerified`。如果全部物理条件满足，`Unknown` 可以选择 `PreserveOpaque`。

### 12.8 各模式重点转换规则

#### 12.8.1 Mode0 ↔ Mode1

0→1：

- type4 Encrypt 若语义/几何/crypto/filesystem 全兼容，允许 `PreserveOpaque`，因此保密区旧密码未知也不妨碍转换；
- mode0 Boot + Share 与 mode1 Combined 语义不同，不能直接 Preserve；
- 当前未实现 Migrate 时，Combined 只能 Rebuild；
- Share 密码是否已知不能影响 Encrypt 的独立透传。

1→0：

- type4 Encrypt 同样优先精确保留；
- Combined 不能直接当成 mode0 Share；Boot/Share 需要 Migrate/Rebuild；
- Encrypt 旧密码未知时仍可在 exact preserve 条件下透传。

#### 12.8.2 Mode0/Mode1 ↔ Mode2

type4 Encrypt 在 mode0、mode1、mode2 之间只有在 4.8 已定义的全兼容条件全部满足时才是 Preserve candidate，不能因为都是 type4 就直接保留。

mode2 CompatibilityReserve 始终按 canonical 规则重建，不承载用户密码/用户文件数据。

因此可以出现：

```text
来源 mode0:
  Share      → Drop/Rebuild/Migrate
  Encrypt    → PreserveOpaque
目标 mode2:
  Reserve    → Rebuild canonical
  Encrypt    → 原 extent + 原 key material
```

这种情况下只保留 Encrypt 域，完全不要求知道 Share 密码。

#### 12.8.3 Mode0 ↔ Mode3

沿用 4.8 已验证方向：Boot/Share 只有语义、位置、大小、物理属性、filesystem/crypto profile 全兼容时才 Preserve。mode0 Encrypt 在 mode3 中没有直接同语义目标，默认 Drop；未来若保数据只能 Migrate。

3→0 时 Boot/Share 独立判断；新增 Encrypt 是独立新域，不能复用 Share 的 FileKey 或密码。

#### 12.8.4 Mode1 ↔ Mode3

mode1 Combined 与 mode3 Share 语义不同，禁止直接 Preserve；mode3 Boot 也不存在 mode1 中的直接同语义来源。第一阶段按 Rebuild，未来通过 Migrate 保数据。

mode1 Encrypt 与 mode3 不存在 type4 同语义目标，因此不能把 Encrypt 密文“改标签”为 Share。

#### 12.8.5 Mode2 ↔ Mode3

mode2 type4 Encrypt 与 mode3 type2 Share 虽然都可能是加密用户区，但 `PartionType` 与语义不同，禁止 Opaque Preserve。保数据必须 Migrate；否则 Rebuild。

#### 12.8.6 任一 EDP 模式 ↔ Plain

Plain 不使用 EDP key material。

EDP→Plain：

- 如果用户要求保留数据，必须把可读来源区域迁移到目标 Plain 分区；
- 加密来源必须验证其自己的来源密码后才能解密迁移；
- 未知密码可以选择放弃该来源区域，但不能把 EDP ciphertext 直接当 Plain 文件系统；
- “恢复普通盘”若不做迁移仍是破坏性重建，不得描述为解密或安全擦除。

Plain→EDP：

- 没有可复用的 EDP key record；
- 新增的每个目标 key domain 独立生成随机 FileKey；
- Share/Encrypt 的目标密码独立；
- 新 FileKey 必须伴随对应文件系统初始化；
- Plain 原文件若要保留，未来走 Migrate，不得通过“保留旧扇区 + 写新 EDP key record”实现。

### 12.9 密码变化与数据变化的正交关系

后续 Review 必须能明确区分：

| 用户意图 | raw FileKey | wrapped FileKey | data extent |
| --- | --- | --- | --- |
| 不知道密码，直接透传 | `K_old`（未知但保留） | 原样 | 0 写入 |
| 验证旧密码，不改密码 | `K_old` | 原样/等价重序列化 | 0 写入 |
| 验证旧密码，改密码 | `K_old` | 用 `P_new` 重新 wrap | 0 写入 |
| 调整布局并保数据 | 视目标策略而定 | 目标 canonical | Migrate 写入 |
| 放弃旧数据/新区域 | `K_new` | 用 `P_new` wrap | 必须完整初始化 |

禁止以下状态进入 commit：

```text
K_new + old ciphertext
P_new + 无法证明对应 K_old/K_new
Unknown password + geometry changed + “Preserve”
Share 的密码/FileKey 被自动复制给 Encrypt
Encrypt 的密码/FileKey 被自动复制给 Share
```

### 12.10 prepare/commit 双层安全门禁

后续实现时至少增加以下 fail-closed 检查：

1. `PreserveOpaque`：目标 key material 必须与来源对应域逐字段一致，data extent 与格式化 write-set 零交集；
2. `PreserveVerified`：已恢复 raw FileKey 的 CRC 必须通过，且目标 key material 与该 FileKey 一致；
3. `RewrapVerified`：source password 已验证、raw FileKey 保持不变、target wrapper 校验通过、data extent 零写入；
4. `Migrate`：来源加密域没有 verified source key 时禁止开始读取/迁移；
5. `Rebuild`：所有需要新 FileKey 的物理加密目标必须存在完整 filesystem initialization plan；
6. 任一区域从 Preserve 被用户勾选“格式化”后，必须显式转成 Rebuild，并重新计算目标 key material；
7. commit 前再次验证“Preserve extents ∩ touched sectors = ∅”；
8. commit 后 readback 验证每个 target key record、FileKeyCRC/wrap mode 和计划一致；
9. rollback/snapshot 继续覆盖协议 touched sectors；涉及 data migration/rebuild 时按现有 transaction 架构扩展数据保护边界，不得降低 LBA3/system-disk/reopen identity 等现有 guard。

### 12.11 必须新增的测试矩阵

实现前先测试，不允许先改 writer。

#### A. 密码域独立性

mode0 至少覆盖四种组合：

```text
Share default PASS / Encrypt default PASS
Share default PASS / Encrypt default FAIL
Share default FAIL / Encrypt default PASS
Share default FAIL / Encrypt default FAIL
```

并验证：

- 一个域输入错误密码不会改变另一个域 action；
- Share/Encrypt 可设置不同 target password；
- 不会生成全局 password fallback；
- Review 能分别显示两个域状态。

#### B. Opaque Preserve

至少验证：

- 未知旧密码 + exact extent → 允许透传；
- LBA7/LBA12 对应 key material 保持逐字段一致；
- data extent 不进入 write-set；
- 改 start/size 任一项 → Opaque Preserve 立即失效；
- Opaque 状态下 UI/CLI 不允许修改 target password。

#### C. Rewrap

至少验证：

- `P_old` 解出 `K_old`；
- `P_new` 重新 wrap 后 raw FileKey 仍为 `K_old`；
- ciphertext 完全不写；
- 旧密码随后不能解开新 wrapper，新密码可以；
- FileKeyCRC 始终对应同一 `K_old`。

#### D. Rebuild

至少验证：

- Rebuild + format/init 缺失 → prepare 必须失败；
- Rebuild + `K_new` + 完整 init → 允许；
- 绝不允许“新 key record + 原密文 extent”。

#### E. 25 个 source/target 组合

对 `Plain/Mode0/Mode1/Mode2/Mode3` 建 table-driven 5×5 planner tests。每一格不要求只得到一个动作，而是验证**每个目标区域**的 canonical mapping/disposition。

重点 golden：

- 0→1：Encrypt exact 可 opaque，Combined 不可直接 Preserve；
- 1→0：Encrypt exact 可 opaque，Combined 不可直接映射为 Share；
- 0/1/2 之间 type4 只有全兼容才 Preserve；
- 0↔3：Boot/Share 按全兼容条件 Preserve；
- mode1 Combined 与 mode0/mode3 Share 永不直接 Preserve；
- mode2 CompatibilityReserve canonical rebuild；
- mode2 type4 ↔ mode3 type2 不可 opaque；
- Plain 不产生来源 EDP key domain；
- Mode1 Combined 物理明文规则不能被 key record/NeedEncrypt 误触发为扇区加密。

### 12.12 分阶段实施顺序（后续独立开发）

本章当前只记录计划；后续开新 worktree 时按以下顺序执行。

#### Phase K0：现状审计与红测试

- 重新基于当时最新 main 审计 `request.password`、`verified_sm4_file_key`、`TargetProvisionPlan`、prepare/commit、TUI；
- 把当前“单密码同时承担 source/target”行为写成失败测试；
- 建 5×5 conversion golden matrix；
- 不改协议生成语义。

#### Phase K1：KeyDomain / PasswordKnowledge 领域模型

- 把来源密码、目标密码、raw FileKey、wrapped material 的职责拆开；
- Share/Encrypt 独立；
- Plain/CompatibilityReserve 明确为无用户密码域；
- API 层禁止密码明文进入日志/序列化。

#### Phase K2：逐域默认密码探测

- canonical LBA12 完整验证；
- 默认密码 PASS 自动预填；
- FAIL 只标记 Unknown；
- 用户输入旧密码可重新验证；
- 添加 default/non-default 混合域回归。

#### Phase K3：RegionMappingPlanner

- source/target 统一 Region 模型；
- 实现 PreserveOpaque / PreserveVerified / Rewrap / Rebuild / Drop；
- `Migrate` 先作为显式 unsupported disposition，绝不静默降级；
- 删除新增 mode-pair 特例的诱因，所有组合走同一 mapper。

#### Phase K4：TUI/CLI Review

- 每个 key domain 独立卡片/字段；
- “默认密码已验证 / 尚未验证 / 用户已验证”状态清晰；
- Unknown 可选择“直接透传”；
- Opaque 时禁用目标密码；
- Rebuild 显示数据丢失；
- Review 逐域显示 FileKey 动作和 data write-set。

#### Phase K5：prepare/commit 安全门禁

- 修复所有可能产生 `K_new + old ciphertext` 的路径；
- Rewrap 只改 wrapper；
- Opaque key material 原样搬运；
- preserved extent 零写入；
- Rebuild 强制初始化；
- readback 验证。

#### Phase K6：可选数据迁移能力

只有在 K0～K5 完整通过后才考虑：

- 同盘不同 extent 的安全文件级迁移；
- EDP→Plain 解密迁移；
- Plain→EDP 导入；
- type2/type4 语义改变时的数据迁移；
- 空间不足、重叠、掉电/失败回滚方案。

未完成 K6 前，所有需要 Migrate 的转换必须明确告诉用户“当前不能无损保留该区域”，由用户决定 Rebuild/Drop。

#### Phase K7：Virtual-HIL

覆盖五态矩阵的协议、key material、文件系统和挂载结果。至少验证：

- unknown-password opaque 场景；
- 双域不同密码；
- change-password only；
- exact preserve；
- forced rebuild；
- mode1 Combined 明文特殊规则；
- mode2 reserve。

#### Phase K8：真实 USB 验收

按风险由低到高选择代表性转换，不一次性对 25 格全部写盘。每次必须：

- 自动备份；
- 整盘确认；
- unmount/lock；
- reopen identity；
- atomic protocol write；
- filesystem/data write；
- readback；
- rollback 证据；
- 制盘后分别验证每个密码域实际可用。

### 12.13 本章完成标准

未来只有同时满足以下条件，才允许把本章从 PLAN 标成 COMPLETE：

1. 全局单一 `request.password` 不再承担多个密码域；
2. Share/Encrypt 等 key domain 可使用不同来源密码和不同目标密码；
3. 默认密码逐域完整验证，PASS 才自动填入；
4. Unknown 密码 + exact geometry 可以安全 Opaque Preserve；
5. Opaque Preserve 不允许改密码且数据零写入；
6. 改密码只 rewrap `K_old`，不重写 ciphertext；
7. 所有 `K_new` 都有强制完整初始化门禁；
8. 5×5 planner matrix 有自动化回归；
9. mode1 Combined 物理明文和 mode2 CompatibilityReserve 特例未被破坏；
10. LBA0～12/LCE golden tests 全绿；
11. fast/full 门禁全绿；
12. 代表性真实 USB 转换验收通过；
13. 文档、TUI、CLI Review 与实际 planner 单一事实源一致。

最终原则：**五种盘型只定义布局；区域语义决定能否保留；密码属于独立 key domain；不知道密码不等于必须破坏数据；改密码不等于换 FileKey；一旦换 FileKey 就必须重建对应数据区。**

## 13. 后续计划：统一 Pane 架构与全盘 DiskLayout（2026-09-26）

> 状态：**PLAN ONLY / 暂不实施**。本章只固化后续 TUI 重构方案，不在当前提交中修改生产代码。本章与第 10、11 章共同构成 TUI 设计约束；如有冲突，以本章对“窗口焦点、窗口滚动、Inspect 四 Pane、Provision 多 Pane、全盘 DiskLayout”的规则为准。

### 13.1 当前结构性问题

当前 main 已有两个应继续复用的基础：

- `src/tui/disk_layout.rs` 已有共享 `DiskLayoutModel`，Inspect 与 Provision 都在调用；
- Inspect topology 已经能按物理 LBA 顺序描述从盘头到盘尾的区域。

但当前 UI 仍有四个架构问题：

1. **Inspect 的磁盘布局不是 Pane。** `AdvancedInspectPanel` 只有 `Tree / Overview / Detail`，但屏幕实际同时存在“磁盘布局 / 结构树 / 节点概览 / 节点详情”四块内容。磁盘布局有边框、有长内容，却没有 focus 和 viewport。
2. **Inspect 的 `j/k` 仍是页面级 tree move。** 当前 Browser 中 `MoveUp/MoveDown` 直接移动 tree selection；切到 Overview/Detail 后，`j/k` 仍会影响 Tree，而不是当前窗口。
3. **Detail 的纵向浏览不完整。** 纯文本 Detail 能用 `detail_scroll`，但 Sector 字段一旦渲染成 Table，目前只有 `h/l` 横向 column viewport，没有 vertical row viewport，长表下半部分无法访问。
4. **Provision 的磁盘布局只覆盖可分区区间。** 官方模式 `provision_layout_model()` 从 `OFFICIAL_PARTITION_START_SECTOR = 63` 开始，并以 `usable_end_lba - 63` 作为总长度，因此没有显示 LBA0～62、LCE 后区域、盘尾等完整物理空间。

本章目标不是继续给各页面补特殊键位，而是建立统一的：

```text
Workspace
  ↓
Pane
  ↓
WidgetRole
  ↓
Pane-local Action
```

### 13.2 Pane 是一等交互单元

建议引入统一 Pane 状态模型，具体类型名可在实现时调整：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PaneId {
    InspectDiskLayout,
    InspectTree,
    InspectOverview,
    InspectDetail,

    ProvisionParameters,
    ProvisionDiskLayout,
    ProvisionSummary,
    ProvisionChanges,
}

#[derive(Debug, Clone, Default)]
struct PaneViewport {
    scroll_y: usize,
    scroll_x: usize,
    selected: Option<usize>,
}

struct PaneFocusState {
    focused: PaneId,
    viewports: BTreeMap<PaneId, PaneViewport>,
}
```

硬性要求：

- focus 与业务 selection 分离；
- 每个可能溢出的 Pane 都拥有自己的纵向 viewport；
- Table 可额外维护横向 viewport；
- Pane 切换、终端 resize、进入/退出 Sector Inspector 后都能恢复原 focus/scroll/selection；
- NavigationStack 保存 Pane focus 和必要 viewport，不再只保存一个页面级 `detail_scroll`。

### 13.3 多 Pane 页面统一键位

Normal mode 下统一：

| 按键 | 语义 |
| --- | --- |
| `Tab` | 下一个 Pane |
| `Shift-Tab` | 上一个 Pane |
| `Ctrl-w h/j/k/l` | 按空间方向切换 Pane |
| `Ctrl-w w / W` | 下一个 / 上一个 Pane |
| `j/k` | 当前 Pane 的纵向操作 |
| `Ctrl-d/u` | 当前 Pane 半页下/上 |
| `PageDown/PageUp` | 当前 Pane 整页下/上 |
| `gg/G` | 当前 Pane 顶部/底部 |
| `Esc` | 返回上一层 |
| `q` | 全局退出，仍受 critical-operation 安全门禁保护 |

`j/k` 的具体行为由 focused Pane 的 WidgetRole 决定：

```text
Tree:
  j/k = 下一/上一节点

Overview:
  j/k = 当前内容滚动一行

Detail Paragraph:
  j/k = 当前内容滚动一行

Detail Table:
  j/k = row viewport 下一/上一行

DiskLayout:
  j/k = 区域明细滚动一行

Provision Parameters:
  j/k = 下一/上一字段
```

禁止再出现“Inspect 页面所有 `j/k` 都直接调用 `move_tree()`”这种页面级硬编码。

### 13.4 h/l 继续按 WidgetRole 解释

保留第 11 章 context-sensitive h/l 原则：

```text
Tree:
  h/l = 折叠/展开、父/子

Table:
  h/l = 横向 column viewport

Input/Insert:
  ←/→ = 文本光标
  h/l = 输入字符或按输入模式定义

纯文本 Overview / Detail / DiskLayout:
  默认不占用 h/l；未来若增加横向 viewport，由 Pane 自己声明
```

KeyMapper 后续至少需要知道：

```text
InputMode
Workspace
Focused Pane
WidgetRole
```

### 13.5 Sector Inspector：扇区切换与滚动分离

上一/下一 sector 属于数据导航，不属于 viewport scroll。

目标语义：

```text
[ / ]       上一个 / 下一个 sector
j / k       当前 Hex/字段视图纵向移动
h / l       当前 Hex/字段视图横向/字节导航
Ctrl-u/d    半页
PageUp/Down 整页
gg / G      顶/底
```

因此：

- `PageUp/PageDown` 不再用于切换 LBA；
- `j/k` 绝不能切换 sector；
- sector navigation 必须有独立 action 和 Help 文案；
- 若 `[`/`]` 与现有绑定冲突，实现阶段可以换成其它独立键，但“扇区切换与窗口滚动分离”不能妥协。

### 13.6 DiskLayoutModel 升级为完整物理盘契约

现有 `DiskLayoutModel` 继续作为共享单一事实源，不新增 InspectLayout / ProvisionLayout 两套模型。

当 `total_sectors > 0` 时必须满足：

```text
first.start_lba == 0

任意相邻 segment:
previous.end_exclusive == next.start_lba

last.end_exclusive == total_sectors

所有 segment:
sector_count > 0
无 overlap
无 hole
```

内部统一使用 `[start, end_exclusive)`；UI 统一显示闭区间 `[start..end]`，禁止 renderer 自己重复算范围。

### 13.7 RegionKind 与 RegionState 分离

颜色表达“区域是什么”，操作状态表达“这个区域会怎么处理”，两者不能混在一个枚举里。

建议语义：

```rust
enum DiskRegionKind {
    Protocol,
    Reserved,
    Unknown,
    Plain,
    Boot,
    Share,
    Combined,
    Encrypt,
    Compatibility,
    Lce,
    Tail,
}

enum DiskRegionState {
    Existing,
    Preserved,
    Rebuilt,
    New,
    Free,
    Dropped,
}
```

例如“保密区 + Rebuild”仍然使用保密区颜色，只额外显示 `Rebuild` 状态；不能因为要重建就换成另一种区域颜色。

### 13.8 Unknown / Reserved / Free / Tail 严格区分

全盘视图至少区分：

```text
Unknown
= Inspect 当前无法确定语义

Reserved
= canonical target 明确要求保留、不允许用户分配

Free
= planner 明确允许分区占用

Tail
= 盘尾保护区域

Protocol
= EDP 协议区域
```

因此 Provision 不得把“不是用户分区”的所有 gap 都叫“空闲”。

例如 `[13..62]`：

- Inspect 可显示“未知区域”；
- canonical Provision 可显示“保留区域 · 不写入”；
- 只有 planner 明确可分配的范围才能标记为 `Free`。

### 13.9 Provision 必须从 LBA0 显示到最后一个 sector

所有目标模式都改为完整物理盘视图，而不是只显示 LBA63 之后的可分区区间。

mode0 示例应按顺序覆盖：

```text
EDP 主协议区        [0..12]
保留/未知区域       [13..62]
启动区              [63..20479]
交换区              [...]
保密区              [...]
LCE                 [...]
LCE 后保留/未知区    [...]
盘尾区域            [last-2047..last]
```

具体边界只能来自 canonical planner / protocol geometry，禁止 TUI 猜测。

mode1 / mode2 / mode3 同样必须完整覆盖 `[0..total_sectors-1]`。

Plain 使用自己的完整磁盘模型，不硬套 EDP 标签；可以是 MBR/保留区、普通分区、Free/Reserved 等，但仍必须从 0 连续到 last sector。

### 13.10 抽取共享 DiskLayoutPane

Inspect 与 Provision 不再各自拼：

```text
bar_line
legend
layout_lines
Paragraph
```

而是共享一个 Pane renderer。

建议模型：

```rust
struct DiskLayoutPaneModel {
    title: String,
    disk_label: String,
    total_sectors: u64,
    total_bytes: u64,
    regions: Vec<DiskLayoutRegion>,
    status_line: Option<String>,
}
```

统一显示：

1. 整盘摘要；
2. `[0..last_sector]`；
3. 比例条；
4. 色标；
5. 按 LBA 排序的区域明细；
6. range / sectors / size / percent；
7. 可选 Preserve / Rebuild / New / Drop 等状态；
8. coverage 验证结果。

Inspect、Provision Form、Provision Review 只负责生成不同 model：

```text
Inspect:
  current / observed

Provision Form:
  target / planned

Provision Review:
  current + target / before + after
```

renderer、比例算法、颜色、legend、range formatter 必须共用。

### 13.11 DiskLayoutPane 自己拥有 vertical viewport

获得焦点后：

```text
j/k         一行
Ctrl-u/d    半页
PageUp/Down 一页
gg/G        首尾
```

建议固定 summary / 比例条 / legend，只让 region detail list 滚动。

窗口高度不足时：

- summary 可压缩；
- bar 保留；
- legend 可折行或压缩；
- 明细始终可以通过 viewport 浏览；
- 不允许简单截断后让底部区域永久不可访问。

### 13.12 Inspect 从三 Pane 升级为四 Pane

最终：

```text
DiskLayout
Tree
Overview
Detail
```

Tab 顺序：

```text
DiskLayout
→ Tree
→ Overview
→ Detail
→ DiskLayout
```

Shift-Tab 反向。

此规则覆盖第 11.7 章旧的 `Tree → Overview → Detail` 三 Pane 循环。

### 13.13 Inspect 宽屏布局

继续保留当前视觉方向：

```text
设备 > disk4 > Inspect                         Esc 返回：设备列表

┌ 磁盘布局 ────────────────────────────────────────────────┐
│ summary / bar / legend / region details                 │
└──────────────────────────────────────────────────────────┘

 磁盘布局 │ 结构树 │ 节点概览 │ 节点详情

┌ 结构树 ──────────┬ 节点概览 ───────┬ 节点详情 ───────────┐
│                  │                  │                     │
│                  │                  │                     │
└──────────────────┴──────────────────┴─────────────────────┘
```

宽屏四个 Pane 可以同时渲染；当前 Pane 只改变 focus border，不决定其它 Pane 是否存在。

### 13.14 Inspect 窄屏布局

窄屏显示四个 Pane 标签：

```text
[磁盘布局] [结构树] [节点概览] [节点详情]
```

一次只显示 focused Pane。

宽/窄屏必须共用同一状态，resize 后保持：

```text
Tree selection + viewport
Overview scroll
Detail row/scroll
Detail column viewport
DiskLayout scroll
focused Pane
```

### 13.15 Tree / Overview / Detail 的独立行为

Tree：

```text
j/k      节点
h/l      折叠/展开、父/子
o        toggle
Enter    进入/打开
gg/G     第一/最后节点
Ctrl-u/d 半页节点
```

Tree selection 变化后，Overview/Detail 内容跟随新节点，并将新节点对应的 Overview/Detail viewport 归零；Tree 自己的 viewport 不受影响。

Overview 是纯只读 scroll Pane：

```text
j/k
Ctrl-u/d
PageUp/Down
gg/G
```

全部只修改 `overview.scroll_y`，禁止改变 tree selection。

Detail：

- Paragraph：`j/k` 纵向 scroll；
- Table：`j/k` row viewport，`h/l` column viewport；
- `Ctrl-u/d`、`PageUp/Down`、`gg/G` 都作用于当前 Detail 内容。

### 13.16 Detail Table 同时显示行、列位置

当前字段表只有横向 `1/2 列`。

后续标题至少显示：

```text
节点详情 · 行 18–41 / 67 · 列 1/2
```

窄屏可压缩为：

```text
详情 18-41/67 · 1/2列
```

用户必须能判断上方、下方、左右是否还有内容。

### 13.17 Provision Form 变成真正双 Pane

当前视觉已经是：

```text
参数 56%
实时布局 44%
```

但后续必须有真实：

```text
ProvisionParameters
ProvisionDiskLayout
```

Tab 可切换。

Parameters focused：

```text
j/k      字段
i        编辑
Space    切换 option
f        填满
Enter    生成计划
```

DiskLayout focused：

```text
j/k
Ctrl-u/d
PageUp/Down
gg/G
```

只滚目标整盘布局。

硬门禁：

```text
ProvisionDiskLayout focused + j/k
不得改变 field_selected
```

### 13.18 Provision Review 也纳入 Pane 系统

建议 Review：

```text
ProvisionSummary
ProvisionDiskLayout
ProvisionChanges
```

- Summary：模式、目标盘、备份、风险、确认摘要；
- DiskLayout：目标整盘空间图，可扩展 Before/After；
- Changes：touched sectors、Preserve/Rebuild/Drop、格式化、数据区动作。

宽屏多 Pane 同时显示，窄屏通过 Tab 切换。长内容必须可滚。

### 13.19 其它 Provision Stage 的原则

不强制一次性把所有 Stage 都拆成复杂 Pane，但遵守：

> **只要一个带边框区域承载独立内容、可能超出可视范围、且用户合理期待能查看完整内容，它就是 Pane，必须有 focus/viewport。**

建议：

| Stage | Pane |
| --- | --- |
| SelectDisk | DeviceList + DeviceSummary（如存在独立详情框） |
| BackupPrompt | Prompt |
| Menu | ModeList + CurrentDiskSummary（如存在独立框） |
| Form | Parameters + DiskLayout |
| Review | Summary + DiskLayout + Changes |
| Confirm | Confirm |
| Running | Progress |
| Result | ResultSummary + VerificationDetails（长内容时） |

### 13.20 Visual focus 与真实 focus 必须一致

以后只有统一 `PaneFocusState.focused` 可以决定 `focused_panel()`。

禁止某个 Pane 画出高亮边框，但事件路由实际无法进入。

测试必须锁住：

```text
focused border
↔ PaneId
↔ keyboard dispatch target
```

三者一致。

### 13.21 统一 VerticalViewport

不再让各页面自己手工算滚动。

建议公共 helper：

```rust
struct VerticalViewport {
    offset: usize,
}

impl VerticalViewport {
    fn line_up(&mut self);
    fn line_down(&mut self, content_len: usize, visible_len: usize);
    fn half_page_up(&mut self, visible_len: usize);
    fn half_page_down(&mut self, content_len: usize, visible_len: usize);
    fn page_up(&mut self, visible_len: usize);
    fn page_down(&mut self, content_len: usize, visible_len: usize);
    fn top(&mut self);
    fn bottom(&mut self, content_len: usize, visible_len: usize);
    fn clamp(&mut self, content_len: usize, visible_len: usize);
}
```

内容变化或 resize 后必须 `clamp()`，不再依赖无限增长的 offset + renderer 容错。

### 13.22 Selection Pane / Scroll Pane / Table Pane 分型

Selection Pane：

```text
Tree
Provision Parameters
Device list
```

`j/k` 改 selection，viewport 自动跟随。

Scroll Pane：

```text
Overview
DiskLayout
纯文本 Detail
```

`j/k` 只改 `scroll_y`，没有业务 selection。

Table Pane：

```text
Inspect Detail Fields
```

`j/k` 改 row cursor/viewport，`h/l` 改 column viewport。

这样 KeyMapper 只需根据 Pane behavior 分发，不再理解每个页面的业务细节。

### 13.23 视觉统一

整体以当前 Provision 的低饱和 TrueColor 方案为准，同一 RegionKind 在 Inspect / Provision / Review 使用同一 theme token。

建议：

```text
Boot              青色系
Share/Combined    低饱和绿色系
Encrypt           紫色系
Compatibility/LCE 黄褐色系
Unknown/Reserved/Tail 深灰系
Plain             中性蓝灰
Protocol          轻 metadata accent
```

Preserve/Rebuild/New/Drop 用低干扰符号 + 文本，不改变区域主色：

```text
✓ Preserve
↻ Rebuild
＋ New
− Drop
? Unknown
```

### 13.24 覆盖旧规则

本章明确覆盖：

- 11.7 “Inspect 三个子工作区” → 四 Pane；
- 11.11 顶部 DiskLayout 从只读固定块 → 可 focus Pane；
- 11.10 Table 只强调 h/l 横滚 → 增加 vertical row viewport；
- 页面级单一 `detail_scroll` → per-pane viewport；
- Provision “实时布局”只读 render block → `ProvisionDiskLayout` Pane；
- Sector Inspector `PageUp/PageDown = 前后 sector` → viewport 翻页；sector 使用独立 action。

继续保持：

- `q` 全局退出；
- `Esc` 返回上一级；
- critical write guard 不变；
- Insert 内左右键只做文本光标；
- LBA0～12/LCE 协议语义和任何写盘安全门槛不变。

### 13.25 Phase P0：失败测试先行

至少先建立：

```text
Inspect Tree focused:
  j/k → tree_selected 改变

Inspect Overview focused:
  j/k → overview.scroll_y 改变
  tree_selected 不变

Inspect Detail focused:
  j/k → detail row/scroll 改变
  tree_selected 不变

Inspect DiskLayout focused:
  j/k → layout.scroll_y 改变
  tree_selected 不变

Provision Parameters focused:
  j/k → field_selected 改变

Provision DiskLayout focused:
  j/k → layout.scroll_y 改变
  field_selected 不变

Tab:
  只改变 focused Pane
  不改变其它 Pane selection
```

### 13.26 Phase P1：Pane 基础设施

- 引入 PaneId / PaneFocus / PaneViewport；
- NavigationStack 保存/恢复 Pane focus；
- Pane next/previous + spatial neighbor resolver；
- KeyMapper 按 Pane + WidgetRole 解释；
- focus border 单一事实源。

**实施状态（2026-09-26）：P0 COMPLETE，P1 COMPLETE。**

- P0 已新增 `tests/tui_pane_contract.rs` 并纳入 `tui_suite`，锁定 Inspect 四 Pane 与 Provision Form 双 Pane 的 focused `j/k`、Tab 前后切换和 selection 不串扰；P0 首次运行按预期因 Pane 基础设施缺失失败，P1 完成后 6/6 转绿。
- P1 已新增统一 `PaneId / PaneFocus / PaneViewport / VerticalViewport`，Inspect 使用 `DiskLayout / Tree / Overview / Detail` 四 Pane，Provision Form 使用 `Parameters / DiskLayout`，Review 预留 `Summary / DiskLayout / Changes`；`NavigationFrame` 可保存/恢复 Pane focus + viewport。
- `Tab/Shift-Tab` 已按当前页面 Pane 顺序切换，`Ctrl-w h/j/k/l` 已接空间邻接 resolver；Inspect 与 Provision 的 `j/k` 已改为按 focused Pane 分发，Provision `DiskLayout` focused 时不会再修改 `field_selected`。
- Pane 行为从 `provision/state.rs` 拆入 `provision/pane.rs`，保持 Provision orchestration 模块边界；正式 fast gate 热缓存复跑为 4 suites / 6 artifacts、0 failures、4.49s。首次冷缓存复跑所有 suite 也为 0 failures，仅因 50.12s 超过 45s timing budget 退出，未发现功能或架构回归。
- 第 12 章继续保持 PLAN ONLY；本章实现始终未修改任何 LBA0～12/LCE 协议语义或写盘安全链。

### 13.27 Phase P2：Full-Disk DiskLayoutModel

- 把 DiskLayoutModel contract 改为整盘连续覆盖；
- Inspect topology adapter 维持真实全盘；
- Provision adapter 从 LBA0 开始；
- 补齐 protocol / reserved / partition / LCE / post-LCE / tail；
- Plain 使用自己的完整区域模型；
- Unknown / Reserved / Free 分离；
- 建 overlap / hole / last-sector 门禁。

### 13.28 Phase P3：共享 DiskLayoutPane renderer

- 抽 summary / bar / legend / details；
- Inspect 删除自己的 layout_lines 拼接；
- Provision 删除自己的 bar + legend + editor-layout 拼接；
- 两边共用 renderer / theme / percent / range formatter；
- 增加 layout vertical viewport。

### 13.29 Phase P4：Inspect 四 Pane

- AdvancedInspectPanel 增加 DiskLayout，或迁移到通用 PaneId；
- Tab/Shift-Tab 四 Pane 循环；
- Ctrl-w spatial focus；
- 宽屏多 Pane 同显；
- 窄屏单 Pane；
- resize 保持 focus/scroll。

### 13.30 Phase P5：Overview/Detail 完整滚动

- Overview 独立 scroll_y；
- Detail Paragraph 独立 scroll_y；
- InspectFields Table 增加 row viewport；
- j/k、Ctrl-u/d、PageUp/Down、gg/G 完整支持；
- 标题显示 row + column position。

**实施状态（2026-09-26）：P2～P5 COMPLETE。**

- P2：`DiskLayoutModel` 已升级为 `[0..total_sectors)` 连续物理盘契约，新增 `DiskRegionKind` 与 `validate_complete/from_claims`；Inspect、官方 Provision、Plain 三类 adapter 均覆盖 LBA0 到最后 sector，并把 `Protocol / Reserved / Unknown / Free / LCE / Tail` 分离。新增 hole / overlap / last-sector 门禁。
- P3：新增共享 `DiskLayoutPane` renderer，统一 summary、比例条、语义颜色、闭区间 range、百分比、legend、details、focus border 与 vertical viewport；Inspect/Provision 已删除各自的磁盘布局拼接路径，Provision 旧 `partition_style` helper 也已移除。
- P4：Inspect 已成为 `DiskLayout / Tree / Overview / Detail` 四个真实 Pane；宽屏四 Pane 同显，窄屏只显示 focused Pane，Tab/Shift-Tab 与 `Ctrl-w h/j/k/l` 均使用统一 PaneFocus，focus/viewport 状态不依赖终端尺寸。
- P5：Overview 与 Detail Paragraph 使用独立 `PaneViewport.scroll_y`；Detail 字段 Table 新增 vertical row viewport，并保留原有 h/l column viewport，标题显示 `行 start–end / total · 列 position`；`j/k`、`Ctrl-u/d`、`PageUp/Down`、`gg/G` 已按 focused Pane 分发并使用真实内容长度。30 行字段表回归可滚动到第 21 行且首行退出视图。
- 正式 `scripts/test-fast.sh`：4 suites / 6 artifacts，0 failures，13.17s；P2/P5 专项以及共享 renderer 的 Inspect/Provision 生命周期测试均通过。
- 下一阶段仅进入 P6，不开始第 12 章。

### 13.31 Phase P6：Sector Inspector 解耦

- 前后 sector 移出 PageUp/PageDown；
- 新增独立 sector action；
- j/k 与 PageUp/PageDown 只负责当前视图；
- Help/footer/USAGE 同步；
- raw/decode/mixed、byte cursor、解析逻辑不变。

**实施状态（2026-09-26）：P6 COMPLETE。**

- 新增 `SectorPrevious / SectorNext` action，Normal 模式使用 `[` / `]` 切换前后 sector；Insert 模式仍把方括号作为普通文本。
- `PageUp/PageDown` 改为当前 sector 内 ±256B 整页移动，`Ctrl-u/Ctrl-d` 保持 ±128B 半页，均不再改变 LBA。
- 原 `advanced_inspect_shift_sector()` 只由独立 sector action 调用；raw/decode/mixed、byte cursor、按需读取、single-flight 与 cache 行为未改。
- Sector Inspector footer 与 `docs/user/USAGE.md` 已同步；专项门禁 3/3 通过。fast suites 全部 0 failures，冷编译仅因 50.30s 超过 45s timing budget，功能门禁无失败。

### 13.32 Phase P7：Provision 多 Pane

- Form：Parameters + DiskLayout；
- Review：Summary + DiskLayout + Changes；
- 宽屏多 Pane、窄屏单 Pane；
- layout focused 时 j/k 不再修改字段；
- Parameters 保持现有 Insert/Space/f 语义；
- Review 长内容全部可滚。

### 13.33 Phase P8：清理与正式门禁

**实施状态（2026-09-26）：P7/P8 IMPLEMENTED，等待 GitHub CI 正式门禁。**

- P7：Form 使用 Parameters + DiskLayout 两 Pane，宽屏同显、窄屏只显示 focused Pane；Review 使用 Summary + DiskLayout + Changes 三 Pane，Tab/Shift-Tab 与 Ctrl-w 空间 focus 统一，三 Pane 长内容均使用各自 vertical viewport。
- Form 的 Insert/Space/f/a/d 只在 Parameters focused 时生效；DiskLayout focused 时字段 selection 不再被 j/k、h/l、Toggle 等输入误改。
- Review 的 j/k、Ctrl-u/d、PageUp/Down、gg/G 全部按 focused Pane 滚动，Enter/e/Esc 保持原确认/导出/返回语义。
- P8 已删除页面级 `detail_scroll`、NavigationFrame 的旧 detail scroll snapshot 与 `advanced_inspect_scroll_detail`；PaneViewport 成为唯一滚动事实源，并新增源码回归门禁。
- Inspect footer 已从旧“j/k 选择 / h/l 树”文案改为 focused-Pane 语义；Sector Inspector 跨 sector 的旧 PageUp/PageDown 文案已清除。
- 本阶段只重构 TUI 状态、渲染和键位路由；第 12 章仍为 PLAN ONLY，未修改 LBA0～12/LCE 协议语义和真实写盘安全链。

清理：

- 页面级 `detail_scroll` 旧路径；
- Inspect Browser 全局 `MoveUp/MoveDown => move_tree`；
- Provision 独立 bar/legend 重复 renderer；
- 旧三 Pane Help/footer；
- Sector Inspector PageUp/PageDown=换 sector 文案；
- 所有“假 focused border”。

验证：

```text
cargo fmt --all -- --check
git diff --check
tui_suite
inspect_suite
provision_suite
scripts/test-fast.sh
python3 scripts/test-full.py --profile full
```

若不改变真实写盘业务语义，本轮只要求真实盘 Inspect 只读视觉/导航验收，不额外增加写盘 HIL。

### 13.34 必须新增的回归门禁

至少覆盖：

1. Inspect Pane 顺序 DiskLayout→Tree→Overview→Detail；
2. Shift-Tab 反向；
3. Ctrl-w h/j/k/l 按屏幕邻接移动 focus；
4. Overview focused 时 j/k 不改 tree selection；
5. Detail focused 时 j/k 不改 tree selection；
6. DiskLayout focused 时 j/k 不改 tree selection；
7. Tree focused 时 j/k 仍选节点；
8. Tree h/l 仍折叠/展开；
9. Detail Table h/l 横滚列；
10. Detail Table j/k 可访问所有行；
11. Detail title 显示正确 row/column position；
12. Overview 可访问末行；
13. DiskLayout 可访问最后 region；
14. Provision Parameters j/k 仍选字段；
15. Provision DiskLayout j/k 不改 field_selected；
16. Provision Form Tab 真正切两个 Pane；
17. Provision Review 长内容都可访问；
18. Inspect/Provision 同 RegionKind 使用同 theme token；
19. DiskLayout 第一段从 LBA0 开始；
20. 最后一段结束于 `total_sectors - 1`；
21. 相邻 segments 连续无 hole；
22. segments 无 overlap；
23. mode0/1/2/3 target 都完整覆盖整盘；
24. Plain target 完整覆盖整盘；
25. Unknown / Reserved / Free 不混用；
26. Inspect current layout 与 topology range 一致；
27. Provision target layout 与 planner/protocol geometry 一致；
28. 宽→窄→宽不丢 focus；
29. resize 不丢各 Pane scroll/selection；
30. Esc/q 安全语义不变；
31. sector previous/next 与 viewport 翻页完全分离；
32. LBA0～12/LCE golden tests 不变；
33. critical write guard 不变。

### 13.35 完成标准

只有以下条件全部满足，本章才能从 PLAN 标为 COMPLETE：

1. Inspect 拥有四个真实可 focus Pane；
2. DiskLayout 能通过 Tab/Ctrl-w 获得焦点并完整滚动；
3. Overview/Detail 的 j/k 只操作当前 Pane；
4. Detail 长 Table 所有行可访问；
5. sector 切换与内容滚动是不同动作；
6. Provision Form 的 Parameters/DiskLayout 都是真实 Pane；
7. Provision Review 关键区域都可 focus/scroll；
8. Inspect 与 Provision 共用唯一 DiskLayout renderer/model/theme；
9. Provision 从 LBA0 连续显示到最后 sector；
10. Unknown / Reserved / Free / Tail / Protocol 语义正确；
11. DiskLayout 自动验证连续、无重叠、无缺口；
12. 宽屏与窄屏使用同一 Pane state；
13. Help/footer 与实际 focused-pane 行为一致；
14. fast/full 与专项门禁全绿；
15. 不改变任何已闭环 LBA0～12/LCE 协议语义和写盘安全门槛。

最终原则：**页面只组织 Pane；Pane 决定按键如何作用于自己的内容；Tab/Ctrl-w 只移动焦点；j/k 永远服务当前 Pane；Inspect、Provision、Review 的磁盘布局全部来自同一份完整物理盘模型，并从 LBA0 连续覆盖到最后一个 sector。**

## 14. 后续计划：Inspect 信息密度与多层解码语义（2026-09-26）

> 状态：**PLAN ONLY / 持续收集**。本章用于当前计划分支继续收集 Inspect 细节问题；本章提交只允许补充调查结论、实现方案与回归门禁，不修改生产代码。后续用户提出的同类问题继续追加到本章，待问题清单确认后再单独进入实现分支。

### 14.1 结构树单扇区节点去掉冗余 `[n..n]`

当前 `src/tui/inspect/render.rs` 的结构树 renderer 对所有节点统一调用 `format_lba_closed_range(start, end_exclusive)`，再无条件拼到节点 label 后，因此 `LBA0` 会显示为 `LBA0 [0..0]`、`LBA12` 会显示为 `LBA12 [12..12]`。这对单个扇区节点重复表达同一信息，降低结构树信息密度。

目标显示规则：

```text
单扇区 Sector 节点：
  LBA0
  LBA1
  ...
  LBA12

多扇区 Region / Extent / Partition / UnknownRange：
  EDP 主协议区 [0..12]
  未知区域 [13..62]
  ...
```

实现约束：

- 只改变**结构树行文本**；内部 `InspectNodeRange` 仍保持 `[start, end_exclusive)`，不改变任何定位、搜索、折叠、懒加载和导航语义；
- `InspectNodeKind::Sector` 不再追加 range suffix；其它节点继续使用统一闭区间 formatter；
- 节点概览中的“范围”仍保留，因为 Overview 的职责就是展示完整元数据，不能因为结构树精简而丢失范围信息；
- 不通过字符串判断 `label.starts_with("LBA")`，必须依据 `InspectNodeKind::Sector`，避免未来本地化或 label 变化造成行为漂移。

回归门禁至少覆盖：

1. Sector `LBA0` 渲染结果不含 `[0..0]`；
2. Sector `LBA12` 渲染结果不含 `[12..12]`；
3. 多扇区 `EDP 主协议区` 仍显示 `[0..12]`；
4. `UnknownRange / Partition / Extent` 的范围显示不受影响；
5. Tree selection、搜索、`gl` 跳转和 lazy sector 分页行为不变。

### 14.2 `Decoded: 0x77 (119)` 与“保密区最大错误次数=255”并不矛盾

当前界面把两层不同的“解码”混在一起显示，因此视觉上像是解析结果不一致。以 LBA12 的 `PassInfo` 为例，真实处理链是：

```text
物理 LBA12 raw sector
  │
  ├─ A6B0 整扇解密（device CRC key）
  ▼
sector plaintext / 当前 UI 的 `Decoded`
  │
  │ PassInfo 内部仍保留字段级存储编码
  │ 对 PassInfo offset 0 / 3 / 6 再 XOR 0x88
  ▼
PassInfo logical value / 当前字段 `Value`
```

代码证据：

- `protocol::lba12::parse_lba12()` 先对完整 512B 执行 A6B0 解密，并把结果保存在 `Lba12View::plain`；`decoded()` 返回的就是这一级 `plain`；
- 随后从 `plain[0x120..0x12e]` 取 14B `stored_pass`，再调用 `PassInfo::decode_stored()`；
- `PassInfo::decode_stored()` 对 offset `0 / 3 / 6` 执行 `^ 0x88`；
- “保密区最大错误次数”正好位于 PassInfo offset `6`。

因此默认逻辑值 `255 = 0xFF` 在 PassInfo 的存储表示中是：

```text
0xFF ^ 0x88 = 0x77
```

所以当前界面看到：

```text
Decoded: 0x77 (119)
Value:   255
```

实际含义是：

```text
Sector decoded byte: 0x77   # 外层扇区已经解密，但 PassInfo 字段级 XOR 尚未解除
Field logical value: 0xFF   # PassInfo::decode_stored() 再 XOR 0x88 后的真正字段值
```

协议解析本身没有把 119 错当成 255；问题在于 UI 把“外层 sector decode”简称成了过于宽泛的 `Decoded`，没有把字段级二次变换展示出来。

### 14.3 计划中的 UI 修正

Sector Inspector 不应继续让用户猜“Decoded 到底解了几层”。目标是把数据层级明确写出来：

```text
Raw byte:          0x.. (...)
Sector decoded:    0x77 (119)
Field decoded:     0xFF (255)      # 仅字段存在额外变换时显示
Transform:         XOR 0x88        # 仅有已证实字段级变换时显示

保密区最大错误次数
Value: 255
```

设计原则：

- `Sector decoded` 只表示 sector-level decoder 的输出，例如 LBA12 A6B0、LBA7 rolling XOR 等；
- `Field decoded` 表示字段模型完成其自身存储编码/解码后的字节值；没有字段级二次变换时不重复显示；
- `Value` 继续表示类型化语义值，例如整数、布尔、枚举、文本；
- renderer 不允许硬编码“PassInfo offset 6 要 XOR 0x88”。字段级变换信息必须来自 inspect/protocol 模型；
- 对无法证明的变换只显示 sector decoded/raw，不推测 `Field decoded`；
- 现有 Raw / Decode / Mixed 三种 sector 视图语义保持不变，本项只修正详情区命名和字段级 provenance 展示。

建议在 `SectorField` 或等价只读 inspection DTO 中增加可选的字段级表示信息，例如：

```text
stored_bytes       # sector decoded 后、字段自身 decode 前
logical_bytes      # 字段自身 decode 后；仅可可靠重建时存在
transform_note     # 例如 "XOR 0x88"，必须来自协议事实
```

具体字段名可在实现时调整，但不能让 TUI renderer 自己重新实现协议算法。

### 14.4 回归门禁

至少新增：

1. LBA12 PassInfo 保密区最大错误次数为 `255` 时，sector decoded byte 固定显示 `0x77 (119)`；
2. 同一字段的 field decoded/logical byte 显示 `0xFF (255)`；
3. 回归验证 `0x77 ^ 0x88 == 0xFF`，并锁定 PassInfo offset 6 的字段映射；
4. 交换区最大错误次数（PassInfo offset 3）同样遵循字段级 XOR 展示；
5. 不需要字段级变换的普通字段不出现伪造的 `Field decoded`；
6. LBA12 `Value` 仍来自 canonical `PassInfo` parser，不从 UI 显示字节反推；
7. Raw / Decode / Mixed 切换、byte cursor、高亮和 active field range 均不回归；
8. LBA0～12/LCE golden protocol tests 保持不变。

### 14.5 实施顺序

后续真正实现时按以下顺序执行：

```text
I0  先补失败测试：单扇区 Tree 文案 + PassInfo 多层 decode 展示
I1  Tree renderer 对 Sector 去掉 range suffix
I2  inspection DTO 增加字段级 stored/logical/provenance（只读）
I3  Sector Inspector 将 `Decoded` 改为 `Sector decoded`
I4  有可靠字段变换时增加 `Field decoded` / `Transform`
I5  更新 Help / USAGE 中对 Raw / Decode / Mixed 的术语说明
I6  跑 inspect/tui 专项、fast/full；只读真实盘验收，不触发写盘
```

本章当前只记录计划。**不得在这个计划分支直接实现 I1～I6。**

### 14.6 盘尾区域子节点必须按物理空间顺序、且不能用重叠兄弟节点表达

当前截图中的盘尾树看起来“没有按顺序排列”，根因不是简单缺少 `sort_by(start_lba)`，而是 `tail_region()` 的建模方式本身存在层级问题。

当前 `src/application/inspect_tree.rs::tail_region()` 先创建一个覆盖**完整 2048 扇区盘尾窗口**的子节点：

```text
盘尾取证窗口 [total-2048 .. total-1]
```

然后又把两个位于这个窗口内部的已知范围作为它的**兄弟节点**追加：

```text
盘尾历史 9 扇区镜像 [total-1024 .. total-1016]
盘尾 end-4 restore-node [total-4 .. total-4]
```

因此三个 child 的 `start_lba` 实际已经是递增的，但第一个 child 覆盖了后两个 child。问题是**兄弟节点彼此重叠**，所以视觉上无法形成“从前到后”的物理磁盘序列。

以当前截图 `total_sectors = 15728640` 为例：

```text
当前：
盘尾取证窗口              [15726592..15728639]
盘尾历史 9 扇区镜像       [15727616..15727624]   # 被上一行包含
盘尾 end-4 restore-node   [15728636..15728636]   # 也被上一行包含
```

这不是理想的物理拓扑树。`盘尾取证窗口` 本质上是“盘尾区域为什么被采集”的**覆盖/证据概念**，不是应与内部特殊扇区并列的互斥 extent。

目标 topology 应把同一层 child 规范化成**不重叠、按 LBA 连续排列**的物理区段，例如：

```text
盘尾区域 [15726592..15728639]
  ├─ 盘尾未分类              [15726592..15727615]
  ├─ 盘尾历史 9 扇区镜像     [15727616..15727624]
  ├─ 盘尾未分类              [15727625..15728635]
  ├─ 盘尾 end-4 restore-node [15728636]
  └─ 盘尾未分类              [15728637..15728639]
```

`盘尾取证窗口 = 最后 2048 扇区` 这一事实仍需保留，但不再作为与内部 extents 重叠的 sibling。可采用以下任一等价设计，实施时优先选择模型最简洁的一种：

1. `盘尾区域` 本身就是该 2048-sector forensic window，Overview/metadata 中注明“取证窗口”；或
2. 在 `盘尾区域` 下增加一个非物理范围的 `Group/Structure` 说明“取证窗口”，但真正的物理 Extent children 仍必须互斥连续。

关键约束：

- **物理 Tree 的同级 Extent 必须按 `start_lba ASC`，且不得 overlap；**
- 一个 sector 可以在语义分类 API 中同时属于多个角色，例如 `TailMetadataMirror + DeviceTailWindow`；这种多重 membership 可以继续由 `inspect_target::regions_for_lba()` 保留；
- 但 Tree 的物理空间表示必须选一个 primary segmentation，次级“被某证据窗口覆盖”的关系通过 annotation/group/metadata 表达，不能制造重叠 sibling；
- `盘尾历史 9 扇区镜像` 和 `end-4 restore-node` 的真实 LBA、协议意义、备份/恢复用途不变；
- 不改变 `DEVICE_TAIL_WINDOW_SECTORS=2048`、`TAIL_METADATA_MIRROR_OFFSET_SECTORS=1024`、`TAIL_METADATA_MIRROR_SECTORS=9`、`TAIL_END4_MIRROR_OFFSET_SECTORS=4` 等已验证常量；
- UI 仍使用闭区间显示，内部继续使用 `[start, end_exclusive)`。

需要新增的 topology 不变量：

```text
对任何表示物理空间分割的 Materialized children：
  child[i].start_lba <= child[i+1].start_lba
  child[i].end_exclusive <= child[i+1].start_lba

对要求完整覆盖 parent 的 Region：
  first.start_lba == parent.start_lba
  child[i].end_exclusive == child[i+1].start_lba
  last.end_exclusive == parent.end_exclusive
```

盘尾专项回归至少覆盖：

1. 15,728,640-sector 示例严格得到上述五段顺序；
2. 9-sector mirror 前后 gap 边界无 off-by-one；
3. `end-4` 后仍保留最后 3 个 sector 的未分类区段；
4. 同级物理 children 无 overlap；
5. 所有 child 合集完整覆盖盘尾 parent；
6. `regions_for_lba()` 对镜像区/end-4 仍可同时返回 forensic tail membership，不因 Tree 去重丢失语义；
7. `gl` 跳转、搜索、lazy sector materialization 仍能精确落到对应物理 span；
8. 备份元数据中的 forensic-tail、historical mirror、restore-node evidence 不改变。

因此该问题应作为后续实现中的独立步骤加入：

```text
I2a  重构 tail topology：将重叠 sibling extents 归一化为按 LBA 连续的 primary spans，保留 secondary evidence membership
```

它属于 Inspect topology/UI 表达修正，不涉及任何盘面协议或写盘行为。

### 14.7 节点概览与节点详情重新分工：Overview 看结论，Detail 看证据

当前两个 Pane 的信息架构不清晰：Overview 主要重复 `节点/类型/范围/Sector count/大小/状态/Decoder` 这类结构元数据，而 Sector 的真正语义字段几乎全部堆进 Detail。结果是用户选中 LBA8 后，第一眼看不到“这个扇区最重要的信息是什么”；Detail 又只有 `字段 / 值 / 分组` 三列，缺少 offset、长度、原始表示、状态等取证维度。

目标原则：

```text
Overview = 回答“这是什么、是否正常、最重要的值是什么”
Detail   = 回答“所有字段在哪里、占多少字节、原始/解码值是什么、证据状态是什么”
Hex      = 回答“具体每个字节长什么样”
```

三层必须互补，不能重复堆同一批信息。

#### 14.7.1 Overview 改为语义摘要，不再以通用元数据为主体

Overview 首屏应优先容纳 3～6 个用户真正关心的语义结论，通用范围信息压成一行即可。

统一结构建议：

```text
┌ 节点概览 · LBA8 ─────────────────────────────┐
│ 设备身份与电子标签                           │
│ ✓ 已识别 · canonical LBA8 · A6B0 prefix      │
│ LBA 8 · 512 B · byte 0x1000..0x11FF          │
│                                               │
│ [身份]                                        │
│ UsbOnlyInfo      140225993400000000            │
│ HostHardinfo     0x00000000 · current-zero    │
│                                               │
│ [电子标签]                                    │
│ <从 ELABEL children 提取的关键键值，最多若干> │
│                                               │
│ [版本 / 写入]                                 │
│ Tool              01 00 00 01                 │
│ Lab               0x00000222                  │
│ writeTime         12345678                    │
│                                               │
│ [存储布局]                                    │
│ logical 0x... B · encrypted prefix ... B      │
│ raw tail ... B                                │
│                                               │
│ 无异常                                        │
└───────────────────────────────────────────────┘
```

这里的重点不是固定这些具体排版字符，而是建立**信息优先级**：

1. 第一行：用户可读的节点语义名，例如 `LBA8 · 设备身份与电子标签`，而不是只显示 `Sector`；
2. 第二行：解析状态 + canonical decoder/profile，一眼确认“能不能信”；
3. 第三行：LBA/范围/大小压缩成一行，不再占 5～7 行；
4. 中间：按该节点语义选出的关键字段；
5. 最后：异常、候选 profile、不确定性；正常的固定零/保留字段不要占 Overview 主体。

Overview 的一条重要规则：**正常且固定的协议校验字段默认折叠成状态，不逐项展示。** 例如 LBA8 的 `LLGB magic`、`MacInfo=0`、`UsbOnlyInfo suffix=0`、`reserved header=0`、`ELABEL offset=0x80` 都属于完整 Detail 的证据，但正常时不应挤占 Overview；只有异常时才提升为警告。

#### 14.7.2 LBA8 的建议摘要字段

LBA8 当前 canonical parser 已提供足够数据，可直接建立高价值摘要，不需要 TUI 猜协议。

一级摘要（默认始终展示）：

```text
语义             设备身份与电子标签
解析状态         已识别 / 未唯一确定 / 缺 device_id
UsbOnlyInfo      当前解析值 + profile
HostHardinfo     0x........ + source/profile
ELABEL           关键键值摘要，或“已解析 N 项”
```

二级摘要（空间足够时展示）：

```text
Tool version
Lab version
writeTime         只按当前已证实表示显示，不擅自转日期
logical length
A6B0 encrypted prefix length
raw tail length
```

仅异常时展示：

```text
magic 不匹配
UsbOnlyInfo profile 多候选
host-hardinfo profile 多候选
固定零字段出现非零
ELABEL terminator/offset 异常
其他 parser 拒绝原因
```

明确不应在 Overview 展开的内容：

```text
64B reserved header 的十六进制全文
UsbOnlyInfo suffix 的16B零
raw tail backing 全量 hex
encrypted backing 全量 hex
每个 ELABEL 子字段的完整列表
```

这些全部属于 Detail/Hex。

#### 14.7.3 Detail 改成真正的“字段证据表”

当前 Detail 只有：

```text
字段 | 值 | 分组
```

不足以承担“所有字段细节”的职责。目标表建议至少包含这些逻辑列：

```text
Offset | Len | Group | Field | Value | Raw/Stored | Type | Status
```

宽屏优先显示：

```text
Offset  Len   Field                    Value                  Status
0x000   4     LLGB magic               LLGB                   Known
0x004   4     logical length           ...                    Known
0x008   4     tool version             01 00 00 01            Known
0x00C   4     lab version              0x00000222             Known
0x010   4     writeTime                ...                    Known
0x014   4     HDSerialInfo/MyHardinfo  0x00000000             Known
0x018   6     MacInfo                  00 00 00 00 00 00      Known
0x01E   16    UsbOnlyInfo              ...                    Known
...
```

通过 `h/l` 横向列 viewport 继续访问：

```text
Group
Raw/Stored
Decoded/logical
Type
Status
Transform / provenance
```

这与 14.2～14.4 的多层 decode 计划统一：Detail 应明确区分 `stored/raw representation` 与 `logical value`，不能把两者塞进同一个“值”列。

#### 14.7.4 Detail 从“滚动表”升级为“可选字段表”

建议增加独立 `detail_selected_row`：

```text
j/k          选择上一/下一字段
Ctrl-u/d     半页移动并保持选择
PageUp/Down  整页
 gg/G        首/末字段
h/l          横向列 viewport
Enter        打开 Sector Inspector，并定位/Pin 到该 Field 起始 byte
o            展开/折叠当前字段 children（如 ELABEL 子项）
y            复制 semantic value
Y            复制 raw/stored bytes
```

这样 Detail 不只是“能滚到底”，而是一个真正可导航的字段检查器。选中行使用统一 selection token，不能只靠滚动位置猜当前字段。

对于 `ELABEL` 这类有 children 的字段：

```text
▸ ELABEL
```

按 `o` 后：

```text
▾ ELABEL
    KeyA    ValueA
    KeyB    ValueB
    ...
```

默认折叠，避免几十个子项淹没其它字段。

#### 14.7.5 引入 UI-neutral 的 Summary 模型，禁止 renderer 按 LBA 写 if/else

不能在 `render.rs` 中写：

```text
if lba == 8 { 显示 UsbOnlyInfo ... }
```

建议 application/inspect 层新增只读摘要 DTO，名称可实现时调整：

```rust
InspectNodeSummary {
    title,
    subtitle,
    status,
    location,
    sections: Vec<SummarySection>,
    alerts: Vec<SummaryAlert>,
}

SummarySection {
    title,
    items: Vec<SummaryItem>,
}

SummaryItem {
    label,
    value,
    importance,
    source_range,
    status,
}
```

由 canonical parser / inspect adapter 决定哪些字段是摘要字段；TUI renderer 只负责排版、颜色和 viewport。

`importance` 至少可区分：

```text
Primary    首屏关键语义
Secondary  空间足够时展示
Diagnostic 只在异常或显式展开时展示
```

这样未来 LBA0～12、LCE、Partition、Region 都能各自提供摘要，但仍共享同一个 Overview renderer。

#### 14.7.6 各节点类型的 Overview 语义

不要只给 LBA8 特判，最终统一为：

```text
Device:
  型号/容量/设备身份/当前盘型/协议识别状态

Region / Extent:
  语义角色/范围/大小/覆盖状态/关键子区域

Partition:
  type/起点/大小/文件系统/NeedEncrypt/物理明密文状态/FileKey 状态

Sector:
  本 LBA 的协议语义 + 关键字段 + decoder/profile + 异常

Field:
  字段语义值/范围/类型/状态/变换摘要

UnknownRange:
  范围/大小/为何未知/已有证据，禁止伪造摘要
```

#### 14.7.7 Overview 与 Detail 的视觉规则

- Overview 使用“短 label + 对齐 value”的 key/value 版式，不用长篇 prose；
- 每个 section 之间空一行或轻分隔，不使用大量高饱和颜色；
- `Primary` 值用普通前景或轻 accent，状态/警告才使用 success/warning/error；
- 固定正常值不反复绿色高亮，避免满屏“正常”造成噪音；
- Detail 是密集表格，颜色只表达 status/type/selection，不给每列随机配色；
- 长 value 必须截断并允许横向 viewport/Enter 深入，禁止把整行撑爆；
- Overview/Detail 都继续拥有独立 vertical viewport，宽/窄屏状态不丢失。

#### 14.7.8 回归门禁

至少新增：

1. 选中 LBA8 时 Overview 首屏必须出现 UsbOnlyInfo、HostHardinfo、解析/profile 状态；
2. LBA8 正常固定零 reserved 字段不占 Overview 主体；
3. profile 多候选或 parser 异常必须进入 Overview alerts；
4. Overview 不复制完整 raw backing；
5. Detail 必须包含所有 canonical `InspectField`，数量与模型一致；
6. Detail 每个字段都能看到 offset + length；
7. 有 stored/logical 两层表示的字段可分别查看；
8. `j/k` 能选择所有 Detail 行且不会改变 Tree selection；
9. `Enter` 从 Detail 选中字段精确进入 Sector Inspector 对应 byte；
10. children 展开/折叠不会丢失父字段选择；
11. resize 后 Overview scroll、Detail selection/scroll/column viewport 均保留；
12. Overview 摘要生成位于 inspect/application 层，源码门禁禁止 `render.rs` 出现 LBA-specific 业务判断；
13. LBA0～12/LCE canonical parser 与 golden tests 不改变。

后续实现顺序增加：

```text
I2b  建 InspectNodeSummary / SummarySection / SummaryItem，只读语义摘要模型
I2c  先为 LBA8 建 summary adapter 与失败测试，再覆盖其它 LBA/Partition/Region
I3a  Overview renderer 改为统一 sectioned key/value dashboard
I3b  Detail 增加 offset/len/stored/logical/type/status/provenance 列
I3c  Detail 增加 selected row、Field→Hex、children 折叠
```

最终标准：**Overview 让用户 3 秒内看懂当前节点最重要的信息；Detail 能完整回答每个字段在哪里、是什么、原始字节如何、如何解码、证据状态如何；Hex 再负责逐字节核验。**
