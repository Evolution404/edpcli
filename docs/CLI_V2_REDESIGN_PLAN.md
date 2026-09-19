# edpcli v2 CLI 重构实施计划

日期：2026-09-18
目标版本：**v2.0.0**
状态：设计冻结，待实现

## 1. 重构目标

当前 CLI 功能完整，但用户需要记住较多“内部数据模型”才能完成普通任务：

- `run` 与 `apply` 只是 dry-run / 真写差异，却占用两个一级命令；
- `meta` / `metainfo` / `inspect` 都能查看物理盘和备份，边界不清；
- `restore` 位于顶层，但其本质属于备份生命周期；
- `backup` 同时使用路径、`onlyid`、`--index`、编号/范围等多套选择方式；
- `inspect` 的位置参数同时可能表示 LBA、备份文件等，解析规则过重；
- 当前没有独立的“立即备份当前插入 U 盘”能力，备份主要由 `apply` 写前自动生成。

v2 的核心目标不是减少功能，而是把用户心智模型收敛为：

> **发现设备 → 查看信息 → 预览/执行改造 → 创建/管理备份 → 必要时底层检查**

普通用户不应先理解 `onlyid`、`device_id`、LBA 编号和备份内部命名规则才能完成日常操作。

## 2. v2 顶层命令

最终第一屏只保留以下一级命令：

```text
edpcli [命令]

常用:
  list              查看当前插入的 U 盘
  info              查看 U 盘或备份详细信息
  apply             预览或执行 U 盘改造
  backup            创建、查看、校验、恢复和清理备份
  inspect           高级：检查底层 LBA/hex 数据

其他:
  convert           高级：离线转换快照
  completion        Shell 补全
  version           版本与构建信息
  help              帮助
```

**无参数 `edpcli` 等价于 `edpcli list`。**

## 3. 明确删除/合并的旧语义

v2 为破坏性 CLI 重构，因此发布时版本升级到 `2.0.0`。项目仍处开发阶段，**不保留旧语法兼容层或隐藏 alias**，避免后续长期维护两套 parser / help / completion / 测试。

删除或合并：

| v1 | v2 |
|---|---|
| `edpcli run` | `edpcli apply --dry-run` |
| `edpcli apply` | `edpcli apply` |
| `edpcli restore` | `edpcli backup restore` |
| `edpcli meta` / `metainfo` | `edpcli info` |
| `edpcli backup rm` | `edpcli backup delete` |
| 普通流程中的 `--onlyid` / `--index` | 以交互选择、统一编号、文件路径或自动匹配代替 |
| `inspect 6 7 12 ...` 的裸 LBA 位置参数 | `inspect --lba 6,7,12 ...` |

`onlyid`、`device_id` 仍然保留在数据模型、身份校验、输出和内部 API 中，只是不再要求普通用户手工输入它们。

## 4. `list`：默认入口

### 用法

```text
edpcli
edpcli list
```

### 行为

- 只读，不写盘；
- 自动发现外接盘；
- 显示设备、容量、总线、VID:PID、姓名、部门、EDP 状态、免密状态、备份数量；
- 如读取裸盘身份/姓名/部门实际遇到权限不足，由程序自己触发平台提权并重执行；
- 没有外接盘时不弹无意义的提权请求；
- 多盘只负责展示，不在 `list` 中引入选择动作。

现有 `Row.dept` / `Row.user`、LBA8 解析和自动提权逻辑直接复用，不另起实现。

## 5. `info`：统一“查看详细信息”

### 用法

```text
edpcli info
edpcli info --disk 2
edpcli info backup.bin
```

### 选择规则

1. 指定文件：查看该备份；
2. 指定 `--disk`：查看该物理盘；
3. 未指定且只有一个可用目标盘：自动选择；
4. 多个目标盘：交互菜单选择；
5. 无盘：提示用户插入设备；不自动转去备份目录猜测。

### 输出结构

