# Provision / LCE / 备份工作区治理方案（2026-10-01）

> 状态：**已完成 / CLOSED**
>
> 关键实现提交：`6070f05`、`2aa0c5c`、`e3ab720`。
>
> 最终 Virtual Disk / FAT / 实体 USB / LCE HIL 证据见：`audit/protocol/notes/provision_lce_fat_hil_2026-10-01.md`。
>
> 下文“当前问题 / 建议实现”保留为计划形成时的历史设计依据，不再表示这些项目仍待实现。

## 1. 基线与目的

本计划最初基于当时本机工作区只读审计结果：

```text
branch: main
HEAD: e3de6f22f029e6a632a72d474a0305e4c558369d
summary: fix(provision): close Plain/EDP planning matrix gaps
workspace: clean
```

本文件是下一轮实现的统一执行计划，只定义目标、边界、数据模型、交互合同、验证门禁和实施顺序。写入本文件时不修改业务代码、不提交、不 push。

本轮治理集中解决五类问题：

1. LCE 已经进入正式制盘写入事务，但缺少制盘完成后的独立协议级读回验收。
2. 卷标展示没有区分“原样保留”与“格式化后的目标卷标”，二合一盘默认卷标语义也不正确。
3. 备份工作区目前只有平面表格，缺少按物理设备快速筛选的设备导航。
4. “备份详情”命名需要调整为“备份信息”，但本轮保持其已有内容不变。
5. “区域覆盖 · `Manifest`”使用 Region / `Extent` / `Artifact` / 覆盖率表达元数据备份，容易把“按设计未备份用户数据”误解为“备份失败”；需要重构为“恢复范围”，并加入容量地图。

最终原则：

> 主页面负责回答用户决策问题；`Inspect` 负责展示协议和 `Manifest` 证据。任何视觉简化都不能削弱现有安全门禁、身份判定和读回验证。

---

## 2. 已确认的产品决策

以下结论在实现阶段不再重新讨论，除非代码事实证明存在冲突。

### 2.1 重建与格式化

普通用户数据分区不存在“Rebuild 但不格式化”的正常状态。

当前应用层已经有硬门禁：

- PreserveExact / PreserveVerified / PreserveOpaque / RewrapVerified：可以保留原 `extent`，不要求重新初始化文件系统。
- Rebuild：普通分区必须显式授权完整文件系统初始化，即必须格式化。
- CompatibilityReserve：不是用户文件系统区域，是唯一允许“固定结构重建但不格式化”的特殊区域。

原因是重建后可能形成新的 key / crypto / `geometry`，禁止 K_new + old ciphertext。

因此 UI 不得表达以下状态：

```text
需重建
未格式化
仍可继续
```

正确语义只能是：

```text
原样保留
  -> 不格式化
  -> 原文件系统、数据与卷标保持原样

需重建
  -> 必须格式化
  -> 使用目标文件系统与目标卷标重新初始化

固定兼容区
  -> 无用户文件系统
  -> 不格式化
```

### 2.2 二合一盘默认卷标

BootShareCombined 的默认卷标改为：

```text
启动区
```

它仍可复用交换区对应的格式化开关 / 文件系统 / key-domain 语义，但卷标默认值不能继续显示为“交换区”。

### 2.3 原样保留时的卷标

“原样保留”的区域不能继续显示表单里的默认目标卷标，否则会让用户误以为执行后卷标会变。

展示合同：

```text
原样保留 + 已读取当前卷标
  卷标：原样保留 · CURRENT_LABEL

原样保留 + 当前卷标未读取
  卷标：原样保留 · 未读取

需重建
  格式化后卷标：TARGET_LABEL

CompatibilityReserve
  不显示卷标
```

不得为了填满 UI 而猜测来源卷标。

### 2.4 mode2 的 0x7E00 区域

0x7E00 = 32256 B = 63 * 512 B sector。

它是 mode2 / WholeDiskEncrypted 官方布局中的固定兼容条目，不是普通用户文件系统，也不是 LBA7 指向的 3072B LCE。

建议用户可见名称：

```text
模式2兼容区 · 固定 63 sector
```

技术详情中可以继续保留 0x7E00。

用户页只表达：

- 官方模式2固定结构；
- 不存放用户文件；
- 不可编辑；
- 不格式化；
- 不属于密码域。

### 2.5 备份设备树

备份表本身继续保留当前统一表格能力，不在表格中插入“设备分组行”。

新增一个左侧设备导航树，只负责筛选右侧表格。

默认节点：

```text
▼ 全部备份
    设备 A
    设备 B
    设备 C
    身份未确认
```

树只到设备级，不把每个备份文件继续展开成第三层。

### 2.6 “备份信息”

现有“备份详情”窗口仅改标题为：

```text
备份信息
```

本轮内容保持一致，不借本轮顺手删字段、改字段顺序或重写快捷键提示。

### 2.7 “恢复范围”

现有“区域覆盖 · `Manifest`”整体替换为“恢复范围”。

“恢复范围”必须同时回答：

1. 恢复后磁盘布局是什么；
2. 哪些区域是完整字节恢复；
3. 哪些区域只是结构恢复；
4. 哪些内容按设计不在备份范围；
5. RestoreContract 允许承诺什么；
6. 恢复后还需要什么后置处理。

`Manifest` / Region / `Extent` / `Artifact` 详细证据下沉 `Inspect`。

