# edpcli 当前架构

本文只描述当前已实现架构，不记录历史重构计划。

## 分层

`CLI / TUI -> application -> domain + stable protocol/provision/filesystem facades -> infrastructure + platform`

当前仍是单个 crate，但已经按责任建立内部边界；旧公开模块先作为兼容门面保留，不一次性搬目录或破坏外部调用。

- `src/cli*.rs`：CLI 参数解析与文本入口；公开命令目录统一由 `src/command_spec.rs` 描述，并供帮助与补全功能共用。
- `src/tui/`：交互式前端。`AppState` 只保留 `shell` 与设备、检查、备份、制盘、恢复五个功能状态；`TaskHub` 负责刷新代次、单任务并发控制、关键写入任务与进度运输。帮助、页内动作提示和可用性描述通过 `ActionSpec` 与键位表共用语义；前端不实现裸盘安全策略。
- `src/application/`：CLI/TUI 共用用例与安全会话。制盘按准备、提交、导出三个职责分离；写服务根模块保留共享事件与身份/安全校验，备份创建、恢复事务、恢复计划分别位于 `write/backup`、`write/restore`、`write/restore_plan`；`TargetSession` 统一写盘状态转换；`CatalogSnapshot` 让同一刷新批次复用一次备份目录只读快照；`OperationError` 在后台任务边界保留错误码与阶段，进入 UI 状态时才映射为展示文本；`ports` 汇总现有能力接口而不破坏稳定门面。
- `src/domain/`：逐步收拢纯值类型和不变量，目前包含 `geometry`、`hardware`、`secret`；不得执行平台命令、文件生命周期或 TUI/CLI 逻辑。历史稳定路径继续通过原模块门面暴露。
- `src/infrastructure/`：副作用实现。目前 `backup_store` 负责目录扫描、预算和容器创建适配，`process` 负责带统一截止时间与输出预算的子进程执行；应用层通过稳定接口使用这些实现。
- `src/media_identity.rs`、`src/partition_table.rs`、`src/disk_layout.rs`、`src/backup_coverage.rs`：仍是与前端无关的稳定门面和读模型；迁移期间不得让 `diskio`、`edpb`、`disk_scan` 反向依赖应用层。
- `src/media_identity_observer.rs`：只读身份观察服务，可读取协议镜像和硬件探测但没有任何写盘状态转换入口。
- `src/provision/`：纯内存制盘领域模型与验证器；Plain 与官方 mode0～3 都通过统一 `ProvisionRequest` 进入应用层。准备阶段的来源画像和密钥域探测分别位于 `prepare/source_profile`、`prepare/key_probe`，提交阶段的分区格式化和纯验证分别位于 `commit/partition_format`、`commit/validation`；事务顺序保持不变。
- `src/filesystem/`：统一文件系统驱动领域；FAT12/FAT16/FAT32/exFAT/NTFS 共享识别和元信息接口。FAT16/FAT32/exFAT 具备格式化与读回能力，FAT12/FAT16/FAT32/exFAT 具备只读文件级分析；NTFS 暂保留识别与元信息。
- `src/protocol/`：LBA0～12、IIR、LCE 的类型化协议模型；`protocol::semantic` 提供跨业务语义，不包含 UI 字段名、颜色或渲染结构。
- `src/diskio/` / `src/edpb/`：保留块设备、事务与容器底层门面；新的备份目录扫描与创建实现逐步收拢到 `infrastructure`，公开路径继续兼容。
- `src/platform/`：macOS/Linux/Windows 的设备、锁定、卸载和平台探测边界；macOS/Linux 共用的 Unix 辅助逻辑位于 `platform/unix_support.rs`。
- `src/tui/operation_progress_render.rs` 与 `operation_progress_status.rs`：备份、恢复、制盘共用的长操作进度页面；业务百分比和阶段计数来自应用事件，前端只渲染。
- `src/disk_scan.rs` 只负责设备只读扫描；同一设备刷新复用备份目录快照做展示匹配，写入/删除授权仍重新验证。CLI 列表排版位于 `src/disk_scan_render.rs`。

## 读写边界

