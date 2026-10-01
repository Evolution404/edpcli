# 制盘计划确认页重构计划（2026-10-01）

## 1. 基线与目标

本计划基于 GitHub `main` 当前基线：

```text
adee228cd11933c38610e5a1fd57db219eed37a3
fix(provision): unify synchronous preflight decisions
```

本轮只重构 **Provision 第 3 步“计划确认”页面**，不重新设计制盘表单，不修改底层协议语义，不重新实现密码/几何判断规则。

当前页面的主要问题：

1. 现有 `计划摘要 | 磁盘布局 | 变更明细 | 固定目标` 的横向多栏布局把信息切得过碎。
2. “变更明细”混入大量 `role/type/extent/crypto/key profile/filesystem` 解释，像协议审计页，不像执行确认页。
3. “固定目标”单独占用一个 Pane，但信息价值低。
4. “计划摘要”和“变更明细”重复表达相同结论。
5. 用户最关心的“按 Enter 后每个区域到底会发生什么、哪些数据会被清空”没有成为视觉中心。
6. 已经进入计划确认页后，不应再出现“需要勾选格式化”“需重建但未授权”之类表单阶段文案。能进入确认页说明同步 preflight 和后台 plan 已经形成可执行结论。

最终目标：

> 计划确认页只回答四件事：最终磁盘长什么样；每个区域会执行什么动作；整体有哪些数据/密码/文件系统变化；当前选中区域为什么这样处理。

---

## 2. 固定页面框架

确认页固定为：

- 顶部：现有步骤条 + 单行目标信息，不作为 Pane。
- 主体：**3 个 Pane**。
- 底部：单行快捷键/状态提示。

禁止继续增加第 4 个“固定目标”Pane。

### 2.1 总体结构

```text
1 制盘配置 ─ 2 生成计划 ─ 3 计划确认 ─ 4 执行 ─ 5 完成
设备 / disk4 / mode0 > 计划确认

disk4 · 125.83GB · USB · 1234:5678 · onlyid 1402259934 · 模式0 → 模式0 · 王伟

┌ 最终磁盘布局 ────────────────────────────────────────────────────────────┐
│                                                                         │
│                         全盘容量地图                                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

┌ 分区执行计划 ─────────────────────────────────┐┌ 执行摘要 ───────────────┐
│                                                ││                         │
│ 区域  起点  容量  动作  数据  文件系统         ││ 总体                     │
│ 启动区 ...                                     ││ 数据影响                 │
│ 交换区 ...                                     ││ 密码变化                 │
│ 保密区 ...                                     ││ 文件系统变化             │
│                                                ││                         │
│                                                ││ 当前区域                 │
│                                                ││ 动作 / 数据 / 密码       │
│                                                ││ 原因 / 结果              │
└────────────────────────────────────────────────┘└─────────────────────────┘

↑↓/jk 选择区域 · o 详情 · Enter 写入确认 · e 导出镜像 · Esc 返回修改
```

### 2.2 比例

Wide / UltraWide 默认：

- “最终磁盘布局”：主区域高度约 22%～28%，整行占满。
- 下半区：
  - “分区执行计划”：宽度约 65%～70%。
  - “执行摘要”：宽度约 30%～35%。

不要做四列竖向切割。

窄终端不允许硬压缩成不可读的三栏。窄屏应保持 3 个逻辑 Pane，但采用单 Pane 聚焦/切换显示，继续复用现有 Pane focus 体系。

---

## 3. Pane 1：最终磁盘布局

### 3.1 职责

只回答：

> 制盘完成后整块盘的最终布局是什么？

内容限定为：

- 最终容量地图；
- 区域名称/颜色；
- 必要容量信息；
- 当前选中区域联动指示。

禁止放入：

- 密码解释；
- `role/type/extent`；
- FileKey 依据；
- 校验过程说明；
- 格式化授权提示；
- 长 reason。

