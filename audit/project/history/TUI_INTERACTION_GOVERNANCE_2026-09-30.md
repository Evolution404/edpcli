# TUI 交互、结果表格与长任务界面统一治理计划（2026-09-30）

> 状态：**已完成 / CLOSED**
>
> 收口基线：`10ba2f7fa17b16a729c26068d846d68dd6cbd5b3`。G0–G6 已落地；`Inspect J`、全屏 `Hex`、`Result` 共享表格交互、输入优先级、全局居中 `Modal`、`OperationProgress`、`Help/Shortcut` 门禁均已有回归覆盖。
>
> 2026-10-05 最终证据：本机 `fast/full/clippy/release` 全通过，`make install` 成功且安装产物 `SHA-256` 与 `release binary` 一致；`TUI demo` 16/16、`keymap` 31/31、`Result Workbench` 4/4、`Inspect/Hex/Jump` 31/31；真实 `disk4` 只读 `info` 与 LBA0–12 `inspect meta` 成功。`Virtual Disk HIL` 在 `macOS arm64`、`Linux arm64/x86_64`、`Windows arm64/x86_64` 全部通过；最新 `Rust CI`（含 `Windows x86_64 full/clippy/release`）全部成功。
>
> 原计划中的“`PTY/TUI` 人工验收”不作为可重复 `CI` 证据：当前自动化执行环境不提供完整终端模拟器的光标应答能力，因此最终门禁采用生产 `renderer` 的 `Ratatui TestBackend`、确定性 `demo` 场景和真实 `keymap/controller` 测试；真实 `USB` 只执行只读检查，未做破坏性写盘。

## 1. 背景与目标

本轮集中收口用户在 2026-09-30 实机审核中指出的全部未完成问题。目标不是继续按页面补丁，而是建立可复用的 TUI 交互契约，使“同一种 UI 元件在所有页面具有相同操作语义”。

当前基线：
- main 基线：`bec1835cd4797cf57171c7c969048fbe5f8a6782`
- 已完成：`Provision/Restore Result Workbench` 的 `h/l` 不再越界切 Pane；`Ctrl-w h/j/k/l` 已有空间导航入口。
- 本计划执行期间禁止 `git reset` / `git clean`，保留 stash 和其他用户数据。
- 小步提交，阶段完成即测试、提交、push。

最终原则：
1. Workspace、Pane、Widget、Table 四种导航职责分离。
2. Result 表格必须真正接入共享 Table Interaction，不允许视觉像表格、交互却自建。
3. `Inspect` 的 LBA Jump 是 workspace 级能力，由单键 `J` 触发居中模态框。
4. 长任务页只回答“整体到哪、当前干什么、发生过什么、是否安全”，不重复信息。
5. Modal 必须基于全终端 viewport 居中，不能以局部 Pane 为坐标系。
6. Keymap、Help、底部提示和测试必须尽量由同一套元数据约束。

---

## 2. 本轮明确变更

### 2.1 删除快捷键

彻底删除：
- `gt`
- `gT`
- `gl`

要求：
- 删除 KeyMapper 映射。
- 删除 `TuiAction::WorkspaceNext / WorkspacePrevious` 等只为 `gt/gT` 存在的死 Action。
- 删除 controller/runtime_input 分发。
- 删除 Help、TUI.md、测试和底部提示残留。
- `g` 前缀只保留仍明确需要的 Vim 语义（例如 `gg`）。

`NavCommand::NextWorkspace / PreviousWorkspace` 如仍被 Tab 的既有设备/备份流程使用，可保留；不得为了删除 `gt/gT` 误删其它合法调用。

### 2.2 `Inspect Jump` 改为 J

`J` 是 `Inspect Browser` 的 workspace 级“跳转到 LBA”命令。

适用范围：
- 结构树
- 节点概览
- 节点详情
- 字段表
- `Hex / Sector Inspector`

行为：
1. 当前在 `Inspect Browser` 任意 Pane 按 `J`。
2. 背景页面、当前 Pane、当前选择不改变。
3. 屏幕中央打开独立 Modal。
4. Modal 仅输入扇区号（LBA）；允许十进制或 `0x` 前缀十六进制。
5. Enter 校验并跳转；Esc 关闭并恢复原上下文。
6. 输入错误直接在 Modal 内显示，不污染全局 `message bar`。
7. `Inspect Running` 阶段不执行 Jump，并给出明确提示。
8. 删除 Jump 中原有的 byte offset / Space 切换语义；`J` 专门代表 Jump to LBA。

### 2.3 `Inspect` 视图模式与 Pane 焦点解耦

当前 `1/2/3` 的显示语义不能继续依赖 Pane focus / scroll_x / sector 是否打开间接表达。

建立显式：
```rust
enum InspectViewMode {
    Business,
    RawFields,
    Hex,
}
```

