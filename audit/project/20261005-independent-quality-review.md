# 其他 AI 工作质量复核 · 2026-10-05

后续状态：本报告保留审计当时的基线和发现；所列剩余项已由后续修复、门禁和真实 USB/TUI 验收关闭，详见 [工程与 TUI 剩余项验收](20261005-engineering-tui-completion.md)。下文的“未改代码/未写盘”仅描述本次独立复核阶段。

复核基线：`e98c394bbec6307bdd946aea0c79985a17c81754`；最后功能提交 `b3b73c45e36a8a7a03fe17d99abdc14b5a5601fa`。审计开始时工作区干净。本次只新增审计记录，未改产品代码、提交、推送或写入物理磁盘。

## 判断

工程改造有实质成果，自动化质量较好；“全部收口”仍需附条件。存在一个已复现的异步会话隔离缺陷，窄屏消息展示仍有缺口，最新安装版的完整实机验收缺少可追溯记录。未发现本轮可确认的 P0 缺陷；通过测试不等于所有交互分支已被覆盖。

## 按优先级处理

### P1 · 来源密码域探测结果可串入另一张制盘表单

位置：`src/tui/provision/task.rs:12`、`task_updates.rs:20`、`runtime_updates.rs:10`。

`KeyVerify` 带有 `session_id` 并在投递时核对；`KeyProbe` 只带任务 generation。退出 Form 会重置 ProvisionState，但 `retain_provision_context` 只使密码验证任务失效，不使 key-probe 任务失效。新表单再次请求探测时，SingleFlight 拒绝新请求且不推进 generation；旧结果随后仍通过 `finish(generation)`，并被无条件交给当前表单。`provision_finish_key_probe` 只核对 Form 阶段，不核对目标或会话。

触发路径：设备 A 的来源探测未完成 → Esc 返回设备 → 选择设备 B 并进入官方制盘表单 → A 的探测完成。旧结果会覆盖 B 的 opaque-profile 属性，默认密码知识也可能被覆盖。

复核用当前库进行确定性状态回放，确认旧 disk6 结果把新 disk7 表单的 `share_opaque_profile` 从 `false` 改为 `true`。回放不读取设备，也未执行制盘。结合上述任务路由，这是实际可达的前端状态污染。底层 prepare 仍重新读盘验证，因此当前证据不能推导为错误写盘。

修复：让请求、结果和投递同时携带表单会话及目标标识；退出/换盘时使旧探测失效。旧探测仍运行时保存最新待执行请求，避免拒绝 B 的请求后让 B 永久缺少探测。增加延迟返回、换盘、退出后重进及连续切换的回归测试。

### P2 · G5 结论缺少最新安装版完整实机验收证据

位置：`audit/project/20261005-architecture-optimization-progress.md:21`。

报告用 full gate、跨平台 CI 和 Virtual Disk HIL 支撑 G5 完成，这些结果已核实，但不覆盖用户要求的新版 TUI 实机交互验收。此前 14:xx 的真实 USB/TUI 记录早于 17:xx 的架构迁移和最新安装。当前报告没有列出新版真实设备操作、终端尺寸、确认/取消/执行路径和对应证据。

本次已补充安装版只读抽查：设备与备份页、160×50 / 120×30 / 80×24 / 40×24、元数据/容量布局窗口切换及帮助分页；两次完整会话正常退出。此抽查不替代新版恢复、格式化、写入进度及结果验收。

修复：明确区分“工程门禁完成”和“实机验收完成”。在重新确认当前测试盘身份后，按用户既有授权补齐新版恢复/格式化、写入中调整尺寸、安全退出请求、结果页及文件系统读回，记录版本、目标身份摘要、预期/实际结果和证据位置。

### P2 · 窄屏消息栏会截断操作提示和错误原因

位置：`src/tui/shell/mod.rs:77`，尤其 `107–109`。

消息栏将 notice 和状态拼为一个不换行的 Paragraph。40 列实机抽查中，备份元数据页提示停在“Ctrl-w w 切…”，容量窗口提示的 Esc 操作被裁切；代码路径也会将超过宽度的错误后半段裁掉。帮助已可分页，但消息没有完整详情入口。

修复：窄屏使用短提示并优先保留键位；长 notice 显示摘要和可打开、可滚动的完整详情。用中文长错误、路径和带阶段/错误码的消息测试，保证信息可完整访问。

## 已核实成果

- domain/infrastructure 职责迁移、恢复功能状态、写服务和制盘准备/提交拆分已落地，重构不是只移动文件或缩减行数。
- 根公开模块确为 12 个，16 个原公开实现模块已收回 crate 内；调用点和 API 清单已迁移。该变化会使旧 Rust 模块路径失效，不能描述为所有库调用完全兼容；当前 crate `publish = false`，CLI/容器兼容性与 Rust 源码 API 兼容性应分别评价。
- `profile_detector` 注册表实际进入 semantic 推断链，共 18 轴、44 状态；来源轴保留候选，缺上下文明确返回 MissingContext。抽查未发现该新注册表的确定回归。
- GitHub 功能提交的 [Rust CI](https://github.com/Evolution404/edpcli/actions/runs/37297350747) 成功；[Virtual Disk HIL](https://github.com/Evolution404/edpcli/actions/runs/37297350831) 的 macOS arm64、Linux arm64/x86_64、Windows arm64/x86_64 五项均成功。最终文档提交的 [CI](https://github.com/Evolution404/edpcli/actions/runs/37297948699) 也成功。
- 本次从独立临时构建目录复跑 `python3 scripts/test-full.py --profile full --max-seconds 900`，退出码 0，8 个 suite、10 个 artifact 和 doctest，0 失败，36.72 秒。`cargo fmt --all -- --check`、`git diff --check` 均通过。
- 交互式 zsh 解析到 `~/.local/bin/edpcli`；实际安装 SHA-256 为 `83033a8a9b5dbaeddda647f8f2782ec97b00077e036dbd519386d8a9893fe243`，与其他 AI 安装记录一致。最新 HEAD 只增加文档，安装版对应最后功能提交是合理的。

## 本次证据与边界

- 进度日志：`audit/ai-progress/20261005-184407-manual.log`。
- 完整测试输出：`/tmp/edpcli-quality-e98c394-full.txt`。
- 状态回放源码与可执行文件：`/tmp/edpcli-quality-stale-probe.rs`、`/tmp/edpcli-quality-stale-probe`。
- 新版只读 PTY 的 ANSI、文本与彩色帧：`~/.local/state/edpcli/audit/20261005-184729-quality/`；原始设备标识仅保留在本机。
- 未以“没有 TODO / 没有 issue / 工作区干净”代替产品完成验收；未把虚拟盘 HIL 当作物理 USB 或终端实机测试。

建议顺序：先补来源探测的会话隔离，再修复消息详情可达性，最后从修复后的安装版执行完整实机矩阵并更新 G5 结论。