### 3.2 数据源

继续复用现有 `DiskLayoutModel` 体系，但确认页必须建立 **prepared-only** 的布局投影，例如 `provision_confirmation_layout_model(&PreparedProvision)`；不得直接复用仍会读取表单草稿的 `provision_layout_model()`。

确认页展示的是 **prepared plan 的最终目标结果**，不得重新从 `ProvisionForm`、`PlainProvisionForm`、`selected_device()` 或 renderer 内推导几何动作。Official 应从 prepared 的最终目标分区 + LCE 几何生成 canonical EDP layout；Plain 应从 prepared 的 `PlainProvisionPlan` 生成 canonical plain layout。

必须增加完整性不变量：Official 确认页的 LBA0–12 为协议区，LBA13 到官方首分区起点之间为保留区域，不能被填成“空闲区域”；特别覆盖 LBA12–62 的回归场景。最终布局必须 `validate_complete()` 通过后才允许进入 Review。

### 3.3 联动

“分区执行计划”当前选中行必须同步激活容量地图对应区域。

联动主键必须使用共享的精确几何选择键（优先复用 `DiskCapacitySelection`：`start_lba + end_exclusive + kind`），不能只用 `PartitionRole / region kind`，更不能通过字符串名称匹配。`PartitionRole` 只作为语义标签/校验辅助；Plain 可存在多个同为 `DiskRegionKind::Plain` 的分区，必须依靠 extent 才能稳定命中唯一地图区域。

---

## 4. Pane 2：分区执行计划

这是确认页主窗口。

只回答：

> 按 Enter 后，每个区域到底会发生什么？

### 4.1 默认表格

Wide 默认字段：

```text
区域      起点 LBA      容量          动作          数据      文件系统
启动区    63            20417         ✓ 保留        保留      保持
交换区    20480         243625983     ✓ 透传        保留      保持
保密区    243646464     2097152       ⚠ 格式化重建  清空      exFAT
```

密码列是否独立显示由实际宽度决定：

- 宽度充足：可增加“密码”列。
- 宽度不足：密码结果进入右侧“当前区域”详情。
- 不允许为了多一列把主要字段压成不可读。

### 4.2 确认页允许出现的最终动作词

进入确认页后，只允许展示已经确定的最终动作：

- `保留`
- `透传`
- `改密`
- `格式化重建`
- `新建`
- `迁移`
- `删除`（仅确有 Drop 语义时）

禁止出现：

- `需重建`
- `需要勾选格式化`
- `尚未获得格式化授权`
- `候选保留`
- `待计划`

这些属于表单/preflight 阶段，不属于计划确认阶段。

### 4.3 “格式化重建”文案

用户已经勾选格式化并成功进入确认页时，必须表达执行结果，而不是再次要求操作。

错误示例：

```text
交换区  ⚠ 需重建
需要：勾选格式化
```

正确示例：

```text
交换区  ⚠ 格式化重建  数据清空  exFAT
```

选中后的详情：

```text
动作    格式化重建
数据    原数据不会保留
密码    使用目标密码 / 生成新 FileKey
文件系统 exFAT

原因
目标几何发生变化，且已经获得格式化授权。

结果
创建新的空文件系统。
```

### 4.4 表格交互

- `j/k`、`↑/↓`：移动当前区域，而不是滚动一段 paragraph 文本。
- 当前行使用共享 table selection 底色，并自动保持在 table viewport 内。
- 切换行时同步：
  - 上方容量地图激活精确 extent；
  - 右侧“当前区域”详情。
- 选中行必须有独立的 confirmation view state；进入 Confirm/ExportPath 后返回 Review 时保留当前行、pane focus、展开状态和 viewport；重新生成新 plan 或 Esc 返回表单后清空并重新初始化。
- 不允许选择变化后重新计算底层计划，只切换展示对象。

---

## 5. Pane 3：执行摘要

