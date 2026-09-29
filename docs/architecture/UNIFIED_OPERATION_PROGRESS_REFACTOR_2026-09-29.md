# 统一备份 / 恢复 / 制盘执行与进度体系重构计划 — 2026-09-29

状态：**已批准，待执行**

基线：

- 分支：`feat/device-workbench-20260928`
- HEAD：`387e1ef refactor(tui): unify confirmations and text hierarchy`
- 当前代码已通过：
  - `cargo test --locked --test tui_suite`
  - `cargo test --locked --test repository_suite architecture_split -- --nocapture`
  - `scripts/test-fast.sh`
  - `python3 scripts/test-full.py --profile full`（需在能找到 `cargo` 的登录 shell / 正确 PATH 下执行）

本计划只处理本次尚未完成的：

1. 备份创建双确认与 Modal 背景空白；
2. 备份 / 恢复 / 制盘三套 Running UI 不统一；
3. 制盘高频 sector 事件污染日志；
4. 总体进度与当前步骤子进度不联动；
5. 长操作进度模型、日志模型和渲染模型缺少统一单一事实源。

不回退已经完成的恢复确认 Modal、全局文字层级、固定底部消息栏及相关 TUI 治理。

---

## 1. 当前已确认的问题

### 1.1 设备页按 `b` 后出现两层确认

当前流程：

```text
Devices / Backups
  -> b
  -> BackupsState::create_choice = Some(BackupCreateChoiceState)
  -> draw_backup_create_choice()
  -> Enter
  -> take_backup_create_choice()
  -> dispatch BeginBackupCreate
  -> begin_write_wizard_for_identity(BackupCreate)
  -> WizardStage::Confirm
  -> draw_wizard() 再显示一次创建备份确认
  -> Enter
  -> confirm_backup_create()
  -> Running
```

因此用户必须按两次 Enter。

这是重复状态机，不是键位问题。

### 1.2 第一个备份 Modal 会把底层 Devices 页面替换为空白

`src/tui/render.rs` 当前顶层分发逻辑把：

```rust
state.backup_create_choice().is_some()
```

作为独占 `content_area` 的页面分支。

结果是：

- 原 Devices / Backups Workspace 不再绘制；
- `draw_backup_create_choice()` 直接占整个 content area；
- Modal 背后不是当前页面，而是空白内容区。

这违反统一 Modal 的 overlay 语义。

### 1.3 Backup / Restore / Provision 的 Running UI 没有真正统一

已有领域基础：

`src/application/progress.rs`

明确声明：

> Application-owned progress contract shared by provisioning, backup and restore.

并已有：

```rust
pub enum OperationKind {
    Provision,
    Backup,
    Restore,
}
```

但实际 TUI 尚未统一：

- Provision：`ProgressEvent` + `ProvisionRunState` + `provision/running_render.rs`
- Backup / Restore：仍大量依赖 `WriteEvent`、`write_progress_text()`、Wizard 自身 progress/log
- 三者在页面结构、日志语义、进度计算上仍是不同实现

### 1.4 制盘总体进度只按“步骤数”前进

当前 Provision 顶部 Gauge 使用：

```rust
event.current / event.total
```

而高频工作量在：

```rust
event.work.current / event.work.total
```

例如：

```text
总体步骤：3 / 7
当前步骤：100 / 1000 sector
```

当前顶部总进度仍约等于 `3/7`，不会随着 `100/1000 -> 200/1000` 连续前进。

这是模型层问题，不能只在 renderer 里修百分比。

### 1.5 高频 sector progress 被当作日志保存

`ProvisionRunState::push()` 当前对每个 `ProgressEvent`：

- 更新 `latest`
- 同时 push 进 `log`

如果底层每个 sector 都发事件，就会产生：

```text
1/1000
2/1000
3/1000
...
1000/1000
```

这种日志没有诊断价值，只会：

- 快速刷屏；
- 占用 log ring；
- 增加 TUI 重绘；
- 淹没真正重要的阶段 / 警告 / 回滚 / 完成事件。

---

## 2. 最终用户流程

