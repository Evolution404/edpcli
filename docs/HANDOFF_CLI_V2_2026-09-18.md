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

## 下一位 AI 从这里开始

1. 先读：
   - `docs/CLI_V2_REDESIGN_PLAN.md`
   - `docs/RELEASE.md`
   - `docs/USAGE.md`
2. 检查 `git status --short --branch`，禁止 reset/clean。
3. 从 **Phase 2 DeviceSelector / BackupSelector** 继续，仍须测试先行；不要削弱 selector
   pinning、系统盘 fail-closed 或 onlyid 防串盘。
4. Phase 3 再正式收口 `info` service 和当前临时保留的内部 `MetaInfoOpts` 命名；
   Phase 5 才接通 `backup create` 执行层。
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