统一按以下信息块展示：

- 设备：设备路径/编号、容量、总线、VID:PID；
- 身份：onlyid、device_id、姓名、部门；
- 状态：EDP/cems、免密、SAFE6、分区摘要；
- 备份：当前盘匹配备份数量、最新备份时间。

复用现有 `metainfo` 领域逻辑；最终删除 `MetaInfoOpts` 和 `Parsed::MetaInfo` 的旧 CLI 形态，重命名为面向任务的 `Info`。

## 6. `apply`：合并 run/apply

### 用法

```text
edpcli apply
edpcli apply --dry-run
edpcli apply --disk 2
edpcli apply --size 100
edpcli apply --force
edpcli apply --yes
```

### 语义

- `apply --dry-run`：执行完整识别和布局计算，但不提交任何写入；
- `apply`：真实写入；
- 不再存在 `run` 一级命令；
- `--force` 仍只用于已经是免密状态时明确允许重复改造；
- `--yes` 仍用于脚本化确认；
- 写盘前仍自动创建备份。

### 安全边界不可回退

必须保持：

- USB 外接整盘确认；
- 系统盘 fail-closed；
- 提权前 selector pinning；
- 写前 LBA0-12 快照；
- 自动备份；
- 卸载/锁卷；
- reopen 后身份和元数据二次确认；
- 原子写入 + sync + 读回校验；
- 失败回滚；
- Linux/Windows virtual-disk HIL。

## 7. `backup`：完整备份生命周期

### 7.1 创建当前 U 盘备份（新增核心功能）

```text
edpcli backup create
edpcli backup create --disk 2
```

要求：

- 直接备份当前插入的目标 U 盘，不依赖 `apply`；
- 多盘时进入统一设备选择器；
- 只读 LBA0-12，不修改介质；
- 必要时自动提权；
- 必须复用 `apply` 写前备份的同一底层 pipeline：
  - 同一命名规则；
  - 同一 LBA0-12 数据格式；
  - 同一 onlyid/device_id/VID/PID/容量元数据；
  - 同一 MD5 sidecar；
  - 同一碰撞保护；
  - 同一目录解析规则；
  - 同一 fsync/目录持久化策略。
- 禁止出现“手动备份”和“apply 自动备份”两套格式或两套安全实现。

建议把现有 backup write 逻辑抽为单一 `create_backup(...)` service，由 `backup create` 与 `apply` 共用。

### 7.2 查看备份

```text
edpcli backup
edpcli backup list
```

`backup` 缺省等价于 `backup list`。

列表使用全局稳定展示编号（当前进程内），同时显示：

- 时间；
- 姓名；
- 部门；
- 原始/免密状态；
- onlyid（作为信息，不要求用户手工输入）；
- 文件名可放在详情或宽终端列中。

### 7.3 恢复

```text
edpcli backup restore
edpcli backup restore 2
edpcli backup restore backup.bin
```

无参数时：

1. 选择/自动确定目标物理盘；
2. 自动按当前盘 onlyid/身份过滤可用备份；
3. 交互选择备份；
4. 校验大小、MD5、onlyid 和当前物理盘身份；
5. 用户确认；
6. 执行现有 restore 原子写入流程。

禁止恢复到其他物理盘的安全规则保持不变。

### 7.4 校验

```text
edpcli backup verify
edpcli backup verify 2
edpcli backup verify backup.bin
```

- 无参数：全部校验；
- 数字：选择当前 `backup list` 语义下的展示项；
- 文件：精确校验该文件。

### 7.5 删除

```text
edpcli backup delete
edpcli backup delete 2
edpcli backup delete 2,4,5
edpcli backup delete backup.bin
```

- `delete` 取代 `rm`；
- 无参数进入交互多选；
- 默认预览 + YES 确认；
- `--yes` 用于脚本；
- 保留“不能误删到目录外”“最后一份保护”等现有门禁。

### 7.6 策略清理

