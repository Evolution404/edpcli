# 动画 worktree 架构审计与下一步计划

日期：2026-09-22  
审计分支：feat/tui-core-animation-20260921  
审计提交：bec2a05988f6ecff122133248b75640acd920162  
工作目录：/Users/zhangyuxi/.devspace/worktrees/edpcli-a89fe67a  
对比基线：feat/provision-new-usb-20260919（c76e401）

## 结论

保留当前单 crate、同步 application service、标准线程后台执行的整体结构。
当前最优先的工作是完善操作生命周期和事件分发，然后降低动画驱动的重复渲染成本，
最后沿真实业务边界收敛 CLI/TUI 共用服务。没有证据支持现在引入 async runtime、
ECS、多 crate 或持久缓存。

## 实施结果

本轮已在同一 worktree 完成优先项 1–5，以及第 6 项的依赖边界收敛：

- 关键任务使用全局互斥 operation slot 和单调 operation id；TaskHub 持有关键 worker
  句柄，终端 I/O 失败时先恢复终端，再等待安全链收尾；
- 关键期的快捷键、command palette 和直接 dispatch 共用同一命令守卫，延迟退出会在
  worker 完成后立即生效，不再依赖下一次键盘事件；
- 设备和备份刷新按稳定身份恢复选择；TUI 确认的 onlyid/device_id 会跨提权保留，
  操作开始前重新读取 LBA4/LBA7 复核；
- 设备/备份扫描合并最后一次待处理刷新，Inspect/Verify 限制为一个运行任务加一个最新
  待处理目标，结果绑定 generation/路径；单份 verify 不再哈希整个备份目录；
- 事件循环改为 dirty redraw，并限制输入风暴下的最大绘制频率；表格只构造 viewport
  可见行；支持 full/reduced/off 三种动画策略；
- 设备安全策略和 Prompter 边界移出 write service，selectors 不再反向依赖
  application::write；
- provision validator 明确区分 LBA4 producer flags、wire profile 和 reader view，原有
  4 个失败已修复。

最终验证：`cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`
以及 `cargo test --all-targets --locked --no-fail-fast` 全部通过，共 418 项测试；当前 macOS
默认 feature 下 `virtual_disk_hil` 为 0 项，跨平台 CI/HIL 仍需由远端流水线完成。

后续仍可独立推进：把 write service 的文本输出升级为类型化进度事件，并进一步让 CLI/TUI
共用备份删除/保留策略的同一个 application command。两项都不再阻塞本轮操作安全与动画
性能修复。

animation.rs 的纯渲染、固定最大 25×13 画布、无磁盘 I/O 是合理设计。
这次分支也包含工作区导航、搜索和 LBA 范围修正，不能只按动画组件评审。

## 初始审计验证范围

- 初始 worktree 干净；本次只新增本计划文档，不修改实现、不提交。
- cargo fmt --all -- --check：通过。
- cargo clippy --all-targets --locked -- -D warnings：通过。
- cargo test --locked --all-targets --no-fail-fast：403 passed，4 failed，0 ignored。
- 19 个 tests/tui_* 测试目标全部通过。
- 失败全部位于 tests/provision_validate.rs，错误均首先落在
  “LBA4 current-writer restore-node profile mismatch”。
- 独立临时 Rust 探针调用当前编译产物，复现运行中可再次提交写意图，以及刷新重置选中盘；
  探针仅操作内存状态，不执行真实写盘，运行后临时文件清理。
- 未进行物理盘写入、真实终端长时间性能采样、Windows/Linux 执行或 HIL。
  全量测试中的 virtual_disk_hil 在当前 macOS/default features 下为 0 项测试，不能视为 HIL 通过。
- 全量测试日志：/tmp/edpcli-animation-audit-tests.log（临时文件，不作为长期归档）。

## 发现与风险

### 1. P1：运行中的写操作可被新向导覆盖（既有架构缺口）

依据：src/tui/mod.rs:235；src/tui/state.rs:504、546、574；
src/tui/task.rs:364。

dispatch_nav_command 在处理 BeginApply/BeginRestore/BeginBackupCreate 时直接调用
begin_write_wizard，不经过 navigate 的关键期守卫。关键期守卫只阻止 Quit/Escape。
运行中按 a 可以把 Running 向导覆盖成 Confirm，再输入 YES 会启动另一个 worker。
TaskHub 的 write/progress 消息没有 operation id，任一完成结果都会清除同一个
critical_operation 标志，因而可能允许另一个仍在运行的任务期间退出。

内存探针输出：
- second_write_accepted=true
- quit_after_first_completion=ExitRequested

修复方向：单一 OperationState、操作 ID、任务入口互斥；关键期的命令许可集中判断。
仅禁用一个按钮不足以覆盖快捷键、命令面板和程序调用路径。

### 2. P1：终端故障可导致关键 worker 随进程结束（既有架构缺口）

