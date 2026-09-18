# edpcli v2 发布后优化审计

日期：2026-09-18  
基线：`main@f218ffc9279160ab529889c9628c5af328278cb9`  
分支：`audit/post-v2-optimization-20260918`

## 目标

在不改变 CLI v2 外部语义、不削弱写盘安全红线、不改变备份格式和 EDP/cems 协议的前提下，
审计发布后的实现，优先处理可验证的正确性、性能、可维护性和跨平台一致性问题。

本 PR 不重复 `docs/CLI_V2_REDESIGN_PLAN.md` 的 Phase 1-9，也不重新设计 CLI。

## 基线门禁

- `git status --short --branch`：clean；
- `main == origin/main == f218ffc9279160ab529889c9628c5af328278cb9`；
- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS。

## 第一轮审计结果

### P0：扇区转换边界需要 fail-closed

`sectors.rs` 的若干底层转换函数仍依赖调用方传入足够长的 slice，内部存在定长切片和
`try_into().unwrap()`。当前真实 `apply` 主路径会先读取完整 512B 扇区，因此正常介质不触发；
但离线输入、未来复用或异常读实现若返回短数据，领域函数仍可能 panic。

计划：先增加“短扇区不得 panic”的失败测试，再把长度校验收敛到领域边界，返回现有
`EdpCliError`，不改变合法输入结果。

### P1：CLI v1 遗留的 onlyid catalog API 可删除

`BackupCatalog::onlyid_group` / `onlyid_index` / `onlyid_values` 已不再被 v2 生产执行路径使用，
只剩旧单元测试。v2 的真实选择统一走 `BackupSelector` 与全局稳定编号。

计划：删除这组死 API 及对应旧测试，把仍有价值的“文件名时间优先于 mtime”排序契约迁到
`BackupSelector` 测试，避免未来再次形成两套编号模型。

### P2：备份发现仍存在两套扫描模型

生产代码同时存在：

- `BackupCatalog::load -> scan_backup_dir`：完整读取、MD5、LBA8 缓存；
- `find_backups`：按文件名 pattern 重复 `read_dir`，再读取 LBA4 做身份终验。

`list`、`info`、`apply` 仍使用后者；backup 管理使用前者。多盘/大量历史备份时存在重复目录
遍历，但 `scan_backup_dir` 又会主动计算所有 MD5，因此不能简单把所有只读查询切到完整 catalog，
否则可能把轻量查询变重。

计划：本 PR 先量化并设计“轻量索引 / 完整健康扫描”边界；只有有明确收益且不增加普通
`list/info` I/O 时才实施。否则记录为后续独立性能 PR。

### P3：大文件职责继续收敛

当前最大生产文件：`cli.rs` 1651 行、`diskio.rs` 1251 行、`inspect.rs` 1212 行。
其中 `cli.rs` 同时承载路由、apply/restore 编排、UI helper 和大量单测；`diskio.rs` 同时承载
raw I/O、备份格式、目录扫描和原子写入。

计划：只做“能够减少重复语义或提高测试隔离”的拆分，不以行数为目的机械拆文件。

## 明确不在本 PR 改动

- EDP/cems 加密/解密算法；
- LBA 布局和分区算法；
- 备份文件格式、MD5 sidecar 格式；
- onlyid/device_id 定义；
- 写盘顺序、回滚协议、安全盘识别；
- 已发布的 CLI v2 命令和参数；
- Release 的 7 套架构矩阵。

## 执行顺序

1. Phase A：短扇区 panic 边界测试与 fail-closed 修复；
2. Phase B：删除 v1 onlyid catalog 死 API，迁移仍有效的排序契约测试；
3. Phase C：备份扫描与 CLI 热路径量化，决定是否实施轻量索引；
4. Phase D：全量 fmt/test/clippy，6 架构 CI + 4 HIL 验收；
5. 根据最终变更按 `docs/RELEASE.md` 决定是否需要新版本；审计 PR 本身不预先升版本。

## 安全红线

继续保持：系统盘 fail-closed、USB 整盘校验、selector pinning、写前备份、卸载/锁卷、
reopen 二次身份确认、原子写入、sync、读回校验、失败回滚、onlyid/LBA4 防串盘恢复。

`backup create` 仍必须保持纯只读介质路径。
