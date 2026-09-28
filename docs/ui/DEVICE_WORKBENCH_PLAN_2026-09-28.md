# 设备工作台重构计划（2026-09-28）

状态：COMPLETE
实施分支：`feat/device-workbench-20260928`  
基线：`82234bea2b58db1dc3025f27e157d0c9fac27382`

## 0. 远端实施状态

截至 2026-09-28，本分支已完成并提交以下主体实现：

- P0：新设备工作台契约测试已建立；
- P1：语义化设备信息树与稳定节点选择已实现，旧摘要索引状态已移除；
- P2：设备 Pane 已收口为 `DevicesList / DevicesTree / DevicesDetail`；
- P3：页面已改为“上方设备列表 + 下方树/详情”结构，并删除冗余“当前设备”标题；
- P4：身份信息与状态诊断详情已接入；
- P5：容量布局使用 `DiskLayoutModel`，区域和尾部子节点可在当前页直接展开；
- P6：备份关系先使用现有已加载的确认/可能关联计数，不增加目录扫描；
- P7：协议摘要已接入，深度字段解析仍保留独立检查入口；
- P8：宽屏、中等宽度和紧凑宽度均使用现有响应式规则；
- P9：帮助文本与展示层拆分已完成；设备页渲染已拆为列表、信息树、详情与展示模型；
- P10：Mac 本机验证与 GitHub 跨平台门禁均已全部通过，本轮设备工作台重构正式收口。

### 本机验证证据

- `cargo fmt --all -- --check`：通过；
- `cargo test --locked --test tui_suite`：273/273 通过；
- `scripts/test-fast.sh`：4 个测试套件、6 个测试产物、0 失败；
- `python3 scripts/test-full.py --profile full`：8 个测试套件、10 个测试产物，加 doctest，0 失败；
- `cargo build --release --locked`：通过；
- `scripts/install-local.sh target/release/edpcli`：安装到 `~/.local/bin/edpcli`，版本 `2.5.0`；
- release 源二进制与已安装二进制 SHA-256 均为 `65be3512603cc6a6052227200517e9c226b723c5fd28bec53c2bccd3dcc2c778`；
- Python PTY smoke：检测到终端光标位置查询并正确回应，随后发送 `q`，进程正常以 `RC=0` 退出。

### 远端验证证据

- 提交 `de2405248d221243cee0902b7fc3341b39277972` 的 Rust CI（run `36377125428`）全部通过：格式、Linux/macOS/Windows 编译以及 Linux/macOS/Windows x86_64 完整测试与 Clippy 门禁均通过；
- 同一提交的虚拟磁盘 HIL（run `36377125435`）4/4 通过：Linux x86_64、Linux ARM64、Windows x86_64、Windows ARM64；
- 远端过程中发现并修复两项既有跨平台测试/构建门禁问题：Plain 只读观察测试夹具的几何证据仅在 macOS 可注入，以及 plist 解析器仅被 macOS 平台实现使用却曾无条件编译；两项修正均未改变生产协议、介质身份判定规则或写盘安全语义。

## 1. 目标

把“设备”一级 Tab 重构为一个完整设备工作台。页面只保留一个“当前设备”来源：**设备列表中的高亮行**。下方不再重复“当前设备 · diskX”、mode、容量、VID:PID、onlyid、备份数等快速摘要条。

最终页面分两层：

1. 上半区：设备列表，用于发现、筛选、比较和选择设备。
2. 下半区：选中设备的工作台，采用“左侧设备信息树 + 右侧上下文详情”的 Master/Detail 结构。

设备工作台必须在不进入深度检查页面的情况下，让用户直接看到并理解：

- 身份信息；
- 容量布局；
- 状态与诊断；
- 当前设备相关备份；
- 协议摘要。

深度检查页面仅保留为深入查看扇区、十六进制、字段和已验证解码结构的入口。

## 2. 明确不做