依据：src/tui/mod.rs:372、377；src/tui/task.rs:364；src/main.rs:22。

draw/poll/read/size 的错误通过 ? 退出 run_loop；TaskHub 丢弃 spawn 返回的 JoinHandle。
main 随后 process::exit。键盘退出的延迟标志不能保障这些异常路径，
原子写的正常错误回滚也无法在进程提前终止后继续完成。

修复方向：运行中的关键任务由 supervisor 持有句柄；终端故障后恢复终端并进入
无 UI 的等待/收尾阶段，待 worker 到达明确终态再退出。不要以终端可用性决定事务寿命。
该保障针对可控错误/展开路径，不声称覆盖 SIGKILL、断电或底层永久挂起。

附带 P2：take_deferred_exit 位于 poll/read 之后；当无键盘事件时 continue 会跳过检查。
用户请求延迟退出后，即使 worker 已完成，也可能要再按键才能真正退出。
应在应用 worker 结果后、等待下一事件前检查退出条件。

### 3. P2：刷新相同列表也会改变操作目标（本分支回归）

依据：src/tui/state.rs:260、674、763。

replace_devices/replace_backups 调用 rebuild_workspace_filter；后者无条件 selected=0。
探针中选中 disk7 后用完全相同的 [disk6,disk7] 列表刷新，选中目标变成 disk6。
搜索过滤更新与数据刷新共用重置逻辑，使刷新、异步扫描完成可能改变后续操作对象。

修复方向：选择保存稳定键；设备使用会话内设备标识并在写前再次校验身份，
备份使用固定路径/必要的内容身份。刷新先恢复选中键，消失时显式清除目标并提示。
搜索编辑的定位策略单独定义。不能把列表位置当作已确认的设备身份。

### 4. P2：任务去重策略不完整，完成消息缺少目标归属（既有架构缺口）

依据：src/tui/task.rs:205、228、246、265、432；src/application.rs:126。

设备/备份扫描有 single-flight，但繁忙时的新刷新直接被丢弃，没有 pending-refresh。
例如创建/删除备份完成时，若旧扫描仍运行，自动刷新可能被吞掉，随后显示旧快照。
Inspect 每次请求都 spawn；generation 仅丢弃结果，不限制并发执行。
Verify 既无并发限制也无目标 ID，而且 verify_backup_exact 会加载整个备份目录；
快速切换目标或连续按 v 可重复全目录校验，结果仅显示“当前备份校验通过”。

修复方向：
- 扫描：一个运行任务 + 一个合并的后续刷新请求。
- Inspect：一个运行任务 + 最新待处理目标，结果携带 request id/target。
- Verify：有界并发/同目标去重，结果绑定路径和请求 ID。
- 写入/删除：全局关键操作互斥，所有消息绑定 operation id。
- 单份校验使用精确目标读取；删除的保留数量与内容复核仍保持 fresh validation。

### 5. P2：动画放大了整个视图的 O(N) 构造成本（既有问题被本分支放大）

依据：src/tui/mod.rs:310、372；src/tui/animation.rs:15；
src/tui/render.rs:218、460；src/tui/state.rs:692、703。

原循环已按 100ms 重绘，本分支改为 80ms 并加入持续变化的画面。
每轮无条件 draw，键盘事件也可触发额外 draw，80ms 并非最大帧率限制。
visible_*_indices 每帧分配/复制索引，Table::new 收集全部 rows，每行重新格式化文本；
Ratatui 输出差分只能减少终端输出，无法省略这些 CPU 构造。
每帧新建 TableState，也没有保留滚动窗口状态。

这里没有测量 CPU/p95 延迟，不能声称已量化卡顿或把三角函数判定为主要瓶颈。

修复方向：
- 引入可注入 clock 和下一帧 deadline；合并事件后按需绘制。
- 动画 tick 与数据版本分开；稳定数据的格式化/过滤仅在数据或查询变化时更新。
- 保留 viewport/offset，构造可见行，避免每帧转换全部记录。
- 搜索预先生成规范化文本，按输入变化过滤；不要在 tick 中做重复工作。
- 提供正常/低动态/关闭动画策略；关闭且无输入/任务变化时不持续重绘。
- 先测 0/100/1,000/10,000 条备份的渲染时间、分配和交互延迟，再决定额外缓存。

### 6. P1：provision 校验基线未收口，展示解码模型被用于生成契约

依据：src/provision/generate.rs:103；src/provision/validate.rs:178；
src/inspect.rs:346、363；tests/provision_validate.rs。

生成器在 rolling XOR 后把 0x45/0x46 改写成 producer flags；
inspect 明确保留官方 rolling-reader 视图，不补偿这两个字节。
validator 却把 decoded 的整个 restore node 与 producer 原文比较，
导致合法生成镜像也在这里失败，并掩盖后续篡改断言。