---

## 3. P0：补齐 verify_lce_readback() 独立协议验收

### 3.1 当前事实

当前制盘链已经完成：

```text
LCE gold 3072B plaintext
  -> build_lce_ciphertext()
  -> zero8 + A7F0 + physical byte offset
  -> 切成 6 * 512B
  -> 放入 OfficialProvisionWriteImage.patch
  -> 与 LBA0-12 一起进入事务写入
  -> sync
  -> 每个 touched sector 精确读回比较
```

相关代码：

- src/provision/lce.rs
- src/provision/write_plan.rs
- src/diskio/transaction.rs
- src/application/provision/commit.rs

当前事务层已经证明“计划写的扇区实际写对了”，但 verify_protocol_readback() 之后的高层协议验收主要再次验证：

- LBA0-12；
- onlyid；
- 分区几何。

它没有独立证明：

```text
最终 LBA7
  -> 解析 3072B LCE pointer
  -> 重新读取真实盘 6 sector
  -> A6B0 zero8 逆变换
  -> 恢复到 gold plaintext
```

### 3.2 新增函数

目标函数：

```text
verify_lce_readback()
```

建议放在：

```text
src/application/provision/commit.rs
```

若后续为了复用拆文件，可以移动到 application/provision/readback.rs，但第一步优先保持小改动。

建议输入至少包含：

- &mut dyn SectorDev
- &PreparedNewProvision
- 已重新读取的 LBA0-12 protocol image，或由函数内部读取

不得使用 prepare 阶段缓存的 LBA7 作为最终证据。

### 3.3 验收步骤

verify_lce_readback() 必须独立执行以下检查：

1. 从最终真实盘重新取得 LBA0-12。
2. 使用最终 LBA7 重新解析 compatibility `geometry`。
3. 确认至少存在协议要求的 3072B pointer entry。
4. 确认相关后续 entry 的 LCE start_sector 一致。
5. 确认 sector_size == 512。
6. 确认 partition_size == 3072。
7. 确认解析结果 sector_count == 6。
8. 确认解析出的 start_lba 与 prepared.lce_start_lba 一致。
9. 确认解析出的 `extent` 与 prepared.plan.lba7_compatibility_extent 一致。
10. 检查 start_lba..start_lba+5 完全位于目标盘范围内。
11. 从 SectorDev 真实读取连续 6 sector。
12. 拼接为 3072B ciphertext。
13. 使用 zero8 和 start_lba * 512 的 64 位物理字节偏移调用 A6B0 解密。
14. 将得到的 3072B plaintext 与 provision::lce_plaintext() 完整逐字节比较。
15. 任意一步不一致即返回 EXIT_TARGET / EXIT_IO 对应错误，禁止继续格式化。

当前 `Inspect` 已使用：

```text
a6b0_full(raw, &[0u8; 8], physical_offset)
```

所以读回验收应复用同一协议事实，不新增第二套 LCE 解密实现。

### 3.4 执行顺序

正式制盘顺序调整为：

```text
准备计划
  -> capture LBA3 / identity pin
  -> transactional protocol + LCE write
  -> transaction exact sector readback
  -> verify_protocol_readback()
  -> verify_lce_readback()
  -> partition geometry / protocol acceptance complete
  -> 才允许执行用户授权的文件系统格式化
```

LCE 验收失败时：

- 不得进入任何分区格式化；
- 事务层若已经确认写入成功，不伪称自动回滚；
- 返回明确的协议验收失败；
- Result / progress 中应保持“协议阶段失败”语义。

### 3.5 测试

至少增加：

- 正确 LBA7 + 正确 6 sector -> pass。
- LCE 任意 1 byte 被篡改 -> fail。
- LBA7 pointer start_lba 与计划不一致 -> fail。
- 两个后续 LBA7 pointer 不一致 -> fail。
- size != 3072 -> fail。
- sector_size != 512 -> fail。
- LCE 越界 -> fail。
- 6 sector 可读但 A6B0 后 plaintext 与 gold 不一致 -> fail。
- mode0 / mode1 / mode2 / mode3 / mode4 的 LCE pointer 规则按现有协议计划分别覆盖。
- 确认 verify_lce_readback 失败时不会进入 format_partition_with_progress()。

### 3.6 验证等级

这是写盘安全链变更，不按“纯 UI”处理。

必须：

```text
cargo fmt --all
git diff --check
scripts/test-fast.sh
python3 scripts/test-full.py --profile full
```

并追加：

- Virtual Disk HIL：必须；
- 实体 USB HIL：在本轮协议链收口前至少完成一轮真实盘制盘 + LCE 读回证据。

---

## 4. P1：卷标与 Rebuild 展示语义治理

### 4.1 当前问题

当前默认值包括：

```text
启动区
交换区
保密区
```

BootShareCombined 当前沿用 share_label，导致默认显示“交换区”。

同时 PreserveExact 场景下，文件系统 `extent` 原样保留，真实卷标也会原样保留，但表单仍可能显示默认目标卷标，造成误解。

### 4.2 统一卷标解析

不要继续把“角色 -> 一个 label 字符串”散落在不同层。

建议建立统一语义：

```text
VolumeLabelDisposition
  PreserveCurrent(Option<String>)
  FormatTarget(String)
  NotApplicable
```

