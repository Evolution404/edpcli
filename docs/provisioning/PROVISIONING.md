# 制盘：当前实现与官方四模式路线图

本文是制盘功能的唯一长期规划文档。它明确区分**当前已实现能力**与**未来必须实现的路线图**；历史阶段计划已删除，后续不得再创建平行的制盘计划作为第二事实源。

## 1. 当前已经实现的能力

当前 `src/provision/` 是纯领域、纯内存能力：

`TargetIdentity + ProvisionMetadata + ProvisionProfile + ProvisionEntropy -> ProvisionImage(6656B) -> ProvisionValidator`

代码位于：

- `src/provision/spec.rs`
- `src/provision/profile.rs`
- `src/provision/generate.rs`
- `src/provision/validate.rs`

该模块禁止打开设备、执行系统命令或提权。硬件发现和真实写盘属于应用层/平台层。

当前 `ProvisionProfile::canonical_v1()` 的配置类型 ID 为 `jiangsu-safe6-nopwd`。当前构造器生成 LBA0、4、6、7、8、11、12，并由 `ProvisionValidator` 做离线一致性验证。

现有 `generate_image()` 继续保留标准 v1 二合一兼容输出；新增 `generate_official_image()` + `OfficialProvisionPlan` 已能纯内存生成四种官方分区模式的 LBA0/LBA7/LBA12，并由独立 `OfficialProvisionValidator` 反向校验 MBR、EDPF 类型/标志、逻辑几何与 LCE。LBA12 当前封装密钥轴也已独立实现为 `ProvisionKeyMaterial`：mode1=A7F0、mode2=SM4-ECB、mode3=AES-128-ECB，三种输出均与官方虚拟写入端逐字节夹具一致；`0000aaaa` 的 v0x0206 隐式有效密码替换也已编码。

**当前构造器不是完整的官方四模式制盘器。** 目前仍只覆盖元数据阶段；LBA7 旧版 8B 封装密钥生成、文件系统阶段和真实设备产品入口仍需完成。

## 2. 已闭环的官方协议事实

协议层已经通过官方写入端/消费端闭环以下事实，未来制盘功能必须直接复用，不得重新猜测：

| 模式 | 官方名称 | `PartionType[]` |
| ---: | --- | --- |
| 0 | 缺省三分区 | `[1,2,4]` |
| 1 | 启动区和交换区二合一 | `[2,4]` |
| 2 | 整盘加密 | `[1,4]` |
| 3 | 内外网通用双分区 | `[1,2]` |

`PartionType`：`1=Boot`、`2=Share`、`4=Encrypt`。

当前一方 `cemsusbregsiter.dll::sub_10041A80` 的 MBR 类型选择分支也已逐指令闭合：模式0→`0x0E`、模式1→`0x07`、模式2→`0x0B`、模式3→`0x0E`。模式1命中“type2 位于 entry0”分支；模式2命中“type1 位于 entry0 且 type4 位于 entry1”分支。该映射已编码为 `official_mbr_partition_type`，不得按文件系统名称自行猜测。

同一一方写入端 `sub_10046D20` 还逐分区类型固定了 `NeedEncrypt`：type1=`0`、type2=`1`、type4=`1`；`CreatePartitions` 的位置规则为 entry0/entry1 的 `NeedDisturb=1`、entry2=`0`。四模式生成器必须按这两条写入端规则序列化，不能沿用旧二合一生成器“所有条目均为1”的简化值。

真实 Netac 当前原盘进一步确认：LBA12 中 type1/`NeedEncrypt=0` 条目的 `UserKeyCRC`、`FileKeyCRC`、16B 封装文件密钥和 `EncryptMode` 全部为0；type2/type4 才写密码/文件密钥材料。当前构造器和独立校验器已按该规则实现“无法确认即拒绝继续”。

LBA7 旧表中条目0 与后续条目的几何规则不同；后续条目可保留各自 `PartionType` 并共同指向 LCE（LBA7 兼容扩展区）。因此制盘功能不得把 LCE 当成 type4 专属区域。详细证据见 [`../protocol/LCE.md`](../protocol/LCE.md)。

## 3. 路线图 A：完整复刻官方四模式新盘制盘

长期目标是把当前单一 `canonical_v1` 拆成正交配置，而不是增加四份复制粘贴模板。

### 3.1 正交轴

至少拆分以下轴：

1. `OfficialPartitionMode`：0/1/2/3 四种官方布局；
2. LBA12 封装文件密钥 / `EncryptMode` 配置类型；
3. 旧版/当前 LBA4 表示配置类型；
4. LBA6 Dept 续段/快照配置类型；
5. LBA8 身份/保留底层字节配置类型；
6. GPT/MBR 配置类型；
7. 写入端代际/兼容版本来源。

软件版本只作为来源信息，不直接代替盘面配置类型。

### 3.2 四模式构造器目标

未来构造器必须从同一 `ProvisionSpec`/类型化协议模型构造四种模式，而不是从供体盘复制元数据。

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

官方“整盘加密”写入端仍生成 `[1,4]`，其中 type1 被强制为 `0x7E00` 字节的兼容/保留几何。未来构造器必须精确复刻该写入端行为，不能把界面描述“只含保密区”简化成单 `[4]` 条目。

## 4. 路线图 B：已有官方盘转换为“启动区和交换区二合一”

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

## 5. 路线图 C：产品入口

四模式领域模型/构造器/校验器完成后，再实现产品入口。目标接口可以包括：

```text
edpcli provision plan
edpcli provision image
edpcli provision verify
edpcli provision write
```

以及 TUI 制盘向导。

这些目前属于未来路线图，不得在用户文档中描述为已发布能力。

真实写盘必须复用应用层写入服务的系统盘保护、USB 整盘门禁、目标固定、写前备份、卸载/锁定、重新打开后身份复核、同步/读回、回滚；不得在 `provision` 模块自行实现第二套裸盘写入路径。

## 6. 实现顺序

1. 把当前 `canonical_v1` 配置类型拆成可组合的类型化轴，同时保持现有输出回归不变；
2. 为四种 `OfficialPartitionMode` 实现纯内存构造器；
3. 用一方写入端/物理测试夹具做逐字节差异测试；
4. 完整实现封装密钥/配置类型矩阵；
5. 实现新盘制盘文件系统阶段；
6. 实现 `ConversionPlan` 的只读分析；
7. 实现前部区域重建 + 最小元数据变换；
8. 增加回滚/读回/虚拟磁盘硬件在环；
9. 最后才接 CLI/TUI 和显式真实设备写入。

## 7. 完成门禁

四模式功能只有同时满足以下条件才可从路线图移到“当前已实现”：

- 对应模式的一方写入端证据已进入测试夹具；
- 生成镜像可被现有类型化解析器/`ProtocolImageView` 反向解析；
- 跨 LBA 不变量全部通过；
- LCE 几何符合官方写入端；
- 文件系统几何/可见性与模式一致；
- `cargo check --all-targets`、定向测试、完整 `cargo test` 全绿；
- 真实设备写入路径继续保留现有无法确认即拒绝继续安全链。