本轮是纯 TUI 信息架构与展示/状态重构。禁止改变：

- LBA0～12 / LCE 协议语义；
- mode0～mode3 与 Plain 语义；
- K6；
- LBA10 trailing 384B；
- device_id / onlyid；
- media identity；
- EDPB；
- 分区几何推导规则；
- 备份格式；
- 制盘事务和所有安全门槛；
- system disk guard；
- whole-USB confirmation；
- mandatory pre-write backup；
- unmount / lock；
- reopen identity verification；
- atomic write / readback / rollback。

渲染层不允许增加磁盘 I/O、文件系统扫描或备份目录遍历。所有展示只能消费现有状态/应用/领域数据。

## 3. 顶层导航不变

一级 Tab 仍只有：

```text
设备 | 备份
```

规则：

- 一级 `Tab/Shift-Tab`、`gt/gT`：只切“设备/备份”；
- 设备页内部 Pane 使用 `Ctrl-w h/j/k/l/w/W`；
- `j/k` 只解释为当前焦点 Pane 内的移动/滚动；
- `o` 只用于树节点展开/折叠；
- `i` 进入深度检查；
- `p` 制盘；
- `b` 创建当前设备备份；
- `r` 刷新；
- `q` 全局退出。

本轮不得重新改变刚收口的 Tab 层级规则。

## 4. 最终页面信息架构