### 2.1 创建备份

```text
Devices / Backups
    |
    | b
    v
[当前 Workspace 上覆盖只读确认 Modal]
    |
    | Enter
    v
[统一 Operation Progress 全屏页面]
    |
    v
[完成 / 失败结果]
```

要求：

- 按 `b` 只出现一次确认；
- Modal 背景必须仍然是当前 Devices / Backups 页面；
- 创建备份是只读操作，不输入 `YES`；
- Enter 一次立即进入 Running；
- Esc 只关闭 Modal，恢复原页面、原焦点、原选择。

### 2.2 恢复备份

```text
Devices 关联备份 / Backups 表格
    |
    | R
    v
[当前 Workspace 上覆盖恢复写入确认 Modal]
    |
    | 输入 YES + Enter
    v
[统一 Operation Progress 全屏页面]
    |
    v
[完成 / 后处理结果]
```

恢复确认继续沿用当前已完成的身份匹配 Modal：

- 匹配等级；
- 当前设备摘要；
- 备份摘要；
- 身份对照；
- 写入 YES 授权。

不得重新引入 Restore Review 全屏中间页。

### 2.3 制盘

```text
Provision 配置 / Review
    |
    | Enter
    v
[当前 Provision 页面上覆盖写入确认 Modal]
    |
    | 输入 YES + Enter
    v
[统一 Operation Progress 全屏页面]
    |
    v
[完成 / 失败结果]
```

三类操作的 Running 页面必须使用同一个共享 renderer。

---

## 3. 统一长操作状态机

目标只保留：

```text
Idle
  -> Confirm
  -> Running
  -> Result
```

不同操作只允许确认策略不同：

| 操作 | Confirm 类型 | YES |
| --- | --- | --- |
| Backup Create | 只读 Action Modal | 否 |
| Restore | Media Write Modal | 是 |
| Provision | Media Write Modal | 是 |

### 3.1 删除 BackupCreateChoiceState

删除：

- `BackupCreateChoiceState`
- `BackupsState::create_choice`
- `begin_backup_create_choice()`
- `take_backup_create_choice()`
- `cancel_backup_create_choice()`
- `runtime_input/backup_choice.rs` 中相应流程
- 顶层 `render.rs` 中独占 content area 的 backup choice 分支

`b` 必须直接 dispatch：

```text
NavCommand::BeginBackupCreate
```

并建立唯一的 BackupCreate Confirm。

### 3.2 Modal 一律 Overlay

顶层 render 顺序统一为：

```text
1. render workspace base content
2. render help / ordinary overlays
3. render confirmation modal
4. render fixed bottom message row
```

任何 Confirm 状态不得作为：

```text
if confirm { render_confirm_instead_of_workspace() }
```

的页面替代逻辑存在。

---

## 4. 统一进度领域模型

### 4.1 继续以 application 层为唯一事实源

保留并扩展：

`src/application/progress.rs`

不得在 TUI 自己制造百分比语义。

目标模型：

```rust
pub struct ProgressEvent {
    pub operation: OperationKind,
    pub phase: Phase,
    pub step: Step,

    pub overall: OverallProgress,
    pub work: Option<WorkProgress>,

    pub detail: Option<String>,
    pub severity: Severity,
    pub log_policy: LogPolicy,
    pub emitted_at: Instant,
}
```

建议：

```rust
pub struct OverallProgress {
    pub current: u64,
    pub total: u64,
}

pub struct WorkProgress {
    pub current: u64,
    pub total: u64,
    pub unit: Unit,
    pub activity: Option<TransactionActivityPhase>,
}
```

不要继续让：

```text
ProgressEvent.current / total
```

同时承担“第几个步骤”和“总体百分比”两个含义。

---

## 5. 总进度与子进度联动

### 5.1 进度 span

每个长操作在执行前建立固定 Progress Plan。

建议用 basis points：

```text
0 .. 10000
= 0.00% .. 100.00%
```

每个步骤占一个总体区间：

```rust
pub struct ProgressSpan {
    pub start: u16,
    pub end: u16,
}
```

