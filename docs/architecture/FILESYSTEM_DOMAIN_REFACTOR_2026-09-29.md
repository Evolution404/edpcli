# 文件系统领域重构计划 — 2026-09-29

状态：**已批准执行**

基线：分支 `feat/device-workbench-20260928`，编写计划时 HEAD 为 `9d5ed1e9d9249e0e183617a17984ed5943041cc0`。

## 1. 目标

建立唯一、独立的文件系统领域层，统一负责文件系统识别、有界元数据提取、格式化、格式化后校验和可选只读分析。备份、检查、制盘、恢复和 TUI 只能调用稳定接口，不再直接包含 FAT/exFAT/NTFS 的磁盘结构知识。

最终验收标准：新增一种文件系统时，通常只需要新增 `src/filesystem/<name>.rs`、在注册表增加一项并补对应测试；备份、检查、恢复、制盘和 TUI 不应新增该文件系统名称的业务分支。

## 2. 安全边界与非目标

- 不降低物理介质身份校验、几何校验、卸载/锁卷、重新打开、`WriteTransactionPlan`、同步、读回和回滚安全要求。
- 文件系统驱动不得接收 `diskN`、VID/PID、USB 序列号、onlyid、device_id 或物理写盘授权状态。
- 文件系统驱动不得直接打开 `/dev/diskN`，不得决定是否允许写盘。
- 元数据备份仍不得保存用户文件、目录清单、FAT/位图/MFT、空闲空间或删除残留。
- Plain 盘允许为生成 `filesystem_hint` / `volume_label_hint` 执行有界、瞬时、只读探测，但被读取的文件系统扇区不得成为 EDPB 可恢复工件。
- EDP 盘的交换区/保密区卷标继续来自已验证的 LBA10 等协议元数据，不得为了卷标额外读取 EDP 数据分区文件系统。
- EDP 加密属于分区转换层，不属于 FAT/exFAT/NTFS 驱动。
- 不引入运行时动态插件机制。

## 3. 当前技术债

文件系统职责目前分散在：

- `src/provision/filesystem.rs`：FAT16/exFAT 构建、卷标校验、稀疏镜像，以及与 SM4 变换耦合的辅助逻辑。
- `src/filesystem_analysis.rs` 及其 FAT/exFAT 子模块：文件系统分析。
- `src/inspect_target.rs`：`FilesystemBootKind` 以及 FAT12/FAT16/FAT32/exFAT/NTFS 识别。
- `src/backup_metadata.rs`：FAT16 卷标解析，以及 exFAT 根目录/FAT 链上 `0x83 Volume Label` 查找。
- `src/application/post_restore/format_operation.rs`：直接依赖具体文件系统格式类型和部分格式化后校验逻辑。

这些重复实现使每增加一种文件系统都要横跨多个业务域修改。

## 4. 目标目录

```text
src/filesystem/
├── mod.rs
├── kind.rs
├── error.rs
├── io.rs
├── metadata.rs
├── format.rs
├── driver.rs
├── registry.rs
├── fat12.rs
├── fat16.rs
├── fat32.rs
├── exfat.rs
├── ntfs.rs
└── analysis/
    ├── mod.rs
    ├── fat.rs
    └── exfat.rs
```

整体调用关系：

```text
backup / inspect / provision / restore
                |
                v
       filesystem registry
                |
        filesystem driver
                |
      metadata / FormatPlan
                |
                v
        PartitionTransform
         plain / EDP-SM4
                |
                v
       WriteTransactionPlan
                |
                v
 identity -> lock -> reopen -> write -> sync -> readback -> rollback
```

## 5. 统一领域模型

### 5.1 `FilesystemKind`

只保留一套文件系统类型：

```rust
pub enum FilesystemKind {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ntfs,
}
```

`FilesystemKind` 负责统一的配置标识和显示标识。`inspect_target::FilesystemBootKind` 与 `provision::OfficialFilesystemFormat` 只允许在迁移阶段临时存在，最终删除。

### 5.2 能力声明

识别能力与格式化能力必须分离：

```rust
pub struct FilesystemCapabilities {
    pub detect: bool,
    pub read_metadata: bool,
    pub format: bool,
    pub verify_format: bool,
    pub analyze: bool,
}
```

初始能力矩阵：