职责：
- `1` -> Business
- `2` -> RawFields
- `3` -> `Hex`

Pane focus 仍独立维护：
- Tree
- Overview
- Detail

Jump 提交时保存 `origin_view` 和必要的 origin pane 上下文。

跳转结果：
- Business：定位目标 LBA 后继续显示业务语义/Decode 页面。
- RawFields：定位目标 LBA 后继续显示原始字段表。
- `Hex`：读取目标 sector 后继续显示 `Sector Inspector/Hex`，并保留 Raw/Decode/Mixed 子模式，不强制重置。
- 不允许 Jump 强制把用户切回 Tree 视图。

### 2.4 Jump Modal 必须全局居中

Jump Modal 使用整个 `frame.area()` / 全终端 viewport 计算居中矩形。

禁止：
- 在 Detail Pane 内追加输入行。
- 以当前 Pane 的 rect 做 centered_rect。
- 打开 Modal 时切换 pane focus。

---

## 3. Result 分区结果表统一

### 3.1 当前问题

`Provision Result / Restore Result` 的“分区结果”视觉上是表格，但没有真正进入共享 `TableInteractionState`，导致：
- `0/$` 不生效；
- `y/Y` 不生效；
- `H/L`、`</>`、`s/S` 容易继续分叉；
- 自己维护 `partition_active_column`。

### 3.2 目标

将 `Provision/Restore Result` 分区表正式接入共享 Table 层。

统一行为：
- `j/k`：行选择；
- `h/l`：激活列；
- `0/$`：第一列/最后一列；
- `H/L`：横向 viewport 平滑滚动；
- `</>`：移动整列；
- `s/S`：排序/恢复默认排序；
- `y/Y`：复制当前单元格/当前整行。

复制必须按当前视觉列顺序输出。

删除/收敛：
- Result 自建的 active-column 状态；
- Result 页面独立的 copy 分支；
- content navigation 到边界时 fallback 成 Pane navigation 的任何逻辑。

`Provision Result` 与 `Restore Result` 必须共用同一交互模型。

---

## 4. 全局输入优先级治理

统一优先级：

### Level 0 — Hard Global
- `Ctrl-C`：全局退出意图。
- 安全写盘事务中只能 Deferred Quit，不能硬中断事务。

### Level 1 — Modal / Input
- Insert/Search/Command/Confirm/Modal 输入优先处理普通字符。
- `q` 在输入模式中必须是字符 q。
- Esc 取消/返回。
- Enter 提交。

### Level 2 — Pane / Focus
- `Tab / Shift-Tab`：当前层级焦点循环。
- `Ctrl-w h/j/k/l`：空间 Pane 导航。
- `Ctrl-w w/W`：顺序 Pane 导航。

### Level 3 — Workspace command
- `Inspect`：`J`、`1/2/3`、`/`、`n/N` 等。
- Provision/Devices/Backups 使用各自业务动作。

### Level 4 — Widget / Table
- `j/k/h/l/0/$/H/L/</>/s/S/y/Y` 等。

禁止页面在进入中央策略前无条件吞掉应由更高层处理的 Action。

---

## 5. Provision 表单 h/l 方向语义

当前 `h`、`l` 可能都落到同一个 toggle/cycle 行为。

目标：
- `h` = previous
- `l` = next
- `Space` = toggle / cycle

对于容量单位、文件系统等多值项必须体现方向；布尔项可保持左右等价，但不得通过“一律 toggle”实现所有字段。

---

## 6. Planning / Exporting 等等待 Modal 全局居中

当前“只读规划 · 生成计划”等 Modal 以 Provision 内容区/局部 Pane 为坐标系，因此明显偏左下。

目标：
- Planning
- Exporting
- 其它同类 waiting/modal surface

统一使用全局 `overlay`：
- 基于整个 `frame.area()` 居中；
- 背景页面保留可见；
- modal 自身不改变底层 pane focus；
- 宽高使用统一 modal helper，避免页面各自算百分比/offset。

验收：
- 160×45、120×36、窄终端下仍以全 viewport 中心为基准；
- 不因左侧/右侧 Pane 比例变化而偏移。

---

## 7. OperationProgress 信息架构统一

`Provision / Backup / Restore` 共用同一个 OperationProgress Contract。

页面分四层：

### 7.1 总体进度
仅显示整个任务进度：
- 单行 Gauge；
- 百分比；
- 不重复当前 step 文案。

### 7.2 当前任务
原“当前状态”改为“当前任务”。

固定信息：
- 阶段：N / M + 阶段名；
- 当前步骤：最醒目；
- 当前 WorkProgress：如 `7 / 13 sector · 53%`；
- 动态描述：如“正在写入协议扇区”；
- 最近活动时间：如“0s 前”。

禁止重复“扇区活动/事务写入/协议事务写盘”等近义字段平铺。

### 7.3 运行记录
原“运行日志 · 最近语义活动”改为“运行记录”。