该 Pane 不再承担“全量变更日志”，只分为两个逻辑段，不额外套子边框。

### 5.1 总体

只回答：

> 整体有没有破坏性变化？

例如无破坏动作：

```text
总体

数据       ✓ 全部保留
密码       ✓ 交换区透传
           ✓ 保密区透传
文件系统   ✓ 不格式化
```

存在格式化：

```text
总体

数据       ⚠ 1 个区域数据将清空
密码       ● 保密区生成新 FileKey
文件系统   ⚠ 保密区格式化为 exFAT
```

不得用“风险等级：低/中/高”代替事实。直接陈述具体数据后果。

### 5.2 当前区域

跟随左侧当前行：

```text
当前区域  交换区

动作      ✓ 透传
数据      保留
密码      保留原密码域
文件系统  保持

原因
来源密码未知，但 key profile 与 extent
满足黑盒透传条件。

结果
FileKey 与 data extent 保持不变。
```

默认原因保持 1～3 行可读摘要。

按 `o` 才展开技术依据，例如：

```text
技术依据
✓ role/type exact
✓ extent exact
✓ physical crypto profile compatible
✓ filesystem payload untouched
```

默认确认页不得直接平铺这些术语。

---

## 6. 顶部信息条与删除项

### 6.1 顶部单行目标信息

把当前“固定目标”Pane压缩成顶部一行：

```text
disk4 · 125.83GB · USB · 1234:5678 · onlyid 1402259934 · 模式0 → 模式0 · 王伟
```

VID:PID 应在空间允许时默认显示；`onlyid`、原生序列号/设备身份中至少选择一个 prepared 阶段已经固定的稳定标识显示，避免只靠会变化的 `diskN` 识别目标。该行必须来自 prepared/bound target snapshot，不能重新读取当前列表选中行；不得形成新的独立窗口。

### 6.2 删除当前独立 Pane

删除/收敛：

- 当前“计划摘要”独立 Pane；
- 当前“固定目标”独立 Pane；
- 当前“变更明细”长文本流式 Pane。

它们的信息分别进入：

- 顶部单行；
- 分区执行计划；
- 执行摘要。

---

### 6.3 最终写入确认模态框

确认页按 `Enter` 进入的 Confirm 不是一个新的业务推演阶段，只是 destructive-write guard。

要求：

- Confirm 使用与 Review **同一个 `ProvisionConfirmationViewModel` / prepared snapshot**；
- 不再只显示“相关结构和数据可能被覆盖”这种泛化警告；
- 至少显示目标设备稳定身份、将清空/删除的具体区域数量与名称、将重建的文件系统，以及“输入 YES 后立即开始写盘”；
- 如果整体无数据清空，也应明确写“数据区域全部保留”，不要仍显示泛化的“可能覆盖”；
- Confirm 返回 Review 时保持原选中区域与展开状态；
- Confirm 本身不得重新访问表单、preflight 或实时设备列表来改变已经审核过的结论。

---

## 7. 信息语义边界

### 7.1 表单页

负责表达“用户还需要做什么”：

- 需重建；
- 请勾选格式化；
- 请设置新密码；
- 原密码验证中；
- 当前草稿非法。

### 7.2 计划确认页

负责表达“系统已经决定将做什么”：

- 透传；
- 改密；
- 格式化重建；
- 数据保留/清空；
- 新 FileKey；
- 目标文件系统。

### 7.3 执行页

负责表达“现在执行到哪里”。

三层语义不得交叉。

---

## 8. 数据模型：建立专用确认页 ViewModel

不要继续让 `review_render.rs` 直接拼接 `Vec<String>` 并判断业务语义。

新增类似：