| 类型 | 识别 | 元数据 | 格式化 | 格式化校验 | 深度分析 |
| --- | --- | --- | --- | --- | --- |
| FAT12 | 是 | 最小 | 否 | 否 | 可选 |
| FAT16 | 是 | 是 | 是 | 是 | 是 |
| FAT32 | 是 | 最小 | 否 | 否 | 是 |
| exFAT | 是 | 是 | 是 | 是 | 是 |
| NTFS | 是 | 最小 | 否 | 否 | 可选 |

### 5.3 分区相对 IO

驱动只能看到分区相对扇区：

```rust
pub trait FilesystemReader {
    fn sector_size(&self) -> u32;
    fn sector_count(&self) -> u64;
    fn read_sector(&mut self, relative_lba: u64)
        -> Result<[u8; 512], FilesystemError>;
}
```

外层磁盘/应用层通过 `PartitionView` 完成 `partition_start + relative_lba` 的绝对 LBA 映射。

### 5.4 元数据投影

```rust
pub struct FilesystemMetadata {
    pub kind: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}
```

`None` 明确表示没有可靠用户卷标，业务层不得生成占位名称。

### 5.5 `FormatRequest`

```rust
pub struct FormatRequest {
    pub filesystem: FilesystemKind,
    pub volume_label: Option<String>,
    pub volume_serial: Option<u32>,
}
```

`None` 是“无用户卷标”的统一领域语义。FAT16 驱动内部把它编码成 BPB `NO NAME    ` 且不创建根目录卷标项；exFAT 驱动内部不创建 `0x83` 卷标项。

### 5.6 `FormatPlan`

文件系统层不得直接写盘：

```rust
pub struct FormatPlan {
    pub filesystem: FilesystemKind,
    pub geometry: FilesystemGeometry,
    pub writes: Vec<FilesystemWrite>,
    pub expected_metadata: FilesystemMetadata,
}

pub struct FilesystemWrite {
    pub relative_lba: u64,
    pub data: [u8; 512],
}
```

现有稀疏文件系统镜像应优先适配复用，不重新实现已经稳定的字节生成逻辑。

### 5.7 类型化错误

```rust
pub enum FilesystemErrorKind {
    Unsupported,
    InvalidGeometry,
    InvalidBootSector,
    InvalidMetadata,
    InvalidVolumeLabel,
    ReadFailure,
    FormatUnsupported,
    CorruptFilesystem,
    ScanBudgetExceeded,
    AmbiguousDetection,
}
```

TUI 与应用层不得通过错误字符串反推文件系统状态。

## 6. `FilesystemDriver` 接口

每个文件系统驱动统一实现正常文件系统能力：

```rust
pub trait FilesystemDriver: Send + Sync {
    fn kind(&self) -> FilesystemKind;
    fn capabilities(&self) -> FilesystemCapabilities;
    fn detect(&self, source: &mut dyn FilesystemReader)
        -> Result<DetectionResult, FilesystemError>;
    fn matches_geometry(&self, source: &mut dyn FilesystemReader, geometry: FilesystemGeometry)
        -> Result<bool, FilesystemError>;
    fn read_metadata(&self, source: &mut dyn FilesystemReader)
        -> Result<FilesystemMetadata, FilesystemError>;
    fn validate_format_request(&self, request: &FormatRequest)
        -> Result<(), FilesystemError>;
    fn build_format_plan(&self, geometry: FilesystemGeometry, request: &FormatRequest)
        -> Result<FormatPlan, FilesystemError>;
    fn verify_format(&self, source: &mut dyn FilesystemReader, geometry: FilesystemGeometry,
        expected: &FilesystemMetadata) -> Result<FormatVerification, FilesystemError>;
}
```

不支持的能力返回类型化 `Unsupported` / `FormatUnsupported`，不要求提供伪实现。

## 7. 静态注册与识别

使用编译期静态注册，不做动态插件。识别结果使用可信度而不是布尔值：

```rust
pub enum DetectionConfidence { NoMatch, Weak, Strong, Exact }
pub struct DetectionResult {
    pub kind: FilesystemKind,
    pub confidence: DetectionConfidence,
}
```

注册表选择最高可信度结果；如果多个驱动得到相同的非零最高可信度，则按歧义失败，禁止依赖注册顺序静默选择。

## 8. 加密边界

最终必须形成：

