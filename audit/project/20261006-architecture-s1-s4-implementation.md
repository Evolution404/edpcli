# edpcli 架构优化 S1–S4 实施记录

日期：2026-10-06。基线：`8da83d2e73cdc2cba5270750ee43308a174b35c3`；本轮保留先前未提交的 S0 修复。

## 实施范围

| 阶段 | 已落地的变化 | 回归证据 |
| --- | --- | --- |
| S1 / F2–F3 | 三平台实际几何观察；未知、非 512B 逻辑块、非整除容量和超出 u32 地址范围拒绝写入；卸载前、重开后重新观察；锁定会话借用唯一设备并持有 guard；真实制盘提交要求来源绑定的备份证明；准备结果字段对外只读 | 几何边界、重开变更零写入、编译失败 doctest、既有身份/换盘/备份故障回归 |
| S2 / F4–F6 | CmdRunner / SectorDev / Clock 定义移入中性 ports；Clock 实现归 infrastructure；文件系统探测归 filesystem；存储实现不再导入 diskio 门面；原生平台实现不再导入 sysinfo；AST 门禁自动发现全源树并解析逻辑模块路径 | 嵌套 use、别名、重导出、super、内联模块、#[path] 重定位及文本误报测试；完整架构套件 |
| S3 / F5–F7 | 展示目录跨刷新缓存、文件身份与修改指纹、读取循环取消、累计字节与期限预算；精确恢复文件避免兄弟目录扫描；双流有界进程输出、类型化退出结果、有限 stderr；Windows suspended child + kill-on-close Job | 同名同尺寸替换/删除拒绝、取消与预算零部分编号、目录基准、Unix 进程回归、Windows 原生测试源码与交叉编译 |
| S4 / F8 | 恢复核心接收结构化 RestoreMetadataRequest，兼容选择/确认入口调用同一核心；保留确认前目标 pin 和备份摘要，核心重新核验；摘要/警告文案归前端；制盘结果状态转换归 ProvisionState | CLI/TUI 共用核心和旧事件序列回归；无 AppState 的状态测试；未确认/摘要冲突零写入；观察回调 panic 隔离 |

## 新边界与兼容性

`src/ports.rs` 定义能力；旧 `platform::system::CmdRunner`、`diskio::SectorDev`、Clock 路径继续再导出。`platform::system` 仍保留 sysinfo 兼容再导出，因此整个 crate 不能宣称是完全无环 DAG；已消除的是审计确认的**具体实现回边**。

`PreparedNewProvision`、`PreparedPlainProvision` 字段改为 crate 内可见，外部使用只读 getter。真实 `commit_provision_on_disk` 改为接收不可公开构造的 `BackedUpPreparedProvision`；调用方通过 `backup_prepared_provision_on_disk` 获得证明，提交前再次核验来源与 SHA-256。旧的三个未使用、可绕过强制备份的低层制盘提交公开包装已移除；仓库中的示例已迁移。CLI 参数和 EDPB 格式保持兼容。crate 内部仍可维护准备结果，不把所有结构声称为语言级全局不可变。

`WriteLocked` 持有设备的独占借用与平台 guard，显式 Drop 保持借用直至 guard 释放；其事务方法只操作该借用设备。内存/镜像的低层 SectorDev 与事务 API 保留，默认 sync 仅用于兼容内存实现，FileDev 使用实际同步；公开低层 API 并不是对任意外部 Rust 程序的安全沙箱。真实产品入口仍负责目标身份/几何与强制备份授权。

`RestoreMetadataRequest` 明确包含已确认事实、目标恢复 pin、预期备份 SHA-256。恢复核心不负责选择或询问；兼容入口保留旧交互文本与事件顺序。重复核验是确认后新鲜度要求，不能用展示缓存替代。进度 sink 异常被隔离，但事务 I/O 错误仍按原安全链返回。

## 读取与进程约束

展示缓存上限为 4096 条 / 64MiB 展示数据。指纹包含规范路径、文件身份、长度、修改/变更信息；Unix 使用设备号/inode/ctime，Windows 使用卷号/文件 ID 与时间。缓存结果统一标注“缓存”，包括坏文件。TUI 刷新单次默认累计内容读取预算 1GiB / 20 秒，每次底层读取最多 64KiB；读取预算为限额加 1 字节的探测方式。取消只影响只读扫描，不能中断关键写事务；同步文件系统调用自身若被 OS 阻塞，token 无法抢占该调用。

扫描失败、取消或超预算返回错误与空结果，不返回重新编号的部分目录。共享快照在设备与备份消费者仍需要时继续存活，全部被新请求替代后取消。恢复、删除和提交重新验证**具体文件**，缓存不参与授权；同名同尺寸替换后，陈旧删除条目不能删除替换文件。

命令 stdout/stderr 公平排空，共享 8MiB 累计输出预算；stdout 最多保留 8MiB，stderr 最多保留 64KiB，兼容错误文本最多引用 512 字符 stderr。结果保留完成类别、退出码、耗时与截断标记；不打印参数数组。Unix 使用自有进程组。Windows 在创建子进程时挂起，加入 kill-on-close Job 后恢复初始线程，正常完成、超时和出错均收尾受控后代。Windows 原生测试包含关闭管道、仅 stderr、超量输出与父进程提前结束/超时后的后代终止；本机仅验证其编译，未运行这些 Windows 场景。

