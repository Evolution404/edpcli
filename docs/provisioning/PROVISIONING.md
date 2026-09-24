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

## 4. 当前实现 B：已有官方盘转换为“启动区和交换区二合一”

这条路线与“新盘制盘”必须使用不同接口/计划，核心原则：

**保留 -> 变换，禁止从模板整体重建。**

转换前必须只读保存并解析：

- LBA0-LBA12 原始 6656B；
- `device_id` / VID / PID / 容量 / `onlyid`；
- LBA3 制造商自有不透明字节；
- LBA4 表示/`HSerial`；
- LBA6 SAFE6 保留底层字节、Dept/User、快照配置类型；
- LBA8 ELABEL/身份/保留底层字节；
- LBA7/LBA12 EDPF 与 PassInfo；
- `UserKeyCRC` / `FileKeyCRC` / 封装文件密钥 / `EncryptMode`；
- type2/type4 几何；
- type4 文件系统几何与必要元数据。

### 4.1 第一版转换的不变量

默认转换不得移动或重新加密 type4：

```text
old type4 StartSector == new type4 StartSector
old type4 PartionSize == new type4 PartionSize
old 封装文件密钥  == new 封装文件密钥
old FileKeyCRC        == new FileKeyCRC
old UserKeyCRC        == new UserKeyCRC
```

新的前部 type2 几何由保留的 type4 起点反推：

```text
new type2 StartSector = 63
new type2 SectorCount = old type4 StartSector - 63
```

### 4.2 前部文件系统不能只改元数据

普通三分区盘的 type1 + type2 不会因为改 LBA0/LBA7/LBA12 就自动成为一个合法的大 type2 文件系统。

因此转换必须先判断：

- **仅改元数据**：只有现有前部文件系统几何已满足目标，或有严格证明可以安全扩展时才允许；
- **重建前部区域**：默认安全路线，重建 LBA63..`old_type4_start-1` 的非保密文件系统，可选恢复用户文件；
- 第一版禁止未经证明的原地移动/扩容。

### 4.3 最小类型化差异

转换应由类型化差异推导允许改写的字段，不能硬编码“固定写某几个扇区”。预期主要涉及 LBA0、LBA7、LBA12，以及必要时同步 LBA6 旧版 MBR 快照；其余字段默认原样保留。

若实际差异出现未授权字段变化，必须无法确认即拒绝继续。

当前 `build_passwordless_conversion()` 已实现上述严格纯内存变换：从源 LBA12/LBA7 解析 type4，保持 type4 的起点、大小、`UserKeyCRC`、`FileKeyCRC`、封装文件密钥和 `EncryptMode` 不变；新的 type2 从 LBA63 延伸到原 type4 起点，并复用同一组密钥材料；LBA0 改为 `0x07` 单可见分区；前部区域重建为空的明文 exFAT。转换产物只包含 LBA0、LBA7、LBA12 和前部稀疏文件系统，不生成 LBA6/LBA9 补丁，因此旧 `apply` 中历史兼容改写不属于这条正式转换路径。错误 `device_id`、缺少 type4、已经是 type2+type4 的盘均直接拒绝。

`atomic_write_passwordless_conversion_sectors()` 已接入 `edpcli provision convert`：它只允许 LBA0/LBA7/LBA12 和 `[63, old_type4_start)` 的前部稀疏扇区，任何触碰 type4 或夹带其他元数据都会在第一笔写入前拒绝。正写顺序固定为“前部文件系统 -> LBA7 -> LBA12 -> LBA0”，失败时以前部和元数据共同的写前镜像回滚并再次读回校验。写入前继续创建现有元数据级 EDPB；当前版本不迁移前部用户文件。旧 `atomic_write_sectors()` 的 LBA0-12 边界保持不变。

## 5. 当前产品入口

CLI 当前提供：

```text
edpcli provision plan
edpcli provision image
edpcli provision write
edpcli provision convert
```

`plan` 只检查目标和布局；`image` 导出与目标硬件身份、容量及原始 LBA3 绑定的最终稀疏镜像，并包含所有已勾选分区的预生成 FAT16/exFAT 格式化扇区；`write` 消费同一份预生成计划执行四模式新盘制盘；`convert` 默认只预览，加 `--write` 后执行严格免密改造。TUI 已提供四模式制盘向导、逐分区格式化选择、文件系统/卷标选择和写前复核。

真实写盘复用应用层系统盘保护、USB 整盘门禁、目标固定、卸载/锁定、重新打开后身份复核、同步/读回、回滚；现有盘转换额外执行写前元数据级备份。`src/provision/` 继续保持纯领域层，不自行打开裸设备。

## 6. 后续扩展顺序

1. 为 `ntfs` / `fat32` 增加跨平台可验证写入实现，开放前保持明确拒绝；
2. 增加旧代 LBA4/LBA6/LBA8 配置类型的显式选择，而不是按软件版本猜测；
3. 为严格免密转换增加前部用户文件迁移，并继续保证 type4 不移动、不重加密；
4. 持续在专用物理设备上扩充 TUI 制盘与逐分区格式化的 HIL 回归；
5. 遇到非已验证 255×63 USB 几何时，增加来自平台真实几何或显式受验证参数的 LCE 路线，不静默猜测。

## 7. 完成门禁

四模式功能只有同时满足以下条件才可从路线图移到“当前已实现”：

- 对应模式的一方写入端证据已进入测试夹具；
- 生成镜像可被现有类型化解析器/`ProtocolImageView` 反向解析；
- 跨 LBA 不变量全部通过；
- LCE 几何符合官方写入端；
- 文件系统几何/可见性与模式一致；
- `cargo check --all-targets`、定向测试、完整 `cargo test` 全绿；
- 真实设备写入路径继续保留现有无法确认即拒绝继续安全链。