```rust
struct ProvisionConfirmationViewModel {
    target: ProvisionConfirmationTarget,
    layout: DiskLayoutModel,
    overall: ProvisionConfirmationOverall,
    regions: Vec<ProvisionConfirmationRegion>,
}

struct ProvisionConfirmationRegion {
    role: PartitionRole,
    start_lba: u64,
    sector_count: u64,
    action: ProvisionConfirmationAction,
    data_effect: ProvisionDataEffect,
    password_effect: ProvisionPasswordEffect,
    filesystem_effect: ProvisionFilesystemEffect,
    reason_summary: String,
    technical_basis: Vec<String>,
}
```

具体名称可按项目现有命名调整，但职责必须保持。

### 8.1 权威数据源

确认页已经发生在后台计划生成成功之后，因此最终动作以：

- `ProvisionPrepared`
- `TargetProvisionPlan`
- `PasswordDisposition`
- `RegionDisposition`
- `format_targets`

为权威来源。

`ProvisionPreflight` 继续负责 **进入后台计划之前的同步阻塞**，确认页不得重新使用 preflight 推演最终 action。

Confirmation ViewModel 构造必须 fail-closed：prepared 中若仍出现 `PasswordDisposition::Blocked`、Rebuild 与 `format_targets` 不一致、目标 extent 无法唯一映射到最终布局、布局不完整/冲突，均视为内部计划不一致，禁止进入可执行 Review，不允许 renderer 用兜底文案把异常伪装成“可确认”。

确认页 renderer 还应禁止读取 `ProvisionForm / PlainProvisionForm / provision_preflight() / selected_device()`；这些都是表单/实时 UI 状态，不是 prepared confirmation 的权威数据源。

### 8.2 禁止 renderer 重算业务规则

`review_render.rs` 只能：

- 布局；
- 选择；
- 样式；
- responsive rendering。

禁止在 renderer 内再次判断：

- 是否需要格式化；
- 是否透传；
- 是否 Rewrap；
- 是否清数据；
- 是否生成新 FileKey。

---

## 9. Pane / Focus 治理

当前 GitHub `main` 的 Provision Review 使用：

```rust
ProvisionSummary
ProvisionDiskLayout
ProvisionChanges
```

本轮语义应收敛为：

1. 最终磁盘布局；
2. 分区执行计划；
3. 执行摘要。

实现时优先把 `PaneId` 命名同步到新职责，避免继续让 `ProvisionSummary / ProvisionChanges` 名称承载完全不同的含义。

建议：

```rust
ProvisionDiskLayout
ProvisionPartitionPlan
ProvisionExecutionSummary
```

逻辑 Pane 顺序建议保持：

```text
ProvisionDiskLayout
→ ProvisionPartitionPlan
→ ProvisionExecutionSummary
```

但 **进入 Review 后的初始焦点必须是 `ProvisionPartitionPlan`**，因为它是确认页主窗口，用户应可直接 `j/k` 检查各区域动作；上方最终布局默认作为联动视觉中心，而不是抢占第一个键盘焦点。

`Enter` 在 Review 的三个 Pane 中语义必须一致：始终进入“写入确认”模态框，不再像旧实现那样在磁盘布局 Pane 上变成“打开详情”。`o` 统一承担详情语义：布局 Pane 上展开/收起尾部细节，执行计划/摘要 Pane 上展开/收起当前区域技术依据。

空间导航：

- 上方布局 ↓ → 分区执行计划；
- 分区执行计划 ↑ → 上方布局；
- 分区执行计划 → → 执行摘要；
- 执行摘要 ← → 分区执行计划。

窄屏仍通过相同 PaneId 切换，不额外制造第二套页面状态。

---

## 10. Responsive 约束

### Wide / UltraWide

采用固定“大地图 + 下方 70/30”框架。

### Normal

仍优先保留三块逻辑：

- 地图高度缩小；
- 左侧表格保留核心列；
- 右侧摘要减少非必要空行。

### Compact

单 Pane 显示：

