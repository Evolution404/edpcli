# 开发期历史兼容代码清理 · 2026-10-06

## 范围与证据

用户要求重点清理开发过程中不再使用的历史代码和兼容代码，保留 EDP 盘协议的历史兼容。以生产调用、测试/示例调用和真实介质协议证据区分代码归属；测试夹具的历史生成能力从生产代码移出，读取能力继续维护。

本次是接口与实现归属清理，CLI 旧兼容参数也按要求移除。外部 Rust 调用方需要采用下表中的当前路径；不保留已删除接口的别名。

## 已清理

| 遗留内容 | 处理与当前入口 |
| --- | --- |
| `application::ports` 重复导出 | 删除模块；能力接口使用根 `ports`，只读证据接口使用 `application::evidence` |
| `tui::event` 键位转发 | 删除模块；键位实现直接使用 `tui::keymap` |
| 根 `crypto/sectors/sysinfo` 与公开路径的实现桥接 | 实现分别直接位于 `protocol::{crypto,sectors}` 和 `platform::system` |
| `diskio` 转发备份目录、创建、配置、时钟 | 删除转发文件；使用 `infrastructure::backup_store::{catalog,display_catalog,create,config}` 与 `infrastructure::clock` |
| `diskio::SectorDev`、`platform::system::{CmdRunner,ExtDisk}` | 删除重复导出；接口使用 `ports::{SectorDev,CmdRunner}`，设备类型使用 `platform::ExtDisk` |
| `read_lba`、`detect_transport`、`copy_text` | 全库确认没有调用，删除 |
| 四个 `u32` 密码学包装与对应 `_offset` 双接口 | 保留完整 `u64` 实现，统一为 `a6b0_decrypt/a7f0_encrypt/a6b0_full/a7f0_full`；协议运算体保持原逻辑 |
| `BackupReport/ExpectedIdentity/ProvisionPrepared/HorizontalScrollState` | 删除类型别名；调用方使用 `MetadataBackupReport/MediaIdentityResumePin/PreparedProvision/TableInteractionState` |
| 恢复的三个 `i32` 包装 | 删除只丢弃结构化结果的包装；使用 `restore_flow_typed/restore_on_disk_typed/restore_on_disk_typed_with_pin` |
| 不返回扫描错误的 `scan_backup_workspace` | 删除；调用当前 `scan_backup_workspace_checked`，报告错误且不发布部分编号 |
| 生产 `write_legacy_v1/v2` 夹具写入入口 | 移到 `tests/support/historical_edpb.rs`；生产只创建 v3，读取 v1/v2 继续保留 |
| 文件名身份解析器与 Core 创建适配 | 文件名解析只保留在测试夹具支持中；生产身份来自容器。原 Core 创建适配的测试改为验证实际 Metadata 创建路径 |
| CLI `--mode` | 删除；统一 `--target mode0/mode1/mode2/mode3/plain`，四种官方协议模式保持原编号与语义 |
| CLI `--volume-label` 回退与重复字段 | 删除；显式使用 `--boot-label/--share-label/--encrypt-label`，默认分别为启动区/交换区/保密区 |
| 命令目录中的 `--password` | 删除已无解析分支的陈旧声明；源/目标密码使用各分区密码参数 |
| `allow_prefill=false` 解析分支 | 唯一调用点始终为 true；删除不可达参数、旧必填校验与默认生成分支，保持当前目标预填行为 |

清理中暴露的 TUI 跨层依赖通过应用层的运行时能力构造与展示类型接口收敛，原架构门禁保持要求。新的防回归门禁检查已移除模块与历史夹具写入入口，CLI 行为测试验证旧参数拒绝且不再出现在命令目录中。

## 明确保留

- `legacy-v0064`、2019 过渡配置、当前与历史 profile 检测与跨 LBA 语义。
- LBA6 历史 MBR 底层区、LBA7/LCE 指针与旧密钥包装、LBA4 历史密钥/身份来源、LBA12 EDPF 历史布局解析。
- 协议密码学运算、旧盘扇区字节契约和真实样本，不改变协议模式编号。
- 历史 EDPB manifest v1/v2、序列号摘要与 `LegacyMigrated` 读取语义；读取历史文件不改写文件。
- 当前应用层边界和平台探测所需的能力入口，包括实际使用的原生探测回退；这些承担现行业务职责。

## 验证

- `scripts/test-fast.sh`：Clippy `-D warnings`、格式与差异检查、table-scroll 门禁通过；8 套件 / 10 构建产物，0 失败，runner 27.70 秒。
- `python3 scripts/test-full.py --profile full --deadline-seconds 900 --max-seconds 900`：8 套件 / 10 构建产物与 doctest 全部通过，0 失败，12.54 秒。
- `cargo check --locked --all-targets --target x86_64-pc-windows-gnu`：通过。
- `cargo check --locked --all-targets --target x86_64-unknown-linux-gnu`：通过。
- `python3 scripts/protocol/audit_baseline.py`：20 份协议样本通过，包括 19 份 strict-encrypted 与 1 份 authentic-mode1。
- 与 HEAD 对比：四个保留的 64 位密码学运算体仅迁移内部调用名；历史 LBA12 解析器仅迁移导入路径；profile、LBA7 兼容几何、官方模式模型、协议证据与样本字节保持不变。
- 生产 Rust 源树净减少 **335 行**，已扣除目录迁移；此数值包括导入和格式变化，不等同于删除逻辑行数。
- 本轮未执行虚拟或实盘 HIL。

修改保留在工作区，提交和本机安装尚未更新。本轮日志：`audit/ai-progress/20261006-103504-manual.log`。
