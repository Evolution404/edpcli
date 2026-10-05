# 架构优化进度审计 · 2026-10-05

本记录描述当前工作区的真实实现状态，用于承接 `20261005-engineering-and-tui-audit.md` 的 G0～G5 方案。当前工作区仍保留此前的大范围未提交修改，本轮没有执行 `git reset`、`git clean`，也没有覆盖既有修改。

## 基线

- 分支：`main`。
- 本轮开始时 `HEAD == origin/main == 47d79878a3b7ea4daa4094b2415b01c480d6bb96`。
- 工作区为脏状态；本轮继续在这些架构迁移修改上收口，不把它们误判为待回退内容。
- 根层仍有 28 个 `pub mod`：其中 12 个定义为稳定外部集成面，16 个定义为 `#[doc(hidden)]` 兼容公开面；兼容模块在测试与 HIL 迁移前不直接改为 `pub(crate)`。
- `tests/architecture_split.rs` 与 `tests/tui_theme_contract.rs` 中仍有 112 处源码读取，后续继续按“行为测试优先、依赖方向门禁保留”治理。

## 阶段状态

| 阶段 | 当前判断 | 主要证据 |
| --- | --- | --- |
| G1 · 确认与可靠性 | 主体完成 | 共享写入确认具有尺寸门禁与详情滚动；帮助可滚动；进程执行器已进入统一截止时间与输出预算实现；`scripts/change_scope.py` 统一本地与 CI 变更路由。 |
| G2 · 任务边界 | 主体完成 | `SingleFlightGate`、`TaskSlot`、`CatalogSnapshot`、秘密值类型与结构化错误已经进入运行时；设备与备份刷新共用一次目录快照。 |
| G3 · UI 契约 | 主体完成 | `ActionSpec` 已成为帮助/提示/可用性的共享描述；确认窗、表单、备份页等已有宽高响应式和对应行为测试。 |
| G4 · 职责迁移 | 主体完成 | 已建立 `domain/` 与 `infrastructure/` 边界，恢复功能状态、写服务及制盘准备/提交职责已按明确边界拆分；公共 API 已形成 12 个稳定根接口和 16 个隐藏兼容接口的迁移层级。 |
| G5 · 工程收口 | 基本完成 | 架构文档、API 兼容分层、fast/full 路由、仓库门禁、Linux/Windows 交叉目标检查与 rustdoc API 验证均已完成；实际虚拟 HIL 运行仍受当前工具安全层限制。 |

## 本轮新增收口

1. 修复恢复后格式化 facade：`format_partition_after_restore_on_disk_assessed` 已从 `application::post_restore` 正确暴露，避免模块拆分后 TUI 调用链断裂。
2. 修复架构门禁事实滞后：`AppState` 已正式包含恢复功能状态，架构测试同步为五个功能状态。
3. 将破坏性写入/恢复后台任务错误提升为 `OperationError`，错误码和阶段保留到 UI 边界才转换为展示文本，并增加直接回归测试。
4. `application/write.rs` 的备份创建与恢复事务分别拆到 `write/backup.rs`、`write/restore.rs`；原 `application::write::*` 公开函数通过 re-export 保持兼容。安全门禁改为约束整个写服务模块树，不再要求函数永久位于单一文件。
5. `application/provision/commit.rs` 的纯验证职责拆到 `commit/validation.rs`，可见性限定在 `application::provision`，不扩大库级公共 API。
6. `application/provision/prepare.rs` 的只读密钥域探测与来源密码验证拆到 `prepare/key_probe.rs`，外部调用路径保持不变。
7. `docs/architecture/ARCHITECTURE.md` 更新为当前 `frontend → application → domain / stable facades → infrastructure / platform` 分层事实。
8. 根公共 API 明确分为 12 个稳定接口与 16 个隐藏兼容接口；兼容接口统一使用 `#[doc(hidden)]`，并增加架构门禁与 rustdoc 根索引验证。
9. Chapter 15 中与 TUI/Provision 具体文件路径耦合的断言改为目录级职责门禁，后续可继续移动文件而不削弱依赖约束。

## 当前规模

- `src/application/write.rs`：588 行；拆分前审计基线为 1049 行。
- `src/application/provision/commit.rs`：526 行；拆分前审计基线为 870 行。
- `src/application/provision/prepare.rs`：736 行；密钥探测职责已独立为 124 行子模块。
- `src/tui/state.rs`：1158 行；审计基线约 1439 行。
- `src/tui/task.rs`：611 行，generation、单任务并发控制与任务结果仍由统一运行时管理。

这些数字只用于识别集中修改点，不作为架构质量的单独验收指标。

## 后续非阻塞项

- 16 个隐藏兼容模块只有在对应集成测试或 HIL 已迁移后才可逐项改为 crate 内部可见；这属于兼容性演进，不再阻塞当前架构收口。
- 剩余源码结构断言继续保留协议金样、事务顺序、安全状态转换和单一职责等必要约束；遇到文件迁移时优先改为行为测试或目录级门禁，不为降低统计数字机械删除。
- `prepare.rs`、TUI 状态与任务运行时后续只在出现新的明确职责边界时继续拆分，不为降低行数机械拆文件。
- 实际 macOS 虚拟 HIL 脚本需要创建并挂载临时虚拟磁盘，当前工具安全层阻止直接执行；`ci-virtual-disk` 测试代码已通过静态编译检查，正式 HIL 继续由仓库 CI 执行。

## 本轮最终验证

- `cargo fmt --all -- --check`：通过。
- `cargo check --locked --all-targets`：通过。
- Linux `x86_64-unknown-linux-gnu` 与 Windows `x86_64-pc-windows-gnu` 的 `cargo check --locked --all-targets`：均通过。
- `cargo check --locked --features ci-virtual-disk --tests`：通过，虚拟 HIL 测试代码能够编译；实际虚拟磁盘执行被当前工具安全层阻止。
- `cargo doc --locked --no-deps`：通过且无 rustdoc 警告；根索引中稳定接口 12/12 可见、兼容接口 16/16 隐藏。
- `git diff --check`：通过。
- `python3 scripts/test-full.py --profile full --max-seconds 600`：最终通过；8 个非 HIL 套件、库、主程序和文档测试全部成功，0 失败，总耗时 42.78 秒。
- `make install`：通过；当前工作区 release 安装到 `~/.local/bin/edpcli`，SHA-256 为 `8dee76480353078b041fe063618e8ed3ca2c3d3631baa05b132038296dbe26c7`，`edpcli version` 报告 2.5.0 / arm64 / release / `47d79878a3b7+dirty`。
- 安装后二进制的设备枚举只读 smoke 被当前工具安全层阻止执行，本轮未绕过该限制，也未触发任何物理盘写入。