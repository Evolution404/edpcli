# S4 冗余治理：只对确认问题实施小步清理

时间 2026-10-08；起点 main `3b899d9`。静态 AST 审计源 `scripts/audit-redundancy.py --check`。本次无真实 USB 写操作、无对协议真相源的删除、无强制清理 Worktree。

## 确认并收敛的跨模块实现

Windows `GetFileInformationByHandle` 的文件身份指纹逻辑原先分别直接存在于 `src/platform/mod.rs::same_open_file_identity` 和 `src/infrastructure/backup_store/display_catalog.rs::fingerprint` 中，两处都拼装卷序列号与 64-bit file index。重复 native 调用容易造成错误码转换、索引高低位拼装等行为漂移。现在统一为平台层的 `filesystem_handle_identity(&File) -> io::Result<(u64,u64)>`：

- 打开介质身份复核仍使用文件指纹；如该 Win32 查询不支持 raw 句柄，继续使用原有 `IOCTL_STORAGE_GET_DEVICE_NUMBER` 后备路径，保持 fail-closed。
- 备份展示缓存继续使用同一卷序列号 / 文件索引二元组，保留 `symlink_metadata`、修改时间、容量、CRC/SHA 与精确恢复路径的独立校验，不扩大其安全职责。
- 仅集中 native 身份读取，不合并不同领域的业务缓存或改变对外接口。

新增第 14 条审计规则 `duplicate_native_file_identity`，检查全仓 `src/**/*.rs` 的多位置直接 Win32 API 调用，只产生**人工候选**；对应 `scripts/tests/test_engineering.py` 自检验证重复路径触发、单一来源通过。

## 177 条候选的保守分类

| 类型 | 数量 | 审计处置 |
|---|---:|---|
| `unreferenced_api` | 5 | 公共 API 可能由外部用户/插件引用；不得仅因仓内未引用删除 |
| `test_only_api` | 57 | 必要的边界测试/协议行为接口；不直接定义为无用代码 |
| `declaration_consumers` | 1 | 公共常量，保留审计/测试对协议长度的行为断言 |
| `ignored_parameter` | 25 | 平台函数统一签名、检测器注册表保留参数；逐一审查前不能删 |
| `forwarding_api` | 89 | 跨平台隔离、入口语义/稳定门面等，转发层可能本身就是架构边界 |
| `duplicate_native_file_identity` | 0 | 单一平台源已收敛并建立回归门禁 |

目前确认问题 **0**，候选 **177**。不存在经静态分析证明可以批量删除的 177 段冗余；S4 以这一项确认的重复实现清理和新门禁收口，不搞全局剥离。后续候选需先附 API 消费者、兼容契约、等价行为测试和收益估算，再单独调整。

## 前置阶段成果及剩余的硬件验收

S0.1/.2/.3 介质身份、格式化边界及故障回滚测试已落地；S1 真实 FAT/exFAT 剖析表明 SM4 转换更值得定点优化；S2.1 去除了制盘与恢复加密分区的两处事务图像复制；S2.2 25 场景 PTY 验收与同号换盘界面回退已实现。S3 **仍为条件启动**：不得在无实测必要性时全面重写 FAT/exFAT 生成器。

**无法在当前无授权物理介质环境下完成的验收**：真实 USB 突然断电、热插拔设备控制器重枚举、加密分区实体写入/恢复、硬件级短写和挂载与主机交互。它们属于明确的独立硬件 HIL 验收门槛，不能将虚拟磁盘成功等同于实体硬件成功。
