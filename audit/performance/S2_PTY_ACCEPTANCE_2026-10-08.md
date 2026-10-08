# S2.2 真实 PTY 演示交互验收：第一批

2026-10-08，edpcli 2.5.0。重现：

```sh
cargo build --locked --release
uv run --locked python scripts/tui-acceptance-s2.py
```

复用 `scripts/tui-replay.py` 已有的独立 PTY、终端大小切换、超时和受控进程组清理机制。**`edpcli demo` 全部使用内存中确定性场景，无真实磁盘/备份副作用；并未通过 `tui` 访问 USB。** 结果摘要：`audit/performance/s2-pty-acceptance-20261008.json`；完整 ANSI 会话/逐帧数据位于不提交的 `target/performance/pty-s2/`。

| 场景 | 结果 | 说明 |
|---|---|---|
| devices | PASS | 设备层级、进入检查与返回、屏幕切换 |
| backups | PASS | 备份树/表、Tab/ESC、屏幕切换 |
| inspect-lba8 | PASS | Inspect 的 J 输入流程、ESC、屏幕切换 |
| provision-form | PASS | 制盘表单 Tab/j/ESC 与屏幕切换 |
| provision-review | PASS | 计划确认返回/焦点与屏幕切换 |
| provision-result-success | PASS | 制盘结果、焦点/导航与屏幕切换 |
| error-state | PASS | 错误提示绘制与退出、屏幕切换 |

**7/7 演示进程以退出码 0 完成、无超时，ANSI 输出非空，所有步骤均有帧记录。**

边界：这是实际 PTY 上的**场景级烟雾测试**，不是输入到输出视觉语义的像素逐点基准，不包含真实 USB 热插拔、权限提升、破坏性制盘、真实恢复、并发后台任务、实际 P95 输入延迟。需要更深入的端到端状态转换/交互断言时，应扩展现有 `tests/tui_*` 的状态驱动回归或引入可验证屏幕快照；不能仅凭 PTY 退出码声称所有 UX 问题已解决。

结论：未观察到现有七类场景的 PTY 崩溃或退出死路。保持 `docs/ui/TUI.md` 两顶层工作区架构，不进行无证据的全局样式重构。下一批优先补热插拔、极窄屏内容可达性、后台失败与长时间运行状态。
