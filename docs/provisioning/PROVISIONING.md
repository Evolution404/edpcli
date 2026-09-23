# Provisioning：当前实现与官方四模式路线图

本文是 provisioning 的唯一长期规划文档。它明确区分 **CURRENT（已实现）** 与 **ROADMAP（未来必须实现）**；历史阶段计划已删除，后续不得再创建平行的 provision plan 作为第二事实源。

## 1. CURRENT：当前已经实现的能力

当前 `src/provision/` 是纯领域、纯内存能力：

`TargetIdentity + ProvisionMetadata + ProvisionProfile + ProvisionEntropy -> ProvisionImage(6656B) -> ProvisionValidator`

代码位于：

- `src/provision/spec.rs`
- `src/provision/profile.rs`
- `src/provision/generate.rs`
- `src/provision/validate.rs`

该模块禁止打开设备、执行系统命令或提权。硬件发现和真实写盘属于 application/platform 层。

当前 `ProvisionProfile::canonical_v1()` 的 profile id 为 `jiangsu-safe6-nopwd`。当前 builder 生成 LBA0、4、6、7、8、11、12，并由 validator 做离线一致性验证。

**当前 builder 不是完整的官方四模式制盘器。** 它只代表一条已经编码和测试的 canonical v1 路径。

## 2. 已闭环的官方协议事实

协议层已经从官方 producer/consumer 闭环以下事实，未来 provisioning 必须直接复用，不得重新猜测：

| mode | 官方名称 | `PartionType[]` |
| ---: | --- | --- |
| 0 | 缺省三分区 | `[1,2,4]` |
| 1 | 启动区和交换区二合一 | `[2,4]` |
| 2 | 整盘加密 | `[1,4]` |
| 3 | 内外网通用双分区 | `[1,2]` |

`PartionType`：`1=Boot`、`2=Share`、`4=Encrypt`。

LBA7 old table 中 entry0 与后续 entry 的 geometry 规则不同；后续 entry 可保留各自 `PartionType` 并共同指向 LCE（LBA7 Compatibility Extent）。因此 provisioning 不得把 LCE 当成 type4 专属区域。详细证据见 [`../protocol/LCE.md`](../protocol/LCE.md)。

## 3. ROADMAP A：完整复刻官方四模式新盘制盘

长期目标是把当前单一 `canonical_v1` 拆成正交配置，而不是增加四份复制粘贴模板。

### 3.1 正交轴

至少拆分以下轴：

1. `OfficialPartitionMode`：0/1/2/3 四种官方布局；
2. LBA12 wrapped-file-key / `EncryptMode` profile；
3. legacy/current LBA4 表示 profile；
4. LBA6 Dept continuation / snapshot profile；
5. LBA8 identity/backing profile；
6. GPT/MBR profile；
7. producer generation / compatibility version provenance。

软件版本只作为 provenance，不直接代替 wire profile。

### 3.2 四模式 builder 目标

未来 builder 必须从同一 `ProvisionSpec`/typed protocol model 构造四种模式，而不是从 donor 盘复制元数据。

每种模式都必须验证：

- LBA0 partition table；
- LBA7 old `EDP_PARTION_INFO`；
- LBA12 current `tagNewEdpPartionInfo`；
- LCE pointer/geometry；
- type1/type2/type4 logical geometry；
- UserKeyCRC/FileKeyCRC/wrapped file key/EncryptMode；
- LBA6/LBA8/LBA11 cross-LBA identity invariant；
- 文件系统可见性和挂载预期。

### 3.3 mode 2 特殊规则

官方“整盘加密”producer 仍生成 `[1,4]`，其中 type1 被强制为 `0x7E00` bytes 的兼容/保留几何。未来 builder 必须精确复刻该 producer 行为，不能把 UI 描述“只含保密区”简化成单 `[4]` entry。

## 4. ROADMAP B：已有官方盘转换为“启动区和交换区二合一”

