# S3 端到端阶段基准、TUI 输入响应与备份列表性能治理

日期：2026-10-08；原始代码版本 `145127647bb7468c4d0bc6bcb75d4e5b834d19a9`，Apple M1 Pro，Rust 1.99.0；本轮**不使用真实 USB、不读写用户备份文件、不修改磁盘写入/回滚策略**。主要基准测试普通临时文件、合成备份和内存 demo PTY；所有原始数据见本报告末尾。

## 1. 实测事务 I/O：逐扇与批量接口的适用范围

新增 `examples/s3_transaction_io_profile.rs`：直接调用生产 `execute_write_transaction_observed`，对每个事务打印 `SyncPreflight/Mirror/Write/Sync/Readback` 时间、读写调用次数、合计耗时。包装器底层始终是可删除的**普通文件 `FileDev`**；`io_limit=1` 仅模拟真实设备 `max_contiguous_sectors()==1` 的接口限制，不模拟 USB 控制器、同步、供电或错误恢复时延。每次测试在完整写后 sync 和全量读回后抽查首末关键扇区数据。

新增 `scripts/benchmark-operation-paths.py`，对 128/1024/4096/16384 扇区、连续与间隔交错 LBA、最大批量 1/128 的 16 种组合执行 3 次，共 48 笔安全临时文件事务。测试强制验证原始逐扇模式调用计数、空洞不越界合并、每事务 2 次 sync、每阶段非零 I/O，不能用 batch 绕过非连续 LBA。

16,384 扇区样本的三次中位数（受主机文件缓存与 sync 影响，不构成 release CI 时序阈值）：

| LBA 分布 | max_batch=1 | max_batch=128 | 解释 |
|---|---:|---:|---|
| 连续 16,384 扇区 | 72.05ms | 14.06ms | 约 5.12×；普通文件批量调用明显减少 |
| 间隔 16,384 扇区 | 70.89ms | 75.53ms | 不连续区域不具备安全合并条件，不体现收益 |

最大批量 1 模式中，16,384 扇区另含 LBA0 Commit：**32,770 次单扇区读取 + 16,385 次单扇区写入 + 2 次 sync**。这是 *Rust SectorDev 接口调用数*，不是 USB 实际 SCSI 命令数。原始设备 `max_contiguous_sectors=1` **保持原样**；即使读批量也必须单独通过真实 raw 设备身份、故障和功耗测试后才考虑实现。不能因为普通文件中观察到 5.12× 就宣称硬件 U 盘会提速。

## 2. TUI：可重复真实 PTY 首输出响应基准

- `scripts/tui-replay.py` 的每个 step 增加 `first_pty_output_ms`，在实际向子进程 PTY 投递 keys/resize 之后开始计时、记录首次 ANSI 字节读到的延迟。仍保留所有原有帧位置与进程所有权/清理语义。
- 修复一次测试中复现的退出竞争：当 owned TUI 进程已经关闭 PTY 但 `poll()` 尚未观察到退出时，后续尝试发送退出 `q` 会收到 `EIO`；现在仅忽略这个特定 PTY 关闭状态，继续等待**同一进程**正常退出，超时才执行已有受限进程组终止流程。没有扩大杀进程权限。
- 新增 `scripts/benchmark-tui-latency.py` 和独立解析/不完整帧门禁测试。4 个生产 demo 渲染场景，每场景 5 次真实 PTY × 6 次按键 = 30 样本。关键动作 `j/k` 必须确有 ANSI 输出，否则 fail-closed。

| 场景 | 首输出 P50 | 首输出 P95 | 样本 |
|---|---:|---:|---:|
| Devices | 1.41ms | 3.95ms | 30 |
| Backups | 1.22ms | 2.21ms | 30 |
| Provision Review | 1.50ms | 4.29ms | 30 |
| Inspect LBA8 | 1.59ms | 3.49ms | 30 |

