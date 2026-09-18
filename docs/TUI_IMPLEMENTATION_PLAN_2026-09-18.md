# edpcli TUI 实施计划

日期：2026-09-18  
分支：`feat/tui-vim-interface-20260918`  
基线：`main@19d50ef286beed330636b10a3b08b9e080b8ecfe`  
目标发布版本：`v2.1.0`（仅在正式发布阶段升级；本分支实现阶段保持当前版本号）

## 1. 目标与边界

在现有 CLI v2 基础上新增跨 macOS / Linux / Windows 的交互式 TUI，使用 `ratatui + crossterm`。TUI 是现有业务能力的新前端，不是第二套业务实现。

核心交互采用 Vim 风格键位：

- `j/k`：上下移动；
- `h/l`：左右切换、展开/收起或在多列区域间移动；
- `gg/G`：首项/末项；
- `Ctrl-d/Ctrl-u`：半页下翻/上翻；
- `/`：搜索；
- `n/N`：下一个/上一个匹配；
- `:`：command palette；
- `Esc`：返回上一层或退出当前输入态；
- `q`：退出非关键态；
- `?`：帮助。

首版 TUI 覆盖：

1. Device dashboard；
2. Backup workspace；
3. Apply / Restore 安全向导；
4. Inspect / hex；
5. 搜索、command palette、帮助；
6. resize / TTY / 异常退出 / 跨平台行为。

现有 CLI v2 命令、脚本、help、completion 和自动化接口继续保留并保持行为兼容。TUI 不替代脚本化 CLI。

## 2. 不可回退的安全红线

TUI 不得复制、缩短、旁路或重新解释现有写盘安全链。以下能力必须由 CLI/TUI 共用的 application/service 层统一执行：

- 系统盘 fail-closed；
- USB 外接整盘确认；
- selector pinning；
- 写前 LBA0-13 快照与自动备份；
- 卸载 / 锁卷；
- reopen 后设备身份与元数据二次复核；
- atomic write；
- sync；
- readback 校验；
- 失败 rollback；
- restore 的 onlyid / LBA4 防串盘终验；
- 既有 Linux/Windows virtual-disk HIL 安全边界。

TUI 只负责收集用户意图、展示状态和驱动 application/service API，不直接调用底层 raw write primitive 组成新的写盘流程。

## 3. 架构方案

### 3.1 分层

目标分层：

```text
cli_args / CLI renderer ─┐
                        ├─> application/service ─> selectors / domain / diskio / platform
tui/event/state/render ─┘
```

计划新增/重构模块：

- `src/application.rs`
  - 面向“任务”的同步 service facade；
  - 暴露 list/info/backup/inspect/apply/restore 所需的稳定输入输出模型；
  - 负责复用现有 selector、备份、识别、写盘安全链。
- `src/tui/mod.rs`
  - TUI 入口与 terminal lifecycle。
- `src/tui/state.rs`
  - 纯状态机，不直接做阻塞 I/O。
- `src/tui/event.rs`
  - 键盘、resize、tick、后台任务结果事件。
- `src/tui/render.rs`
  - ratatui 视图渲染。
- `src/tui/command.rs`
  - `:` palette 的解析与 dispatch。
- `src/tui/task.rs`
  - 设备扫描、备份扫描、只读 inspect 等后台任务调度。

具体文件可在实现过程中按职责继续拆分，但不得让 TUI 直接依赖平台 API。

### 3.2 application/service 层原则

先抽 service，再做 TUI。每个 service 必须满足：

- CLI 与 TUI 共用同一函数/对象；
- 返回结构化结果，而不是强耦合 `println!`；
- 需要用户确认时，以显式 decision/prompt 边界表达；
- 写盘关键阶段由 service 控制生命周期，不由 UI 控制；
- 能通过 fake runner / fake sector device / fake prompt 做测试；
- 不因 TUI 引入第二套 selector、backup catalog 或 raw-disk transaction。

现有 `Prompter`、`Ctx`、`DeviceSelector`、`BackupSelector`、`create_backup`、atomic write / rollback 测试能力优先复用。

### 3.3 TUI 状态机

建议顶层状态：

```text
Dashboard
BackupWorkspace
Inspect
Wizard(Apply | Restore)
Search
CommandPalette
Help
FatalError
```

写盘向导内部至少包含：

```text
SelectTarget
Preflight
Plan
Confirm
CriticalOperation
Result
```

`CriticalOperation` 为不可粗暴中断区：

- `q` / `Esc` 不退出；
- `Ctrl-C` 不直接终止进程；
- UI 只显示“关键写盘阶段不可中断”；
- service 完成到安全检查点后再响应取消/退出；
- 若底层返回错误，统一进入已有 rollback 路径。

取消语义必须区分：

- 尚未进入写盘关键阶段：允许安全取消；
- 已进入关键阶段：延迟取消，直到 service 到达可安全退出点。

### 3.4 非阻塞 redraw

以下动作不得阻塞 redraw：