```text
edpcli backup prune
edpcli backup prune --keep 3
edpcli backup prune --yes
```

继续使用 `prune`，明确表示“按保留策略清理”，与人工 `delete` 区分。

## 8. `inspect`：高级工具，消除位置参数歧义

### 用法

```text
edpcli inspect
edpcli inspect --disk 2
edpcli inspect backup.bin
edpcli inspect --lba 7
edpcli inspect --lba 6,7,8,12
edpcli inspect backup.bin --lba 7,12 --hex
edpcli inspect --lba 7 --raw
edpcli inspect --lba 7,12 --export ./out
```

规则：

- 数字不再作为裸位置参数；
- LBA 必须通过 `--lba`；
- 备份文件仍可作为唯一位置参数，因为其类型无歧义；
- `--hex` / `--raw` 互斥；
- `--export` 保留；
- 自动识别 device_id，只有无法识别的离线高级场景才允许显式 `--id`。

删除面向用户的 `--onlyid + --index` inspect 组合。

## 9. `convert`：保留为高级离线命令

保留：

```text
edpcli convert --dir snapshot --id DEVICE_ID [--size GB] [--out DIR]
```

`convert` 是协议/回归/工程工具，不进入主帮助“常用”区域，不与物理盘选择逻辑合并。

## 10. 参数模型重构

建议最终 Parser 结构接近：

```text
Command
├── List
├── Info(SourceOpts)
├── Apply(ApplyOpts { dry_run, force, yes, ... })
├── Backup(BackupCommand)
│   ├── Create(DeviceOpts)
│   ├── List
│   ├── Restore(BackupSelector)
│   ├── Verify(BackupSelector)
│   ├── Delete(BackupSelector)
│   └── Prune
├── Inspect(InspectOpts)
├── Convert(ConvertOpts)
├── Completion
├── Version
└── Help
```

不要在 parser 中继续保留：

- `Parsed::Run`；
- 顶层 `Parsed::Restore`；
- `Parsed::MetaInfo`；
- `BackupAction::Rm`；
- 兼容旧命令的 alias 分支。

## 11. 统一选择器

当前多个模块分别实现“选盘 / 选备份 / onlyid + index”。v2 应抽象两个用户级选择器：

### DeviceSelector

负责：

- `--disk` 显式目标；
- 单盘自动选；
- 多盘交互；
- 提权前固定到平台原生 selector；
- 系统盘/非 USB/分区目标拒绝。

### BackupSelector

负责：

- 无参数交互选择；
- 数字编号；
- 文件路径；
- 必要时按当前物理盘身份过滤；
- 输出统一编号和选择行为。

业务命令不再自己解析 onlyid/index 组合。

## 12. 帮助与交互设计

### 主 help

必须保持一屏可读，不展开 inspect/backup 的全部高级语法。

### 子命令 help

```text
edpcli help backup
edpcli help inspect
edpcli help apply
```

分别展开对应领域。

### 错误信息

错误信息要直接给下一步：

- 多盘：直接展示编号菜单，不要求重跑命令；
- 权限不足：自动请求权限，不提示用户手工 `sudo`；
- 没有备份：提示 `edpcli backup create`；
- 想预览改造：提示 `edpcli apply --dry-run`；
- 旧命令输入（v2）：作为未知命令报错，并给新命令建议，但**不执行兼容逻辑**。

例如：

```text
错误: v2 已取消 `run`。
请使用: edpcli apply --dry-run
```

这只是迁移提示，不是兼容路径。

## 13. 测试先行实施顺序

### Phase 0：冻结现有安全基线

- 全量 `cargo test --all-targets`；
- `cargo clippy --all-targets -- -D warnings`；
- 6 架构 CI；
- Linux/Windows arm64+x86_64 virtual-disk HIL。

### Phase 1：Parser / help 新模型

先写失败测试，再重构 argv parser：

