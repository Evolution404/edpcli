# 原生多规格虚拟磁盘矩阵验收（2026-10-10）

本报告记录已执行的测试。全部使用在 macOS 新建的**普通稀疏文件**，不访问或写入实体 U 盘。

## 八种原生逻辑扇区规格

| 逻辑扇区大小 | 来源与目标模式组合 | 文件系统处理 |
| --- | --- | --- |
| 512B | 25 组 | 写入并重新打开校验 FAT/exFAT |
| 1024B | 25 组 | 写入并重新打开校验 FAT/exFAT |
| 1536B | 25 组 | EDP 协议及未格式化普通盘分区表 |
| 2048B | 25 组 | 写入并重新打开校验 FAT/exFAT |
| 2560B | 25 组 | EDP 协议及未格式化普通盘分区表 |
| 3072B | 25 组 | EDP 协议及未格式化普通盘分区表 |
| 4096B | 25 组 | 写入并重新打开校验 FAT/exFAT |
| 8192B | 25 组 | EDP 协议及未格式化普通盘分区表 |

**累计 200 组真实普通文件上的破坏性来源盘→目标盘重建验证。** 来源和目标分别涵盖普通盘、Mode0、Mode1、Mode2、Mode3。每组均排他创建全新的目标稀疏文件，重新打开校验全部已写入原生块，并校验分区模式及容量。

各 EDP 虚拟盘的 LCE 均在重新打开文件后读取、解密，核对固定 3072B 内容及尾部零填充。标准扇区规格下，每个已格式化的 FAT/exFAT 分区均经过正确的原生数据解密，再进行文件系统启动扇区识别。来源文件空闲区域的测试标记不会出现在破坏性重建的目标文件中。

另外对上述 **8 种原生块大小**逐一使用普通文件模拟原生设备，验证只读打开、重新打开的文件身份及容量核对、完整块写入、WAL 持久化、模拟中断、原始块精确恢复及禁止重复恢复。负向检查覆盖来源/目标模式不符、几何不一致、目标文件已存在和来源文件损坏。

实现文件：`src/application/provision/native_virtual_transition.rs`、`src/application/provision/tests.rs`、`src/diskio/native_device.rs`。

## 本地复测命令

```sh
cargo test --lib virtual_native_ -- --nocapture
cargo test --lib ordinary_file_native_port_wal_commit_and_recovery_all_512_multiples -- --nocapture
scripts/test-fast.sh
uv run --locked python scripts/test-full.py --profile full
```

## 必须保留的限制

这些验证属于**破坏性重新制盘模拟**，不是无损转换。不会继承来源用户文件、密码域、FileKey、未知元数据或保密数据。1536B 等非标准文件系统扇区大小只验证原生通信、协议和 WAL，不代表 FAT/exFAT 能合法格式化或挂载。

本轮没有实体盘、真实操作系统卷挂载或密码解锁验收，不能解除实体盘写入门禁。仍需实现包含数据保留、独立来源认证及完整密钥处置的通用转换执行链。
