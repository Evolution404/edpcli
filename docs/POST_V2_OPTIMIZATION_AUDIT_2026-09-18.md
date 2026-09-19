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

- `BackupCatalog::load -> scan_backup_dir`：完整读取、SHA-256、LBA8 缓存；
- `find_backups`：按文件名 pattern 重复 `read_dir`，再读取 LBA4 做身份终验。

`list`、`info`、`apply` 仍使用后者；backup 管理使用前者。多盘/大量历史备份时存在重复目录
遍历，但 `scan_backup_dir` 又会主动计算所有 SHA-256，因此不能简单把所有只读查询切到完整 catalog，
否则可能把轻量查询变重。

计划：本 PR 先量化并设计“轻量索引 / 完整健康扫描”边界；只有有明确收益且不增加普通
`list/info` I/O 时才实施。否则记录为后续独立性能 PR。

### P3：大文件职责继续收敛

当前最大生产文件：`cli.rs` 1651 行、`diskio.rs` 1251 行、`inspect.rs` 1212 行。
其中 `cli.rs` 同时承载路由、apply/restore 编排、UI helper 和大量单测；`diskio.rs` 同时承载
raw I/O、备份格式、目录扫描和原子写入。

计划：只做“能够减少重复语义或提高测试隔离”的拆分，不以行数为目的机械拆文件。

## 已完成优化

### A. 短扇区输入统一 fail-closed

提交：`318f65b fix: fail closed on truncated sector input`

- 先新增会让旧实现 panic 的回归测试，确认旧实现确实在 511B 输入上越界；
- LBA0/6/7/12 转换、`looks_nopwd`、`parse_lba12` 和主 `convert` 均在切片前验证扇区长度；
- 异常输入现在返回现有错误类型，不再依赖上层“必定传入 512B”的隐含前提；
- 合法 512B 输入的 golden 结果保持不变。

### B. 删除 v1 onlyid catalog 选择模型

提交：`39ebac5 refactor: remove v1 backup catalog selectors`

- 删除 `onlyid_group` / `onlyid_index` / `onlyid_values`；
- 生产选择逻辑只保留 v2 `BackupSelector + 全局稳定编号`；
- “文件名时间优先于 mtime”的排序契约迁入 catalog 基础测试；
- 本提交净删除 68 行，减少未来重新形成双编号模型的风险。

### C. 备份目录查询减少重复 I/O

提交：`8490655 perf: scan backup directory once per lookup`

- `find_backups` 从“每个 pattern 重复 `read_dir`”改为一次目录快照后内存匹配；
- device_id 精确层级、legacy 兼容层级、通用兜底层级语义保持不变；
- LBA4 最终身份过滤、排序和 fail-closed 规则保持不变；
- 新增层级优先级回归测试，防止通用 pattern 抢先混入其他型号备份。

### D. info / inspect 复用只读句柄与扇区缓存

提交：`091143b perf: reuse read-only sector handles and cache`

- 新增 `SectorReadCache`，只允许用于只读展示/诊断路径；
- physical `info`、physical/backup `inspect`、backup summary 不再每读一个 LBA 都重新打开文件；
- `info` 预读 LBA7/LBA4 后，摘要阶段再次请求同一 LBA 时直接复用；
- `apply/restore` 的写前/写后安全复核明确不使用该缓存，仍执行 fresh read。

### E. list 单设备扫描去重扇区读取

提交：`f544b2c perf: cache sectors during list scan`

- 测试先证明免密盘原实现会在一次 `list` 扫描中读取两次 LBA12；
- 每个设备扫描新增只读 sector cache；
- 免密判断与分区展示共享已读 LBA12，实际底层读取由 2 次降为 1 次。

### F. backup catalog 每份内容只计算一次 SHA-256

提交：`e58f552 perf: hash backup contents once per scan`

- 原实现为 `content_sha256` 计算一次，又为 sidecar 健康校验计算一次；
- 现在一次计算同时服务内容摘要与 sidecar 比对；
- `.sha256` 格式、健康状态和删除前内容复核语义不变。