## 目录基准

运行：`cargo run --locked --example catalog_refresh_bench`。只创建宿主机临时容器，不打开裸设备。macOS 本机 debug 构建；每个数据集 1 次应用缓存未命中扫描 + 9 次命中刷新。mixed 为一半正常容器、一半 8192B 无效容器。

| 数据集 | 个数 | 首次 ms | 首次读取 B | 刷新 P50 ms | 刷新 P95 ms | 刷新读取 B | 命中数 | 进程峰值 RSS KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| valid | 10 | 43.046 | 191160 | 0.269 | 0.403 | 0 | 10 | 7664 |
| valid | 100 | 236.714 | 1911780 | 2.166 | 2.400 | 0 | 100 | 10080 |
| valid | 1000 | 2174.506 | 19119780 | 22.837 | 23.316 | 0 | 1000 | 25584 |
| mixed | 10 | 12.331 | 137020 | 0.232 | 0.284 | 0 | 10 | 25664 |
| mixed | 100 | 138.817 | 1370290 | 2.211 | 2.431 | 0 | 100 | 26048 |
| mixed | 1000 | 1149.194 | 13703890 | 23.160 | 24.043 | 0 | 1000 | 29696 |

16MiB 无效容器扫描在启动 1ms 后请求取消；从扫描启动到退出实测 1.931ms，累计读取 327776B，返回“目录扫描已取消”。这个指标包含取消请求前的等待，不是单独计量 token 生效延迟。

“首次”指应用缓存未命中，OS 文件缓存可能已热；不是冷盘或真实 USB 性能。读取量计容器内容，不含 metadata/stat 系统调用。P95 使用 9 次样本的最近秩最大值；RSS 是同一进程跨数据集的累计峰值，不是每个目录单独的驻留增量。不能据此承诺跨机器固定性能提升。

## 验证结果

| 检查 | 结果 |
| --- | --- |
| `scripts/test-fast.sh`（已包含强制总 watchdog） | 退出 0；8 suites / 10 artifacts，0 failures，runner 14.93s；fmt、diff、Clippy 与 table-scroll 前置门禁通过 |
| `python3 scripts/test-full.py --profile full --deadline-seconds 900 --max-seconds 900` | 退出 0；8 suites / 10 artifacts，0 failures，15.56s；另含 doctests 与 Python runner / PTY 行为回归 |
| `cargo check --locked --target x86_64-pc-windows-gnu --all-targets` | 退出 0，7.44s；包括 Windows 几何及进程树测试编译 |
| `cargo check --locked --target x86_64-unknown-linux-gnu --all-targets` | 退出 0，11.32s；包括 Linux 几何适配测试编译 |
| 宿主机目录基准 | 6 组数据集通过，取消测试通过，详见上表 |
| watchdog 行为 | 静默 compile/doctest、关闭 stdout、后代终止、超时结果投影与预算文档/CI 一致性回归通过 |

验证使用当前构建缓存，耗时不是冷构建性能。fast 的 14.93s 是 runner 区间，前置 fmt/Clippy/table-scroll 耗时另计；900 秒强制期限覆盖整个 fast 命令。

本机输出：`/tmp/edpcli-remaining-final-fast.log`、`/tmp/edpcli-remaining-final-full.log`、`/tmp/edpcli-remaining-windows.log`、`/tmp/edpcli-remaining-linux.log`、`/tmp/edpcli-catalog-benchmark.log`。实时日志：`audit/ai-progress/20261006-084013-manual.log`；上述运行日志不作为持久 Git 工件，关键结果已保存于本报告。

## 发布与验收边界

本轮未执行物理或虚拟 HIL，未提交、推送、合并或替换 `~/.local/bin/edpcli`。本机 macOS 非 HIL 回归与跨平台编译不能代替 Windows/Linux 原生执行，以及仓库要求的物理介质场景验收。实际逻辑块已取自平台观察，Windows 物理块暂未知；512e 可接受，4Kn 不支持写入。

原审计 F9 的预算默认值已集中到 `scripts/test-budgets.json`；fast/full 增加 900 秒强制总 watchdog，覆盖 fast 前置检查以及 full 编译/测试/doctest。编译另有 600 秒、doctest 另有 180 秒期限；退出 124 和阶段标签区分真正超时与 `--max-seconds` 的性能回归判定。Python 行为测试以静默长任务、关闭 stdout、后代进程验证期限和清理，并校验文档/CI 与配置一致。Unix 使用自有组及后代枚举，Windows watchdog 使用 taskkill /T；它与产品命令执行器的 Job 生命周期实现是不同边界，Windows 原生 watchdog 行为本机尚未运行。

Windows 实现参考 [Microsoft Job Objects 文档](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) 与 [AssignProcessToJobObject API](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject)。