例如：

```text
身份复核       0%  ->  5%
元数据备份     5%  -> 15%
锁定设备      15%  -> 20%
协议写入      20%  -> 65%
分区格式化    65%  -> 90%
读回          90%  -> 97%
写后身份      97%  ->100%
```

### 5.2 子进度插值

若当前步骤：

```text
span = 20% .. 65%
work = 100 / 1000
```

则总体：

```text
20% + (65%-20%) * 100/1000
= 24.5%
```

要求：

- overall 必须随 work 连续前进；
- overall 单调不下降；
- work 变化不能导致顶部 Gauge 停住；
- 操作成功完成严格达到 100%；
- rollback 等异常阶段需要定义清楚进度显示语义，禁止伪装成“正常继续前进”。

---

## 6. 统一 Running 状态

新增共享状态，例如：

```rust
pub struct OperationRunState {
    pub kind: OperationKind,
    pub title: String,
    pub started_at: Instant,
    pub last_activity_at: Instant,
    pub latest: Option<ProgressEvent>,
    pub log: VecDeque<OperationLogEntry>,
}
```

Backup / Restore / Provision 均使用这一模型。

删除 / 收敛：

- `ProvisionRunState` 私有重复语义；
- Wizard 内独立的 `progress` / `progress_log`；
- Backup / Restore 的 `WriteEvent -> write_progress_text()` 作为主 Running UI 数据源。

`WriteEvent` 若仍有领域事件价值，可以：

- 转换为统一 `ProgressEvent`；
- 或只保留非进度业务事件；
- 不能继续作为另一套 TUI progress contract。

---

## 7. 统一 Progress Renderer

新增共享组件，例如：

`src/tui/ui/operation_progress.rs`

或：

`src/tui/operation_progress_render.rs`

唯一入口：

```rust
draw_operation_progress(frame, area, state)
```

三类操作不再分别维护 Running 布局。

### 7.1 标准页面结构

```text
┌ <操作标题> ───────────────────────────────────────────────┐
│ 目标        disk5 · model · size                         │
│ 状态        当前阶段                      已运行 8s       │
├ 总体进度 ────────────────────────────────────────────────┤
│ 37%  ███████████████░░░░░░░░░░░░░░░░░                  │
│ 当前阶段    元数据 / 事务写入 / 格式化 ...              │
├ 当前步骤 ────────────────────────────────────────────────┤
│ 写入协议区域                                             │
│ 100 / 1000 sector · 10%                                 │
│ ████░░░░░░░░░░░░░░░░░░░░░░░░░░                        │
├ 最近活动 ────────────────────────────────────────────────┤
│ ✓ 身份复核完成                                           │
│ ✓ 锁卷完成                                               │
│ → 正在写入协议区域                                       │
└──────────────────────────────────────────────────────────┘
```

统一：

- Panel / Card 风格；
- Gauge 样式；
- 标题层级；
- body / secondary / muted 文字；
- 运行时间；
- 当前活动；
- 日志窗口；
- 退出安全语义；
- 窄终端降级布局。

### 7.2 操作名

仅标题不同：

- Backup：`备份制作`
- Restore：`恢复备份`
- Provision：`制盘执行`

---

## 8. 日志与高频进度彻底分离

### 8.1 Progress Snapshot

以下事件只更新 latest / gauges：

- sector current 变化；
- bytes current 变化；
- 相同步骤内的高频 transaction activity；
- 格式化连续写入 / readback progress。

不进入日志 ring。

### 8.2 Log Entry

日志只记录语义事件：

- phase start；
- step start；
- step completed；
- warning；
- error；
- rollback start / result；
- retry；
- fallback；
- operation complete。

建议新增：

```rust
pub enum LogPolicy {
    SnapshotOnly,
    Append,
    AppendOnChange,
}
```

或者 application 层直接区分：

```rust
ProgressUpdate::Snapshot(...)
ProgressUpdate::Log(...)
```

### 8.3 UI 更新节流

底层仍可高频报告真实进度，但 TUI 没必要每 sector 重绘。

建议：