```text
FilesystemDriver::build_format_plan()
             |
             v
       plaintext FormatPlan
             |
             v
       PartitionTransform
       - IdentityTransform
       - EdpSm4Transform
             |
             v
       WriteTransactionPlan
```

文件系统模块不得导入 FileKey 包装/解包、密码策略、LBA7/LBA12、onlyid/device_id 或 EDP 密钥域语义。`encrypt_sparse_mode2` 等职责迁移到独立的分区转换层。

## 9. 分析能力边界

现有 `filesystem_analysis` 保留价值，但迁移到 `filesystem/analysis/` 作为可选扩展。核心能力是识别、有限元数据、格式化和校验；目录遍历、使用量统计、文件枚举、文件内容定位与流式读取属于可选分析能力。一个文件系统即使暂时没有分析器，也可以注册检测或格式化能力。

## 10. 业务域迁移后的行为

### 10.1 备份

Plain 备份统一调用 `PartitionView -> registry.detect -> driver.read_metadata -> ManifestPartition hint`。`backup_metadata.rs` 最终不得再出现 FAT/exFAT 偏移、GBK 卷标规则、根簇遍历或 `0x83` 常量。

EDP 备份继续以协议为事实源：原始 LBA0-12、验证后的 LBA7 兼容区域、确认的盘尾恢复结构、LBA12 几何/密钥域字段、LBA10 交换区/保密区卷标。不得为了卷标调用数据分区文件系统驱动。

### 10.2 检查

`inspect_target` 把文件系统识别委托给注册表，需要更深分析时再调用驱动/analyzer。最终文件中不得保留 FAT/exFAT/NTFS 签名解析。

### 10.3 制盘

制盘层选择 `FilesystemKind`，构造通用 `FormatRequest`，从驱动取得 `FormatPlan`，再应用可选分区转换与既有写盘安全事务。不得直接调用 `build_empty_fat16` / `build_empty_exfat`。

### 10.4 恢复

`PartitionFormatRequest` 改用 `FilesystemKind`。卷标改为 `Option<String>`：可靠备份提示为 `Some`，用户清空为 `None`，老备份/未知卷标为 `None`。恢复、格式化、密钥域重建继续保持独立确认。

## 11. EDP 分区 role 语义修正

本轮一并修正当前仅根据 PartionType 推导 role 的简化逻辑：

- mode0：`boot`、`share`、`encrypt`；
- mode1：`boot_share_combined`、`encrypt`；
- mode2：`compatibility_reserve`、`encrypt`；
- mode3：`boot`、`share`。

原始协议字节仍是恢复事实源，此项只修正清单的类型化语义。

## 12. 最终删除项

最终删除或消除直接使用：

- `inspect_target::FilesystemBootKind`；
- `inspect_target::detect_plain_filesystem`；
- `provision::OfficialFilesystemFormat`；
- `backup_metadata::decode_fat16_boot_label`；
- `backup_metadata::read_exfat_volume_label`；
- `src/filesystem` 之外的文件系统特定格式化分支；
- `src/filesystem` 之外的文件系统特定卷标校验；
- `src/filesystem` 之外的文件系统特定格式化读回校验；
- 迁移后的顶层 `src/filesystem_analysis.rs` 及旧子路径；
- 位于文件系统格式化模块内的 EDP 加密 helper。

同时审计 `PARTITION_PREFIX_SECTORS` / `PARTITION_SUFFIX_SECTORS` 等历史“文件系统元数据范围”假设；没有当前协议/迁移消费者的直接删除。

## 13. 实施阶段

### F0 — 冻结现有行为

状态：`COMPLETE`

锁定 FAT12/16/32/exFAT/NTFS 识别；FAT16 卷标、无卷标、格式化和读回；exFAT `0x83` 卷标、根目录/FAT 链有界读取、无卷标、格式化和读回；Plain 只保存 hint；EDP LBA10 卷标不读数据分区文件系统；加密格式化字节行为；恢复原卷标/用户覆盖/无卷标；现有写盘安全门槛。

退出条件：生产逻辑开始迁移前，专项测试全部通过。

### F1 — 建立文件系统领域骨架

状态：`COMPLETE`

创建 `mod.rs`、`kind.rs`、`error.rs`、`io.rs`、`metadata.rs`、`format.rs`、`driver.rs`、`registry.rs`。先使用适配层/转换连接现有实现，不改变行为。

退出条件：全部 target 可编译，新领域专项测试通过。