若不希望新增公共 enum，也至少在 presentation 层提供一个统一 resolver，所有 TUI 文案与计划确认复用。

角色默认值：

```text
Boot               -> 启动区
BootShareCombined  -> 启动区
Share              -> 交换区
Encrypt            -> 保密区
CompatibilityReserve -> N/A
```

### 4.3 来源卷标

当前 FilesystemMetadata 已有：

```text
volume_label: Option<String>
```

FAT16 / FAT32 / exFAT 已能可靠读取卷标；其他文件系统目前并非都能提供。

因此扩展 source evidence 时必须保持 Option：

- 能可靠读取 -> 携带当前卷标；
- 无法读取 -> None；
- 不得根据表单默认值反推来源卷标。

可以选择：

- 扩展 ExistingPartition 增加 volume_label: Option<String>；
- 或新增只读 presentation/source evidence 投影。

优先选择不会污染协议核心模型的最小方案。

### 4.4 UI 与交互规则

制盘表单和计划确认页共同遵守以下卷标展示：

```text
Preserve*
  卷标：原样保留 · <当前卷标>
  或
  卷标：原样保留 · 未读取

Rebuild
  格式化后卷标：<目标卷标>

CompatibilityReserve
  卷标：不适用
```

但“格式化”不能继续被当成一个与 Rebuild 无关的自由 bool。交互必须由 preflight 结论驱动。

普通用户数据分区只允许以下三种交互状态：

```text
Preserve
  -> 当前 extent 可安全原样保留
  -> 格式化默认关闭
  -> 卷标字段只读，显示来源真实卷标/未读取
  -> 用户可以主动选择“重新格式化”

RequiredRebuild
  -> 当前 extent 无法安全原样保留
  -> 格式化自动开启
  -> 格式化不可取消
  -> 卷标字段为可编辑的“格式化后卷标”
  -> 文件系统字段按目标规则可编辑

UserRequestedRebuild
  -> 原本可以 Preserve，但用户主动要求重新格式化
  -> 格式化开启
  -> 用户可以取消
  -> 卷标字段切换为可编辑的“格式化后卷标”
  -> 取消后重新回到 Preserve
```

CompatibilityReserve 单独处理：

```text
CompatibilityReserve
  -> 固定结构重建
  -> 不是用户文件系统
  -> 不显示格式化开关
  -> 不显示卷标输入
```

实现上不要求一定新增公开 enum，但状态语义必须能区分：

```text
Preserve
RequiredRebuild
UserRequestedRebuild
NotApplicable
```

禁止继续只靠一个裸 `format_boot/share/encrypt: bool` 推断“这个 true 是系统强制还是用户主动选择”，否则无法稳定实现锁定、取消和回退。

#### 4.4.1 Plain -> mode0 的默认体验

普通盘选择 mode0 后，进入表单立即执行同步 preflight。最常见情况下三个目标分区都无法 `Preserve`，因此自动呈现：

```text
启动区
  状态        需重建
  文件系统    FAT16
  格式化      ✓ 必须
  卷标        启动区

交换区
  状态        需重建
  文件系统    exFAT
  格式化      ✓ 必须
  卷标        交换区

保密区
  状态        需重建
  文件系统    exFAT
  格式化      ✓ 必须
  卷标        保密区
```

用户不需要额外勾选三次格式化；只在确实需要时修改文件系统或目标卷标。

如果某个目标区域满足 `Preserve`，例如普通盘恰好存在与 mode0 启动区完全一致的物理 FAT16 `extent`，则显示：

```text
启动区
  状态        ✓ 原样保留
  文件系统    FAT16
  格式化      否
  卷标        MYBOOT · 原样保留
```

若来源卷标未能可靠读取：

```text
卷标        未读取 · 原样保留
```

此时卷标不可编辑，因为不格式化就不会写入新卷标。

#### 4.4.2 用户主动把 `Preserve` 改成重新格式化

当某个区域当前为 `Preserve` 时，用户仍可在“格式化”字段按 Space 主动重建：

```text
Preserve
  格式化：否
      ↓ Space
UserRequestedRebuild
  格式化：✓ 重新格式化
  卷标：<目标默认卷标，可编辑>
```

用户再次 Space 取消后，只要 `geometry` / `filesystem` / crypto 条件仍满足 `Preserve`，就恢复：

```text
Preserve
  格式化：否
  卷标：<来源卷标，只读>
```

这类格式化必须与 RequiredRebuild 的“✓ 必须”有明显视觉区别。

#### 4.4.3 参数变更必须即时联动

任何会影响保留条件的字段变化后，都要立即重跑同步 preflight，而不是等 Enter 提交才发现冲突。

例如启动区原本：

```text
✓ 原样保留
LBA63 + 20417 sector + FAT16
```

用户修改容量后不再与来源 `extent` 完全一致：

```text
20417 sector -> 30000 sector
```

UI 必须立即切换为：

```text
状态        需重建
格式化      ✓ 必须
卷标        启动区
```

如果用户随后把参数改回可 `Preserve` 的精确条件，而且没有保留“用户主动重新格式化”的显式意图，则恢复 `Preserve`。

因此交互主流程固定为：

