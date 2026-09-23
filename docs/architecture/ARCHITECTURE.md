# edpcli 当前架构

本文描述当前仓库已经实现的架构，不记录历史重构计划。

## 分层

`CLI / TUI -> application/service -> domain/protocol/backup -> platform + disk I/O`

- `src/cli*.rs`：CLI v2 解析和文本入口。
- `src/tui/`：交互式前端；只调用应用层/服务层，不直接实现裸盘安全策略。
- `src/application/`：CLI/TUI 共用的任务边界，包含设备扫描、备份、检查和写入事务。
- `src/protocol/`：LBA0-LBA12、IIR、LCE 等纯协议解析/验证模型。
- `src/provision/`：纯内存制盘领域模型；不得打开设备、执行命令或提权。
- `src/backup_*` / `src/edpb.rs`：元数据/深度备份与 EDPB 容器。
- `src/platform/`：macOS/Linux/Windows 的设备、锁定、卸载和平台探测边界。

## 读写边界

只读路径使用只读设备句柄；`list/info/inspect/backup create/deep` 不应进入写盘准备流程。真实写盘只允许经过应用层写入服务：固定目标选择器/身份、备份、卸载/锁定、重新打开后复核、原子写、同步/读回、失败回滚。

TUI 后台工作线程只返回结构化结果，不直接向终端写输出。进入关键写入阶段后，退出请求延迟到安全收尾完成。

## 协议事实源

LBA0-LBA12 的类型化解析器、配置类型轴和跨 LBA 校验器位于 `src/protocol/`；机器证据位于 `audit/protocol/`。产品代码不得从历史分析文档复制“常量”或重新发明未知字段。

## 备份架构

元数据备份保存 LBA0-LBA12 和必要证据区；深度备份在此基础上通过 `PartitionReader` 只读获取文件系统元数据。当前 FAT16/FAT32/exFAT 可生成目录清单；NTFS 仍采用无法确认即拒绝继续并报告不支持。普通文件负载不属于深度目录清单的读取目标。

## 制盘架构

`src/provision/` 当前提供 `ProvisionSpec -> generate_image -> ProvisionValidator` 的纯内存链。它与真实设备写入服务解耦；当前仓库不应把历史计划中的 CLI/TUI `provision` 入口当作已实现产品能力。

## 验证

提交前至少执行 `cargo fmt --all`。核心门禁是 `cargo check --all-targets`、相关定向测试和完整 `cargo test`；协议变更还必须通过协议账本、字段目录和文档契约测试。