- 无参数 => list；
- `info`；
- `apply --dry-run`；
- `backup create/list/restore/verify/delete/prune`；
- `inspect --lba`；
- 旧命令只给迁移错误，不进入兼容实现。

暂不改底层磁盘逻辑。

### Phase 2：统一 DeviceSelector / BackupSelector

- 抽选择逻辑；
- 删除重复 onlyid/index 解析；
- 保持 selector pinning 和 fail-closed。

### Phase 3：`info`

- 复用 metainfo service；
- 删除旧 meta/metainfo CLI 入口和 parser 技术债。

### Phase 4：`apply --dry-run`

- `run` 流程迁入 apply；
- 删除 `Parsed::Run`；
- 真写路径与 dry-run 共享计划生成逻辑。

### Phase 5：`backup create`

- 先为“独立备份当前 U 盘”写集成测试；
- 抽出 apply/create 共用 backup service；
- 验证生成文件与现有自动备份格式完全一致；
- 验证纯只读，不进入 prepare_write/unmount/write 路径。

### Phase 6：backup restore/verify/delete/prune 统一

- 顶层 restore 下沉；
- rm -> delete；
- 统一选择器；
- 删除旧 onlyid/index UI 语义。

### Phase 7：inspect 收口

- `--lba` 明确化；
- 删除数字位置参数；
- 删除 onlyid/index 组合；
- 保持 raw/decoded/export 能力。

### Phase 8：补全 / 文档 / 技术债清理

- completion 与新 parser 完全同步；
- README / USAGE 全面改写；
- 搜索并禁止旧顶层命令和旧语法重新出现；
- 加 CLI grammar 门禁。

### Phase 9：v2 发布验收

- `Cargo.toml` 升到 `2.0.0`；
- 本地全量测试、fmt、clippy；
- PR 6 架构 CI + 4 HIL 全绿；
- 合并 main 后再跑 main 门禁；
- tag `v2.0.0`；
- 验证 7 个 release 包、SHA-256、SBOM、manifest；
- Apple Silicon 本机安装 macOS arm64 包并 smoke：`version/list/info/backup list`。

## 14. 必须新增的测试

至少覆盖：

1. `edpcli` 无参数等价 `list`；
2. `list` 姓名/部门展示；
3. `list` PermissionDenied 后自动提权决策；
4. `info` 单盘自动选、多盘选择、备份文件；
5. `apply --dry-run` 零写入；
6. `apply` 写前备份仍存在；
7. `backup create` 对当前盘直接生成 6656B + MD5；
8. `backup create` 与 apply 自动备份格式/命名元数据一致；
9. `backup create` 权限不足自动提权，但永不调用写盘 prepare/unmount；
10. `backup restore` 只显示/接受属于当前盘的备份；
11. `backup delete` 编号/多选/路径安全；
12. `inspect --lba` 解析与范围 0..12；
13. 旧 `run/meta/metainfo/restore/backup rm` 返回迁移提示；
14. completion 不再补全旧语法；
15. 平台边界门禁继续禁止业务层出现 OS 专用命令/路径。

## 15. 非目标

本次不要顺手修改：

- EDP/cems 协议和加密算法；
- LBA 格式；
- 备份文件格式；
- onlyid/device_id 定义；
- 分区布局算法；
- 三平台硬件识别策略；
- 原子写入/回滚算法；
- Release 架构矩阵。

如果实现过程中发现这些领域的问题，另开 issue/后续 PR，不和 CLI v2 重构混在一起。

## 16. 提交与工程规范

- 禁止 `git reset` / `git clean`；
- 测试先行；
- 小 commit，按 Phase 分组；
- 每个可独立验证的阶段及时 push；
- 不保留 WIP stash 作为长期状态；
- 不引入为了“兼容 v1”而存在的 parser 分支；
- 每个阶段更新本计划中的完成状态或交接文档；
- 未通过全量门禁不得合并 main；
- 未通过 main 复验不得打 `v2.0.0` tag。
