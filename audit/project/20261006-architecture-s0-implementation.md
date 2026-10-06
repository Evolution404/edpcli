# 架构优化 S0 实施记录

日期：2026-10-06。基线：`8da83d2e73cdc2cba5270750ee43308a174b35c3`。
对应方案：[整体架构审计](20261006-overall-architecture-audit.md)，阶段 S0（F1，及 F9 文档纠偏）。

## 最终行为

- 官方制盘先提交协议，再按顺序执行所选分区格式化。首个失败后停止格式化，剩余所选分区为 `Skipped`，不会调用其写入器。此前成功的分区与已提交协议保留各自成功事实。
- `OperationError` 保留原始退出码、阶段及失败所影响范围的介质状态。完整回滚保留码 7；回滚失败保留码 6；已经开始写入但后续验证无法证明状态时标为 `Unknown`，前端退出码为 6，原错误码继续保存在错误对象中。
- CLI 制盘返回结果中的失败码。CLI 恢复后明文格式化、原密钥格式化与密钥域重建发生失败后结束后续操作，返回相应错误码；不再打印失败后继续写下一分区或最终返回成功。
- TUI 显示失败分区已回滚、介质中间态、状态未知及未执行分区。制盘后台异常按状态未知处理；后台错误投递至结果状态时不再丢弃类型信息。
- TUI 恢复后的三类写入操作出现中间态或状态未知时，清除恢复后的身份 pin，将旧评估置为禁止操作，要求重新检查设备。元数据恢复已完成且读回通过的事实仍保留。完整回滚不会被当作整个多阶段操作撤销。
- 格式化事务后的 boot 校验和重新评估失败均标为状态未知，不会发布完成进度。

停止所有首个格式化失败是本阶段选择的显式策略：即便失败分区完整回滚，本轮也不继续其它分区写入。这样统一保留失败范围和未执行范围，并由新一轮设备检查决定后续动作。

## API 迁移

这些是公开 Rust API 的类型变化，调用方需同步迁移：

| 接口 | 变化 |
| --- | --- |
| `OperationError` | 新增 `media_state` 字段、`with_media_state()`、`exit_code()` |
| `PartitionFormatResult.result` | 从字符串错误改为 `PartitionFormatError::{Failed, Skipped}` |
| `PostRestoreFormatError::Operation` | 载荷从 `String` 改为 `OperationError`；新增 `UnverifiedAfterWrite` |
| `EncryptedPartitionReinitializeResult.result` | 错误从 `String` 改为 `OperationError` |
| `ProvisionExecutionStatus` | 新增 `FormatRolledBack / MediaIntermediate / MediaStateUnknown`；穷举匹配需更新 |
| `AppState::provision_finish_write` | 错误参数从 `String` 改为 `OperationError` |

显示层通过 `Display` 转为文本；调用方判断退出语义应使用 `exit_code()`，判断是否可继续操作应使用介质状态。兼容字符串构造的 `.into()` 仍可用，但不会凭文本伪造退出码或回滚证明。

## 回归验证范围

新增测试通过真实共享扇区事务和稀疏文件系统写入器注入一次写失败、sync 失败、读回失败、读回不一致与持续回滚失败；断言失败分区以外没有写入尝试，并验证码 6/7 与已提交协议事实。

另外覆盖写前拒绝、未知状态、先成功一个分区后失败并跳过后续分区、恢复后 boot 校验及重新评估读失败、三类恢复后操作授权失效、制盘后台失败状态投递和 TUI 结果状态渲染。已有恢复后回滚测试补充了错误码与介质状态断言。

| 门禁 | 结果 |
| --- | --- |
| `cargo fmt --all` / fast 内的格式和 diff 检查 | 通过 |
| fast 内的 `cargo clippy --all-targets --locked -- -D warnings` | 通过 |
| table-scroll 专项门禁 | 4 项通过 |
| `scripts/test-fast.sh` | 退出 0；8 suites / 10 artifacts，0 failures；runner 26.08 秒 |
| `python3 scripts/test-full.py --profile full --max-seconds 900` | 退出 0；8 suites / 10 artifacts，另含 doctests，0 failures；10.77 秒 |

均为本机缓存构建下的非 HIL 验证。输出位于 `/tmp/edpcli-s0-fast-final.log` 和 `/tmp/edpcli-s0-full.log`；实时记录位于 `audit/ai-progress/20261006-082638-manual.log`。先前测试编译中出现的私有常量引用和未限定模块路径已修复，最终门禁全部通过。

## 后续阶段与边界

S1–S4 尚未实施：真实逻辑扇区探测、不可变准备计划、备份证明和句柄能力约束；解除依赖环和增强架构门禁；目录扫描缓存/预算和进程诊断；前端功能状态迁移。

本阶段没有执行物理 USB 或独立 Virtual Disk HIL，没有替换用户安装二进制，没有修改 EDPB `metadata_only` 合约，也没有承诺格式化失败可恢复此前用户文件数据。完整回滚只证明当前扇区事务触及范围恢复，不表示之前阶段撤销。