相关 provision/inspect 文件相对上述基线没有差异，因此不是本分支直接修改引入；
本次没有另行运行基线提交测试。必须单独修复并验证，再宣称分支全绿。

修复方向：区分 wire bytes、reader view、producer profile；
校验器按明确 profile 验证 wire 契约，避免将用于展示兼容历史介质的解码模型当作
生成器的规范模型。保留独立官方证据/固定向量，不能仅让生成器与校验器共享相同实现
而互相证明正确，更不能放宽安全断言使测试变绿。

## 目标架构

单 crate 内明确如下职责，按业务边界逐步迁移：

CLI / TUI adapters
  -> application（devices、backups、inspect、write）
  -> protocol/domain（身份、扇区、生成 profile、校验）
  -> ports（SectorDev、CmdRunner、Clock、ProgressSink）
  -> platform / storage adapters

TUI 内部：
event source -> update(state, event) -> effects -> TaskSupervisor -> typed completion events
                                   -> view model -> render
clock/deadline ---------------------> animation presentation state

- application：返回结构化结果/错误/进度，确认意图显式建模。
- TUI runtime：终端生命周期、时钟、事件与 effect 执行；不负责打开磁盘、拼业务上下文。
- TUI state：WorkspaceState、Selection/Filter、Overlay、OperationState；
  用互斥枚举表达弹层和操作阶段，减少多个 Option/bool 的非法组合。
- render：设备/备份/inspect/向导按职责分视图，统一布局计算与 viewport 参数。
- write service：继续作为唯一写盘安全链；前端不得缓存或绕过写前 fresh validation。
- backups service：CLI delete/prune 与 TUI delete 共用保留规则、确认快照和执行逻辑。
- inspect：把分析数据与 CLI ANSI renderer 分开，再让 CLI/TUI 共用读取/分析服务。
- selectors：目前依赖 application::write::Prompter/guard 和 ui，application 又调用
  selectors；应拆开纯目标解析、目标安全策略与交互选择。
- diskio：包含扇区 I/O、原子事务、备份目录配置、命名、扫描、保留策略；
  随服务迁移分别归入 storage/backup/config，避免仅按行数机械拆文件。
- provision：保留纯生成模型，但将可复用协议结构下沉，减少依赖展示侧 inspect。

## 下一步实施顺序与验收

| 顺序 | 建议独立改动 | 验收条件 |
| --- | --- | --- |
| 1 | 修复操作重入、operation id、终端错误收尾、延迟退出 | 连续 a/YES、命令面板、备份删除组合不会并发进入关键任务；旧完成消息不能解除新任务锁；fake worker + 故障 backend 证明终态前进程不返回；无按键也能完成延迟退出 |
| 2 | 独立修复 provision profile 校验 | 4 个失败转绿；wire/reader/producer 的区别有独立证据向量；完整协议和生成测试继续通过 |
| 3 | 统一 update/effect 与任务调度 | 关键期所有入口使用同一许可规则；慢扫描后新刷新恰好执行一次；Inspect/Verify 并发有界；过期结果不覆盖当前目标；Press/Repeat/Release 在输入和导航入口统一处理 |
| 4 | 修复选择与视图状态 | 相同/重排列表刷新保持选择；目标消失显式清除；过滤、Tab、Inspect 返回保持一致语义；窗口尺寸决定半页步长 |
| 5 | 动画与列表渲染优化 | 注入时钟验证帧上限与事件合并；动画 tick 不重新生成全表文本；大列表仅构造可见行；关闭动画时无空闲重绘；提供优化前后相同输入/尺寸的测量 |
| 6 | application 服务收敛与领域拆分 | CLI/TUI 共用 backup 策略和 inspect 读取；进度为类型化阶段，不再从 ANSI 字符串截取；移除任务层 FileDev/Ctx 装配；保留 CLI 文本/退出码和备份格式契约 |

第 1、2 步应先完成，再进行较大的结构调整。第 3—6 步按独立可回归改动推进，
每步仅拆当前正在改变的职责。预计提交数量随失败测试和评审调整，不预设大规模重写。

## 测试与安全约束

现有部分性能/生命周期测试只检索源码字符串，GenerationGate/SingleFlightGate 测试
只验证小组件，无法证明真实 dispatch、TaskHub 和终端错误路径的行为。
保留必要的架构门禁，同时补可注入 fake executor/event source/clock/backend 的行为测试。

最终门禁：
- fmt、locked all-targets test、clippy；
- macOS/Linux/Windows CI；
- Linux/Windows virtual-disk HIL；
- 写前备份、卸载/锁卷、reopen 身份复核、sync/readback/rollback、onlyid/LBA4 防串盘
  测试保持有效；
- backup create 继续只读打开介质；
- 不因性能优化缓存写前安全探测，不提前升级版本或发布。
