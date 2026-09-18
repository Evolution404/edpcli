# CLI v2 重构交接

仓库：`Evolution404/edpcli`
分支：`refactor/cli-v2-redesign-20260918`
基线 main：`2f434053be149c6036f6b9d07c4f55e66656ee01`
目标：按 `docs/CLI_V2_REDESIGN_PLAN.md` 实现 edpcli v2 CLI，最终发布 `v2.0.0`。

## 当前状态

CLI v2 已进入实现阶段。

### Phase 1：Parser / help 新模型 — 已完成

- 裸 `edpcli` 已改为等价 `edpcli list`；
- 新增 `info` grammar，旧 `meta/metainfo` 不再进入执行路径，只返回迁移提示；
- `run` 已从 parser 删除，`apply --dry-run` 成为唯一 dry-run CLI；
- 顶层 `restore` 已删除，迁移到 `backup restore`；
- `backup` grammar 已切换为 `create/list/restore/verify/delete/prune`，`backup rm` 只返回迁移提示；
- `inspect` 已要求显式 `--lba 6,7,12`，不再接受裸数字 LBA，也不再接受用户级 `--onlyid/--index`；
- 主帮助已收敛为 v2 一级命令；旧 parser enum 分支 `Parsed::Run/Restore/MetaInfo`、`BackupAction::Rm` 已删除；
- 新增 `tests/cli_v2_parser.rs` 锁定 v2 grammar 与旧语法拒绝行为；
- 现有 CLI/离线/inspect/跨平台测试已迁到 v2 语法。

Phase 1 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS；
- 搜索 `Parsed::Run|Parsed::Restore|Parsed::MetaInfo|BackupAction::Rm`：0 命中；
- 用户可见旧语法只剩 parser 中有意保留的 `backup rm -> backup delete` 迁移错误提示。

v1.1.0 仍是当前正式发布版本；按计划尚未提前修改 `Cargo.toml` 版本。既有三平台
arm64/x86_64 + macOS Universal Release、6 架构 CI、4 套 Linux/Windows virtual-disk HIL
仍是安全基线，不得回退。

### Phase 2：统一 DeviceSelector / BackupSelector — 已完成

- 新增 `src/selectors.rs`：
  - `DeviceSelector` 统一显式 `--disk`、单盘自动选、多盘交互选择；
  - 提权重执行统一通过 `DeviceSelector::pin_argv` 固定为平台原生 selector；
  - 实际 resolve 仍调用既有 USB 整盘/系统盘 fail-closed 门禁；
  - `BackupSelector` 统一备份全局稳定编号、文件路径、范围、多选；
  - 恢复场景可通过内部 `for_onlyid` 视图按当前盘身份过滤，onlyid 不再作为用户选择语法。
- `backup list` 在全目录视图下改为全局稳定编号，不再每个 onlyid 分组从 [1] 重新计数。
- parser 已无法触达的 `InspectOpts/SourceOpts.onlyid/index` 字段及 info/inspect 旧选择分支已删除。
- apply/restore 公共外壳、info、inspect 的提权前 pinning 已统一走 `DeviceSelector`。
- 新增 `tests/selectors.rs`，覆盖显式/单盘/多盘选择、native selector pinning、
  全局备份编号、范围解析和内部身份过滤。

Phase 2 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS。

旧 completion 仍有 v1 补全词，这是 Phase 8 的明确清理项，不属于执行兼容路径。

### Phase 3：info — 已完成

- parser/执行层统一使用 `InfoOpts` / `Parsed::Info`，临时 `SourceOpts` /
  `MetaInfoOpts` alias 已删除；
- `info` 未指定来源时只走统一设备选择器：单盘自动选、多盘交互、无盘直接提示插入设备，
  不再自动扫描备份目录猜测来源；
- `info <备份.bin>` 继续复用现有 metainfo 协议解析，但输出已经收敛为：
  - 设备；
  - 身份；
  - 状态（含 EDP/cems、免密、SAFE6、分区）；
  - 备份；
- 物理盘 info 会按当前盘身份统计匹配备份数量并显示最新备份时间；
- 备份文件 info 会明确显示当前备份文件；
- metainfo summary 新增基于既有 `looks_nopwd` 的只读免密判断，不新增协议算法。

Phase 3 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS；
- 搜索 `MetaInfoOpts|SourceOpts|Parsed::MetaInfo|metainfo_flow`：0 命中。

### Phase 4：apply --dry-run — 已完成

- `run` 已彻底退出执行模型，`apply --dry-run` 与真写共用同一识别/转换主流程；
- 内部执行模式由两个布尔参数收敛为显式 `ApplyMode::DryRun` /
  `ApplyMode::Write { force }`，避免调用方把预览/真写或 force 语义传反；
- 新增 dry-run 副作用契约测试，明确锁定：
  - 不创建备份目录/备份文件；
  - 不进入 YES 确认；
  - 不 reopen 为读写；
  - 不执行任何扇区写入；
- 原有转换结果、系统盘 fail-closed、USB 整盘校验及真写原子写入/回滚路径保持不变。

Phase 4 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS；
- `tests/cli_offline.rs` 新增 dry-run 无写阶段副作用测试：PASS。

### Phase 5：backup create — 已完成