- Tab / Shift-Tab 或现有 pane navigation 在 3 Pane 间切换；
- 表格列裁剪使用共享 table viewport；
- 当前区域详情不得挤到表格内部；
- 列降级顺序必须固定：`区域 / 动作 / 数据` 永远保留，`文件系统` 次之，`容量 / 起点 LBA / 密码` 依宽度逐步隐藏或进入右侧详情；不能把破坏性动作列隐藏在横向滚动之外。

除宽度外还必须处理 **短终端高度**：当主体高度不足以稳定显示“地图 + 下方两 Pane”时，直接进入单 Pane 模式或 Mini map，而不是继续按 22%～28% 百分比分配导致下半区只剩 1～2 行。底部快捷键也按当前 Pane/context 裁剪，不要求 Compact 强行塞满整句。

禁止：

- 在窄屏重新恢复旧的纵向长文本；
- 用大量换行让一个字段占 2～3 行；
- 为保持所有列强行压缩中文字段。

---

## 11. 文件级实施范围

基于当前 GitHub `main`，至少涉及：

### 必改

- `src/tui/provision/review.rs`
  - 删除当前“summary lines + change rows”混合模型；
  - 建立 confirmation ViewModel。

- `src/tui/provision/review_render.rs`
  - 改为“顶部全宽布局 + 下方 70/30”；
  - 渲染执行计划表和执行摘要；
  - 实现 responsive。

- `src/tui/pane.rs`
  - 调整 Provision Review PaneId、顺序和空间导航。

### 复用/可能调整

- `src/tui/provision/layout.rs`
  - 继续提供 `DiskLayoutModel`；
  - 不增加确认页业务判断。

- `src/tui/provision/preflight.rs`
  - 保持同步提交前判断；
  - 不作为确认页最终动作权威源。

- `src/provision/reprovision/plan.rs`
  - 原则上不为 UI 改写；
  - 仅当缺少明确的最终 data/password/filesystem effect 时，增加无 UI 语义的纯 domain helper。

- 共享 table / pane / theme 模块
  - 尽量复用，不创建 review 专属键位系统。

---

## 12. 实施阶段

### P0 — 契约测试先行

先锁定：

- 计划确认页只有 3 个逻辑 Pane；
- Wide 是“上 1 + 下 2”，不再是横向 3/4 栏；
- 不存在“固定目标”独立 Pane；
- 初始焦点是 `ProvisionPartitionPlan`；
- Review 任意 Pane 按 Enter 都进入写入确认，不能触发其他详情动作；
- 确认页禁止“需重建”“需要勾选格式化”等表单文案；
- 行选择以精确 extent 联动地图和右侧详情；
- Review 的布局与目标身份只能来自 prepared snapshot，禁止回读表单草稿/当前设备选择。

### P1 — Confirmation ViewModel

把 `ProvisionPrepared / TargetProvisionPlan` 投影成纯展示模型。

要求：

- domain → ViewModel 单向；
- renderer 不做业务决策；
- Official / Plain 共用同一确认页框架。

### P2 — 页面框架重构

实现：

```text
Header
Final Disk Layout
Partition Execution Plan | Execution Summary
Footer
```

先保证结构，不在此阶段继续增加视觉装饰。

### P3 — 执行计划表

实现：

- 区域选择；
- table selection；
- 容量地图联动；
- 行动作/数据/文件系统结果。

### P4 — 执行摘要

实现：

- 总体数据影响；
- 总体密码变化；
- 总体文件系统变化；
- 当前区域摘要；
- `o` 技术依据展开。

### P5 — Responsive / Navigation

完成 Wide / Normal / Compact。

统一 Pane focus、Tab、Ctrl-w 空间导航和底部提示。

### P6 — 门禁与实盘只读验收

运行：

```text
cargo fmt --all
cargo check --locked --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test tui_suite
cargo test --test provision_suite
cargo test --test repository_suite
./scripts/test-fast.sh
python3 scripts/test-full.py --profile full
cargo build --release --locked
```

实机验收只做只读计划生成和页面观察，不执行真实破坏性写盘。