### 4.1 Wide

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ 设备列表                                                                    │
│ > disk4 ...                                                                 │
│   disk5 ...                                                                 │
├───────────────────────┬─────────────────────────────────────────────────────┤
│ 设备信息树            │ 当前节点详情                                        │
│                       │                                                     │
│   身份信息            │                                                     │
│ ▾ 容量布局            │                                                     │
│   ├ EDP 主协议区      │                                                     │
│   ├ 空闲区域          │                                                     │
│   ├ 启动/交换区       │                                                     │
│   ├ 保密区            │                                                     │
│   ▸ 尾部区域          │                                                     │
│   状态与诊断          │                                                     │
│   备份关系            │                                                     │
│   协议摘要            │                                                     │
└───────────────────────┴─────────────────────────────────────────────────────┘
```

禁止出现：

```text
当前设备 · disk4
EDP mode1 · 健康 · 8.05 GB · VID:PID ... · onlyid ... · 备份 5
```

因为设备列表高亮行已是唯一当前设备来源。

### 4.2 Medium

- 设备列表约 42% 高度；
- 下部工作台约 58% 高度；
- Tree 约 35%，Detail 约 65%；
- Detail 中原本双栏的信息卡自动改为纵向布局。

### 4.3 Compact

一次只渲染一个设备 Pane：

```text
DevicesList -> DevicesTree -> DevicesDetail
```

由 `Ctrl-w` 导航，selection、展开状态、详情滚动不得丢失。

## 5. Pane 架构

淘汰：

```text
DevicesList
DevicesSummary
DevicesStats
```

改为：

```text
DevicesList
DevicesTree
DevicesDetail
```

含义：

- `DevicesList`：设备表；
- `DevicesTree`：设备信息树；
- `DevicesDetail`：右侧上下文详情。

`DevicesStats` 不再作为独立 Pane，相关信息并入“状态与诊断”。

## 6. 设备信息树

固定一级节点：

```text
Identity
Capacity
Status
Backups
Protocol
```

只有真正有子节点的节点显示 `▸/▾`。

容量布局展开时动态生成子节点：

```text
容量布局
├─ EDP 主协议区
├─ 空闲区域
├─ 启动/交换区
├─ 保密区
└─ 尾部区域
```

尾部区域继续展开：

```text
尾部区域
├─ LCE
├─ 空闲/保留区域
├─ 历史备份镜像
└─ 盘尾恢复节点
```

动态节点必须由 `DiskLayoutModel` 生成，不允许在 Devices 页面重新推导分区几何。

## 7. 状态模型

旧模型：

```rust
summary_selected: usize
summary_expanded: u8
```

不能继续作为新版树的主状态。

新增语义键，例如：

```rust
enum DeviceInfoNodeKey {
    Identity,
    Capacity,
    LayoutSegment {
        start_lba: u64,
        kind: DiskRegionKind,
    },
    TailGroup,
    Status,
    Backups,
    Protocol,
}
```

Tree presentation 使用稳定语义 key，不依赖“第几行”。

建议 presentation node：

```rust
struct DeviceInfoTreeNode {
    key: DeviceInfoNodeKey,
    depth: u8,
    label: String,
    value: Option<String>,
    expandable: bool,
    expanded: bool,
}
```

状态至少维护：

- selected node key；
- expanded node set；
- Devices pane focus；
- DevicesDetail viewport。

切换设备时：

- 能在新设备找到同一语义节点，则保持；
- 动态 LayoutSegment 在新盘不存在时回退到 Capacity；
- Detail scroll 回顶部；
- 不允许留下悬空 index。

## 8. 身份信息详情

右侧至少显示：

- disk；
- model；
- protocol/bus；
- 物理容量；
- VID:PID；
- serial；
- onlyid；
- device_id；
- department；
- user/name；
- confirmed provision kind；
- 身份可靠度；
- 身份依据。

身份可靠度必须复用现有 media identity / identity pin 的真实结论，不新建第二套判定。

## 9. 容量布局详情

### 9.1 Capacity 根节点

直接显示：

- 全盘比例条；
- legend；
- 区域表：
  - 区域；
  - LBA 闭区间；
  - sector count；
  - human capacity；
  - percentage；
  - 状态/类型。

删除现有：

```text
尾部区域详情可在 Inspect 全盘布局中按 o 展开
```

尾部必须在当前设备页面直接展开。

### 9.2 LayoutSegment

选中任一 segment 时右侧显示：

- label；
- kind；
- start LBA；
- end LBA；
- sector count；
- capacity；
- percentage；
- 当前 segment 在全盘 bar 中的位置；
- 只有已有证据的 protocol/partition role/file-system 等附加字段。

没有证据的内容不得猜测。

### 9.3 TailGroup

选中 TailGroup 时显示其覆盖范围、总容量和 children 表。

选择 child（LCE / BackupMirror / RestoreNode / Free 等）时，显示该 child 的真实几何信息。

## 10. 状态与诊断详情

只展示当前扫描阶段真实可知的信息，例如：

- USB / 外接设备状态；
- raw access 是否可用；
- EDP/Plain 是否已确认；
- identity evidence；
- 分区几何；
- LCE 几何；
- 规范化布局是否完整；
- backup relation count；
- probe error / denied reason。

禁止在这里声称只有制盘事务中才成立的 readback、rollback 等结果。

## 11. 备份关系详情

设备页只显示**当前选中设备相关的备份**。

备份一级 Tab 继续负责“全局所有备份”。

如果 AppState 目前只有：

- confirmed count；
- possible count；

则第一阶段先显示计数和状态，不允许 Renderer 临时遍历备份目录。

只有已有备份目录/状态能提供具体条目时，才展示条目表。

## 12. 协议摘要详情

只展示帮助用户快速判断设备的业务摘要，例如：

- EDP / Plain；
- official mode；
- LBA0～12 是否可确认；
- partition count；
- LCE 是否可确认；
- department / user / label / onlyid 等已经扫描出的关键协议信息。

完整字段/原始数据/十六进制始终通过 `i` 进入深度检查。

## 13. Presentation / Renderer 分层

本轮不要继续膨胀 `src/tui/devices/render.rs`。

目标结构：

```text
src/tui/devices/
├── mod.rs
├── state.rs
├── presentation.rs
├── render.rs
├── list_render.rs
├── tree_render.rs
└── detail_render.rs
```

职责：

- `presentation.rs`：真实领域/状态 -> UI 展示模型；
- `list_render.rs`：设备表；
- `tree_render.rs`：左树；
- `detail_render.rs`：右侧详情；
- `render.rs`：仅做响应式布局与组合；
- `state.rs`：树选择/展开/窗格状态。

Renderer 不做业务 I/O。

## 14. 视觉规则

继续使用现有低饱和 TrueColor Theme。

- 当前 Pane：focused border；
- 当前 Tree node：`▌` + accent；
- 磁盘区域：继续复用 `DiskRegionKind` 语义颜色；
- 通过/警告/错误：复用现有语义主题；
- 避免大面积高饱和 reversed background；
- 不引入另一套私有颜色。

## 15. 交互

### DevicesList

- `j/k`：换设备；
- 表格 `h/l, </>, 0/$, H/L, s/S` 保持当前统一契约；
- `Enter`：focus 到 DevicesTree；
- `Ctrl-w`：Pane 导航。

### DevicesTree

- `j/k`：移动可见节点；
- `o`：展开/折叠；
- `Enter`：focus 到 DevicesDetail；
- `Ctrl-w`：Pane 导航；
- `Esc`：优先返回 DevicesList focus。

### DevicesDetail

- `j/k`：滚详情；
- `gg/G`、`Ctrl-u/Ctrl-d`：视口导航；
- `Esc`：返回 DevicesTree focus。

业务动作仍基于当前设备：

- `i` 深度检查；
- `p` Provision；
- `b` Backup；
- `r` Refresh。

## 16. 测试门禁

必须新增正式回归：

1. 一级 Tab 仍只有 Devices/Backups；
2. 设备页不存在冗余“当前设备 · diskX”标题；
3. 不存在额外快速摘要条；
4. Pane 顺序为 DevicesList/DevicesTree/DevicesDetail；
5. 设备高亮变化会更新 Tree/Detail；
6. Capacity `o` 当前页展开；
7. TailGroup `o` 当前页展开；
8. segment 几何与 `canonical_layout()` 完全一致；
9. 规范化布局失败只显示原因，不猜布局；
10. Tree `j/k` 不滚 Detail；
11. Detail `j/k` 不改 Tree selection；
12. `Tab` 仍只切一级 Tab；
13. `Ctrl-w` 只切设备内部 Pane；
14. `i/p/b` 保持原业务入口；
15. 切换不同设备时动态 node selection 安全回退；
16. Compact 模式 Pane 切换不丢 selection / expanded / viewport；
17. Devices renderer 不新增 disk/file I/O；
18. 协议/写盘安全契约保持不变。

## 17. 实施阶段

### P0 — 契约测试
先写失败测试锁定新 Pane、无冗余标题、Tree/Detail 语义和 Tab 不跨层。

### P1 — 树领域/状态
实现 `DeviceInfoNodeKey`、展开状态、稳定选择与设备切换回退。

### P2 — Pane 架构
把 `DevicesSummary/DevicesStats` 迁移为 `DevicesTree/DevicesDetail`。

### P3 — 页面骨架
实现设备列表 + 下方 Tree/Detail 响应式骨架，删除“当前设备”标题和摘要条。

### P4 — Identity + Status
先实现身份和状态详情，验证 Master/Detail 基础架构。

### P5 — Capacity
接入 `row.canonical_layout()`；实现全盘 bar、segment table、区域详情和 TailGroup 当前页展开。

### P6 — Backups
接入已有备份状态/目录能提供的当前设备关联信息；没有条目证据时只显示真实计数。

### P7 — Protocol
实现轻量协议摘要，完整解析继续留给深度检查。

### P8 — Responsive
收口 Wide/Medium/Compact。

### P9 — Help/docs
已同步 footer、`?` help、当前计划状态，并完成设备展示层拆分。

### P10 — Validation
本机快速/完整门禁、release 构建、本地安装、PTY 启动/退出验证与 GitHub 跨平台门禁均已通过，关闭条件已满足。

### P11 — 设备信息导航与容量地图统一

本阶段按 2026-09-28 实机界面复核继续收口：

- 设备信息树统一支持 `gg/G`，分别跳到当前可见树的首节点和尾节点；设备详情中的 `gg/G` 继续保持顶部/底部滚动语义；
- 容量布局根节点、普通容量区域、尾部区域和尾部子节点共享同一张全盘容量地图，切换节点时地图不消失、不换结构，只更新激活区域和 `▲` 位置；
- 全盘容量地图使用“坐标层 + 三行区域带 + 当前区域说明层”，不再使用一行细进度条；
- 极小区域采用最小可见宽度，真实 LBA、容量和占比不做视觉补偿；
- 尾部在全盘地图中始终保持一个聚合区域；选择 LCE、历史备份镜像等尾部子节点时，仅在聚合尾部范围内按真实 LBA 移动位置指示；
- 删除旧的尾部操作说明，`o` 只作为统一的树节点展开/折叠操作；
- 底部帮助与完整帮助页同步展示设备信息树 `gg/G` 与详情 `gg/G` 语义；
- 新增正式回归，锁定树首尾跳转、容量地图持续显示、不同区域激活位置变化、三行区域带以及尾部子节点保持同一地图。

P11 已完成：设备信息树首尾跳转、固定全盘容量地图、区域激活联动、尾部子节点定位、多行区域带、帮助文案和旧提示清理均已落地。本机 `tui_suite` 277/277 通过，快速门禁 4 个测试套件/6 个产物/0 失败，Clippy 严格警告门禁通过，完整门禁 8 个测试套件/10 个产物加 doctest/0 失败。

### P12 — G 视口修复与容量地图视觉升级

实机验收发现设备信息树按 `G` 后只剩“协议摘要”一行。根因是把最后节点索引直接写入纵向滚动偏移，导致“选择位置”和“视口起点”混用。P12 改为：

- `gg/G` 只改变设备信息树的语义选择；
- 设备信息树渲染器根据真实可见高度自动计算滚动起点，保证选中节点可见并尽量保留周边上下文；
- `j/k/gg/G` 共用同一套“选中节点必须处于可见视口”的规则，避免尾节点贴顶后只剩一行；
- 全盘容量地图增加 `0% / 25% / 50% / 75% / 100%` 刻度轴；
- 分区带改为圆角外框，并由“名称行 + 容量/占比行”构成更厚的区域块；
- 当前区域继续整块使用主题选择背景和粗体，同时保留 `▲` 的真实 LBA 定位；
- 地图下方增加独立“当前选中”信息卡，集中展示区域名称、LBA 范围、容量和占比；
- 容量根节点显示“全盘布局”信息卡，普通区域和尾部子节点继续共享同一张全盘地图。

P12 已完成：新增 `G` 视口回归和容量地图视觉结构回归，本机 `tui_suite` 278/278 通过，快速门禁 4 个测试套件/6 个产物/0 失败，Clippy 严格警告门禁通过，完整门禁 8 个测试套件/10 个产物加 doctest/0 失败。

### P13 — 容量地图终端原生激活态

实机验收发现大面积 selection 背景在终端中表现为突兀的灰绿色矩形，与设计目标不符。本阶段改为终端原生层级表达：

- 激活区域不再使用大面积背景填充；
- 激活区域使用 accent 前景色、粗体文本、粗水平边框 `━` 与粗垂直分隔 `┃` 建立视觉焦点；
- 保留 `▲` 的真实 LBA 定位，不用背景色模拟 GUI 发光；
- 坐标轴改为轻量 `┈` 虚线，降低与容量主体的视觉竞争；
- 分区内容按可用宽度自适应降级：完整区域优先显示名称与容量/占比；较窄区域保留完整名称；极窄区域降级为 `▌`；
- 数值只有在完整放得下时才显示，不再出现 `6.6`、`25.` 之类被截断的数字碎片；
- 当前选中信息卡保持弱边框、强内容，用于承载完整 LBA、容量与占比，因此主地图无需为极小区域强塞文字；
- 新增回归锁定“容量地图不得使用 selection 背景”和“极小区域不得显示截断数值”。

P13 已完成：本机专项测试和 `tui_suite` 280/280 通过，快速门禁 4 个测试套件/6 个产物/0 失败，Clippy 严格警告门禁通过，完整门禁 8 个测试套件/10 个产物加 doctest/0 失败。

### P14 — 激活轮廓与极小区域收口

实机复核继续发现三个视觉问题：容量根节点中的极小区域单竖条容易被误认为局部激活；具体分区激活时只有粗水平/竖线而四角仍是轻边框；极小区域用单个竖条占位本身也不美观。本阶段统一修正：

- 容量布局根节点没有具体活动区域时，地图主体不得出现任何重边框或伪激活字符；
- 激活分区使用完整闭合重轮廓：顶部 `┏━━┓`、两侧 `┃`、底部 `┗━━┛`；相邻分区边界在进入/离开激活区域时同步切换重角；
- 极小区域不再使用单竖条占位，完整名称放不下时内部留空，只保留真实分区格子和边界；
- 极小区域数值继续只在完整放得下时显示，不允许截断数字；
- 新增回归分别锁定“根节点零伪激活”和“活动分区完整四角重轮廓”。

P14 已完成：专项回归通过，`tui_suite` 281/281 通过，快速门禁 4 个测试套件/6 个产物/0 失败，Clippy 严格警告门禁通过，完整门禁 8 个测试套件/10 个产物加 doctest/0 失败。

### P15 — 语义底色与边界职责分离

用户实机复核认为仅靠边框颜色区分区域不够明显。本阶段改为“底色识别区域、边框表达选择状态”：

- 普通区域使用低亮、低饱和语义底色；EDP、空闲、启动/交换、保密、尾部等即使内部无文字，也能通过色块本身区分；
- 激活区域使用同一语义色系稍亮一级的底色，同时继续使用 accent 重边框与粗体文字；
- 禁止重新使用通用 `selection` 背景，避免再次出现灰绿色大贴片；
- 普通共享边界统一使用中性弱边框色，不再让共享竖线承担左右区域的颜色归属；
- 只有激活区域接管其完整闭合重轮廓，因此区域识别和选择反馈职责分离；
- TrueColor 使用中央 Theme 中的低饱和语义背景；ANSI256 使用对应暗色索引；ANSI16 降级为黑底/活动深灰底，同时保留语义前景色；
- 极小区域内部继续允许完全留空，但背景必须铺满其真实可视格子，不再需要字符占位；
- 回归门禁锁定：设备容量地图必须使用中央 `disk_region_fill`，且不得使用通用 selection 背景。

P15 远端实现与验证已完成：提交 `9ce4ec121f07ab331e067ce615e5558773e24fa4` 的 Rust CI（run `36383562895`）跨 Linux/macOS/Windows 全部通过；Virtual Disk HIL（run `36383562856`）4/4 通过。Mac 当前离线，因此本阶段未执行本机 release 安装；待 Mac 恢复后只需拉取最新分支、重新编译安装并进行实机视觉复核。

### P16 — whole-disk 普通盘识别与制盘预检

实机验收发现 Kingston DataTraveler 3.0（`disk4`，15.5 GB）是合法的 whole-disk NTFS / superfloppy：文件系统直接从 LBA0 开始，没有 MBR/GPT 分区表。旧逻辑只把有效 MBR/GPT 当作 Plain 正向证据，因此被错误标成“盘型未确认”。本阶段按 fail-closed 原则补齐：

- Plain 正向识别新增 whole-disk FAT/exFAT/NTFS 路径，但必须通过现有严格 boot-sector 校验；随机数据、只有 `55 AA`、EDP 身份残留都不能因此降级为 Plain；
- EDP 识别失败仍不等于 Plain；只有 `device_id=None`、`onlyid=None` 且分区表或 whole-disk 文件系统具有正向证据时才确认；
- 修复 macOS `diskutil info -plist` 整盘容量真相源：优先 `IOKitSize → DiskSize → Size`，`TotalSize` 只作兼容兜底，避免把文件系统卷容量误当物理介质容量；
- 设备枚举和身份观察器统一使用同一物理容量规则；
- K6 Plain 来源解析支持 whole-disk 文件系统作为单一来源范围；
- Plain→EDP 如果目标接收分区被用户明确选择格式化/重建，则跳过无意义的源文件迁移；如果用户要求保留文件，则继续执行严格迁移预检；
- 当前 NTFS 文件清单迁移尚未实现，因此默认“不格式化/保留”会明确 fail-closed；不会为了允许制盘而静默丢弃 NTFS 文件；
- 对 `mode1` 完全重建，实盘只读 `provision plan` 已验证 `--format-share --format-encrypt` 可以生成完整计划；未执行任何真实写盘。

实盘只读证据：

- `disk4`：USB `0951:1666`，物理容量 `30277632 * 512B`；
- NTFS boot sector 声明 `30277631` 个文件系统扇区，严格 NTFS 校验通过；
- `device_id=None`、`onlyid=None`；
- 修复后 `edpcli list` 显示 `disk4 ... 普通盘`；
- 修复后 `mode1 --format-share --format-encrypt` 只读计划成功，默认保留路径因 NTFS inventory 未实现而安全拒绝；
- 本机 fast：8 个测试套件/10 个产物/0 失败；Clippy `-D warnings` 通过；full：8 个测试套件/10 个产物加 doctest/0 失败。

### P17 — 容量布局语义色与激活态统一

实机复核确认容量布局仅靠边框区分不够明显，且旧激活态统一使用全局蓝色 accent 会覆盖分区自身语义。P17 统一为“区域识别靠自身色系，激活靠同色系增强”：

- Boot 使用青绿色、Share/Combined 使用绿色、Encrypt 使用紫色、Plain 使用蓝色、Compatibility 使用琥珀色、LCE 使用紫罗兰、Tail 使用蓝色、Free/Unknown 使用中性灰；
- 普通区域使用更可辨识但仍低饱和的语义底色；
- 激活区域保持自身色相，只提升底色亮度、文字亮度和粗边框，不再统一套全局蓝色 accent；
- 激活区域 label 保持原文字，不再增加 `●` 前缀；当前位置继续由 `▲` 和重边框表达；
- 左侧容量布局树展开后的分区子项文字按 `DiskRegionKind` 使用对应语义色；
- 容量布局下方区域列表按 `DiskRegionKind` 使用对应语义色，不再统一白色；
- 内容行里的左右竖边界 cell 与区域主体使用同一背景，消除横向主背景缝隙；上下横边框行保持无背景；
- 普通共享边界使用弱中性前景，但背景归属于相邻区域；激活区域接管自己的完整重轮廓；
- TrueColor、ANSI256、ANSI16 均继续由中央 Theme 统一降级，设备页不得私自定义颜色；
- 新增回归锁定分区 active hue、label 不改写、树/区域列表语义色和边框背景连续性。

P17 已完成：专项颜色契约与 `tui_suite` 285/285 通过，fast 4 个测试套件/6 个产物/0 失败，Clippy `-D warnings` 通过，full 8 个测试套件/10 个产物加 doctest/0 失败。

### P18 — 容量地图边框背景溢出修复

P17 实机复核发现：如果把区域背景直接应用到上/下横边框 cell，终端会把整个字符单元格背景铺满，视觉上表现为色块越过边框向外溢出。P18 将“轮廓”和“内容边界”拆成两个明确语义：

- `disk_region_outline()` 只提供分区语义前景色和激活加粗，不设置背景；用于上/下横边框、四角及横边框交界；
- `disk_region_boundary()` 继承区域 fill 背景并使用对应边框前景色；只用于内容行里的左右竖边界和分区共享竖边界；
- 内容主体继续使用 `disk_region_fill()`；
- 激活区域仍保持自身色系重边框，label 不增加额外符号；
- 新增回归锁定“横边框无背景、内容行竖边界继承背景”的契约。

P18 已完成：专项回归 4/4、`tui_suite` 285/285、fast 4 个测试套件/6 个产物/0 失败、Clippy `-D warnings`、full 8 个测试套件/10 个产物加 doctest 均通过。

### P19 — QuadrantInside 半单元格无缝边界

P18 实机复核继续暴露终端 cell 的物理限制：普通 box-drawing 边框字符与整格背景无法同时做到“无空白且不溢出”。P19 不再继续微调整格背景，而改用 Ratatui 原生 `QUADRANT_INSIDE` 半单元格边界：

- 外框采用 `▗▄▄…▄▖ / ▐…▌ / ▝▀▀…▀▘`，边界向区域内部占半个字符单元；
- 上/下边界的另一半 cell 明确使用页面背景，因此不会向外溢出，也不会重新出现整格空白带；
- 左/右外边界使用 `▐/▌`，外侧半格为页面背景，内侧半格为分区边界色；
- 两个分区的共享边界使用 `▐/▌` 的前景/背景双色组合，在同一个终端 cell 内分别承载左右两侧颜色，不再使用整格中性分隔线；
- 激活区域仍保持自身分区色系，通过更亮的语义边界与底色表达，不重新增加圆点或修改 label；
- 树和区域列表语义色保持 P17 规则；
- 测试直接锁定 Ratatui `QUADRANT_INSIDE` 字符集、外框半格背景和共享边界双色组合，替代 P18 的整格边框背景契约。

P19 已完成：容量地图专项 4/4、`tui_suite` 285/285、fast 4 个测试套件/6 个产物/0 失败、Clippy `-D warnings`、full 8 个测试套件/10 个产物加 doctest 均通过。

### P20 — 半单元格边框继承分区语义色

P19 实机复核发现 QuadrantInside 的几何已经正确，但普通边框仍沿用了旧的统一 `border_subtle` 蓝灰色，造成绿色/紫色分区内部与外框颜色语义不一致。P20 统一边框颜色来源：

- 普通分区边框前景色 = 对应 `DiskRegionKind` 的普通语义前景色；
- 激活分区边框前景色 = 对应 `DiskRegionKind` 的激活语义前景色；
- 上/下半格边框、左右外边界都遵守同一语义色规则；
- 分区共享 `▐/▌` 边界不再使用一侧填充背景代替边框色，而是前景/背景分别取左右两侧的语义边框色；
- 因此绿色 Share/Combined 使用绿色边框，紫色 Encrypt 使用紫色边框，Protocol/Tail/Compatibility 等均保持各自色系；
- 不使用全局强调色或统一中性边框覆盖分区语义。

P20 已完成：专项语义色契约通过，`tui_suite` 285/285、fast 4 个测试套件/6 个产物/0 失败、Clippy `-D warnings`、full 8 个测试套件/10 个产物加 doctest 均通过。

## 18. 完成标准

只有同时满足以下条件，才可标记 COMPLETE：

- 设备页已从 flat summary 改为 List + Tree + Detail；
- 无冗余“当前设备”标题/快速摘要；
- Identity/Capacity/Status/Backups/Protocol 五类信息都在当前设备页可访问；
- Capacity/Tail 在当前页面直接展开，不要求先进入深度检查；
- 动态布局全部来自 `DiskLayoutModel`；
- Wide/Medium/Compact 都可用；
- 快捷键遵守当前全局导航契约；
- 测试和 CI 通过；
- 没有改变任何协议/写盘安全语义。

结论：以上完成标准全部满足，本计划标记为 COMPLETE。
