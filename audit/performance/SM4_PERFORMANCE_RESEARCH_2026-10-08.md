# edpcli SM4 性能研究：轮密钥复用与并行边界

研究日期：2026-10-08，代码锚点 `0867977137d459d3c6c2e767f91d532c569acc0d`，主开发机 Apple M1 Pro（8 核、MacBookPro18,3），Rust 1.99.0/aarch64-apple-darwin。

**本轮没有修改生产密码算法、分区协议、制盘代码、Cargo 依赖、密钥或真实 USB 设备。** 仅分析源码并在仓库忽略的 `target/performance/sm4-study/` 运行独立 Rust Release 对照原型。采样原始数据保存为 `audit/performance/sm4-performance-20261008.json`。

## 1. 确认的性能根因（源码级）

- `src/protocol/crypto.rs`：`sm4_round_keys(&[u8;16])` 产生 `[u32;32]`；`sm4_crypt_block` **每调用一次**都重新展开这 32 个轮密钥，随后仅加密/解密一个 16B 数据块。
- `src/partition_transform.rs`：`EdpSm4Transform::transform_sector` 对每 512B 扇区循环调用 32 次 `sm4_encrypt_block`；`decrypt_sector`（测试）和 `decrypt_mode2`（Inspect 等只读解密）同样按 16B 块重复展开。
- `src/filesystem/image.rs`：`SparseFilesystemImage::transformed` 遍历全部稀疏元数据扇区，生成独立 `Arc<BTreeMap<u64,[u8;512]>>`。加密内容每次都使用同一个 FileKey，不存在需要每 16B 重新派生密钥的协议要求。
- `src/provision/keys.rs` 的 LBA12 密钥封装仅加密一个 16B FileKey，暂不构成吞吐瓶颈；针对大规模扇区转换进行预展开最有效。

上一轮 S1 的真实驱动 FAT32：65,544 元数据扇区约 32 MiB、构建中位数约 26ms、SM4 变换中位数约 836ms；确认 SM4 变换比文件系统元数据生成更加耗时，但**该时间不能直接用下面的独立原型替代**。

## 2. 隔离原型与兼容性核验

原型**从当前** `src/protocol/crypto.rs` 提取实际 `SM4_SBOX`、`sm4_tau`、`sm4_round_keys`、`sm4_crypt_block` 到项目忽略的工作目录；实验版仅将 `sm4_crypt_block` 的轮密钥输入改为预展开只读 `&[u32;32]`，其余 32 轮计算、字节序和数据块处理顺序均不变。编译标志：`rustc --edition=2024 -C opt-level=3 -C target-cpu=generic`，没有新增依赖。

准备确定性扇区数据，以 `BTreeMap<u64,[u8;512]>` 逐扇遍历映射构建密文图；测时包含密文图分配/构造，但不包括输入准备与结果摘要计算。测试 4 组不同 FileKey，各 1,024 个扇区验证三种路径密文一致，并逐块测试解密与明文一致；经典 SM4 KAT：key 和 plaintext `0123456789abcdeffedcba9876543210`，expected ciphertext `681edf34d206965e86b3e94f536e4246`，加密/解密/缓存轮密钥路径全部通过。所有完整 65,544 扇区比较得到相同的逐数据汇总摘要；并行不同线程数摘要一致。

### A. 32 MiB 镜像：轮密钥展开频率

| 实现（均为单线程） | 扇区数 | 5 次测试中位数 | 吞吐量 | 对比当前 |
|---|---:|---:|---:|---:|
| 当前：每 16B 数据块展开 | 65,544 | 886.56ms | 36.1 MiB/s | 1.00× |
| 每 512B 扇区展开 | 65,544 | 515.49ms | 62.1 MiB/s | 1.72× |
| **整个镜像展开一次** | 65,544 | **500.09ms** | **64.0 MiB/s** | **1.77×** |

整个镜像预展开只需约 128B 轮密钥数组，省去的不是每个数据块的 32 轮运算，而是额外的 32 轮密钥派生。1.77× 为此机器上该**隔离 CPU/内存负载**的中位数比，不是全流程或实体 U 盘速度保证。

### B. 预展开后，并行 worker 数量

| 实例 | 单线程 | 2 workers | 4 workers | 8 workers |
|---|---:|---:|---:|---:|
| 193 扇区（FAT16 类小镜像） | 1.44ms | 0.86ms | 0.52ms | 0.55ms |
| 4,096 扇区（2 MiB） | 31.85ms | 16.91ms | 9.03ms | 8.87ms |
| 65,544 扇区（32 MiB） | 499.25ms | 268.23ms | **144.17ms** | **99.10ms** |

