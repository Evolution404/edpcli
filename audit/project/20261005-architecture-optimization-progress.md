# 架构优化进度审计 · 2026-10-05

本记录描述当前工作区的真实实现状态，用于承接 `20261005-engineering-and-tui-audit.md` 的 G0～G5 方案。当前工作区仍保留此前的大范围未提交修改，本轮没有执行 `git reset`、`git clean`，也没有覆盖既有修改。

## 基线

- 分支：`main`。
- 本轮开始时 `HEAD == origin/main == 47d79878a3b7ea4daa4094b2415b01c480d6bb96`。
- 工作区为脏状态；本轮继续在这些架构迁移修改上收口，不把它们误判为待回退内容。
- 根层已从 28 个 `pub mod` 收敛为精确 12 个稳定外部集成模块；原 16 个 `#[doc(hidden)]` 兼容模块均已迁移为 `pub(crate)`，测试与 HIL 改走稳定门面。
- `tests/architecture_split.rs` 与 `tests/tui_theme_contract.rs` 的静态源码读取点已从 112 处降至 91 处；固定子文件归属改为模块树唯一所有权门禁，协议金样、事务顺序、安全状态转换等必要结构约束继续保留。

## 阶段状态

| 阶段 | 当前判断 | 主要证据 |
| --- | --- | --- |
| G1 · 确认与可靠性 | 主体完成 | 共享写入确认具有尺寸门禁与详情滚动；帮助可滚动；进程执行器已进入统一截止时间与输出预算实现；`scripts/change_scope.py` 统一本地与 CI 变更路由。 |
| G2 · 任务边界 | 主体完成 | `SingleFlightGate`、`TaskSlot`、`CatalogSnapshot`、秘密值类型与结构化错误已经进入运行时；设备与备份刷新共用一次目录快照。 |
| G3 · UI 契约 | 主体完成 | `ActionSpec` 已成为帮助/提示/可用性的共享描述；确认窗、表单、备份页等已有宽高响应式和对应行为测试。 |
| G4 · 职责迁移 | 完成 | 已建立 `domain/` 与 `infrastructure/` 边界，恢复功能状态、写服务及制盘准备/提交职责已按明确边界拆分；crate 根只保留 12 个稳定公开模块，16 个历史兼容模块全部收回 crate 内部。 |
| G5 · 工程收口 | 完成 | 架构文档、API 门面、fast/full 路由、仓库门禁、跨平台 CI 与五平台 Virtual Disk HIL 均已形成闭环；Windows CRLF 与 cfg 专属 Clippy 问题也已由远端矩阵发现并修复。 |

## 本轮新增收口

1. 修复恢复后格式化 facade：`format_partition_after_restore_on_disk_assessed` 已从 `application::post_restore` 正确暴露，避免模块拆分后 TUI 调用链断裂。
2. 修复架构门禁事实滞后：`AppState` 已正式包含恢复功能状态，架构测试同步为五个功能状态。
3. 将破坏性写入/恢复后台任务错误提升为 `OperationError`，错误码和阶段保留到 UI 边界才转换为展示文本，并增加直接回归测试。
4. `application/write.rs` 的备份创建与恢复事务分别拆到 `write/backup.rs`、`write/restore.rs`；原 `application::write::*` 公开函数通过 re-export 保持兼容。安全门禁改为约束整个写服务模块树，不再要求函数永久位于单一文件。
5. `application/provision/commit.rs` 的纯验证职责拆到 `commit/validation.rs`，可见性限定在 `application::provision`，不扩大库级公共 API。
6. `application/provision/prepare.rs` 的只读密钥域探测与来源密码验证拆到 `prepare/key_probe.rs`，外部调用路径保持不变。
7. `docs/architecture/ARCHITECTURE.md` 更新为当前 `frontend → application → domain / stable facades → infrastructure / platform` 分层事实。
8. 根公共 API 最终收敛为 12 个稳定模块；原 16 个隐藏兼容模块全部改为 `pub(crate)`，集成测试、示例与 HIL 已迁移到 `application`、`protocol`、`platform`、`inspect`、`cli`、`edpb` 等稳定门面。
9. 与 TUI/Provision/Inspect 具体子文件路径耦合的断言进一步改为目录级职责与唯一所有权门禁；源码读取点由 112 降至 91，不削弱协议、安全和事务约束。

## 当前规模

- `src/application/write.rs`：588 行；拆分前审计基线为 1049 行。
- `src/application/provision/commit.rs`：526 行；拆分前审计基线为 870 行。
- `src/application/provision/prepare.rs`：736 行；密钥探测职责已独立为 124 行子模块。
- `src/tui/state.rs`：1158 行；审计基线约 1439 行。
- `src/tui/task.rs`：611 行，generation、单任务并发控制与任务结果仍由统一运行时管理。

这些数字只用于识别集中修改点，不作为架构质量的单独验收指标。

## 后续非阻塞项

- 根兼容公开层已清零；后续新增外部能力必须挂到既有稳定门面，禁止为了测试方便重新增加根级实现模块。
- 剩余源码结构断言继续保留协议金样、事务顺序、安全状态转换和单一职责等必要约束；遇到文件迁移时优先改为行为测试或目录级门禁，不为降低统计数字机械删除。
- `prepare.rs`、TUI 状态与任务运行时后续只在出现新的明确职责边界时继续拆分，不为降低行数机械拆文件。
- 本机工具安全层阻止直接执行虚拟磁盘脚本，但 GitHub `Virtual Disk HIL` 已在提交 `b2d295c` 上完成正式验证：macOS arm64、Linux arm64/x86_64、Windows arm64/x86_64 五个作业全部通过。

## 本轮最终验证

- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。
- `cargo check --locked --all-targets`：API 私有化和调用路径迁移完成后通过。
- Windows `x86_64-pc-windows-gnu` 的 `cargo check --locked --all-targets`：通过。
- Windows `x86_64-pc-windows-gnu` 的 `cargo clippy --locked --all-targets -- -D warnings`：通过；期间发现的新门禁 `manual_contains` 警告已修复。
- 当前环境对 Linux cross-target 与本机 Virtual-HIL 相关命令执行存在安全层拦截；本轮 push 后以 GitHub Linux 原生矩阵和 Virtual Disk HIL 作为最终证据，不把工具拦截记为代码失败。
- `cargo doc --locked --no-deps`：通过且无 rustdoc 警告；根索引稳定接口 12/12 可见，16 个历史实现模块 16/16 不再出现在根 API 文档面。
- `tests/architecture_split.rs` 与 `tests/tui_theme_contract.rs` 的静态源码读取点从 112 降至 91；`repository_suite` 103/103 通过。
- `python3 scripts/test-full.py --profile full --max-seconds 600`：通过；8 个非 HIL 套件、库、主程序和文档测试全部成功，0 失败，总耗时 41.30 秒。
- 最终提交、GitHub CI/HIL 与干净 release 安装证据待本轮代码提交后补齐。