### G. info 自动提权保持备份目录一致

提交：`aa450cb fix: preserve backup directory across info elevation`

- `info` 物理盘路径现在与 list/apply/backup create 一样，将环境变量来源的备份目录转换为
  显式 `--backup-dir` 带过平台提权边界；
- 避免 `EDPCLI_BACKUP_DIR` 未继承时，提权前后备份数量/最新备份信息漂移。

### H. list 每个物理盘只打开一次只读 raw fd

提交：`a248c20 perf: reuse raw disk handle during list scan`

- 一次 `list` 会话内按 disk 复用 `FileDev`；
- 与 E 项的 sector cache 叠加后，同一盘既不会重复 open，也不会重复读取同一 LBA；
- 仅作用于只读 list 路径，不改变 apply/restore 的 reopen/新鲜度安全语义。

### I. Shell completion 改为 metadata-only 轻量索引

提交：`ac2f77b perf: make shell completion metadata-only`

- 旧实现的 `backup-number` / `backup-file` 补全依赖完整 `BackupCatalog`，会读取每份 `.bin`、
  计算 SHA-256 并解析内容；
- 新实现只扫描普通 `.bin` 文件名并校验备份命名格式，不打开备份内容；
- `backup-number` 仅使用可识别备份数量，`backup-file` 仅返回文件名；
- 新增源码级性能门禁：`completion.rs` 禁止重新依赖 `BackupCatalog` / `BackupSelector`；
- 正常 `backup list/verify/delete` 仍使用完整健康扫描，未削弱校验语义。

### J. 固化只读 I/O 性能契约

提交：`538d50c test: lock read-only io performance contracts`

- 抽出仅供只读扫描使用的 `ReadOnlyDiskPool`，测试锁定同一物理盘在一次 `list` 会话内只打开一次；
- `SectorReadCache` 测试改为真实 `info` 访问序列：先预读 LBA7/LBA4，再请求
  LBA0/4/6/7/8/11/12，底层实际读取固定为 7 个唯一 LBA；
- 已有 `identify_list` 测试继续锁定同一次设备扫描中 LBA12 不得重复读取；
- 这些缓存/池均明确只属于展示和诊断路径，apply/restore 安全终验不使用。

## 本轮审计后暂不实施的候选

- **机械拆大文件**：`cli.rs` / `diskio.rs` / `inspect.rs` 仍较大，但当前没有足够证据证明单纯拆文件
  能改善正确性或性能，暂不制造无收益 churn；
- **统一 `find_backups` 与完整 `BackupCatalog` 扫描**：完整 catalog 会读取所有备份并校验 SHA-256，
  若强行给 list/info 共用，可能把轻量查询变重，因此保留“轻量查找 + 完整健康扫描”两个职责；
- **修改 release profile 追求更小二进制**：当前本机 macOS arm64 release binary 约 1.29MB，
  依赖树无重复 crate；没有必要为了体积引入 `opt-level=z` / `panic=abort` 等行为变化；
- **缓存 apply/restore 系统探测或扇区读取**：明确不做。写盘安全边界需要 fresh validation，
  性能收益不能覆盖介质更换/状态变化风险。

如果后续单独做性能 PR，优先用真实多盘/大量备份目录基准再决定是否引入持久索引，而不是
先增加缓存格式和失效逻辑。

## 明确不在本 PR 改动

- EDP/cems 加密/解密算法；
- LBA 布局和分区算法；
- 备份文件格式、SHA-256 sidecar 格式；
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

当前 A-J 已完成。最终变更均保持 CLI/API/备份格式兼容；如果用户要求正式
发布，按 `docs/RELEASE.md` 应作为 **PATCH** 版本递增，即 `2.0.1`。仅审计/PR 阶段不提前改版本。

## 安全红线

继续保持：系统盘 fail-closed、USB 整盘校验、selector pinning、写前备份、卸载/锁卷、
reopen 二次身份确认、原子写入、sync、读回校验、失败回滚、onlyid/LBA4 防串盘恢复。

`backup create` 仍必须保持纯只读介质路径。