该表每种 worker 数 4 次独立顺序交错测试；包含分组线程启动、数据转换、汇总成 BTreeMap 的成本，未计内存图生成。主机 8 核、8 workers 的最优数据是原单线程缓存方案的约 5.0×，相对原每块展开约 8.9×；**实际业务应考虑保留 TUI 响应、其他后台任务和峰值内存，不能默认耗尽全部 CPU**。这组样本也不证明小容量场景值得开启线程池。

## 3. 安全与兼容性：不可改变的约束

1. 继续保持 **EDP 的现行 SM4 单块加密语义**。16B 块独立、无需 LBA tweak/padding；不能改为 CBC、CTR、GCM 或任意安全性不同的密文格式，否则旧盘不兼容。项目其他 A6B0/A7F0、AES128 Mode3 的密钥封装与 SM4 分区数据通路需明确分隔。
2. 轮密钥来自敏感 FileKey，不能设置全局明文缓存、打印轮密钥或跨分区误用；缓存生命周期应随单个变换/请求结束，并评估使用内存清零。当前 `EdpSm4Transform` 派生 `Debug`，含私有 `file_key`；调试格式化可暴露密钥，建议整改为字段脱敏，新增 secret-not-in-Debug 测试。
3. 当前 `sm4_tau` 用 `SM4_SBOX[secret-derived-index]` 数据依赖查表，**不能保证软件常数时间行为**。缓存轮密钥本身并不修复这一侧信道；不可因为第三方软件“更快”就直接切换。RustCrypto `sm4` 官方也警示未独立安全审计/未确认常数时间。安全实现替换需单列威胁建模与性能对照。
4. 不删除当前按块 SM4 公共 API，保持 `wrap_file_key`、`decrypt_mode2`、Inspect、post-restore 与现有文件系统格式化/原始盘镜像兼容；`sm4_encrypt_block`/`sm4_decrypt_block` 应继续对单块快速使用。
5. SM4 Arm 指令为 **FEAT_SM4** 可选硬件能力；这台 Apple M1 Pro 的 `sysctl hw.optional.arm.FEAT_SM4` 返回 `unknown oid`，因此**没有得到可安全启用 SM4 专用指令的正面证据**，不应假定存在硬件加速，亦不能由此证明 CPU 物理上完全不支持。

## 4. 推荐实施次序与准入测试

### P0：低风险、收益确定：轮密钥一次展开（建议优先）

抽出内部 `Sm4RoundKeys` / `Sm4Context`（持有 32 个轮密钥），添加 `encrypt_block_preexpanded`、`decrypt_block_preexpanded` 内部版本。`EdpSm4Transform` 在创建时展开一次并复用；`decrypt_mode2` 每次调用只创建一次上下文。单块公共 API 可以继续创建临时上下文并调用共用的 32 轮核心。自定义 `Debug` 避免 FileKey/轮密钥泄露，明确生命周期与敏感内存清除策略；不得简单在结构体中引入全局 static 缓存。

**验收**：标准 SM4 KAT、历史协议金样本、所有现存分区和密钥封装测试、4 组 FileKey × 至少 1,024 扇区 × 全部字节、跨分区/错误密码/加密解密/恢复后的文件系统验证；真实 FAT32 32MiB `S1` 前后同机重复样本，附 RSS 峰值。Fast/Full/Clippy/Windows/Linux/macOS CI 同一提交 SHA，虚拟 HIL 不写物理 USB。批准生产更改前核对 P0 新老二进制密文逐字节匹配。

### P1：可选、门限触发：只在较大镜像上限流并行（P0 后再评估）

在已存在的加密镜像构建路径设置足够高的元数据扇区门槛，允许 2–4 个后台 workers 并行，保留扇区有序、相同 FileKey 独立上下文、错误传播、取消/进度节流、回滚前置验证。验证小负载不会因为线程建立及排序额外放慢，控制内存峰值/CPU 饱和，不用无界 Rayon 全局池与事务同时抢占 CPU。若 P0 的实际用户体感和总耗时已满足目标，则无需立项 P1。

### P2：硬件/第三方实现替代（低优先级）

仅当平台实际提供 FEAT_SM4 可信检测且有完整 CI/运行时回退，再研究指令级实现；RustCrypto/其它软件库需独立评估性能、时序安全、锁定版本、跨平台构建及密文兼容。**不建议直接以 OpenSSL/AES 指令替代现有 SM4**；它们不是同一算法。

## 5. 结论与边界

**优先批准 P0（轮密钥复用）**：有独立、可重复性能与逐字节一致的实验依据；生产软件未修改，性能收益尚须在实际 edpcli 功能路径重新核验。P1 作为条件性工作，不应一次性推进 P0、P1、P2。现有文件系统、写盘事务、EDP 几何与回滚保护无需也不允许因本次研究而改变。