- 设备扫描；
- 备份目录扫描；
- 备份元数据解析；
- 只读 inspect 数据读取；
- 可明显超过一帧预算的文件系统操作。

实现约束：

- UI 主线程只处理 terminal event、状态更新和 render；
- 后台任务通过 channel 把结果投递回主事件循环；
- 使用 generation/request id 防止旧扫描结果覆盖新状态；
- 同类扫描去重，避免 resize 或键盘事件触发重复全量扫描；
- 首版不要求 async runtime，优先使用标准线程 + channel，减少 runtime 复杂度。

写盘 service 可以占用独立 worker，但状态机必须明确进入 `CriticalOperation`，且不能因前端退出而丢弃线程或孤立写盘任务。

## 4. 命令入口

计划新增显式入口：

```text
edpcli tui
```

是否让“裸 `edpcli`”自动进入 TUI 不在本次默认范围内，避免破坏现有 CLI v2 的“裸命令 == list”契约。后续如要调整，必须单独评审并更新 surface guard。

TUI 内 `:` command palette 采用任务语义，不重新实现 shell parser。首版候选：

- `:devices`
- `:backups`
- `:inspect`
- `:apply`
- `:restore`
- `:refresh`
- `:help`
- `:quit`

可以支持带少量 UI 参数，但不得形成与 CLI v2 重复的第二套完整 grammar。

## 5. UI 信息架构

### Device dashboard

- 左侧/主区：设备列表；
- 详情区：容量、总线、VID:PID、onlyid、device_id、User、Dept、EDP/免密状态；
- 底部状态栏：快捷键、扫描状态、错误提示；
- 多盘时 `j/k` 选择；
- `Enter/l` 进入详情或操作区；
- `r` 可作为 refresh 快捷键，但不是必需 Vim contract。

### Backup workspace

- 全局稳定编号与 CLI `backup list` 保持同源；
- 显示备份所属盘、时间、User/Dept、免密/加密原盘、MD5 健康状态；
- 可从当前设备过滤对应备份；
- restore 仍由 application/service 做 onlyid/LBA4 防串盘终验。

### Apply / Restore 安全向导

展示而不重写安全逻辑：

1. 目标设备；
2. 当前身份与状态；
3. 操作计划；
4. 将创建/使用的备份；
5. 风险提示；
6. 最终确认；
7. 安全链阶段进度；
8. 成功/rollback/错误结果。

确认不得用单键误触直接写盘。首版保留高意图确认，例如需要输入 `YES`，或等价的明确双阶段确认；具体实现以现有 CLI 安全语义为最低标准。

### Inspect / hex

- LBA 0-13 列表；
- `j/k` 选择 LBA；
- `Enter/l` 展开；
- raw/decoded/结构化字段切换；
- hex 视图支持滚动；
- `/` 搜索 hex/ASCII/字段文本；
- 不改变 `inspect.rs` 的领域解析逻辑。

## 6. Phase 0 → 7

## Phase 0 — Contract 门禁

先写失败测试，锁定架构与安全边界：

- TUI 依赖必须存在但版本号不得提前升级到 2.1.0；
- CLI v2 parser/help/completion/surface guard 全部继续通过；
- TUI 模块不得直接调用平台 raw write API；
- TUI apply/restore 必须经过 application/service；
- 现有 system-disk/USB whole-disk/selector pinning/backup/reopen/atomic/sync/readback/rollback 合约不得弱化；
- `edpcli tui` parser contract；
- 非 TTY 下的预期行为先定义并测试。

完成标准：先看到新 contract tests 在旧实现上失败，再进入 Phase 1。

## Phase 1 — TUI / event / state 基础

- 引入 `ratatui`、`crossterm`；
- terminal enter/leave RAII；
- panic/error 时恢复 alternate screen / raw mode；
- event loop；
- Vim 导航基础：
  - j/k/h/l
  - gg/G
  - Ctrl-d/u
  - Esc/q/?
- resize；
- 基础 snapshot/state tests。

## Phase 2 — Device dashboard

- 先把 device scan / info 数据抽到 application/service；
- CLI list/info 改为消费相同结构化结果；
- TUI 后台设备扫描；
- generation 防陈旧结果；
- dashboard 渲染；
- 不阻塞 redraw 的回归门禁。

## Phase 3 — Backup workspace

- 抽 backup catalog/list/verify 所需结构化 view model；
- CLI 和 TUI 共用 `BackupSelector` 稳定编号；
- 后台备份扫描；
- workspace 选择、过滤、详情；
- User/Dept、MD5、免密状态完整展示；
- backup create 通过共用 service 执行，保持纯只读语义。

## Phase 4 — Apply / Restore 安全向导

- 先把现有 apply/restore orchestration 抽入 application/service；
- CLI 调用新 service 后行为保持不变；
- TUI 仅驱动同一 service；
- 明确 critical section；
- q/Esc/Ctrl-C 在 critical section 不得直接终止；
- 失败必须走既有 rollback；
- fake disk 测试覆盖：
  - 系统盘拒绝；
  - 非 USB 整盘拒绝；
  - pinning；
  - 自动备份；
  - reopen 身份改变拒绝；
  - readback 失败 rollback；
  - restore 串盘拒绝。