```text
用户修改会影响布局/文件系统/加密语义的字段
        ↓
立即同步 preflight
        ↓
当前目标是否可 Preserve
   ┌───────────────┐
   │               │
   是              否
   ↓               ↓
Preserve        RequiredRebuild
格式化关闭      格式化自动开启并锁定
显示来源卷标    显示目标卷标输入
   │
用户主动 Space
   ↓
UserRequestedRebuild
格式化开启但可取消
显示目标卷标输入
```

RequiredRebuild 普通分区不再依赖用户手工“授权勾选”后才变成可执行状态；进入这种状态时格式化就是该计划不可分割的一部分。底层 prepare/commit 仍保留 fail-closed 门禁，任何状态漂移导致 Rebuild + 未格式化都必须拒绝执行。

### 4.5 CLI / 应用层一致性

不能只改 TUI 默认值。

必须审计并统一：

- src/tui/provision/form.rs
- src/tui/provision/field_presentation.rs
- src/tui/provision/validation.rs
- src/application/provision.rs
- src/cli_args/provision.rs
- src/provision/reprovision/*
- 计划确认 presentation

确保 TUI、CLI、preview、commit 使用同一个默认卷标规则。

### 4.6 mode2 兼容区文案

将主要用户文案从：

```text
0x7E00兼容保留区
```

调整为更可理解的：

```text
模式2兼容区 · 固定 63 sector
```

技术详情仍可展示：

```text
0x7E00 B / 32256 B / 63 sector
```

不得把它与 LCE 合并展示。

---

## 5. P2：备份工作区增加左侧设备树

### 5.1 总体框架

Wide / Standard 的备份主工作区调整为：

```text
┌ 备份概览 ───────────────────────────────────────────────────────────┐
│ 总计 / 盘型统计 / 搜索状态                                         │
└────────────────────────────────────────────────────────────────────┘

┌ 设备 ───────────────┐ ┌ 备份列表 ───────────────────────────────────┐
│ ▼ 全部备份       77 │ │ 现有统一表格                               │
│   Netac A        31 │ │                                            │
│   SanDisk B      18 │ │                                            │
│   aigo C         12 │ │                                            │
│   身份未确认      8 │ │                                            │
└─────────────────────┘ └────────────────────────────────────────────┘

┌ 备份信息 ────────────────────────┐ ┌ 恢复范围 ───────────────────────┐
│ 原内容保持一致                   │ │ 容量地图                         │
│                                  │ │ 区域恢复状态                     │
│                                  │ │ 恢复能力                         │
└──────────────────────────────────┘ └──────────────────────────────────┘
```

### 5.2 树不是第二张表

设备树只承担：

- 选择“全部备份”；
- 选择某一可靠设备身份；
- 选择“身份未确认”桶；
- 展开 / 折叠设备列表；
- 把右侧备份表筛选到当前范围。

不得：

- 把所有备份文件继续展开到树里；
- 在树中复制表格列；
- 在树中提供删除 / 恢复 / `verify` 等备份操作。

所有备份操作仍只针对右侧表格当前选中的真实 BackupWorkspaceItem。

### 5.3 分组身份规则

必须复用现有身份语义，不允许 TUI 按“型号 + 容量”重新发明分组。

强设备身份继续使用 backup_group_key()：

优先：

```text
usable USB serial
```

否则要求：

```text
device_id + onlyid
```

禁止仅依靠：

- 型号；
- 容量；
- VID:PID；
- 文件名。

“身份未确认”是一个虚拟筛选桶：

```text
身份未确认    N
```

其含义是“N 份无法可靠归属到物理设备的备份”，不是“N 份来自同一台未知设备”。

backup_list_group_key() 可以继续服务非破坏性列表语义，但设备树真实设备节点必须以强身份为边界，不能把 singleton entry 冒充成已确认设备。

### 5.4 建议数据模型

在 application read-side 增加稳定投影，避免 TUI 重新解析 BackupEntry：

```text
BackupDeviceGroup
  key
  display_label
  count
  representative_identity
  newest_global_index
```

BackupWorkspaceItem 可增加：

```text
device_group_key: Option<String>
```

或等价的只读强身份引用。

TUI `state` 增加：

```text
BackupDeviceFilter
  All
  Confirmed(String)
  Unresolved

backup_device_tree_expanded: bool
backup_device_tree_selected: usize
```

### 5.5 设备标签

默认一行应尽量让用户“认盘”，但不塞满所有身份字段。

建议：

```text
Netac OnlyDisk · 3513095381    31
```

显示后缀优先级可以是：

1. 可安全显示的硬件序列号短值；
2. onlyid；
3. VID:PID；
4. 无可靠标识时只显示型号 / “身份未确认”。

完整身份继续留在备份信息或状态提示中。

### 5.6 过滤流水线

右侧 visible_backup_indices 的逻辑顺序固定为：

```text
全部 BackupWorkspaceItem
  -> 设备筛选
  -> 搜索筛选
  -> 表格排序
  -> visible_backup_indices
```

不得先排序再把设备组拆散，也不得让设备筛选重新编号备份。

BackupWorkspaceItem.index 必须保持 CLI `restore` / `verify` / delete 共用的全局编号。

### 5.7 搜索语义

当前设备筛选与搜索叠加：

```text
设备：Netac A
搜索：2026-09-30
=> 只在 Netac A 的备份中显示匹配项
```

清空搜索后：

```text
仍保持 Netac A
```

不能自动跳回“全部备份”。

### 5.8 表格排序

表格排序只作用于当前筛选后的备份集合。

现有统一表格能力保持：

- h/l 激活列；
- H/L 横向视口；
- 0/$ 首末列；
- </> 移动列；
- j/k 行移动；
- y/Y 复制；
- Space 多选；
- 横竖滚动条；
- 当前列排序。

设备树不改变这些合同。

### 5.9 设备树键位

设备树固定支持：

```text
j / k       上一项 / 下一项
gg          跳到第一项，即“全部备份”
G           跳到最后一个当前可见节点
o           展开 / 折叠根节点
Enter       进入右侧备份表
Ctrl-w h/l  在设备树与备份表之间移动 Pane 焦点
```

不使用 0/$ 作为设备树首尾移动主键，避免与统一表格“首列 / 末列”语义冲突。

边界：

- gg / G 只针对当前可见树节点；
- 根节点折叠时，G 停在根节点；
- 根节点折叠不能清除当前设备筛选；
- 设备树顺序不受右侧搜索排序影响。

### 5.10 即时筛选

在设备树内 j/k 移动时，右侧表格立即跟随当前树节点更新。

Enter 的作用不是“应用筛选”，而是：

```text
把焦点交给右侧备份表
```

这样设备树是真正的导航器，而不是二次确认列表。

---

## 6. P3：“备份详情”改名“备份信息”

本轮只改标题：

```text
备份详情
->
备份信息
```

以下现有内容本轮保持不变：

- 身份关系；
- 健康状态；
- warning；
- provision kind；
- 容量；
- VID:PID；
- model；
- device_id；
- onlyid；
- `canonical` identity；
- user / dept；
- 时间 / 编号；
- SHA；
- 文件名 / 路径；
- 现有动作提示。

原因：

本轮右下“恢复范围”已经是一次较大的信息架构变更，同时左侧还增加设备树。为了控制变更面，不在同一轮继续重构“备份信息”的字段内容。

后续如果仍显拥挤，再单独立项。

---

## 7. P4：“区域覆盖”重构为“恢复范围”

### 7.1 根因

当前 BackupCoverage 以：

- Region 数；
- `Extent` 数；
- `Artifact` 数；
- captured / total sector；
- completeness；

来表达 `Manifest`。

元数据备份中，用户数据区本来就不在备份合同内，所以会出现：

```text
partition.type2
0 / 243625984 sector
未采集
```

这在实现层是正确的，但视觉语义像“备份完成度 0%”，会误导用户。

因此主页面不再展示“百分比覆盖”。

### 7.2 新窗口固定三层

“恢复范围”从上到下固定：

```text
1. 容量地图
2. 区域恢复状态
3. 恢复能力
```

#### A. 容量地图

容量地图回答：

> 用这份备份恢复结构后，磁盘的区域布局是什么。

必须复用现有 DiskLayout / 容量地图视觉体系：

- 分区语义色一致；
- 保留区 / 空闲区 / EDP 分区颜色一致；
- 极小区域遵循现有最小可见规则；
- LCE 可以用尾部小标记或 `overlay` 标识；
- 不为备份页新建另一套颜色和 bar 算法。

示例：

```text
容量地图
┌────────────────────────────────────────────────────────┐
│ 保留 │ 启动 │             交换区          │ 保密区 │▌ │
└────────────────────────────────────────────────────────┘
                                               ↑ LCE
```

容量地图表达 `geometry`，不表达“备份了多少用户数据”。

#### B. 区域恢复状态

主页面只允许三类状态：

```text
完整恢复
结构恢复
不在备份范围
```

定义：

完整恢复：
- 备份容器中实际保存了恢复所需字节；
- 对应 `Artifact` / `Extent` 完整；
- `restore` path 会写回这些字节。

结构恢复：
- 恢复会重建该区域的位置、大小、类型或协议关系；
- 但原区域内部用户数据不在备份里。

不在备份范围：
- RestoreContract 明确不承诺恢复；
- 或 `Manifest` 明确 NotCaptured / EvidenceOnly 且恢复逻辑不写回。

示例：

```text
区域                 恢复结果
────────────────────────────────────────────
EDP 协议 LBA0-12     ✓ 完整恢复
启动区               ✓ 结构恢复 · 数据内容未备份
交换区               ✓ 结构恢复 · 数据内容未备份
保密区               ✓ 结构恢复 · 数据内容未备份
LCE                   ✓ 完整恢复 · 6 sector
用户文件              — 不在备份范围
```

禁止继续使用：

```text
0 / N sector
0%
```

去表达“按设计未备份”。

#### C. 恢复能力

恢复能力来自 RestoreContract，而不是 TUI 硬编码。

当前 metadata_only 合同具有：

```text
restores_partition_structure = true
restores_edp_protocol        = Plain 时 false，EDP 时 true
restores_filesystem          = false
restores_user_data           = false
post_restore_assessment_required = true
```

主页面可以转换成：

```text
可恢复
✓ 分区结构
✓ EDP 协议元数据
✓ LCE / 已纳入恢复合同的协议对象

不包含
— 原文件系统状态
— 目录树
— 用户文件内容

恢复后
⚠ 需要执行后置评估；文件系统不在本备份恢复合同内
```

Plain 盘必须自动显示“不适用 EDP 协议 / LCE”，不能套用 EDP 模板。

### 7.3 不硬编码类型文案

建议增加只读投影：

```text
BackupRestorePreview
  total_sectors
  partition_layout
  lce_extent
  region_statuses
  restore_contract
  post_restore_note
```

BackupRestoreRegionStatus：

```text
CompleteBytes
StructureOnly
OutOfScope
PartialOrInvalid
```

其中 PartialOrInvalid 只用于真正异常 / 不完整的 `Manifest`，不与 OutOfScope 混用。

TUI 只渲染投影，不重新解释 `Manifest`。

### 7.4 数据来源

优先来自已验证 EDPB `manifest`：

- `Manifest`.partitions；
- `Manifest`.regions；
- `Manifest`.extents；
- `Manifest`.artifacts；
- RestoreContract；
- BackupCoverage；
- provision kind；
- 已确认 LCE `extent`。

不要根据文件名、表格列或当前插入 USB 推断恢复范围。

### 7.5 `Inspect` 下沉

以下内容从主“恢复范围”移出，进入 i / `Inspect`：

- Region id；
- `Extent` id；
- `Artifact` id；
- `Manifest` schema/version；
- captured sector 精确计数；
- source_extent_ids；
- RestorePolicy；
- completeness 原始值；
- SHA / raw `mapping` 技术关系。

主页面只保留恢复决策语义。

---

## 8. 响应式布局

### 8.1 Wide

固定：

```text
顶部：备份概览
中部：设备树 | 备份表
底部：备份信息 | 恢复范围
```

设备树建议宽度约 24～28 cells，优先保证备份表主体宽度。

### 8.2 Standard

仍显示设备树，但压缩到约 20～22 cells。

底部根据现有布局能力：

- 宽度允许：备份信息 | 恢复范围；
- 宽度不足：两个 Pane 保持可切换，不把恢复范围内容挤成不可读文本。

### 8.3 Compact

不强塞左侧侧栏。

设备树变成当前 Backups workspace 的一级全屏导航 Pane：

```text
设备

全部备份
Netac A
SanDisk B
身份未确认
```

Enter 进入备份表。

备份表选中后再进入备份信息 / 恢复范围。

Compact 恢复范围至少显示：

```text
恢复范围

[容量地图]

✓ 分区结构
✓ EDP 协议 / 或不适用
✓ LCE / 或不适用
— 用户数据

Enter / i 查看详细恢复范围
```

---

## 9. Pane 与状态机

建议新增：

```text
PaneId::BackupDevices
```

Backups workspace 的逻辑 Pane：

```text
BackupDevices
BackupList
BackupSummary
BackupCoverage   // 内部枚举名可暂时保留，用户标题改“恢复范围”
```

如果要彻底收口命名，可后续将 BackupCoverage 重命名为 BackupRestoreScope，但不要求为本轮 UI 目标做大范围符号重命名。

焦点规则：

- 设备树移动只改变 filter，不改变全局编号；
- Enter 从设备树进入 BackupList；
- BackupList 的选择继续是实际备份行；
- BackupSummary / BackupCoverage 继续跟随 selected_backup()；
- 设备筛选导致当前备份不可见时，选择应落到新 visible list 的第一个有效项；
- 若筛选后为空，右侧显示空态，不保留悬空 source index。

---

## 10. 共享容量地图要求

恢复范围中的容量地图不能复制一套计算逻辑。

应复用：

- DiskLayout `geometry`；
- 共享 partition `semantic` colors；
- 共享容量单位格式；
- 共享最小 segment 可见宽度；
- 共享 active / inactive segment 视觉规则。

但备份页容量地图是只读：

- 不需要区域激活箭头；
- 不响应格式化字段联动；
- 可以随区域状态列表选择做高亮，但这是后续增强，不列为 P0。

---

## 11. 安全边界

### 11.1 设备树绝不能扩大 destructive authority

树上的分组只用于非破坏性导航。

删除 / prune 的设备身份门禁继续由现有：

```text
backup_group_key()
```

以及对应 destructive policy 决定。

不得因为 UI 把若干条放在同一个“身份未确认”桶里，就允许批量按设备删除。

### 11.2 恢复范围不能过度承诺

RestoreContract 是产品承诺上限。

即使 BackupCoverage 某个区域有若干 captured sector，也不能擅自显示“恢复文件系统”或“恢复用户数据”。

### 11.3 LCE readback fail-closed

verify_lce_readback() 失败时：

- 不进入格式化；
- 不降级为 warning；
- 不因为事务层逐扇区 readback 已成功而跳过；
- 不使用 prepare 阶段缓存代替最终真实盘证据。

### 11.4 卷标 fail-closed

无法读取来源卷标时显示“未读取”，不得显示表单默认值冒充来源卷标。

---

## 12. 预计文件影响面

P0 LCE：

- src/application/provision/commit.rs
- src/application/provision/tests.rs
- 必要时 src/provision/lce.rs 增加只读 decode helper，但优先复用 crypto::a6b0_full
- Virtual Disk HIL / 对应测试 `fixture`

P1 卷标与兼容区：

- src/tui/provision/form.rs
- src/tui/provision/field_presentation.rs
- src/tui/provision/validation.rs
- src/application/provision.rs
- src/cli_args/provision.rs
- src/provision/`layout`.rs
- src/provision/reprovision/*
- 计划确认 presentation / tests

P2 设备树：

- src/application.rs
- src/diskio/backup_catalog.rs（优先复用，非必要不改变 destructive contract）
- src/tui/`state`.rs
- src/tui/pane.rs
- src/tui/backups/render.rs
- Backups workspace 键位 / reducer / tests
- 表格 search / selection tests

P3 / P4 信息窗口：

- src/tui/backups/render.rs
- src/backup_coverage.rs
- src/edpb/model.rs（优先只读，不改变持久化 schema）
- 新增 BackupRestorePreview 时放 application/read-side 合适模块
- DiskLayout 共享 presentation 模块
- 对应 snapshot / render / `state` tests

文档：

- docs/ui/TUI.md
- docs/backup/EDPB_FORMAT_V1.md
- 必要时 docs/provisioning/PROVISIONING.md

注意：实现阶段先重新 git status，若工作区有用户未提交修改，必须保留，不得 reset / `clean` / 覆盖。

---

## 13. 实施顺序

### P0 — LCE 协议闭环

1. 新增 verify_lce_readback()。
2. 接入 commit：protocol readback 后、format 前。
3. 补协议单测。
4. fast/full。
5. Virtual Disk HIL。
6. 实体 USB LCE 验收。

这是第一优先级，不与 UI 混在同一个大提交。

### P1 — 卷标语义与 mode2 兼容区

1. 统一 BootShareCombined 默认“启动区”。
2. 统一 CLI / TUI / application default resolver。
3. `Preserve` 时不再显示目标默认卷标。
4. 能可靠读取时展示来源真实卷标。
5. 引入可区分的交互语义：`Preserve` / RequiredRebuild / UserRequestedRebuild / NotApplicable。
6. RequiredRebuild 自动开启格式化并锁定，用户不需要手工勾选，也不能取消。
7. `Preserve` 允许用户按 Space 主动进入 UserRequestedRebuild；再次取消时恢复 `Preserve`。
8. 参数变化后立即同步 preflight，并即时切换“原样保留 / 必须重建”。
9. Rebuild 明确显示“格式化后卷标”，`Preserve` 的来源卷标保持只读。
10. 兼容区不显示格式化开关和卷标。
11. mode2 主文案改为“模式2兼容区 · 固定 63 sector”。

### P2 — 设备树

1. 建立 application 只读 device-group projection。
2. 新增 BackupDeviceFilter。
3. 新增 BackupDevices Pane。
4. 渲染根节点 / confirmed devices / unresolved bucket。
5. 接入即时设备筛选。
6. 接入 jk / gg / G / o / Enter。
7. 搜索与 filter 交集。
8. 表格排序、选择、复制、删除、恢复行为回归。

### P3 — “备份信息”标题

单独小提交：

```text
备份详情 -> 备份信息
```

不改变内容。

### P4 — “恢复范围”

1. 新建 BackupRestorePreview 只读投影。
2. 从 `manifest` / RestoreContract / `coverage` 构建恢复语义。
3. 复用共享容量地图。
4. 增加区域恢复状态。
5. 增加恢复能力。
6. 删除主页面 0/N sector 百分比表达。
7. `Manifest` 技术细节接入 `Inspect`。
8. Wide / Standard / Compact 验收。

---

## 14. 建议的小步提交边界

建议按以下提交拆分，避免出现一个无法审计的大提交：

```text
1. feat(provision): verify LCE readback before formatting
2. test(provision): cover LCE protocol readback failures
3. fix(provision): align volume-label disposition semantics
4. fix(provision): use boot label for combined boot-share
5. ui(backups): add device navigation tree
6. test(tui): cover backup device filter navigation
7. ui(backups): rename backup summary to backup info
8. ui(backups): render restore scope and capacity map
9. test(tui): cover restore-scope projections and responsive layout
10. docs: record backup workspace and LCE governance
```

实际 commit 名可调整，但不要把协议写盘变更和纯 UI 混在一个提交。

---

## 15. 测试矩阵

### 15.1 LCE

必须覆盖：

- 6 sector 正确；
- pointer 错；
- pointer 不一致；
- size 错；
- sector_size 错；
- 越界；
- ciphertext 篡改；
- plaintext 不等于 gold；
- format 未开始断言；
- 多模式协议计划。

### 15.2 卷标与格式化交互

必须覆盖：

- Boot 默认“启动区”；
- Share 默认“交换区”；
- Encrypt 默认“保密区”；
- BootShareCombined 默认“启动区”；
- `Preserve` + known label；
- `Preserve` + unknown label；
- `Preserve` 卷标字段只读；
- RequiredRebuild 自动 format=true；
- RequiredRebuild 的格式化不可取消；
- RequiredRebuild 显示 format target label；
- `Preserve` 按 Space -> UserRequestedRebuild；
- UserRequestedRebuild 再次取消 -> `Preserve`；
- `geometry` 修改导致 `Preserve` -> RequiredRebuild；
- `geometry` 改回后可按规则恢复 `Preserve`；
- 用户主动重新格式化意图与系统 RequiredRebuild 不混淆；
- CompatibilityReserve 无格式化开关、无卷标；
- 任何内部状态漂移形成 Rebuild + format=false 时，prepare/commit 继续 fail-closed。

### 15.3 设备树

必须覆盖：

- 默认选中“全部备份”；
- root 展开；
- o 折叠 / 展开；
- j/k；
- gg；
- G；
- Enter 转焦点；
- 折叠不清除当前 filter；
- confirmed identity 分组；
- unresolved bucket 不被当成同一设备；
- global backup index 不变；
- 当前设备 filter + 搜索；
- 清搜索保留设备 filter；
- 设备 filter + 列排序；
- 筛选后 selected_backup 有效；
- 空组空态；
- destructive action 仍使用真实 backup 行。

### 15.4 恢复范围

必须覆盖：

- Plain metadata_only；
- EDP metadata_only；
- LCE complete；
- LCE missing / partial；
- protocol complete；
- data region NotCaptured -> 显示“不在备份范围/结构恢复”，不能显示 0% 失败；
- restores_user_data=false；
- restores_filesystem=false；
- post_restore_assessment_required=true；
- 容量地图 partition `geometry`；
- 小 LCE segment；
- Wide / Standard / Compact。

### 15.5 回归门禁

每个阶段至少：

```text
cargo fmt --all
git diff --check
scripts/test-fast.sh
```

P0、P2/P4 完整收口后：

```text
python3 scripts/test-full.py --profile full
```

只有写盘 / 恢复后端变更才要求 HIL；纯标题与纯渲染提交不得伪装为新的 HIL 证据。

---

## 16. 验收标准

本轮全部完成后，应能满足：

### 制盘

- LCE 的 6 sector 不仅被事务逐扇读回，还被最终 LBA7 重新定位、A6B0 解密并与 gold 3072B 完整比较。
- LCE 协议验收失败时绝不进入格式化。
- BootShareCombined 默认卷标为“启动区”。
- `Preserve` 不再把默认目标卷标伪装成最终卷标，来源卷标只读显示。
- 普通 RequiredRebuild 自动开启并锁定格式化，用户不需要额外手工勾选。
- `Preserve` 可以由用户主动切换为 UserRequestedRebuild，并可取消回 `Preserve`。
- 会影响保留条件的参数修改后立即 preflight，状态与卷标交互同步更新。
- UI 不出现“重建但不格式化”的可执行状态，底层仍保留 fail-closed 门禁。
- mode2 0x7E00 区与 3072B LCE 在 UI 和模型中保持明确分离。

### 备份工作区

- 左侧存在设备导航树。
- 默认显示全部备份。
- 可靠设备按强身份分组。
- 身份不足进入“身份未确认”筛选桶。
- 支持 j/k、gg/G、o、Enter。
- 设备筛选、搜索、排序按固定顺序叠加。
- 全局备份序号不变化。
- 当前表格样式与统一表格操作不被破坏。

### 备份信息

- 标题为“备份信息”。
- 本轮字段内容与现状一致。

### 恢复范围

- 标题为“恢复范围”。
- 首先显示恢复后容量地图。
- 明确区分“完整恢复 / 结构恢复 / 不在备份范围”。
- EDP / Plain 根据真实 RestoreContract 自动变化。
- 用户数据按设计未备份时，不再显示误导性的 0% 覆盖率。
- `Manifest` / Region / `Extent` / `Artifact` 技术细节可在 `Inspect` 查看。

---

## 17. 非目标

本轮不做：

- 不改变 EDPB 外层格式版本；
- 不把 metadata_only 升级为文件数据备份；
- 不实现目录树备份；
- 不改变 prune / delete 的 destructive identity authority；
- 不重新设计“备份信息”字段内容；
- 不在设备树中展开每一份备份文件；
- 不为备份页创建独立颜色主题；
- 不把 0x7E00 兼容区与 LCE 合并；
- 不用型号 / 容量代替强身份；
- 不把 UI projection 反向变成协议真相源。

---

## 18. 实施前检查

开始任何实现前必须重新执行：

```text
git status --short --branch
git rev-parse HEAD
git log -8 --oneline --decorate
```

如果发现与本计划写入时不同的未提交修改：

- 先审计差异；
- 保留用户修改；
- 不 reset；
- 不 `clean`；
- 不覆盖；
- 根据最新 main 调整实施位置，但不重复已经完成的工作。

本文件经用户审核后再开始编码。

---

## 19. 最终实施状态（2026-10-01）

本计划已经完成并收口：

- P0：`verify_lce_readback()` 已接入正式写盘链，并在格式化前 fail-closed；
- P1：`Rebuild` / `Preserve` / 卷标 / mode1 二合一 / mode2 兼容区语义已落地；
- P2：备份设备树、筛选、滚动、Pane 导航与强身份几何门禁已落地；
- P3：“备份信息”命名已完成；
- P4：“恢复范围”容量地图、`RestoreContract` 投影及 `Manifest` 技术证据下沉 `Inspect` 已完成；
- FAT16 / FAT32 / exFAT 文件系统写入能力已通过逻辑门禁、macOS 虚拟盘 HIL 和实体盘格式化读回；
- 实体 disk5 已完成 mode0 真实制盘，协议、几何、LCE 6-sector gold plaintext、FAT16/FAT32/exFAT 全部读回通过；
- 第二实体 disk4 的现有 mode0 LCE 也独立解码为同一 gold plaintext；
- HIL 发现的重建文件系统覆盖错误与短版/带版本号 `device_id` 假换盘问题已修复并补回归测试。

最终证据以 `audit/protocol/notes/provision_lce_fat_hil_2026-10-01.md` 为准。本计划不再保留未完成实施项。
