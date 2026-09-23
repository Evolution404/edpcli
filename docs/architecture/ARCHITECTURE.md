# edpcli 当前架构

本文描述当前仓库已经实现的架构，不记录历史重构计划。

## 分层

`CLI / TUI -> application/service -> domain/protocol/backup -> platform + disk I/O`

- `src/cli*.rs`：CLI v2 解析和文本入口。
- `src/tui/`：交互式前端；只调 application/service，不直接实现 raw-disk 安全策略。
- `src/application/`：CLI/TUI 共用的任务边界，包含设备扫描、备份、inspect、写入事务。
- `src/protocol/`：LBA0–12、IIR、LCE 等纯协议解析/验证模型。
- `src/provision/`：纯内存 provisioning domain；不得打开设备、执行命令或提权。
- `src/backup_*` / `src/edpb.rs`：Metadata/Deep Backup 与 EDPB 容器。
- `src/platform/`：macOS/Linux/Windows 的设备、锁定、卸载和平台探测边界。

## 读写边界

只读路径使用只读设备句柄；`list/info/inspect/backup create/deep` 不应进入写盘准备流程。真实写盘只允许经 application write service：固定 selector/身份、备份、卸载/锁定、reopen 后复核、原子写、sync/readback、失败回滚。

TUI 的 worker 只返回结构化结果，不直接向终端写输出。进入 critical write 阶段后退出请求延迟到安全收尾完成。

## 协议事实源

LBA0–12 的 typed parser、profile axis 和跨 LBA validator 位于 `src/protocol/`；机器证据位于 `audit/protocol/`。产品代码不得从历史分析文档复制“常量”或重新发明未知字段。

## 备份架构

Metadata Backup 保存 LBA0–12 和必要证据区；Deep Backup 在此基础上通过 `PartitionReader` 只读获取文件系统 metadata。当前 FAT16/FAT32/exFAT 可生成 inventory；NTFS 仍 fail-closed 为 unsupported。普通文件 payload 不属于 Deep inventory 的读取目标。

## Provisioning 架构

`src/provision/` 当前提供 `ProvisionSpec -> generate_image -> ProvisionValidator` 的纯内存链。它与真实设备写入服务解耦；当前仓库不应把历史计划中的 CLI/TUI provision 入口当作已实现产品能力。

## 验证

提交前至少执行 `cargo fmt --all`。核心门禁是 `cargo check --all-targets`、相关定向测试和完整 `cargo test`；协议变更还必须通过 protocol ledger/catalog/documentation contract。
