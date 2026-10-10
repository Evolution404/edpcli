# 多逻辑扇区虚拟块设备制盘端到端验收（2026-10-10）

## 范围与边界

- 环境：macOS arm64；开发分支 `feat/native-4kn-wal-staging-20261010`；目标是验证**正式 CLI** 在 macOS OS Disk Image 原生块设备上的写入、WAL 同步/回读、重新挂载与设备来源模式识别。
- 四种正式制盘逻辑扇区：512B、1024B、2048B、4096B；每个虚拟盘 536870912B，逐次用 `diskutil info -plist` 证实 `WholeDisk=true`、`Internal=false`、`VirtualOrPhysical=Virtual`、`BusProtocol=Disk Image`、`DeviceBlockSize` 和 `TotalSize`，并用 `hdiutil info` 验证镜像归属；写入命令均显式 `--include-virtual --disk <虚拟设备> --yes`。
- 现场 `/dev/disk6` 是物理 USB；**没有对任何物理设备执行写入**。原有其他 AI 的 Disk Images 也未写入。结束后本次测试 Disk Images 全部弹出并移除，临时镜像只在 `/tmp/` 使用。
- 8192B OS Disk Image 可以成功挂载为 8192B 原生块设备，但 `provision plan --target plain` **在只读规划阶段明确拒绝**：`exFAT仅允许标准512B/1024B/2048B/4096B原生扇区格式化`，退出码 3；未写入 8192B 测试盘。

## 25 种模式转换 × 4 种扇区

来源及目标集合均为 `{plain, mode0, mode1, mode2, mode3}`。以定向欧拉遍历覆盖全部 **25 个有序模式对**，每次正式 `provision write` 后弹出/重新挂载虚拟块设备，`edpcli list --include-virtual` 按确切 disk 行核对目标模式；每种扇区完整遍历仅需 26 次正式写入，源模式取上一步重新识别的目标模式，不是离线投影测试。

| 逻辑扇区 | 首轮连续矩阵 | 隔离重试 | 去重模式对结果 |
| --- | --- | --- | --- |
| 512B | 25/25 PASS | 不需要 | 25/25 |
| 1024B | 22/25 后异常中断 | `mode2→mode3`、`mode3→mode3`、`mode3→plain`：3/3 PASS | 25/25 |
| 2048B | 25/25 PASS | 不需要 | 25/25 |
| 4096B | 25/25 PASS | 不需要 | 25/25 |

**总计：100/100 有序模式对在真实 OS 虚拟块设备上取得通过证据。** 1024B 未达到单次连续无中断 25/25：第一次在 Mode2→Mode3 开始后停止，旧脚本把 CLI stdout 丢弃，因此原始失败的准确阶段/根因尚不明；单独重建来源 Mode2→Mode3 时完整写入及重新识别成功；随后隔离运行脚本 `EDPCLI_HIL_SEGMENT=tail` 重试最后三组全部成功。此情况不可记为根因已修复。脚本现已在 CLI 失败时打印该次写入日志末尾，便于将来追踪偶发失败。

### 证据文件（Mac 本地，测试产物不跟踪 Git）

- `target/hil/multisector-matrix-512B-20261010.log`
- `target/hil/multisector-matrix-1024B-20261010.log`
- `target/hil/multisector-matrix-1024B-tail-retry-20261010.log`
- `target/hil/multisector-matrix-2048B-20261010.log`
- `target/hil/multisector-matrix-4096B-20261010.log`
- `target/hil/multisector-matrix-20261010.tsv`（保留原始 1024B 首轮失败状态，不篡改为通过）

## 独立加密与文件系统验收

`EDPCLI_HIL_SECTORS='512 1024 2048 4096' EDPCLI_HIL_MODES='plain mode0' bash scripts/ci/macos-cli-native-virtual-hil.sh`：各规格用独立 OS Disk Image 执行正式 CLI 写入，重新打开原生块设备后调用**独立只读** `native_cli_crypto_hil::formal_cli_native_crypto_readback`；Plain 验证 exFAT BPB/MBR 和 FAT 元数据，并通过 macOS 真正挂载、写测试文件、卸载/弹出/重新挂载、逐字节读回；Mode0 验证 EDPF LBA7/LBA12、LCE、FileKey 解封装及 CRC、启动卷 FAT 元数据和两块加密 exFAT 的独立解密及 FAT 检查。额外针对 Mode0 用不同的测试目标密码重制并独立验证 FileKey 解封装。

| 逻辑扇区 | Plain exFAT 持久化 | Mode0 独立协议/加密校验 | 自定义密码解封装 | 原生 LCE 占块 |
| --- | --- | --- | --- | --- |
| 512B | PASS | PASS | PASS | 6 |
| 1024B | PASS | PASS | PASS | 3 |
| 2048B | PASS | PASS | PASS | 2 |
| 4096B | PASS | PASS | PASS | 1 |

启动区：512/1024/2048B 为 FAT16；4096B 为 FAT12（按本次小容量默认启动卷的 FAT 自动判定）；Mode0 交换与保密分区物理加密，独立解码均识别 exFAT。日志：`target/hil/multisector-crypto-fs-20261010.log`，脚本退出码 0。

## 本地门禁与工具改动

- 将 `scripts/ci/macos-native-mode-matrix-hil.sh` 改为逻辑扇区可配置、OS Disk Image 设备号稳健提取、按确切设备行核对回读、欧拉遍历 25 种转换，以及 `EDPCLI_HIL_SEGMENT=tail` 的断点复验；正式 CLI 失败时保留当前调用的日志末尾用于诊断。
- 将 `scripts/ci/macos-cli-native-virtual-hil.sh` 与独立只读 `tests/hil/native_cli_crypto_hil.rs` 扩展到四种原生逻辑扇区，保持默认 HIL 入口兼容。
- `cargo fmt --all -- --check`、`bash -n`、`git diff --check` 和四规格独立 HIL 校验器构建通过。
- 本地 `scripts/test-fast.sh` 首次功能用例零失败（8 套/10 产物），但 64.46 秒超过性能门槛 60 秒；未改变阈值，预热编译缓存后复跑成功，**8 套/10 产物/0 失败，29.24 秒**。不查看 GitHub CI。

## 重跑与限制

```bash
EDPCLI_HIL_SECTOR_BYTES=1024 bash scripts/ci/macos-native-mode-matrix-hil.sh
EDPCLI_HIL_SECTOR_BYTES=1024 EDPCLI_HIL_SEGMENT=tail bash scripts/ci/macos-native-mode-matrix-hil.sh
EDPCLI_HIL_SECTORS='512 1024 2048 4096' EDPCLI_HIL_MODES='plain mode0' bash scripts/ci/macos-cli-native-virtual-hil.sh
```

**已验证**：正式 CLI OS 块写入/WAL/回读、四规格 100 个来源→目标模式对的重建与重新识别、Plain 文件内容持久化、Mode0 默认/自定义密码域和独立加密 FAT 解码。

**本次未证明**：TUI 的人机交互 PTY 全链路；不格式化时的原分区用户文件保留或密码仅重包裹的端到端数据不变性；Mode1/2/3 的逐分区独立解密校验；实体 U 盘兼容性；8192B 的写入可用性；1024B 首次矩阵中断的根因。本报告不得代替上述专项验收，也不包含 GitHub CI（仅最终合并 main 时查看）。