这条路线与“新盘制盘”必须使用不同 API/plan，核心原则：

**Preserve -> Transform，禁止 Rebuild-from-template。**

转换前必须只读保存并解析：

- LBA0–12 原始 6656B；
- device_id / VID / PID / capacity / onlyid；
- LBA3 manufacturer-owned opaque bytes；
- LBA4 representation/HSerial；
- LBA6 SAFE6 backing、Dept/User、snapshot profile；
- LBA8 ELABEL/identity/backing；
- LBA7/LBA12 EDPF 与 PassInfo；
- UserKeyCRC / FileKeyCRC / wrapped file key / EncryptMode；
- type2/type4 geometry；
- type4 文件系统 geometry 与必要 metadata。

### 4.1 第一版转换的不变量

默认转换不得移动或重新加密 type4：

```text
old type4 StartSector == new type4 StartSector
old type4 PartionSize == new type4 PartionSize
old wrapped file key  == new wrapped file key
old FileKeyCRC        == new FileKeyCRC
old UserKeyCRC        == new UserKeyCRC
```

新的前部 type2 geometry 由保留的 type4 起点反推：

```text
new type2 StartSector = 63
new type2 SectorCount = old type4 StartSector - 63
```

### 4.2 前部文件系统不能只改 metadata

普通三分区盘的 type1 + type2 不会因为改 LBA0/LBA7/LBA12 就自动成为一个合法的大 type2 文件系统。

因此转换必须先判断：

- **metadata-only**：只有现有前部文件系统 geometry 已满足目标，或有严格证明可以安全扩展时才允许；
- **front-region rebuild**：默认安全路线，重建 LBA63..old_type4_start-1 的非保密文件系统，可选恢复用户文件；
- 第一版禁止未经证明的 in-place move/grow。

### 4.3 最小 typed diff

转换应由 typed diff 推导允许改写的字段，不能硬编码“固定写某几个扇区”。预期主要涉及 LBA0、LBA7、LBA12，以及必要时同步 LBA6 legacy MBR snapshot；其余字段默认 preserve。

若实际 diff 出现未授权字段变化，必须 fail-closed。

## 5. ROADMAP C：产品入口

四模式 domain/builder/validator 完成后，再实现产品入口。目标接口可以包括：

```text
edpcli provision plan
edpcli provision image
edpcli provision verify
edpcli provision write
```

以及 TUI Provision Wizard。

这些目前属于 ROADMAP，不得在用户文档中描述为已发布能力。

真实写盘必须复用 application write service 的系统盘保护、USB whole-disk guard、selector pinning、写前备份、unmount/lock、reopen 身份复核、sync/readback、rollback；不得在 provision 模块自行实现第二套 raw-write 路径。

## 6. 实现顺序

1. 把当前 `canonical_v1` profile 拆成可组合的 typed axes，同时保持现有输出回归不变；
2. 为四种 `OfficialPartitionMode` 实现纯内存 builder；
3. 用 first-party producer/physical fixture 做 byte-level differential tests；
4. 完整实现 wrapped-key/profile 矩阵；
5. 实现 fresh-provision filesystem stage；
6. 实现 `ConversionPlan` 的只读分析；
7. 实现 front-region rebuild + minimal metadata transform；
8. 增加 rollback/readback/virtual-disk HIL；
9. 最后才接 CLI/TUI 和显式真实设备写入。

## 7. 完成门禁

四模式功能只有同时满足以下条件才可从 ROADMAP 移到 CURRENT：

- official producer evidence 对应模式已进入测试 fixture；
- generated image 可被现有 typed parser/`ProtocolImageView` 反向解析；
- cross-LBA invariants 全部通过；
- LCE geometry 符合官方 producer；
- 文件系统 geometry/visibility 与模式一致；
- `cargo check --all-targets`、定向测试、完整 `cargo test` 全绿；
- 真实设备写入路径继续保留现有 fail-closed 安全链。
