# S4：多分区真实规划资源剖析与 PTY 首次稳定重绘响应门禁

日期 2026-10-09；开始基线 `38283c83b224b11f9dfe014c7cb6c1349fcd3a5d`，macOS Apple M1 Pro；**不读取或写入实体 U 盘，不修改盘的原始 I/O、回滚、写盘授权或协议密文逻辑**。本次侧重严谨测量和长期可重复的验收门禁，不基于不充分的假设更换文件系统架构。

## 1. 从真实规划调用建立多分区资源基准

新增 `examples/s4_multi_partition_memory_profile.rs` 和 `scripts/benchmark-s4-multi-format.py`，不是构造连续随机扇区，而是直接调用生产 `plan_format_targets_typed()` 与 `build_plain_provision_write_plan()`，使用真实的 FAT16/FAT32/exFAT 元数据生成器及 EDP FileKey/SM4 变换。场景分别为：

- `official-small`：32 MiB FAT16、128 MiB FAT32、256 MiB exFAT，按 mode0 的三分区语义构造完整官方规划，含 1 明文镜像复用、2 加密镜像变换。
- `official-large`：32 MiB FAT16、8 GiB FAT32、4 GiB exFAT，同样使用完整 mode0 三分区规划，不实际分配 12 GiB 介质文件。
- `plain-large`：与 large 对应的三个文件系统，使用 Plain 三分区的生产元数据写计划，并确认另外有 12 扇区的清理触碰；Plain 无 SM4 变换。
- `budget-reject`：故意请求两个 32 GiB 级 FAT32 的累计元数据资源，验证在尚未 materialize 巨大 BTreeMap 之前被 64 MiB image/512 MiB working 默认预算拒绝。

每个场景各执行 5 个**全新 Release 进程**，记录 metadata sectors、预估工作载荷、计划构造耗时及由 `getrusage()` 提供的进程 **RSS 高水位**。这些数值不是实时 live heap 或 allocator 分配总量，也不包含真实设备卸载、回滚镜像读取和写后读回。

| 场景（5 次中位数） | 元数据扇区 | 有效镜像载荷 | 预估工作载荷 | 规划耗时 | RSS 高水位 |
|---|---:|---:|---:|---:|---:|
| EDP 三分区 small | 4,970 | 约 2.43 MiB | 约 19.4 MiB | 33.96ms | 约 16.73 MiB |
| EDP 三分区 large | 67,325 | 约 32.9 MiB | 约 263.0 MiB | 507.84ms | 约 175.70 MiB |
| Plain 三分区 large | 67,325 + 12 协议清理扇区 | 约 32.9 MiB | 约 263.0 MiB（文件系统三项） | 44.03ms | 约 169.84 MiB |
| 超预算 | 131,112 | 超过 64 MiB | 超过 512 MiB | 构建前拒绝 | 约 5.28 MiB |

**结论**：本样本 EDP 约 508ms 与 Plain 约 44ms 不能简单作为等价操作的加速比：虽然两者使用相同文件系统总体规模，生成过程中生成器、保留镜像、SM4 工作量和 Plain 元数据构造路径有差异。已有 P0 的 SM4 轮密钥复用仍然是最直接的已验证加速。实际峰值受到 BTreeMap、Arc 镜像及其他临时 Vec 影响，而预算 `payload*8` 是保守上限，不宜仅凭 RSS 小于预算就下调或扩容。

## 2. 区分 TUI 首字节、首次稳定输出与语义正确

S3 的 `scripts/tui-replay.py` 原只测按键到首 PTY 字节。对“完整画面”的更精确观测需要专门的渲染结束或 terminal framebuffer oracle；直接把同一个 step 中**最后一个**输出字节当作完成时间是不正确的，因为已有动画会产生独立异步输出。

本次新增 `OutputBurstTracker`，只追踪**首次** PTY 输出 burst，并在该 burst 后安静至少 20ms 才确认其最后一个输出字节的时间 `settled_pty_burst_ms`。后来发生的无关动画不会改变已完成的首次 burst。对每个 j/k 动作要求真实 PTY ANSI 流新增内容与合法的前后时间，缺失、超时或无真实响应立即失败。测试专门模拟后续动画不改变首次 burst 结果。

延迟测量仅对生产渲染器的**内存 demo 场景**进行，4 场景 × 5 次 PTY × 每次 6 个 j/k 动作，每场景 30 样本。`first_pty_output_ms` 和 `settled_pty_burst_ms` 都包括主机进程调度与读取延迟，不能等同于 TUI 代码内部耗时。后者是**首次稳定的输出片段**而不是可证明的语义完整屏幕；对于动画中途重绘、长字段、恢复前卸载等场景还需要真实帧缓冲对照与状态回放。

完整原始统计及逐次 manifest 路径见 `audit/performance/s4-tui-repaint-20261009.json`。不建立共享 CI 的绝对毫秒红线，避免 GitHub Runner 抖动造成假阳性；用可复现脚本、因果归属与 fail-closed 验证保证可比性。

## 3. 本轮实施与未实施

实施：新增可复现多分区资源 bench + parser/预算/镜像 alias 门禁；PTY 首次 burst 静默判断及隔离动画测试；现有 PTY 捕获、退出管理、表格语义门禁保留。*未改动生产文件系统、SM4 并行、用户磁盘 I/O、资源预算上限、热插拔或写盘事务。*

后续需要的证据：真实设备全流程的阶段耗时与功耗/缓存同步行为、后台任务竞争下 UI 语义屏幕验证、基于受控虚拟介质的格式化故障注入，以及多分区源数据的结构优化 PoC；这些不能由本轮普通文件/内存基准替代。高风险事项必须另行审批授权后在隔离 HIL 执行。

## 4. 复现

```
uv run --locked python scripts/benchmark-s4-multi-format.py --samples 5
uv run --locked python scripts/benchmark-tui-latency.py --repeats 5
uv run --locked python -m unittest scripts.tests.test_benchmark_s4_multi_format scripts.tests.test_benchmark_tui_latency scripts.tests.test_tui_replay
```

测试/提交流程还必须通过 Cargo fmt、Clippy、Fast、Full、冗余审计和同一 Git SHA 下的 GitHub Rust CI + Virtual Disk HIL。