- 状态保存最新值；
- TUI refresh 目标 10～20 Hz；
- phase / warning / error / completion 事件立即刷新；
- 不丢失最终 current / total。

节流只能在 UI / event transport 层做，不允许牺牲事务安全与底层真实计数。

---

## 9. Backup Progress

备份是只读长操作，也必须显示统一 Running 页。

建议步骤：

```text
1. 目标身份复核
2. 几何读取
3. 分区结构读取
4. EDP 协议元数据读取
5. EDPB 生成
6. SHA-256 / 工件校验
7. 完成
```

要求：

- 不显示虚假的 sector progress；
- 有真实 byte / sector 数时才显示当前步骤 Gauge；
- 没有连续计数的短步骤只显示当前状态；
- 完成后自动触发备份列表 refresh。

---

## 10. Restore Progress

建议步骤：

```text
1. 目标身份复核
2. 备份完整性与几何校验
3. 卸载 / 锁卷 / reopen
4. 元数据事务写入
5. sync
6. readback
7. 写后身份检查
8. 后处理评估
9. 完成
```

恢复仍必须保留：

- physical identity pin；
- system disk / whole USB checks；
- geometry checks；
- lock / reopen identity recheck；
- atomic transaction；
- sync / readback；
- rollback。

UI 重构不得降低任何写盘安全门槛。

---

## 11. Provision Progress

现有 `provision/running_render.rs` 的信息可以迁移，但不得继续保留为独立最终实现。

重点修复：

1. 顶部总体 Gauge 改为 overall；
2. 当前步骤单独 Gauge 使用 work；
3. 当前步骤显示真实：
   - current；
   - total；
   - unit；
   - percent；
4. sector progress 不进入 log；
5. phase / step 日志只记录状态边界；
6. Backup / Restore 与 Provision 最终使用同一 renderer。

---

## 12. 实施阶段

### P0 — 失败测试锁定现状

先写失败测试，不改实现。

必须锁定：

- Devices 按 `b` 后 Workspace 底图仍存在；
- 一次 Enter 即 Confirm -> Running；
- 不再存在两层 BackupCreate confirm；
- Esc 关闭 Modal 后焦点 / 选择不变；
- Backup / Restore / Provision Running 使用同一 renderer；
- overall 随 work 连续增加；
- high-frequency work 不污染 log。

### P1 — 删除 BackupCreateChoiceState

完成：

- `b` 直达唯一 BackupCreate Confirm；
- 删除 create_choice 状态和 input handler；
- Confirm 作为 overlay。

P1 完成后先单独验收：

```text
Devices -> b -> Modal -> Enter -> Running
```

### P2 — 建立共享 OperationRunState

把三类操作统一到一个 Running state。

只迁移状态，不先改底层 progress contract。

### P3 — ProgressEvent 语义拆分

引入：

- overall progress；
- work progress；
- log policy / semantic log event。

建立 progress span / interpolation。

### P4 — 共享 Operation Progress Renderer

创建唯一共享页面。

先让三类操作都能用同一 renderer，哪怕部分 backend 仍经过 adapter。

### P5 — Backup / Restore 接入统一 ProgressEvent

清理：

- `write_progress_text()` 作为主进度来源；
- Wizard 私有 progress log。

### P6 — Provision 接入最终模型

迁移现有 provision running renderer，修复：

- overall / work 双 Gauge；
- high-frequency log spam；
- overall 子进度联动。

### P7 — 事件合并 / UI 节流

确保：

- 高频 I/O 不刷日志；
- 最新 progress 不丢；
- 页面稳定；
- CPU / 终端重绘合理。

### P8 — 技术债清理与门禁

最终 grep：

- 不存在 `BackupCreateChoiceState`
- 不存在第二套 long-operation progress renderer
- 不存在 backup/restore 私有 progress log
- 不存在每 sector append log 的实现

---

## 13. 必须新增的测试

### 13.1 Backup Confirm

```text
backup_b_opens_overlay_without_replacing_devices_workspace
backup_confirm_requires_only_one_enter
backup_confirm_escape_preserves_device_focus_and_selection
```

