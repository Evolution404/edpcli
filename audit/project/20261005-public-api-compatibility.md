# 公共 API 兼容清单 · 2026-10-05

本清单记录 2026-10-05 的架构迁移状态，作为历史审计基线保留。2026-10-06 按用户要求移除开发期兼容导出与旧包装；当前接口以 [架构文档](../../docs/architecture/ARCHITECTURE.md) 和 [技术债清理记录](20261006-development-compatibility-cleanup.md) 为准。

## 稳定根接口

当前 crate 根只保留 12 个公开模块：`application`、`cli`、`cli_args`、`command_spec`、`completion`、`diskio`、`edpb`、`inspect`、`platform`、`protocol`、`provision`、`tui`。它们是唯一允许直接出现在根 rustdoc 索引中的稳定集成面。

## 已收回的历史兼容模块

以下 16 个原 `#[doc(hidden)] pub mod` 已全部迁移为 `pub(crate)`：`backup_catalog`、`backup_metadata`、`common`、`crypto`、`disk_scan`、`filesystem`、`filesystem_capability`、`identify`、`inspect_target`、`metainfo`、`partition_transform`、`sectors`、`selectors`、`sha256`、`sysinfo`、`ui`。

测试、示例与 HIL 不再直接依赖这些根实现模块；需要的能力分别通过稳定门面访问：备份与元信息经 `application`，文件系统经 `application::filesystem`，协议密码学/扇区解析经 `protocol`，设备探测经 `platform`，Inspect 上下文经 `inspect`，CLI 文本渲染经 `cli::terminal_ui`，容器摘要经 `edpb`。

## 验证

`library_root_exposes_stable_interfaces_only` 现在同时锁定两条规则：16 个历史模块必须保持 crate 内部可见，根层 `pub mod` 必须精确等于 12 个稳定接口，并禁止重新引入 `#[doc(hidden)]` 兼容公开层。