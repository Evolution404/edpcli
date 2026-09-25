# edpcli 当前架构

本文只描述当前已实现架构，不记录历史重构计划。

## 分层

`CLI / TUI -> application/service -> provision + protocol + backup -> platform + disk I/O`

- `src/cli*.rs`：CLI 参数解析与文本入口；公开命令目录统一由 `src/command_spec.rs` 描述，并供 help/completion 共用。
- `src/tui/`：交互式前端；制盘、检查、备份、设备工作区分别维护状态/渲染/任务逻辑，不直接实现裸盘安全策略。
- `src/application/`：CLI/TUI 共用应用服务；制盘按 prepare/commit/export 分离，真实写盘安全事务只存在于这一层。
- `src/provision/`：纯内存制盘领域模型与验证器；Plain 与官方 mode0～3 都通过统一 `ProvisionRequest` 进入应用层。
- `src/protocol/`：LBA0～12、IIR、LCE 的类型化协议模型；`protocol::semantic` 提供跨业务语义，不包含 UI 字段名、颜色或渲染结构。
- `src/diskio/`：块设备、写事务、备份配置、备份目录和备份创建按职责拆分。
- `src/backup_*` / `src/edpb.rs`：元数据/深度备份与自包含 EDPB 容器。
- `src/platform/`：macOS/Linux/Windows 的设备、锁定、卸载和平台探测边界。

## 读写边界

只读路径使用只读设备句柄；`list/info/inspect/backup create/deep` 不进入写盘准备流程。真实写盘必须经过应用层安全服务：系统盘保护、USB 整盘确认、写前备份、卸载/锁卷、重新打开后的身份复核、原子写、同步/读回、失败回滚。

CLI 与 TUI 的制盘能力共用同一 `ProvisionRequest::{Official, Plain}` 和 prepare/commit 服务。Plain 是普通 MBR 磁盘目标，不属于官方 mode 编号，也不得映射为 mode4。

TUI 后台任务只返回结构化结果，不直接向终端写输出。进入关键写入阶段后，退出请求延迟到安全收尾完成。

## 协议与语义事实源

LBA0～12 的类型化解析器、配置类型轴和跨 LBA 语义位于 `src/protocol/`；机器证据位于 `audit/protocol/`。LCE、`legacy-v0064`、历史 MBR 底层保留区等真实协议兼容属于协议事实，不能因为名称含有“历史兼容”含义就删除。

`protocol::semantic` 是元信息、制盘验证器与检查器之间共享的类型化语义层。元信息和制盘验证器不依赖检查器的 `Field/View` 展示模型；检查器只负责把协议语义映射为面向 CLI/TUI 的字段、树和十六进制视图。

## 备份架构

当前正式备份格式是自包含、自校验 `.edpb`。运行时不再读取或生成旧 `.bin/.md5/.sha256` 备份链；已经迁移成 EDPB 的历史快照仍可按 `LegacyMigrated` 清单语义只读解析。

元数据备份保存 LBA0～12 和必要证据区；深度备份在此基础上通过 `PartitionReader` 只读获取文件系统元数据。当前 FAT16/FAT32/exFAT 可生成目录清单；无法确认的格式采用“无法确认即拒绝继续”的安全策略。

## 制盘架构

官方 mode0～3 与 Plain 都是正式产品能力。CLI/TUI 均支持 plan/write；可确定性导出的目标共用 image/export 路径。应用层先 prepare 出不可变计划，再由 commit 执行安全写入，领域层不直接打开设备、执行平台命令或提权。

## 验证

日常开发使用 `scripts/test-fast.sh`；合并、发布和大范围重构使用 `python3 scripts/test-full.py --profile full`。Virtual-HIL 独立运行，不混入普通 fast/full。所有提交前执行 `cargo fmt --all` 与 `git diff --check`；协议相关修改还必须通过协议字段、真实样本和文档契约门禁。
