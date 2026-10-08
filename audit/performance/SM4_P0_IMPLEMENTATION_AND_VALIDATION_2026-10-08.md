# SM4 P0 已实施：FileKey 轮密钥复用及原始密文兼容性

日期 2026-10-08；原始生产代码锚点 `378d95c9b43b10edb5d8a29b92ff8b6f9bc1a07f`；Apple M1 Pro / Rust 1.99.0，macOS。所有运行只使用临时普通文件与虚拟数据，**绝无实体 USB 写入**。

## 代码修改与行为

- `src/protocol/crypto.rs`：新增内部 `Sm4Cipher`，构建时从 FileKey 生成一次 `[u32;32]` 轮密钥，`encrypt_block()`/`decrypt_block()` 共用原算法的完整 32 轮逻辑；外部 `sm4_encrypt_block` 与 `sm4_decrypt_block` 接口和单块密文输出**完全保留**。单块 API 仍局部展开一次，不引入全局或跨分区密钥缓存。
- `src/partition_transform.rs`：`EdpSm4Transform::new(FileKey)` 创建一次 `Sm4Cipher`，每 512B 扇区的 32 个独立 16B 块使用同一 schedule，整个文件系统镜像仍由上层原有不可变变换接口管理。对象不再保留原始 FileKey 的第二个副本；新增脱敏 `Debug`，不泄漏派生轮密钥。
- `decrypt_mode2`：对完整输入缓冲区展开一次轮密钥并复用；仍验证 16B 整除、无 padding、按相同 ECB 风格独立数据块解密。Inspect/恢复后读取不会因优化改变 Raw/Decode 对应关系。
- 未变更 EDP FileKeyCRC、EncryptMode、实际 LBA、分区起止、Data/Metadata/Commit 时序、前置验证、镜像/回滚、进度处理、设备身份、TUI 或文件系统格式。

## 独立于新实现的协议兼容性证据

1. **历史源码对照**：从 `git show 378d95c:src/protocol/crypto.rs` 提取旧 SM4 实现，单独 Release 编译，针对 4 个固定测试密钥及每个密钥 1024 个完整 512B 扇区生成共 2 MiB 密文并计算旧版 SHA-256。四个旧版摘要作为单元测试内固定黄金值，新实现逐字节 hash 相等。密钥包含 `00..00`、`ff..ff`、真实开发测试固定 FileKey 和标准测试密钥。
2. **标准向量**：SM4 KAT `0123456789abcdeffedcba9876543210` → `681edf34d206965e86b3e94f536e4246`，公共单块与缓存轮密钥加解密均必须通过。
3. **变换及 Inspect 路径**：按每 128 个扇区抽查完整 `decrypt_sector()`、`decrypt_mode2()` 与原单块 API 逐块密文一致；拒绝 1、15、17、511、513 字节截断输入。
4. **密钥隐私**：不同 FileKey 的 `EdpSm4Transform` 不产生不同 Debug 输出；不打印 raw FileKey 或 round schedule。**注意**：为避免新增依赖或不可靠自行编写 volatile 擦除，本轮不承诺强制清零所有内存副本。SM4 查表软件实现的常数时间属性并未因此改变；后续安全审计单独进行。

## 同机交错 A/B 性能（生产文件系统驱动）

为消除此前不同编译批次及机况带来的 RSS 漂移：使用同一 Git HEAD 的 `git archive` 临时构建旧版和当前工作区的 P0 Release，分别保留独立的二进制，在一台 Mac 上顺序交错运行，每组 **5 次**，采集相同格式化驱动的真实 `SparseFilesystemImage::transformed()` 时间与进程高水位 RSS。两版均在临时普通文件上执行现有事务的写入与读回，模拟整盘的**元数据稀疏写入**，并不代表实体盘物理吞吐。

| 真实驱动场景 | 优化前 SM4 | P0 SM4 | 速度提升 | 前 RSS 峰值 | P0 RSS 峰值 |
|---|---:|---:|---:|---:|---:|
| FAT32, 65,544 元数据扇区（约 32 MiB） | 868.79 ms | **468.29 ms** | **1.86×** | 176,512 KiB | **173,360 KiB** |
| exFAT, 32,077 元数据扇区（约 15.7 MiB） | 412.56 ms | **227.91 ms** | **1.81×** | 89,392 KiB | **87,936 KiB** |

源数据：`sm4-p0-interleaved-ab-20261008.json`，包含单次实验、系统高水位 RSS 与对照顺序。阶段性跨批次（旧 S1 为 3 次，新 P0 为 5 次）的 FAT16/FAT32/exFAT 全样本对照见 `sm4-p0-real-filesystems-20261008.json`；只有**同环境交错 A/B**用于判断内存是否改变。吞吐改善只对 SM4 变换阶段成立，整盘制盘时间会受设备写入、卸载、sync、格式化策略影响。

## 安全与下一阶段门禁

- P0 只实现轮密钥复用，不引入多线程 P1、不用假定存在的 Apple SM4 硬件指令、不更换算法实现和工作模式。硬件 SM4 能力及恒定时间 S-box 仍需专门的安全审计。
- 保持资源预算保守：真实样本显示总体 RSS 受文件系统稀疏元数据图与回滚镜像规模影响；不足以仅凭轮密钥优化降低全流程资源门槛。
- 在推广前必须通过本机 `cargo fmt`、Fast/Full/Clippy/全仓冗余、虚拟 HIL、Windows/macOS/Linux GitHub CI 同提交 SHA；真实 USB 验收仍需独立授权与隔离环境。
