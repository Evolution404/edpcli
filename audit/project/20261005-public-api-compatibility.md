# 公共 API 兼容清单 · 2026-10-05

本清单用于架构迁移期间区分稳定外部集成面与兼容公开面。

## 稳定根接口

当前稳定根接口共 12 个：`application`、`cli`、`cli_args`、`command_spec`、`completion`、`diskio`、`edpb`、`inspect`、`platform`、`protocol`、`provision`、`tui`。它们继续正常出现在 rustdoc 根索引中。

## 兼容公开接口

以下 16 个模块仍因集成测试、HIL 或示例直接使用而保持 `pub`，但统一标记 `#[doc(hidden)]`：`backup_catalog`、`backup_metadata`、`common`、`crypto`、`disk_scan`、`filesystem`、`filesystem_capability`、`identify`、`inspect_target`、`metainfo`、`partition_transform`、`sectors`、`selectors`、`sha256`、`sysinfo`、`ui`。

这些模块不能作为新的稳定外部依赖入口。只有在对应测试/HIL 已迁移后，才能逐项改为 `pub(crate)` 或私有。

## 验证

`library_root_exposes_stable_interfaces_only` 同时检查两层接口。`cargo doc --locked --no-deps` 通过且无 rustdoc 警告；生成的根索引中稳定接口 12/12 可见，兼容接口 16/16 隐藏。
