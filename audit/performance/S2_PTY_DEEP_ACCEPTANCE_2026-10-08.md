# S2.2 第二轮 PTY、安全热插拔和并发任务复核

日期 2026-10-08，macOS、edpcli 2.5.0；基础版本 `2b2c49d`，新测试在其基础上执行。

## 已验证的关键修正

上轮 `scripts/tui-acceptance-s2.py` 把 `q` 退出放在终端 `resize` 之前。真实进程提前退出，导致原来的 `frames > 0` / exit 0 **不能证明 viewport resize 确实得到执行**。本轮将缩放移至退出之前，断言 PTY 每一步都有完整 frame，且 40×12、160×45 两次缩放的 ANSI 位置都有实际字节增长；并补齐脚本自检。这是修复验收方法，而非改变 TUI 设计。

`uv run --locked python scripts/tui-acceptance-s2.py --all-scenes --idle-seconds 5`

项目全部 **25 个**生产渲染器 demo 场景在受控真实 PTY 中退出正常、没有超时，尺寸变更均有输出。原始数据：`audit/performance/s2-pty-deep-acceptance-20261008.json`；完整捕获保留在 `target/performance/pty-s2/`。空闲测试使用 devices 场景运行 5 秒，计算该进程完整会话 CPU 秒/墙钟秒的比率（此项包含初始渲染及正常按键，不是独立纯空闲 CPU 采样）。

## 模拟热插拔与延迟任务行为

生产 UI 代码 `src/tui/state.rs::replace_devices` 原先主要以 `disk` 编号识别制盘目标。同一编号被新 U 盘占用，会使 `Form/Planning/Review/Confirm` 等前置阶段保留过期目标。新增**原/新扫描行 `MediaIdentityPin` 的严格比较**，在写入开始前的状态发生编号复用/身份变化时清空表单上下文并返回设备列表，普通同身份刷新不取消。`Running/Result` 不因可能的合法协议变化自动清退，继续依靠底层写租约和同一介质身份验证保护写入。相应 state 模拟测试覆盖 Form/Planning/Review/Confirm、正常刷新、Running/Result。

后台任务的旧代返回由 `src/tui/task_gate.rs` 的 generation 和 single-flight 门禁保护；`tests/tui_nonblocking.rs` 与完整 TUI suite 已覆盖旧结果拒绝、非阻塞渲染及部分异常流程。本轮没有宣称在真实 USB 上完成物理断连或硬件故障注入。也没有引入新的快捷键、配色或冗余表格组件。

## 未能由当前技术安全验证的范围

不能把 DEMO 视为实盘验收：真实 USB 插拔、外接硬件故障、特权卸载/挂载时序、断电、设备写入期间断连及多种 Windows/macOS 异步并发都需要隔离 HIL/授权真实设备测试。PTy 帧增长仅代表重绘输出，不等于每个像素和焦点状态完全正确。输入到显示的 P95 延迟还需要明确的带时间戳事件驱动采集，不能用整次回放时长代替。

后续建议：扩大可模拟的任务竞争/设备枚举顺序状态测试，若发现复现缺陷再调整 UI；避免基于无证据的主观假设全面重做现有布局。
