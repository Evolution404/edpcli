# S5：制盘与恢复事务故障注入、失败状态和 TUI 竞争治理

日期 2026-10-09；基线 `b02534faece3641434fb00f2f179aa6074266895`，按用户授权落实 S5.1–S5.4。**不操作实际 U 盘，不降低读前校验、分区界限、原始设备单扇区 I/O、完整回滚与读回要求。**

## S5.1 共享事务完整故障矩阵

现有 `tests/provision_transaction_write.rs` 已覆盖 `SyncPreflight` 失败、回滚镜像首次/中途读取失败、正常写入失败、写后同步失败、读回 I/O 失败、瞬态与永久回滚错误、连续批量扇区读回损坏、数据/协议顺序及 LBA0 提交最后。保留既有证据，不造冗余事务实现。

新增可逐点验证的故障：

| 测试 | 注入点 | 断言 |
|---|---|---|
| 有序写入失败 | Data LBA63、Data LBA64、Metadata LBA12、Commit LBA0 各一次 | 全量触碰扇区 bit-for-bit 恢复；回滚 LBA0 最后；回滚读回完成；错误码 `EXIT_ROLLED_BACK` |
| 半扇区已写但设备报错 | 上述四处各一次，错误前实际写入前 256B | 不相信失败意味着未写入；仍回滚全量旧字节 |
| 读回硬错误或静默损坏 | 四个读回位置各一次、分别 EIO 与读取字节翻转 | 恢复旧字节；同步次序保持 |
| 原 FileKey 重初始化 | 数据 LBA100、101 与协议 LBA7、LBA12 四处逐点写失败 | 全部原数据和旧密钥元数据恢复、协议 LBA12 最后 |
| EDP 与 Plain 格式化 | 两个目标分区，两个写入位置部分变更、两个读回损坏及一次写后同步失败 | 目标原内容恢复，不越过分区边界，Plain 制造商 LBA3 无写入 |
| 回滚持续失败（已有） | 写/同步/验证持续故障 | `EXIT_INTERMEDIATE`，禁止宣称成功回滚；保留隔离与人工恢复指引 |

使用独立 `TransactionFaultMatrixDev` 模拟器直接调用 **生产共用** `execute_write_transaction_observed`、`execute_borrowed_reinitialize_transaction`、`execute_borrowed_data_transaction_scoped_observed`，不复制被测生产事务代码。注入的半扇区写入是模拟固件写入部分字节后仍返回错误的保守场景；**不是实际硬件原子性验证**。

## S5.2 失败后状态与文案

复核 `src/tui/restore_completion_state.rs`：三种完成路径（普通格式化、加密格式化、重新初始化）已经通过结构化 `MediaState::Unknown/Intermediate` 对后续格式化授权调用 `quarantine_post_restore`，清空 `format_target_pin`，把评估状态标记为 `Unsupported`。

发现一个已有展示矛盾：即使介质状态未确认，三个错误提示仍一律追加“元数据恢复仍保持成功”。本轮根据 **结构化 media_state** 统一控制失败提示；必须重新检查的情形改为“介质状态未确认，已停止后续写入；请重新检查设备或从备份恢复”，仅对不要求重新检查的安全失败维持原有元数据恢复成功说明。增加错误提示安全语义回归，不改变业务异常类型与回滚数据。

## S5.3 TUI 单飞/代际任务竞争

`src/tui/task_gate.rs` 原 `TaskSlot` 仅记录布尔 running 与当前 request generation；旧代任务完成后启动新代，随后若旧完成事件迟到/重复到达，`finish_latest(old)` 仍然无条件释放 `running`，导致新 worker 尚未结束即可错误重新启动下一轮任务。新增 `running_generation` 精确绑定**正在执行**的 worker（与可持续增大的最新请求 generation 区分），旧/重复完成只允许 `Deliver(false)`，不释放正在运行的新代 worker、不取出 pending request。

覆盖旧 worker 重复完成、最新任务多轮覆写、invalidate 后旧响应、旧恢复或写入结果不清空新关键操作、重复 Enter 无法开启第二写盘任务、介质中间态错误码依旧保留；修正已有 `key_probe_keeps_latest_form...` 测试中过时的“旧完成释放新 worker”假设，并验证真实新 worker 结束后才释放。关键写入依旧使用 operation_id 与单个 worker 句柄保护。

## S5.4 CI 隔离虚拟磁盘故障注入

`tests/virtual_disk_hil.rs` 新增 `#[ignore]` 的 `virtual_disk_partial_write_faults_restore_entire_metadata_exactly`，仅在编译 `ci-virtual-disk` feature 并由 CI 创建**已证明为 loop/VHD/disk image 的原始虚拟设备**后执行：既有 `ci_prepare_virtual_write` 和 `is_raw_device_path` 守卫必须成功。每次先拍摄原 LBA0–12，再对第 1、第 7、最后一个写入位置实施“已改动该扇区前 256B、返回写错误”，生产 `atomic_write_sectors` 应完整回滚并读回全部 LBA0–12，与写前字节逐一比较。串行 `HIL_LOCK` 防止虚拟磁盘 HIL 用例并发写同一临时卷。CI 结束后已有脚本重新挂载原文件系统并验证保留的文件内容。

只接受 GitHub Linux loop、Windows VHD、macOS disk image 的隔离设备；**此 HIL 不可绕过到用户的真实 USB、不可宣称证明掉电和控制器固件异常下的真正原子性**。本轮不调整 `FileDev` 的原始盘批处理上限。

## 复现、门禁与剩余限制

```
cargo test --locked --test provision_suite fault_matrix_
cargo test --locked --test provision_suite scoped_edp_and_plain_format_failures_restore_exact_partition_bytes
cargo test --locked --lib tui::task::tests::
cargo test --locked --features ci-virtual-disk --test virtual_disk_hil --no-run
cargo clippy --locked --all-targets --all-features -- -D warnings
scripts/test-fast.sh
uv run --locked python scripts/test-full.py --profile full --deadline-seconds 900
uv run --locked python scripts/audit-redundancy.py --check
```

HIL 只在 GitHub 隔离虚拟磁盘 Runner 中通过 workflow 执行。仍需后续单独授权的实体验收：macOS/Windows 上真实拔盘、断电、USB 桥不完整写、设备重编号、驱动缓存及锁卷失效；不允许在普通本机演示环境尝试这些破坏性操作。