- `backup create [--disk N]` 已接入真实执行层：
  - 单盘自动选、多盘走统一 `DeviceSelector`；
  - 显式目标在提权前继续执行系统盘/USB 整盘 fail-closed；
  - 裸盘权限不足时由 CLI 自身提权并把目标 pin 到平台原生 selector；
  - 提权后始终以只读方式打开目标盘；
- 原 `backup_disk(...)` 已收敛并重命名为唯一 `create_backup(...)` service；
  `apply` 写前自动备份与独立 `backup create` 现在明确共用：
  - 相同 LBA0-13 输入；
  - 相同 onlyid/device_id/VID/PID/容量元数据；
  - 相同文件命名与 `_nopwd` 标记；
  - 相同 MD5 sidecar；
  - 相同 create-new 碰撞保护；
  - 相同 fsync + 目录持久化策略；
- 新增只读契约测试：测试 runner 故意不提供卸载命令，`backup create` 仍成功，
  且无确认、无 reopen、无扇区写入；
- 新增同源格式测试：手动备份与 apply 自动备份在同一时间/同一设备事实下生成
  相同文件名、7168B 内容和 MD5；
- 新增跨平台 CLI 门禁：`backup create --disk <不存在目标>` 在所有平台均在写前拒绝。

Phase 5 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS；
- 搜索旧内部 `backup_disk`：0 命中；
- `backup create` 只读/同源格式定向测试：PASS。

### Phase 6：backup restore / verify / delete / prune 统一 — 已完成

- `backup list` 的编号已成为唯一全局稳定编号，verify/delete/restore 的数字选择全部与
  list 使用同一 `BackupSelector`；
- `backup verify 1`、`backup delete 1,3` 等不再经过旧 onlyid/index 解析分支；
- `backup delete` 无参数时会展示全局编号并进入交互多选，同时保留：
  - 备份根目录路径约束；
  - 删除前内容摘要二次复核；
  - 同一物理盘至少保留 1 份备份的保护；
- `backup prune` 已移除旧 onlyid 用户筛选路径，继续按每盘分组策略清理；
- `backup restore <全局编号>` 会先按当前盘 onlyid 过滤该全局编号，编号指向其他盘时拒绝；
- `backup restore <备份文件>` 允许用户重命名过的合法备份进入既有内容校验链，
  最终仍以当前盘与备份 LBA4 16B 身份标签做防串盘硬终验；
- restore 的 MD5、免密快照阻断、reopen 后身份复核、prepare_write、原子写入/回滚路径
  均保持原安全语义；
- 已删除旧 `backup_rm`、`backup_verify_select`、盘内编号解析与 onlyid 删除选择器实现。

Phase 6 本地门禁：

- `cargo fmt --all -- --check`：PASS；
- `cargo test --all-targets`：PASS；
- `cargo clippy --all-targets -- -D warnings`：PASS；
- restore 定向回归：27/27 PASS；
- backup / CLI UX / selectors 定向回归全部 PASS。

## 下一位 AI 从这里开始

1. 先读：
   - `docs/CLI_V2_REDESIGN_PLAN.md`
   - `docs/RELEASE.md`
   - `docs/USAGE.md`
2. 检查 `git status --short --branch`，禁止 reset/clean。
3. 从 **Phase 7 inspect 收口** 继续，仍须测试先行；不要削弱 selector pinning、系统盘
   fail-closed 或 onlyid 防串盘。
4. Phase 7 删除 inspect 内部残余的 onlyid/index/无盘猜备份路径，确保用户级 LBA 只能通过
   `--lba` 指定，同时保留 raw/hex/export 与离线备份文件检查能力。
5. 小 commit、及时 push，阶段完成后更新本交接文档。

## 已冻结的关键决策

- v2 不保留 v1 旧语法兼容层；旧命令只给迁移提示。
- 无参数 `edpcli` => `list`。
- `meta/metainfo` => `info`。
- `run` => `apply --dry-run`。
- 顶层 `restore` => `backup restore`。
- `backup rm` => `backup delete`。
- 新增 `backup create`，可直接备份当前插入 U 盘。
- `backup create` 与 `apply` 写前自动备份必须共用同一 backup service/格式。
- 普通用户不再依赖 `--onlyid/--index` 完成备份选择。
- `inspect` 使用显式 `--lba`，不再用裸数字位置参数表示 LBA。
- 目标发布版本为 `2.0.0`，但 **不要在 Phase 1 就提前改版本号**；完成 CLI cutover、测试和文档后再升版本。

## 安全红线

不得削弱：系统盘 fail-closed、USB 整盘校验、selector pinning、写前备份、卸载/锁卷、reopen 二次身份确认、原子写入、sync、读回校验、失败回滚、onlyid 防串盘恢复。

`backup create` 必须是纯只读路径：允许因裸盘读取自动提权，但不得进入 prepare_write / unmount / lock / raw write。

## 完成定义

只有以下全部满足才可合并并发布：

- 本地 `cargo test --all-targets` 全绿；
- `cargo clippy --all-targets -- -D warnings` 全绿；
- 6 架构 CI 全绿；
- Linux/Windows arm64+x86_64 四套 virtual-disk HIL 全绿；
- README/USAGE/completion/help 全部切换到 v2；
- 旧 CLI grammar 门禁通过；
- 合并 main 后再次全绿；
- 最终 `Cargo.toml` = `2.0.0`，tag 与版本严格一致；
- 7 个 Release 包 + SHA/SBOM/manifest 独立验收；
- 本机最终安装 macOS arm64 v2.0.0 并 smoke。