只记录：
- step started；
- step completed；
- warning；
- failure；
- rollback；
- 重要 checkpoint。

推荐结构：
```text
时间       状态  阶段   事件
12:48:21   ✓    备份   元数据备份完成
12:48:24   •    写入   开始协议事务写盘
12:48:29   ✓    写入   协议事务写盘完成 · 13 sectors
```

禁止按 sector 刷历史日志。

### 7.4 安全提示 / Footer
当前存在：
1. 页面自己的 safety hint；
2. 多分配一行导致黑色空行；
3. global dynamic status 又重复“安全写盘事务执行中…”。

目标：
- Operation Running 页底部只保留一行安全提示；
- 删除 `Constraint::Length(2)` 但只渲染一行造成的空行；
- Running 阶段不再重复显示等价的 global dynamic status；
- notice/warning 只有真实事件发生时才显示；
- `Backup/Restore/Provision` 共用同一规则。

---

## 8. Help / 文档 / 快捷键真相源

立即修正已知过期内容：
- `Inspect Help` 中 `1/2/3/4` 改为 `1/2/3`；
- 删除 `gt/gT`；
- 删除 `gl`；
- 新增 `J = 跳转到 LBA`；
- Result 表格 Help 必须包含完整 Table Contract。

阶段性目标：
- Keymap、Help、底部 hint、测试尽量由同一 Shortcut/Capability 元数据驱动；
- 至少建立架构测试，禁止 Help 声明不存在的快捷键。

---

## 9. 架构门禁

必须增加/维护以下测试：

1. `g + t` / `g + T` / `g + l` 均无绑定。
2. `J -> InspectJump`。
3. `Inspect Browser` 各 Pane/`Hex` 都能打开 Jump Modal。
4. Jump Modal 基于全 viewport 居中。
5. Jump `Business/RawFields/Hex` 后仍保持原 ViewMode。
6. Result 分区表必须报告为共享 Table。
7. Result 支持 `0/$`。
8. Result 支持 `y/Y`，复制内容与当前视觉列顺序一致。
9. `h/l` 到表格边界不切 Pane。
10. Insert/Search/Command 中 `q` 是字符，不是 Quit。
11. Operation Progress safety footer 只占一行。
12. Running 页面不重复 dynamic safety status。
13. `Inspect Help` 不得再出现 `4` / `gl` / `gt/gT`。
14. Planning/Exporting Modal 中心以全 viewport 为基准。

---

## 10. 实施顺序

### G0 — 当前快捷键残留清理
- 删除 `gt/gT`；
- 删除 `gl`；
- `J -> InspectJump`；
- 更新 Help/docs/tests。
- 独立提交。

### G1 — `Inspect ViewMode` + Jump Modal
- 建 `InspectViewMode`；
- Jump `state` 改为真正 `overlay modal`；
- 取消 begin_jump 时强制清 sector/切 Tree；
- 实现 `Business/RawFields/Hex` 的跳转后呈现；
- 独立提交。

### G2 — Result Table Contract
- `Provision/Restore Result` 接入共享 Table；
- 完成 `0/$ H/L </> s/S y/Y`；
- 删除专用 active-column 状态；
- 独立提交。

### G3 — Input Policy + Provision Form
- 修 q/Ctrl-C 输入优先级；
- 收口 Tab/Ctrl-w；
- 修 Provision `h/l` previous/next；
- 独立提交。

### G4 — Modal 全局居中
- Planning/Exporting/等待 Modal 统一 `overlay helper`；
- 以 `frame.area()` 居中；
- 独立提交。

### G5 — OperationProgress 信息架构
- 当前任务；
- 运行记录；
- Footer Contract；
- 删除重复安全状态与黑空行；
- `Provision/Backup/Restore` 同步；
- 独立提交。

### G6 — Help / Shortcut `metadata` / architecture gates
- 收口统一元数据；
- 补全部回归门禁；
- 更新 `docs/ui/TUI.md`；
- 独立提交。

---

## 11. 最终验收

每阶段：
- `cargo fmt --all`
- `git diff --check`
- focused tests
- 小步 commit + push

最终：
1. `scripts/test-fast.sh`
2. full gate
3. Clippy / release build
4. Virtual Disk HIL（涉及实际写盘逻辑未变也必须确认无回归）
5. `make install`
6. 校验 `target/release/edpcli` 与 `~/.local/bin/edpcli` SHA-256 一致
7. PTY/TUI 人工验收：
   - Result 表 `0/$ y/Y H/L </>`
   - `Inspect` `J` 各 ViewMode
   - Planning Modal 全局居中
   - Progress 页“总体进度 / 当前任务 / 运行记录 / 一行安全提示”
   - q/Ctrl-C/Tab/Ctrl-w
8. 只读真实 USB 检查；除非用户明确要求，不做破坏性真实写盘。