**警告**：以上为 demo 子进程输入至首个 PTY 输出时间，包含调度/读取延迟；不能认为已测得按键到**语义正确的完整重绘**的 P95，更不是大型真实设备表格性能。真实背景任务、多窗口极限及完整帧结束延迟仍需要独立门禁。本轮没有凭此改变 TUI 的 25ms 任务轮询和 50ms 最小渲染间隔。

## 3. 备份展示缓存：一次目录规范化，避免 O(N) 路径规范化

经 `src/infrastructure/backup_store/display_catalog.rs` 审计，展示扫描的每个普通文件已经通过 `fingerprint()` 过滤 symlink、设备节点和非 `.edpb` 文件，却仍调用 `fs::canonicalize(&path)`，每刷新 1000 个文件需额外规范化 1000 次路径。直接子文件的规范路径可安全地由**一次 `fs::canonicalize(dir)` + `DirEntry::file_name()`** 得出；父目录是符号链接时仍解析到同一绝对缓存键，目录文件元数据指纹依然新鲜检查。

**实施**：只调整展示缓存键生成。不变更 `scan_backup_file_impl` 的验证、`ReadControl` 中断/预算、备份列表排序、删除计划时新鲜哈希校验、恢复前验证或文件访问权限。新增 Unix 父目录 symlink 别名测试验证热缓存命中但返回用户使用的原路径，并保留原有“同名文件替换后拒绝陈旧删除条目”测试。

为避免文件缓存引起虚假提升，单独编译旧版 `1451276` 和新版 Release 可执行文件，**旧新交错 5 轮**，每轮重建 10/100/1000 个正常或混合模拟 `.edpb`。1000 文件汇总中位数：

| 测试负载 | 优化前 | 优化后 | 提升 |
|---|---:|---:|---:|
| 正常 1000 文件热缓存刷新 | 24.255ms | 8.953ms | 2.71× |
| 混合 1000 文件热缓存刷新 | 22.268ms | 8.314ms | 2.68× |
| 正常 1000 文件首次扫描 | 141.753ms | 136.302ms | 1.04× |
| 混合 1000 文件首次扫描 | 132.100ms | 122.484ms | 1.08× |

因此这是**列表热刷新**提升，不是全流程备份创建/恢复吞吐提升。展示扫描没有借用缓存作为删除/恢复的最终安全决定依据。

## 4. 后续实施准入（本轮审计未授权真实盘写入）

- 下一步 P1：分析真实大型加密格式化的**各分区累积** RSS，保留 `FormatResourceEstimate::working_payload_bytes = payload*8` 的保守内存上限，除非有完整资源证据，否则不下降预算，不立项全流式重写。
- 下一步 P2：在可重复输入下做 PTY **首响应、完整画面、后台任务完成通知**分离的时延/用户任务状态验证；只修复确认存在的问题。
- 下一步 P3：独立受控实体 USB HIL 测试不同原始 I/O 策略；**未授权前默认单扇区不变**。批量读和写均不能只凭普通文件数据推定安全。
- 性能阈值目前仅用于主机间对照，不在共享 CI 设置绝对毫秒硬门槛；正确性、数据边界、读回及回滚仍为不可放松门禁。

## 5. 证据与复现

- `uv run --locked python scripts/benchmark-operation-paths.py --samples 3` → `audit/performance/s3-operation-paths-20261008.json`
- `uv run --locked python scripts/benchmark-tui-latency.py --repeats 5` → `audit/performance/s3-tui-latency-20261008.json`
- 独立交错旧新版 5 轮：`audit/performance/s3-catalog-ab-20261008.json`
- `scripts/tests/test_benchmark_operation_paths.py`、`scripts/tests/test_benchmark_tui_latency.py`、`scripts/tests/test_tui_replay.py` 和 display catalog 原有/新增测试
- 输出 `target/performance/` 保留临时原型与 ANSI 捕获，仓库中仅跟踪可复现脚本及必要的 JSON 汇总；任何触发实体 U 盘操作需另外审批。