### F2 — 迁移 FAT16

状态：`COMPLETE`

把 FAT16 识别、BPB 校验、卷标编解码、无卷标语义、空文件系统构建、格式化请求校验和读回校验迁入 `filesystem/fat16.rs`。备份/检查/制盘/恢复改走驱动，再删除其他位置的 FAT16 重复知识。实施中补充了通用 `matches_geometry`，用于把 FAT16 hidden sectors、后续 exFAT partition offset 等几何确认也收敛到驱动内。

### F3 — 迁移 exFAT

状态：`IN_PROGRESS`

把 exFAT 识别、几何解析、根目录/FAT 链上的有界 `0x83` 卷标读取、无卷标语义、空文件系统构建、格式化校验和读回校验迁入 `filesystem/exfat.rs`，删除外部重复逻辑。

### F4 — 统一只读检测驱动并切业务调用

状态：`PENDING`

建立 FAT12/FAT32/NTFS 最小驱动，按现有能力提供识别和元数据读取。把备份、检查、制盘、恢复、TUI 请求类型切到 `FilesystemKind` / `registry`，并修正 EDP 模式感知的清单角色。删除旧枚举或临时别名。

### F5 — 抽离分区转换层

状态：`PENDING`

把 SM4 文件系统输出变换移出文件系统领域；同一个明文文件系统驱动同时服务 Plain 和加密 EDP 分区。保持现有加密格式化/重建安全语义。

### F6 — 迁移 analysis 并清理技术债

状态：`PENDING`

把 `filesystem_analysis` 迁到 `filesystem/analysis`，更新正式迁移消费者，删除旧模块、适配层、别名和重复辅助函数，并执行 grep 门禁证明具体文件系统知识不再泄漏到业务域。

## 14. 最终 grep 门禁

```text
rg "detect_plain_filesystem" src
rg "FilesystemBootKind" src
rg "OfficialFilesystemFormat" src
rg "decode_fat16_boot_label|read_exfat_volume_label" src
rg "build_empty_fat16|build_empty_exfat" src --glob '!filesystem/**'
rg "0x83" src --glob '!filesystem/**'
```

结果应为空，或仅存在于明确标记的迁移兼容/测试声明中。业务域不得根据具体文件系统名称决定备份/检查/恢复/制盘逻辑。

## 15. 测试门禁

每阶段执行 `cargo fmt --all`、`git diff --check`、受影响专项测试；修改公共类型/模块时执行 `cargo check --all-targets`；阶段提交前执行 `./scripts/test-fast.sh`。

大范围里程碑和最终完成执行 `python3 scripts/test-full.py --profile full`。文件系统字节或写盘规划变化时执行 Virtual Disk HIL。真实 USB 破坏性测试不自动执行。

## 16. 提交顺序

1. `docs(architecture): plan filesystem domain refactor`
2. `test(filesystem): freeze filesystem behavior`
3. `refactor(filesystem): add domain interfaces`
4. `refactor(filesystem): migrate fat16 driver`
5. `refactor(filesystem): migrate exfat driver`
6. `refactor(filesystem): route application through registry`
7. `refactor(filesystem): extract partition transforms`
8. `refactor(filesystem): move analysis and remove legacy APIs`

阶段必须可审计，不因为提交方便而把行为改变和大范围迁移静默压成一个提交。

## 17. 最终验收条件

1. 只有一套 `FilesystemKind`。
2. FAT16/exFAT 的识别、元数据读取、格式化、校验仅存在于文件系统驱动。
3. FAT12/FAT32/NTFS 检测仅存在于文件系统驱动。
4. `backup_metadata` 不含文件系统磁盘结构偏移/签名。
5. `inspect_target` 不含 FAT/exFAT/NTFS 解析器。
6. 制盘/恢复通过通用接口请求格式化。
7. 加密作为文件系统 plan 之后的分区转换。
8. 文件系统驱动不含 EDP 协议/密钥域知识。
9. 驱动永远不接收 `diskN` 或物理身份。
10. 元数据备份仍不保存文件系统/用户数据工件。
11. 老备份无卷标继续得到 `None`，不生成占位名称。
12. EDP LBA10 卷标继续在不读数据分区的情况下工作。
13. mode1/mode2 清单角色语义正确。
14. fast/full 门禁通过。
15. 新增文件系统通常只需要驱动、注册项和测试。