---

## 13. 必须新增的回归场景

至少覆盖：

1. Mode0 → Mode0，全区域保留/透传。
2. Verified 原密码 + 新密码不同，显示“改密”，数据保持。
3. 用户已明确勾选格式化，确认页显示“格式化重建”，禁止再出现“需要勾选格式化”。
4. 格式化重建时明确显示“数据清空 + 新文件系统”。
5. 保密区重建时明确显示新 FileKey 语义。
6. Plain 制盘确认页显示“新建/格式化”的最终动作，不套 EDP 密码语义。
7. `j/k` 切换执行计划行时：
   - 地图激活区域同步；
   - 右侧当前区域同步。
8. `o` 只控制技术依据展开，不改变计划。
9. Wide viewport 采用上 1 + 下 2。
10. Compact viewport 不出现横向挤压或旧三栏页面。
11. 确认页文本 grep 门禁：
   - 禁止 `需要勾选格式化`
   - 禁止 `尚未获得格式化授权`
   - 禁止把 `Blocked` 作为可执行确认动作。
12. renderer 架构门禁：`review_render.rs` 不直接匹配 `PasswordDisposition / RegionDisposition` 做业务决策，也不得读取 `ProvisionForm / PlainProvisionForm / provision_preflight() / selected_device()`。
13. prepared-only 布局：故意让表单草稿与 prepared fixture 不一致，确认页仍必须完全展示 prepared 几何。
14. Official reserved 回归：LBA0–12=协议区，LBA13 到首分区起点=保留区域；LBA12–62 不得渲染为空闲区域。
15. Plain 多分区：两个及以上 `DiskRegionKind::Plain` 行用精确 extent 联动到各自地图区域，不能因 kind 相同选错。
16. Review 初始焦点为分区执行计划；三个 Pane 的 Enter 均只进入 Confirm。
17. Review → Confirm/Export → Review 保持 selected row / pane / expansion；Review → Form → 新 plan 后重新初始化这些 view state。

---

## 14. 验收标准

本计划完成必须同时满足：

1. 用户进入第 3 步后，第一眼可以看到最终磁盘布局。
2. 第二视觉中心是“分区执行计划”，而不是长 reason。
3. 用户无需阅读协议术语即可知道哪些数据保留、哪些数据清空。
4. 已授权格式化的区域只显示“格式化重建”，不会再次提示用户“去勾选格式化”。
5. 默认页面不平铺 `role/type/extent/physical crypto/key profile`。
6. 只有按 `o` 才显示技术依据。
7. 不再存在独立“固定目标”大 Pane。
8. 不再存在旧式“计划摘要 + 变更明细”重复信息。
9. Official 与 Plain 使用同一页面骨架。
10. 页面状态、底层 prepared plan、执行动作三者语义一致。
11. fast/full/clippy/release 全部通过。
12. 不放宽现有架构硬门禁来迁就实现。
13. Review 的目标身份、几何、动作、数据/密码/文件系统后果全部来自同一 prepared snapshot；实时设备列表变化不能改变确认页已经审核的事实。
14. 任意 Pane 的 Enter 都只进入写入确认；只有输入精确 `YES` 才开始写盘，页面文案不得把第一次 Enter 描述成“立即执行”。
15. Plain 多分区与 Official 保留区域都能稳定联动容量地图，不依赖非唯一的 region kind。

---

## 15. 本轮不做

本计划不包含：

- 修改协议字段定义；
- 修改 LBA0–12 / LCE / 尾部协议结构；
- 修改 `ProvisionPreflight` 的既有同步阻塞语义；
- 重新设计制盘表单；
- 重新设计执行进度页；
- 重新设计结果页；
- 增加 GUI 风格卡片、渐变、大按钮等终端无法稳定实现的效果。

本轮只把“计划确认页”从当前的高密度审计页面，重构成一个真正的 **执行决策确认页**。