## Phase 5 — Inspect / hex

- 抽只读 inspect service；
- TUI LBA/fields/hex 展示；
- 搜索当前内容；
- 大量 hex 滚动性能门禁；
- 禁止 TUI 自己复制 sector decode 算法。

## Phase 6 — 搜索与 `:` command palette

- `/`、`n/N`；
- `:` palette；
- 命令历史可选；
- 所有 palette action 映射现有 TUI intent/service，不形成第二套业务实现；
- `?` 帮助覆盖 Vim 键位。

## Phase 7 — 跨平台 / resize / TTY / 异常退出 / 性能门禁

CI 必须覆盖 macOS / Linux / Windows：

- build/test/clippy；
- terminal state unit tests；
- resize；
- 非 TTY；
- EOF；
- Ctrl-C；
- panic/error terminal restore；
- 后台扫描不会阻塞 render；
- 陈旧扫描结果不会覆盖新 generation；
- 写盘 critical section 不被 q/Esc/Ctrl-C 粗暴中断；
- CLI v2 全量回归；
- Linux/Windows virtual-disk HIL 保持全绿。

性能目标以“交互不卡顿”为约束，不用不稳定的墙钟绝对值做脆弱门禁。测试优先锁定：

- render/event loop 不执行设备/备份全量扫描；
- 单次 key event 不触发同步磁盘枚举；
- 同类后台扫描有限并发；
- 旧 generation 结果被丢弃；
- 大列表滚动只更新 state，不触发重新扫描。

## 7. 测试策略

新增测试建议：

- `tests/tui_contract.rs`
- `tests/application_service.rs`
- `tests/tui_state.rs`
- `tests/tui_safety.rs`
- `tests/tui_nonblocking.rs`

优先纯函数/状态机测试，减少依赖真实 TTY。必须真实 terminal 行为时使用可注入 backend/event source，CI 不依赖人工输入。

现有以下测试属于长期回归门禁，不得删除或降低断言：

- `cli_v2_parser.rs`
- `cli_v2_surface_guard.rs`
- `platform_boundary.rs`
- `platform_cli_matrix.rs`
- `atomic_write.rs`
- `backup.rs`
- `selectors.rs`
- `virtual_disk_hil.rs`

## 8. 提交策略

保持小 commit、及时 push。建议粒度：

1. `docs: add TUI implementation plan`
2. `test: lock TUI architecture contracts`
3. `refactor: introduce application service boundary`
4. `feat: add TUI event and state core`
5. `feat: add device dashboard`
6. `feat: add backup workspace`
7. `feat: add safe apply and restore wizards`
8. `feat: add inspect workspace`
9. `feat: add search and command palette`
10. `test: harden cross-platform TUI lifecycle and performance`
11. 最终发布阶段再做 `release: v2.1.0`

不把多个 Phase 混在一个大 commit 中。

## 9. 发布约束

实现阶段不改当前 `Cargo.toml` 版本号。

只有 Phase 0-7 全部完成、CI/HIL 全绿、PR review 收口后，正式发布时才：

- 按 SemVer 升级为 `2.1.0`；
- 更新 Cargo.lock、README/USAGE/RELEASE；
- 创建 `v2.1.0` tag；
- 沿用现有多平台 release matrix；
- 对 Release 资产、SHA-256、SBOM、manifest 做独立验收。

## 10. 完成定义

本 PR 可以进入合并/发布阶段的最低条件：

- TUI 功能覆盖 Phase 0-7；
- CLI v2 命令和脚本行为无回归；
- application/service 成为 CLI/TUI 共享唯一业务入口；
- TUI 无独立 raw-write orchestration；
- 设备扫描和备份扫描不阻塞 redraw；
- critical write 阶段 q/Esc/Ctrl-C 不会粗暴中断；
- macOS/Linux/Windows CI 全绿；
- Linux/Windows virtual-disk HIL 全绿；
- fmt/test/clippy 全绿；
- 文档与帮助完整；
- 正式发布前才升级到 v2.1.0。


## 实施状态

截至 2026-09-19，Phase 0–7 的代码路径已经完成，当前进入最终 CI/HIL 收口：

- application/service 已成为 CLI/TUI 共用边界；
- Device dashboard 与 Backup workspace 使用后台 generation worker；
- Apply/Restore 通过共享 write service，提权重启固定 disk/backup，关键阶段延迟退出；
- Inspect 复用领域 analyzer，支持字段、decoded/raw hex、搜索与滚动；
- `/`、`n/N`、`:` command palette 已实现；
- TTY、resize、terminal RAII、worker panic containment、非阻塞与大列表状态门禁已加入；
- CI 增加 rustfmt、locked test/clippy/build；
- 实现阶段仍保持 `2.0.1`，只有最终 CI/HIL 全绿并进入正式发布时才升级 `2.1.0`。