### 13.2 Shared Renderer

```text
backup_restore_provision_share_operation_progress_renderer
operation_progress_layout_is_stable_across_operation_kind
```

### 13.3 Progress Math

```text
overall_progress_interpolates_current_work
overall_progress_is_monotonic
overall_progress_reaches_100_percent_on_success
overall_progress_does_not_divide_by_zero
```

示例断言：

```text
span 2000..6500
work 100/1000
overall = 2450
```

### 13.4 Log Coalescing

```text
sector_progress_updates_snapshot_without_appending_log
step_change_appends_one_semantic_log_entry
warning_and_error_are_never_coalesced_away
final_progress_snapshot_is_preserved
```

模拟 1000 个 sector update：

- latest = 1000/1000；
- log 不得增加 1000 行；
- 只允许阶段 / step / 完成语义日志。

### 13.5 Overlay Geometry

用 `TestBackend`：

1. 渲染 Devices 基础页；
2. 按 `b` 后渲染；
3. Modal 外区域必须与基础页一致；
4. footer 固定消息栏位置不变化。

---

## 14. 架构门禁

在 `tests/architecture_split.rs` 增加：

- BackupCreate 不得再拥有 create-choice 中间状态；
- Running UI 只有一个共享 owner；
- ProgressEvent 只有 application 层定义；
- renderer 不得自行计算业务 overall；
- Backup / Restore / Provision 不得各自维护重复 Gauge / Running 页面。

不允许通过提高文件行数上限掩盖拆分问题。

---

## 15. 验证顺序

每阶段：

```bash
cargo fmt --all
git diff --check
```

专项测试先行。

最终必须：

```bash
cargo test --locked --test tui_suite
cargo test --locked --test repository_suite architecture_split -- --nocapture
scripts/test-fast.sh
python3 scripts/test-full.py --profile full
```

注意 WebCodex 环境 plain PATH 可能找不到 `cargo`，必要时使用：

```bash
zsh -lic 'cd /Users/zhangyuxi/Desktop/edpcli && python3 scripts/test-full.py --profile full'
```

Virtual Disk / real USB HIL 与 fast/full 分开。

如本轮修改触及真实写盘 transaction / restore / provision backend，必须补受影响的 Virtual Disk HIL；若只修改事件投影和 TUI renderer，也至少运行已有 progress / transaction 专项测试。

---

## 16. Git 与安全要求

- 开始前：

```bash
git status --short --branch
git rev-parse HEAD
git log -8 --oneline --decorate
git fetch origin
```

- 禁止：
  - `git reset`
  - `git clean`
  - 覆盖他人未提交修改
  - 因文档描述落后而重复已经完成的恢复 Modal / 文字颜色治理
- 测试先行；
- 小步 commit；
- 每完成一个稳定阶段及时 push；
- 所有新 WebCodex session 遵循 `AGENTS.md` 建立 `audit/ai-progress/` 日志。

---

## 17. 最终验收标准

只有全部满足才算完成：

1. Devices 按 `b`：
   - 只弹一次 Modal；
   - Modal 背后仍是当前 Devices 页面；
   - 一次 Enter 直接开始备份。
2. Restore：
   - `R -> YES -> Running`；
   - 无额外 Review / Confirm 页面。
3. Backup / Restore / Provision：
   - 完全使用同一 Running 页面；
   - 同一 Theme / Gauge / Card / footer / log 规则。
4. Progress：
   - 总体 Gauge 连续响应当前步骤进度；
   - 当前步骤显示 `current / total / unit / percent`；
   - overall 单调且最终 100%。
5. 日志：
   - 不逐 sector 打印；
   - 只保留语义事件；
   - warning / error / rollback 不丢失。
6. 架构：
   - 删除 `BackupCreateChoiceState`；
   - 删除重复 Running renderer / progress state；
   - application progress contract 成为唯一事实源。
7. 门禁：
   - fmt / diff check；
   - TUI suite；
   - architecture suite；
   - fast；
   - full；
   - 必要 HIL 全通过。