只读路径使用只读设备句柄；`list/info/inspect/backup create` 不进入写盘准备流程。真实写盘必须经 `TargetSession<ReadOnly> -> TargetSession<PreparedWrite> -> TargetSession<WriteLocked>` 显式状态转换，并保持系统盘保护、USB 整盘确认、卸载/锁卷、重新打开后的身份复核、同步/读回、失败回滚。制盘在写盘前强制创建元数据备份；恢复后的格式化和密钥域重建是恢复事务之外的独立授权操作。

CLI 与 TUI 的制盘能力共用同一 `ProvisionRequest::{Official, Plain}` 和 prepare/commit 服务。Plain 是普通 MBR 磁盘目标，不属于官方 mode 编号，也不得映射为 mode4。

应用层通过 `WriteEvent`、`MetadataBackupReport`、`MetadataRestoreOutcome`、`PostRestoreAssessment`、制盘报告和检查工作区返回结构化结果；后台破坏性写入与恢复任务使用 `OperationError` 保留错误码和阶段，UI 边界再转为可显示文本。备份、恢复、制盘的长操作统一投影为 `OperationRunState`：`OverallProgress` 表示总体基点，`StageProgress` 表示逻辑步骤 `N/M`，`WorkProgress` 表示当前扇区或字节工作量。高频普通进度和回滚工作快照在入队前合并，诊断、错误、边界和最终结果可靠交付；终端正常刷新上限约 20 Hz。进入关键写入阶段后，退出请求延迟到安全收尾完成。

## 协议与语义事实源

LBA0～12 的类型化解析器、配置类型轴和跨 LBA 语义位于 `src/protocol/`；机器证据位于 `audit/protocol/`。LCE、`legacy-v0064`、历史 MBR 底层保留区等真实协议兼容属于协议事实，不能因为名称含有“历史兼容”含义就删除。

`protocol::semantic` 是元信息、制盘验证器与检查器之间共享的类型化语义层。元信息和制盘验证器不依赖检查器的 `Field/View` 展示模型；检查器只负责把协议语义映射为面向 CLI/TUI 的字段、树和十六进制视图。

## 备份架构

当前正式备份格式是自包含、自校验 `.edpb`。运行时不再读取或生成旧 `.bin/.md5/.sha256` 备份链；已经迁移成 EDPB 的历史快照仍可按 `LegacyMigrated` 清单语义只读解析。

备份统一使用 v3 `metadata_only` 合约。Plain MBR/GPT 保存分区表原始元数据和类型化几何；EDP 保存 LBA0～12、LBA7 指向的 LCE 与已确认盘尾协议对象。备份不采集文件系统、目录或用户文件；可用 USB 序列号原文保存在 v3 容器中，历史 v1/v2 序列号摘要保持读取兼容。旧版深度备份格式已移除，不再创建、读取、校验或检查。元数据恢复只写回 `restorable` 原始工件并返回类型化报告；后续评估、格式化与密钥域重建分别走独立服务与安全链。

## 制盘架构

官方 mode0～3 与 Plain 都是正式产品能力。CLI/TUI 共用只读准备和真实提交服务，可确定性导出的目标共用镜像导出路径。应用层先生成不可变计划，再由提交阶段执行安全写入，领域层不直接打开设备、执行平台命令或提权。已有盘通过区域处理策略选择原样保留、验证保留、密钥重新包装、重建或丢弃。Provision 不读取、暂存或搬运用户文件；语义或几何无法原地兼容的目标区域必须明确重建。备份恢复仍是独立的元数据恢复链，不恢复文件数据。

## 验证

日常开发使用 `scripts/test-fast.sh`，默认总耗时预算 **45 秒**；合并、发布和大范围重构使用 `python3 scripts/test-full.py --profile full`，CI 总耗时预算 **120 秒**。`scripts/change_scope.py` 是本地与 CI 共用的变更分类事实源，已跟踪和未跟踪文件都参与路由，未知或公共测试基础设施变更保守选择完整非 HIL 套件。运行器在存在 `sccache` 时自动启用编译缓存并关闭 Cargo 增量编译，缺少缓存程序时自动退回直接 `rustc`。虚拟 HIL 独立运行，不混入普通 `fast/full`。所有提交前执行 `cargo fmt --all -- --check`、`cargo check --locked --all-targets` 与 `git diff --check`；协议相关修改还必须通过协议字段、真实样本和文档契约门禁。
