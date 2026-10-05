# 架构优化进度审计 · 2026-10-05

本记录描述 2026-10-05 架构优化从 G0～G5 到最终收口的真实状态。历史临时审计、复现脚本与基准文件在结论并入本记录后已清理；全过程未使用 `git reset`、`git clean` 覆盖既有工作。

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

- 当前产品工程没有已知阻塞任务：GitHub open issue=0、open PR=0，现行架构/TUI/制盘治理文档均已明确收口，源码未发现真实 TODO/FIXME。
- 协议字段指南仍明确提示“配置类型检测器尚未在目录中登记”：`profile_axes.tsv` 已登记 18 个轴与 44 个状态，但尚未维护“轴状态 → 运行时 detector 符号”的统一目录。这属于协议目录/研究增强，不影响当前解析、制盘或验收。
- 协议逆向文档仍保留少数写入端来源/配置类型选择器的溯源缺口，例如 LBA0 引导代码配置类型选择与制造商元数据来源；字段语义、实现和行为测试目录本身保持已闭环口径，不将来源研究缺口误写成产品功能故障。
- FAT32/NTFS 目标写入器仍按 2.5.0 发布契约明确拒绝；这是显式能力边界，不是静默降级或当前回归。
- `prepare.rs`、TUI 状态与任务运行时只有在出现新的明确职责边界时才继续拆分，不为降低行数机械拆文件。

## 本轮最终验证与仓库卫生

- `cargo fmt --all -- --check`、`git diff --check`、`cargo check --locked --all-targets`：通过。
- Windows `x86_64-pc-windows-gnu` 的 all-targets check 与 `clippy -D warnings`：通过；期间发现的新门禁 `manual_contains` 警告已修复。
- `cargo doc --locked --no-deps`：通过且无 rustdoc 警告；根索引稳定接口 12/12 可见，16 个历史实现模块 16/16 不再进入根 API 文档面。
- 静态源码读取点从 112 降至 91；`repository_suite` 103/103 通过；最终 non-HIL full gate 0 失败，总耗时 41.30 秒。
- 提交 `62154653866326002dbeb1cecf3ee55009b81773` 的 GitHub `Rust CI` 全矩阵成功；`Virtual Disk HIL` 在 macOS arm64、Linux arm64/x86_64、Windows arm64/x86_64 五个平台/架构作业全部成功。
- 本地/远端历史分支已清理，仓库只保留 `main` / `origin/main`；历史 worktree 已全部移除，磁盘上无残留 edpcli worktree 目录。
- 8 个未跟踪的历史审计/benchmark/repro 文件已在确认结论被后续实现和本记录覆盖后删除；未把带本机路径/实机证据的临时材料补提交到仓库。
- `cargo clean` 删除 352,489 个构建文件，共 68.6 GiB；随后清理 Python `__pycache__`/`.pyc` 与 `.DS_Store`。最终 release 使用临时 `CARGO_TARGET_DIR` 构建并安装，临时目录自动删除，仓库保持无 `target`。
- 当前安装版：edpcli 2.5.0，Git `621546538663`，release/aarch64-apple-darwin，SHA-256 `17435d66a9758ac25c9460a47d309d67d78c482c5b1b2c7141d81cb58c58e2b1`。
