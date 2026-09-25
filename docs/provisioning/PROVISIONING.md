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

1. Phase 8：真实 USB 验收；
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
│ o 展开/折叠  Enter查看  g 跳转  / 搜索  Tab面板  ? 帮助  q返回             │
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

g              跳转 LBA / offset
/              搜索

PageUp         上一个 sector
PageDown       下一个 sector
Home           当前 region 开头
End            当前 region 末尾

← / →          Hex byte 光标 / 输入框文本光标
r              Raw / Decode / Mixed
?              当前上下文快捷键帮助
Esc / q        返回
```

底部状态栏不一次性列出所有快捷键，只显示当前上下文最有用的一组，例如：

```text
o 展开  Enter查看  g 跳转  / 搜索  Tab面板  ? 帮助
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

- Phase I1 审计已完成：CLI 已支持任意 `u64` LBA/range/count，TUI Advanced 也具备任意合法 LBA 读取能力；确认历史主要双轨来自旧 `InspectWorkspace` 固定 LBA0～12 路径，以及 CLI/TUI 各自维护的 protocol/non-protocol decode 与 meta 决策。
- Phase I2 已完成：application 层统一 `SectorReader` / checked range reader、`Protocol / LCE / Partition` decoder registry、`decode_sector()` / `sector_meta_text()`；CLI `raw/decode/meta` 已删除自己的 EDPB/物理盘 reader、partition boot 读取、decode/meta/export 循环，只负责来源选择、参数转换和渲染统一 `AdvancedInspectWorkspace`。旧 `InspectWorkspace` 已删除，普通 Inspect 与 Advanced Inspect 共用同一 backend。
- 统一 Field 模型已落地：协议 parser 原有字段语义不改，由 application materialize 为绝对 `[start,end_exclusive)` byte range、Field type、raw bytes、decoded bytes、status、label/value/group/children；range 模型从第一版即可无损表达跨 sector 字段，unknown/reserved/preserved 状态枚举已预留，当前已验证协议字段标记为 Known。
- Phase I3 已完成模型层：新增 UI-neutral 的 `InspectTopology / InspectNode / InspectNodeRange / InspectChildren`，形成 Device → Region → Extent → lazy Sector → Field 结构；LBA0～12、LCE、各 partition/data、盘尾取证窗口、盘尾 9-sector mirror、end-4 restore-node 与 unknown complement 均可表达。LBA0 sector stub 显式包含 MBR partition-table Structure；任意 LBA 可映射到一个或多个所属 Region。
- 大 Region/Partition/unknown range 不生成全量 Sector vector，只保存 `LazySectors { start_lba, sector_count }`，按 offset+limit materialize sector page；tail mirror/end-4 嵌套在 tail region 内，避免顶层重复区域。拓扑复用 EDPB 的 `SemanticStatus`，但不把 TUI 展开/焦点状态塞入备份领域模型。
- `AdvancedInspectWorkspace` 已直接携带同源 topology，因此 CLI/TUI 后续不需要重新推导磁盘区域。I3 定向门禁：tree 5/5、application inspect 11/11、CLI 6/6、TUI workspace 5/5、TUI lifecycle 13/13，`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I4 已完成：旧“高级检查参数表单 → 平铺 LBA 结果”双轨已删除，选择物理盘/EDPB 后直接后台建立 topology 并进入统一 Browser。宽屏采用 Tree / Overview / Detail 三栏，中等宽度为左树 + 右侧上下两栏，窄屏复用同一 Browser 状态纵向排列，不维护第二套窄屏状态机。
- 树交互已落地：`j/k` 与上下键移动，`o` 展开/折叠，Enter 查看/进入，Tab 正向 Tree→Overview→Detail、Shift+Tab 反向切换，Esc 返回；selection 通过 `visible_window()` 始终保持在可视窗口内。旧 `AdvancedInspectStage::Form/Result`、`AdvancedInspectForm`、旧 result navigation 与参数表单文案均已清零。
- viewport virtualization 已落地：每个 lazy extent 独立维护 sector window offset，每页最多 materialize 64 个 Sector，并通过“上一页/下一页”控制行翻窗；200-sector 回归样本验证第一页仅 LBA2048..2111、第二页仅 LBA2112..2175，旧页节点立即退出树，树规模不随分区总容量线性增长。I4 定向门禁：TUI lifecycle 13/13、TUI state 38/38、Inspect workspace 5/5、Inspect scroll 3/3，`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I5 已完成：Sector 行 Enter 后通过独立 generation + single-flight worker 按需读取，不把 I/O 放进 TUI state；同一 application backend 新增 Sector Inspector 专用 fail-soft decode，decoder 不适用时仍返回 512B raw 并携带 `decode_error`，CLI `decode` 仍保持严格 fail-closed。
- Sector Inspector 固定逻辑视图为 32×16B Hex + ASCII，显示 sector-relative offset 与 disk-absolute byte offset，支持 byte cursor（←/→ ±1B、j/k 或 ↑/↓ ±16B）、PageUp/PageDown 跨 Sector且保留 cursor、`r/d/m` 直接切 Raw/Decode/Mixed；小终端仅滚动可视行，不改变 32×16 数据模型。
- 当前 byte 会映射同源 `InspectField`，详情显示 typed value、field type/status、绝对 byte range；无已知 field 时明确显示 Unknown，不推测语义。按 `o` 展开当前 byte bit 与已验证 Field child。metadata LBA0～12 常驻，非 metadata 按需缓存最多 5 个 Sector，避免全盘浏览退化为无界内存增长。
- I5 专项门禁：application Inspect 12/12、CLI Inspect 6/6、Sector Inspector 2/2、TUI lifecycle 13/13、TUI state 39/39 通过；完整 `cargo fmt --all -- --check`、`cargo check --all-targets`、`cargo test --all-targets`、`git diff --check` 均通过。下一步进入 Phase I6：完成 Hex ↔ Field 双向定位、多行 range、Unknown/Reserved/Preserved 可视语义和 copy/yank。
- Phase I6 已完成：Hex byte → Field 继续直接使用 canonical `InspectField` 绝对 byte range 反查；树中 Field → Hex 新增反向入口，Enter Field 后直接打开 Sector Inspector、跳到字段起始 byte，并 pin 当前 Field。完整 Field range 使用绝对 byte range 高亮，可跨 16B 行；跨 sector Field 通过 PageUp/PageDown 保留 pin 并在相邻 sector 定位到 range 交集起点，手动移动 byte 后自动退出 pinned Field，恢复 cursor-driven 反查。
- `Known / Unknown / Reserved / Preserved` 保持独立状态并使用现有 Theme semantic style 渲染，不引入 Inspect 私有硬编码颜色；Unknown byte 继续明确显示未分类，不自动降级为 padding/Reserved。`y` 使用 TUI 内部 yank register 复制当前 Field 语义值或当前 byte，`Y` 复制当前 Field raw range；该 register 跨平台、可测试，不在 TUI state 中引入 `pbcopy`/`xclip` 等平台命令依赖。
- I6 专项门禁：Sector Inspector 4/4 通过，覆盖 Field→Hex、Hex→Field、跨行/跨 sector range、Unknown/Reserved/Preserved、yank register；完整 `cargo test --all-targets` 通过（最终长时 Runner Job `exit_code=0`），`cargo fmt --all -- --check`、`cargo check --all-targets` 与 `git diff --check` 通过。
- Phase I7 已完成：Browser 中 `g` 打开统一 Jump 输入，支持十进制/0x 十六进制 LBA 与绝对 byte offset，并使用 checked arithmetic 将 offset 无损映射为 LBA + sector-relative byte；越界、非法和 u64 overflow 均 fail closed，不做 silent clamp。Jump 命中 lazy extent 时仅切换该 extent 的 64-sector window、展开必要祖先并选中目标 Sector；byte offset 继续复用现有 Sector worker，Inspector cursor 精确落到目标 byte，event loop/state 不直接执行磁盘 I/O。
- `/` 结构化搜索已完成：搜索 canonical topology 的 Region / Extent / Structure / Group / Field label，以及当前已知/cached `InspectField.value`；纯 topology 路径查找和 cached sector 结构匹配位于 `application::inspect_tree`，TUI 不复制 decoder/parser。命中会自动展开 Tree 路径并定位目标；Field 命中可直接继续既有 Field→Hex。搜索不会为大分区 materialize 全量 Sector，也不会执行同步 full-disk raw scan；raw pattern search 仍明确留在第二阶段。
- I7 已完成全部门禁：Sector Inspector 9/9（含 LBA jump、hex LBA、absolute byte offset 精确 cursor、lazy extent 自动翻页、invalid/overflow/out-of-range、Field label/typed value 搜索、40×10/80×24/120×36 prompt 渲染与 cache/virtualization 不退化）、TUI state 39/39、TUI lifecycle 13/13、Inspect tree 5/5、documentation layout 5/5 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、680/680 tests passed、0 failed。下一步进入 Phase I8：视觉统一与窄屏收口。
- Phase I8 已完成实现：全局渲染入口新增 `ThemeToken` 语义层，原有 accent/secondary/success/warning/error/muted/selection 全部经统一 token 映射；Advanced Inspect 区域不再直接引用 `Color::*`。Unknown/Reserved 使用 muted 语义，Preserved 使用 success，当前 byte/field/panel selection 继续使用统一 selection token。
- Advanced Inspect 新增与其它页面一致的 `结构树 / 节点概览 / 节点详情` Tabs；`Tab/Shift+Tab` 继续只切换既有 `AdvancedInspectPanel` 状态。Tree 的 `▶` 仅表示当前键盘焦点且全屏唯一，展开/折叠改用 `−/+`，避免与 focus marker 混淆；selected 背景只覆盖箭头和实际 row 内容，不涂满到 panel padding/border。
- I8 窄屏采用同状态单面板降级：宽屏仍为三栏，中屏仍为左树 + 右侧上下两栏；当宽度 <92 或有效高度 <14 时只渲染当前 active panel，Tree selection、detail scroll、Sector byte cursor 都不被响应式布局改写。Sector Inspector 在 60×18 下仍会自动滚到 cursor 所在 16B 行，并保留精确 byte selection。
- I8 已完成全部门禁：TUI contract 8/8、TUI lifecycle 13/13、Sector Inspector 10/10、Backup workspace 5/5、旧 Inspect workspace 5/5、Search/Command 4/4、documentation layout 5/5 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets`、`git diff --check` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、682/682 tests passed、0 failed。下一步进入 Phase I9：回归与真实盘只读验收。
- Phase I9 自动验收已完成：新增 `inspect_full_disk_acceptance`，用正式 `generate_official_image()` 生成 mode0～mode3 四种官方协议形态，逐一验证 LBA12 logical partitions 与 LCE 同时进入统一 full-disk topology，且 LBA0 MBR 对同一官方分区的可见映射不会再生成重复 Region；4,000,000,000-sector sparse disk 的高 LBA Jump 仍只 materialize 至多 64 个 Sector，Unknown decode 保持 fail-closed。
- I9 审计补齐 Plain 盘拓扑：LBA0 中签名有效、范围合法且未被 LBA12 同范围覆盖的 MBR primary entry 现在以 `MBR Pn` lazy Partition Region 进入全盘树；decoder 保持 `None`，因此普通分区 raw 可达但不会猜测 EDP 加密语义。越界 MBR entry 直接忽略，不 silently clip；与 EDP partition 同范围时不重复。对应 Inspect tree 门禁 8/8 通过。
- I9 只读边界新增 application 门禁：`SectorReader` audit fixture 证明 Raw Inspect 只读取显式请求的 sector，不触发额外数据区读取，更没有写接口；application Inspect advanced tests 8/8、full-disk acceptance 3/3、TUI state 39/39、Sector Inspector 10/10 均通过；`cargo fmt --all -- --check`、`cargo check --all-targets`、`git diff --check` 通过，长时 `cargo test --all-targets` 最终 `exit_code=0`、689/689 tests passed、0 failed。
- 真实盘只读验收状态：2026-09-25 检测到 `/dev/disk4`，8.1GB 外置物理 USB，`disk4s1` 当前挂载于 `/Volumes/启动区`。整个审计未执行 mount/unmount/eject/provision/write。当前 Runner 对 raw device 没有读取权限，`sudo -n true` 明确返回 `password is required`；因此真实盘 raw Inspect 与前后 sector hash 对比尚未执行，Phase I9 不得标记为全部完成。待取得管理员只读授权后，只运行 `inspect raw/meta/decode` 和只读前后哈希，且保持现有 mount 状态不变。
