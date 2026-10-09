# edpcli 全链路多逻辑扇区支持实施方案

> 日期：2026-10-09；设计基线：`bb19ab430e05498df0c57cf4681c98d0bcef799a` (`main`)。状态：**待审核、未实施**。
> 目标：在不降低现有设备/备份/写盘安全门禁的前提下，统一支持具有不同逻辑扇区大小的介质。

> **最高优先级红线 A — 官方行为证据：** 任何 4Kn 协议/几何/文件系统/写盘兼容行为，必须建立官方 Windows/官方 Linux 的生产端、消费端或实际官方产物证据对应表。不能凭自身推测补齐，也不能把“静态看见某函数”冒充已验证的端到端官方产物。
> **最高优先级红线 B — 旧功能零退化：** 不允许因新几何路径修改已验证 512B 设备的协议字节、制盘与恢复结果、`.edpb` 行为、密码与 LCE 兼容性、安全准入、现有 TUI 交互和性能约束。实施前冻结行为基线，实施后按功能逐项比较；任何差异必须解释、经审核并添加回归。
> **两条红线实行一票否决：** 官方关键证据未闭环，不得宣称 4Kn 完整支持；任意旧功能回归失败，不得合并、发布或替换已安装的稳定版本。

## 0. 范围、验收口径与不可破坏的约束

- 必须覆盖：macOS/Linux/Windows 设备发现、只读 `list/info/inspect`、USB 身份识别、EDP `LBA0..12`、原生尾部数据、MBR/GPT、各模式和 Plain、分区容量地图、`.edpb` 元数据备份及恢复、制盘/格式化、LCE/加密/密码/复制、CLI/TUI、进度与错误报告。保留当前 仅元数据备份 备份语义；不在本项目中偷偷将其定义为整盘用户数据备份。
- 硬件认证主线：512B、4096B。1024B、2048B、8192B 等具备架构容纳能力，采用逐档认证开关；未验证尺寸仍允许安全发现，禁止危险写入。设备逻辑扇区大小不是物理扇区大小：512e 应按 512B 逻辑 I/O，物理扇区用于报告/对齐策略。
- **固定协议载荷（多数 `LBA0..12` 原有字段前 512B）与设备原生逻辑块（512/4096B）是两个不同类型**。原生 `LBA=n` 的字节偏移始终是 `n * logical_sector_bytes`；不是 `n*512`，也不能将 4Kn LBA 值乘 8 当成 EDP LBA。
- 原生 LBA11: `0x000..0x1FF` 的 DRKB/PDKB 已在真实 U391 通过 `DiskSize` 密钥验证；`0x200..0xFFF` 有 1883 个非零字节。2026 版官方 `EdpEDiskCtrl.dll` 在 LBA11 只有 512B 初始化缓冲区，却按设备 `BytesPerSector` 调用 `WriteFile`，存在栈内存越界读取风险。历史 4Kn 盘尾部应作为 **OpaqueTail** 保存/显示，不得再泄露原始内容到常规日志。新盘写入必须初始化完整原生扇区，不复刻官方泄漏。
- 无论什么尺寸，不能放宽只读状态确认、设备重新打开身份绑定、原备份 SHA256、逐扇区读回、失败回滚；在 4Kn 写门禁未全绿前不可向实体 4Kn 磁盘写任何一字节。

## 1. 已发现的代码阻塞点（确切文件锚点）

| 代码位置 | 现状与整改要求 |
|---|---|
| `src/common.rs` | `SECTOR=512` 同时被当成协议长度和物理 I/O 长度；保留只用于 `PROTOCOL_PAYLOAD_BYTES` 的历史语义，禁止在设备 I/O/几何中隐式使用。 |
| `src/domain/hardware.rs::ObservedDeviceGeometry::writable_protocol_sectors` | 几何与 `512B` 写入许可耦合；拆 `validated_geometry` 与 `write_capabilities`。 |
| `src/platform/{macos,linux,windows}.rs` + `src/platform/system.rs` | 已探测部分原生几何，但需要经打开句柄复核并统一块计数/容量；Linux `/sys/.../size` 是 512B 单位，必须先转字节。 |
| `src/diskio/device.rs::FileDev` | 读/写/批量读写与 `read_sector_u64` 固定 `LBA*512` 和 512B；改原生块接口及动态长度，检查原始设备的 I/O 对齐。 |
| `src/diskio/transaction.rs` | `SectorDev`/patch/rollback/逐扇读回按 `[u8;512]`、`u32` LBA 设计；事务必须对完整原生块做快照、哈希、回滚。 |
| `src/provision/{generate,validate,layout,write_plan,plain,partition_format}.rs` | 协议 KDF、容量、分区生成和文件系统均含 512B 扇区假设；应按字节容量及原生扇区建模。 |
| `src/provision/reprovision/model.rs` | `LBA7/12 sector_size==512` 的硬条件；改与设备实际块长一致性及分区长度整除检查。 |
| `src/filesystem/{io,driver,fat12,fat16,fat32,exfat,format,analysis/*}.rs` | `FilesystemReader` 返回 `[u8;512]`，多个驱动显式禁止非 512B；统一 native-aware 读/写、BPB/BPB_shift、FAT 表/FSInfo/备份引导区容量计算与验证。NTFS 只保持现有已支持能力，不额外扩充功能。 |
| `src/edpb/{model,write,validate,read}.rs` | 已具备 `logical_sector_size`/区段元数据，但 v3 校验器拒绝非 512B，回放采用 `.as_chunks::<512>()`；更正动态步长、尺寸/区段/双 SHA 契约。 |
| `src/backup_metadata.rs` + `src/application/write/{backup,restore,restore_plan}.rs` | 完整 13 个原生 LBA、分区元数据、镜像/尾部偏移、恢复计划与几何门禁需一致按设备单位运行。 |
| `src/application/inspect/*`, `src/inspect/*`, `src/tui/inspect/*` | Raw 需展示 4096B、Decode 仅有根据证据明确的字节域、Mixed 仅已解码改变位置着色；搜索、跳 LBA、复制、导出、树/表格均保持统一。 |
| `src/application/disk_layout.rs`, `src/tui/{devices,backups,provision,restore}/*` | 容量地图、分区步进、Sector 输入单位、格式化/恢复确认和结果页更新为原生 LBA 且显示扇区大小。 |

## 2. 统一领域模型与 API

新增 `DeviceGeometry { capacity_bytes: u64, logical_bytes: NonZeroU32, physical_bytes: Option<NonZeroU32>, total_lbas: u64 }`；构造时必须校验 `capacity_bytes > 0`、整除逻辑块、`logical_bytes` 为受支持的 2 的幂且 `>=512`，以及 `u64` 安全溢出。测试设备可模拟其他尺寸，实盘写许可另判。构建 `DeviceLba(u64)`、`ByteOffset(u64)`、`ByteLength(u64)` 的轻量类型或至少集中转换 API，明确 `DeviceLba` 与历史 `512-byte unit` 不能混算。对 `MBR u32`、驱动协议 `u32` 界限单独拒绝，读取通道保留 `u64`。

确立三个互相独立的事实：

1. `NativeSector`：`lba: u64`、完整 `bytes: Vec<u8>`、尺寸严格等于 `geometry.logical_bytes`；设备读写、原始 `Inspect`、备份/恢复、事务边界仅消费此类型。
2. `ProtocolSector512`：`[u8;512]`，由 `NativeSector.bytes[..512]` 经长度检查提取，用于现有 DRKB/PDKB、A6B0、LBA4/7/8/12 解析和金标兼容；不得反过来冒充可直接写入的原生扇区。
3. `OpaqueTail`：`NativeSector.bytes[512..]`，默认隐藏内容，仅可展示偏移/长度/校验摘要；旧盘原样保留，新建时明确写零；官方历史尾部异常不可作为默认协议字段解释。

I/O 层提供类似：

```rust
trait NativeBlockDevice {
    fn geometry(&self) -> &DeviceGeometry;
    fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>>;
    fn read_blocks(&mut self, start: u64, count: u64) -> io::Result<Vec<u8>>;
    fn write_block(&mut self, lba: u64, full_block: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}
```

`read_blocks` 须限制单次 I/O 内存、校验 `offset=checked_mul(lba,logical_bytes)` 与 `length=count*logical_bytes`，不能越界；真实 USB 默认保持保守单块写入，允许文件/内存虚拟设备批量 I/O。对原始设备严禁通过 `read 4096 -> write 512` 来伪造 RMW；必须明确读出/原样合并/一次完整块写回。

保留旧的 `SectorDev` 作为**仅 512B 兼容适配器**，逐步迁移调用方后删除其运行时歧义，禁止 4Kn 进入旧 512B I/O 写路径。完整处理 macOS (`DKIOCGETBLOCKSIZE`/`DKIOCGETBLOCKCOUNT` / IOKit)、Linux (`BLKSSZGET`/`BLKGETSIZE64`、sysfs fallback)、Windows (`IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`/alignment descriptor/设备长度)；重新打开前后复核容量、扇区几何、USB/onlyid/device_id 及句柄身份，不依赖盘符或路径推断。

## 3. 协议、分区和 LCE

- 以原生 LBA 编号读取 `LBA0..12`。解码仍用前512B 已知字段，`ProtocolImage512(6656B)` 作为解析器/金标兼容表示，**不等于完整原盘备份**；同时持有 `NativeProtocolImage(13*logical_bytes)` 用于 Raw/备份/事务/尾部保护。禁止错误复用 6656B 作为 4Kn 恢复有效数据。
- `LBA7/LBA12` 各条 `sector_size` 必须与设备几何一致（512/4096），分区起止/大小单位及其整除要交叉验证；分区型别、数量、密码/加密模式不变。`LBA11` KDF 容量输入为 `capacity_bytes` 而非 `total_lbas*512`；历史 CHS 修复配置类型独立探测，不能因 4Kn 特判。验证设备 ID、VID/PID、LBA7、LBA11、LBA12 的一致性。
- MBR 在原生 LBA0 前512B（签名仍 `0x1FE`）；分区项 `start_lba`/`sector_count` 均以原生逻辑块计。GPT Primary Header 位于原生 LBA1，分区项占用原生块数要动态计算，并验证 Header CRC、Partition Array CRC、Backup GPT 与盘尾边界。MBR/GPT 所有 u32/u64 转换必须 checked。
- 兼容区/LCE（含历史 6 扇区、布局定位、CHS 回退、LBA7 compat、尾部镜像、引导区间）逐项辨析哪些量是**扇区数量**、哪些是**固定字节长度**、哪些来自官方**文件系统行为**。`512KiB` 之类字节偏移不可再直接写成 `1024 native sectors`。没有 4Kn 官方证据的 LCE 位置/加密拓展，不得擅自按乘8缩8推断后写盘。
- 解析差异必须分为：`unsupported sector geometry`、`corrupt protocol`、`identity mismatch`、`not enough evidence`，不得把 4Kn 盘错误报为 Plain 或损坏盘。

## 4. 文件系统、制盘和分区布局

- 统一 `FilesystemGeometry.sector_size`，`FilesystemReader` 和 格式化写入器 由 `[u8;512]` 升级成 `NativeSector`/动态块；一切 FAT12/16/32/exFAT BPB 都根据原生 `BytesPerSector` 与 `SectorsPerCluster` 计算 FAT 表、簇数、根目录和数据区；exFAT 的 `BytesPerSectorShift`、`SectorsPerClusterShift`、boot regions、校验、bitmap 等分别校验。FAT/FS 验证/扫描与生成使用同一几何真相源。
- 现有 FAT12 只读检查和历史 FAT12 盘（如 U391 启动区）不得因格式化不支持而被误报；若 FAT12 生成尚未验证，标为 `read-only/unsupported-format`，仅允许无损保留和元数据备份/恢复。FAT16/FAT32/exFAT 4Kn 写入通过各自虚拟盘金标后才能解禁。NTFS 不增新能力。
- `Mode0` 及 Mode1–4、Plain 均以**字节容量为核心**生成默认值，转换成目标原生 LBA 时指定 `floor/ceil/alignment` 规则且校验无重叠；保留 512B 现行默认分区结果的逐字节回归，**不能把原本 20417 个 512B 启动扇区直接解释为 20417 个 4096B 扇区**。真实 U391 的启动区示例为 `start=63,count=2497,sector_size=4096`，仅作 4Kn 正向兼容证据，不能硬编码成为全部 4Kn 默认。
- UI 的 `sector` 一律说明是“当前介质原生逻辑扇区”，并在旁显示 `512B/4096B`；MiB/GiB 为字节绝对容量。表单输入、容量地图箭头、计划确认、预估变化/需重建/可透传判断、结果页和 format-per-partition 不得各自重复实现几何换算。跨几何制盘默认强制重新规划，不能默默复用旧 LBA。
- `Plain` 的 MBR/GPT 格式化、分区起点、尾部 GPT/LBA 元数据规则也必须通过 4Kn 模拟，不得仅处理 EDP 注册盘。

## 5. 备份、恢复、事务与安全

- `.edpb` 现有清单已含 `logical_sector_size`、extents 和原生长度计算概念；保留现有 v3 512B 读取器/写入器兼容（旧归档原始哈希和字节不得变），评估通过向后兼容 minor 版本还是新 major 引入 4Kn。决定依据是**旧读取器是否拒绝/误解新格式**，不得只改版本号。4Kn 原始协议制品 长度为 `13*4096=53248B`；必须保存包括 LBA11 尾部的全部字节，并保留独立 512B 解析投影。
- 仅元数据备份 **不是整盘备份**。采集所有当前契约要求的分区表、文件系统关键扇区、协议镜像、尾部镜像和 LCE；任何目标几何必须在 清单/区段/来源哈希 内表述清楚。处理现有 `.as_chunks::<512>()`、`backup_metadata.rs`、tail 512KiB 镜像及分区相对偏移。元数据和未被采集的用户数据不可混淆；UI 明确恢复后是否需格式化/重扫。
- 同几何恢复：容量、逻辑块大小、媒体身份、分区表、相应完整 原生扇区区段 必须匹配并回读复核。**512↔4096 跨逻辑扇区恢复默认拒绝**；这不是原样恢复，而是新制盘/显式转换，需单独设计且绝不隐式重解释 LBA 数值。不同实际容量亦需维持现有明确匹配/安全约束。
- 事务计划 `WriteTransactionPlan` 以原生完整块为唯一写原子；合并同一块上对不同协议/文件系统字节域的所有变更，保留未拥有的字节。写前按 LBA 采集真实完整块+SHA-256，拒绝重复/相互覆盖及跨分区写入；元数据 LBA0 最后提交，GPT 双副本/FS 元数据的写序有独立一致性约束；写后 Flush+逐块对比，失败按**同一完整块集合**回滚并二次验证。大容量事务限制镜像缓存内存并妥善处理恢复失败的 `intermediate` 状态。
- 官方 4Kn LBA11 遗留尾部属于未识别 `OpaqueTail`：对已有盘的非破坏性更新必须保留；新建盘完整 4096B 初始化为零后构造前512B，杜绝内存泄漏；用户若要清理历史尾部只能通过单独明确操作及额外验证，而非格式化/改密时静默清理。原始尾部默认不上传日志或错误报告。
- 进度单位来源统一为 `bytes`、`native_blocks`、`operation_steps` 三类，不同子进度不得混用 512B 等价扇区；日志和用户提示输出两种几何事实与所选能力级别。

## 6. 阶段划分与每一阶段验收门禁

| 阶段 | 工作内容 | 完成定义 |
|---|---|---|
| P0 基线 | 冻结 512B 金标与现行 `.edpb` 兼容基线；录制 U391 只读取证的哈希/尺寸与预期解密结果，排除隐私字节入库。 | 512B 现有 fast/full 门禁、金标与 3 平台构建无回退。 |
| P1 类型和几何 | 定义 `DeviceGeometry`、原生 LBA/字节单位、I/O 容量检查与写入许可状态。 | 512/4096、未知/错误尺寸、容量溢出、跨平台几何组合单测全绿。 |
| P2 原生 I/O | 重构 `FileDev`/SectorDev 与 fake devices，实现 4Kn 块读、批量、buffer 严格校验、重开复核；首阶段只读。 | 在真实 U391 上 `EINVAL` 消失；LBA0..12 各返回4096B；禁止任何写入。 |
| P3 协议与识别 | 512B 投影 + 4Kn 原始扇区双轨；LBA7/11/12、MBR/GPT、语义状态/分类、历史 DRKB/PDKB 兼容。 | U391 与 512B 正例身份/盘型正确，LBA11 PDKB UID 一致；不再误判坏盘/Plain。 |
| P4 `Inspect`/TUI 只读 | Raw 全原生块、Decode/Mixed 已知字段、OpaqueTail 及正确偏移；容量地图/设备列表、LBA Jump/搜索/复制/导出。 | 全局 4K/512 可视化无错位、超界、滚动回归；不暴露不明敏感尾部。 |
| P5 备份与只读审计 | `.edpb` 动态块长度与无损 extents、元数据覆盖，支持 4Kn 备份创建/校验/预览、文件系统只读检查。 | U391 备份 13*4096 协议镜像及所有契约元数据，SHA 全部通过；旧 512B 备份可验证。 |
| P6 分区/FS 纯规划 | Mode0-4/Plain 容量模型、FAT16/FAT32/exFAT 4Kn 生成与读取、MBR/GPT、LCE 证据收敛。 | 纯内存和临时文件上完整创建-解析-回放，512B 金标无改变，4Kn 模拟金标通过。 |
| P7 虚拟写入事务 | 完整原生块快照、写入/回读/失败回滚、短写/掉电模拟、LBA0 提交顺序、FS/GPT 一致性。 | 故障注入 全覆盖；同扇区多字段合并；尾部无越界读取。 |
| P8 真实盘写许可 | 先 512B 实盘回归，再经明确用户批准选择**可擦写测试 4Kn 设备**实施创建、格式化、恢复及失败回滚 HIL。 | USB 断开、拔插、重开身份、写盘回读、挂载验证、故障恢复全部通过后才能为对应尺寸解除写门禁。 |
| P9 全部工作流 | 打通 CLI/TUI、备份恢复、改密、复制、Plain 转 EDP、恢复结果、操作进度与 CI。 | 512B/4096B 全部适用功能均被 E2E 用例覆盖；三平台 CI 和文档一致。 |
| P10 收口 | 统一模块消重、文档/使用说明与持续防回归规则，删除无意义固定 512B I/O 假设。 | fast/full、fmt、clippy、redundancy audit、HIL 均绿，保留读写兼容及安全证据。 |

执行原则：先建立 P0 门禁再小步提交，每阶段独立 PR/变更集与回归；更新 `docs/EDP_PROTOCOL_LIVE_STATUS.md`，不要使用 `git reset`/`git clean`。**只读认证 ≠ 实盘写许可**，绝不跳过 P6/P7 直接开启 4Kn 写入。

## 7. 关键回归矩阵（必须自动化）

- 几何：512n、512e（512 逻辑/4096 物理）、4096n、未知/0/非2幂、1K/2K/8K 模拟，以及临界容量/越界/`u32` MBR 与 `u64` LBA边界；Linux 512-byte-unit 到字节换算；USB 重插设备路径复用。
- 协议：完整 `13*512` 与 `13*4096` 读取，LBA11 尾部零/非零、原样保留、解码 UID、变更前后 raw hash；LBA7/12 sector_size 正确/错配、VID/PID 变化、CHS/精确容量两配置类型、LCE/tail 证据边界。
- 分区表：MBR type1/2/4、Plain、GPT 主/备头、数组跨块、非对齐分区、MBR `0x55AA` 与 4K 偏移不变、>2TiB 分区字段上限与拒绝逻辑。
- 文件系统：FAT12 4Kn 只读；FAT16/FAT32/exFAT 512/4096 创建、验证、簇大小/表大小边界、坏 BPB、FSInfo/备份 boot、exFAT boot checksum；加密/非加密、格式化前后重新扫描；不支持的组合报明确 `unsupported-format`。
- 备份恢复：新旧 512B EDPB、4Kn EDPB、缺块/哈希失配/部分损坏、跨几何拒绝、同几何可恢复、备份目录显示容量/型号/几何；用户数据不在 仅元数据备份 恢复范围内。
- 写盘故障：断电/拔插/半扇模拟短写/读回不一致/身份变化、阶段故障覆盖、完整块 回滚与校验；虚拟设备能检测任何非完整 4096B 写入或对未拥有字节的篡改。
- TUI：Sector 单位提示、Space 单位切换、分区布局与容量地图联动、计划确认、`Inspect` 搜索/跳转、Raw/Decode/Mixed、结果和恢复页窗口、进度条；所有已有键位/表格滚动门禁必须复用。

## 8. 写盘开关（不可跳过）

建议把能力显式分为 `Discover / Read / Backup / Plan / VirtualWrite / PhysicalWrite`。只要任一模块仍需要 `512B` 固定设备块假设，4Kn 的 `PhysicalWrite` 不可授权。授予写许可的至少条件：稳定且已复核的几何、完整协议/来源盘型身份、完整分区/LCE/FS 能力、可靠事务回滚、同几何备份恢复、安全确认、对应平台实盘 HIL 证据。若用户要求未认证 FS/模式，则只阻止对应写入动作，不影响其他可用只读功能。

## 9. 首个实施任务与交接要求

推荐从 **P0-P3 一次只读闭环**开始：让 `sudo edpcli list/info/inspect` 在当前 U391 能识别为有效的 4Kn EDP 三分区盘，并展示与既有手工取证相同的 device_id，且 Raw LBA11 返回完整 4096B；不触及任何实体写盘权限。随后 P4/P5 处理所有用户可见的只读和无损备份能力。对 512B 盘必须保持像素/字节级既有行为不回退。

审核重点：是否认可首批认证仅 512/4096、跨几何恢复默认禁止、LBA11 OpaqueTail 原样保留、仅元数据备份 语义不扩张，以及分阶段才解禁 4Kn 实盘写入。


## 10. 官方一方实现：强制参考、对照和证据矩阵（新增最高优先级）

### 10.1 参考源与证据优先级

**研究顺序必须是：官方写入端 → 官方读取/消费端 → 官方生成的原生盘面 → edpcli 适配实现 → 独立回放/虚拟 HIL。** 不允许直接“把 512 换成 4096”，也不允许根据单份反编译推断官方所有版本的行为。

| 官方参考物 | 本机证据位置/入口 | 作用 | 使用限制 |
|---|---|---|---|
| Windows 当前 CEMS 注册制盘 | `/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll(.m)`；`CUsbRegsiter::RegsiterUsb @ 0x1003B560`，`BuildSector11 @ 0x10014720` | 追踪注册前读取 13 个原生扇区、指定 LBA 字段覆盖与整体写回；确定旧尾部保留语义 | 它不同于 `EdpEDiskCtrl.dll::WriteToUsb11`；需要确认调用路径，不能混同两个组件 |
| Windows 当前磁盘操作/初始化 | `.../VRV/cems/ydcc/edpediskctrl.dll(.m)`；`ReadDisk @ 0x10032310`，`WriteToUsb11 @ 0x100215E0`，`WriteNtDisk @ 0x10021510` | `IOCTL_DISK_GET_DRIVE_GEOMETRY` 和 `BytesPerSector` 传播；按原生块读写及已确认的 512B 写缓冲区缺陷 | 对漏洞做**安全性差异**，必须明确标记为不复刻的官方缺陷；不能拿缺陷当协议规定 |
| Windows 历史构建 | `/Users/zhangyuxi/Desktop/u_disk/VRV/edp/EdpEDiskCtrl.dll(.m)`；`WriteToUsb11 @ 0x10018180`、`WriteNtDisk @ 0x100180D0` | 检查跨代几何行为、历史 LBA11 尾部与写入回归 | 不将旧版行为不加区分合并为当前唯一规则 |
| Linux 官方带 DWARF 的实现 | `/Users/zhangyuxi/Desktop/u_disk/cn.com.vrv.cems.ydcc/files/cems/ydcc/libcemsfilesyscheck.so`；`diskfile.cpp` 对应 `BuildSector11/ReadSector11`、`BuildSector7/12`，`datasecrity.cpp` | 恢复原始 ABI、固定 512B 协议负载、加密与几何语义；与 Windows 独立交叉验证 | Linux 校验器并不证明 Windows 格式化和写盘功能的实际支持程度 |
| 历史物理金标和现行真实设备 | `audit/protocol/gold/`、`audit/protocol/evidence_manifest.tsv`、`gold_samples.tsv`；当前 aigo U391 的**只读**验证 | 现有 512B 严格样本和 4Kn 原生磁盘真实行为；记录完整 SHA-256、几何和可重放的无敏感摘要 | 历史 6656B 不能被误称为 4Kn 的 `13*4096B` 完整备份；敏感尾部不得提交 |
| 标准协议/可复现审计 | `docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md` 第 2/5 章、`audit/protocol/byte_ledger.tsv`、`field_catalog.tsv`、`tests/protocol_behavior/lba11.rs` | 明确协议所有者、证据级别、字节长度、容量配置类型和完整测试 | 目前完整闭环是**每个协议扇区前512B**，不是 4Kn 原生尾部 |

### 10.2 每个模块必须建立证据映射（实施时维护）

必须在对应 PR/阶段的证据报告里填写下列字段：`功能/字段/设备单位`、`官方生产者（二进制+函数+地址/源码位置）`、`官方消费者`、`官方盘面样本及 SHA256`、`适配函数/模块`、`512B 回归用例`、`4096B 正向用例`、`负向/故障用例`、`验证状态`。未验证项标 **BLOCKED**，不得靠猜测打勾。

| 业务面 | 官方优先核查问题 | 最低准入证据 |
|---|---|---|
| 磁盘几何 | 官方在哪个入口得到 BytesPerSector / 盘容量 / CHS；LBA 单位有无分叉？ | `DeviceIoControl` 调用参数+使用链+512/4096 实盘对照 |
| LBA0–12 协议 | 各构造器覆盖多少字节？`13*BytesPerSector` 读取/写回是否读改写？ | 官方生成与消费路径各 1 条；完整原生块前后哈希/所有者表 |
| LBA11 身份 | 固定 512B DRKB/PDKB、DiskSize/RepairChs 两条 KDF、4Kn 尾部是否被保留？ | 官方两个写入端区别 + U391 PDKB/UID 成功重放 + 尾部保留测试 |
| LBA7/LBA12、分区模式 | 官方 4Kn `sector_size`、`partition_size`、StartSector 的原生 LBA 单位；Mode0–4 类型组合 | 官方读写字段追踪 + 原盘一致性；没有某模式官方 4Kn 证据就暂不解锁该模式写入 |
| MBR/GPT | 官方写 MBR/GPT 时扇区编号、入口偏移、备份头和阵列占块量怎么算？ | 官方写入函数定位 + 4Kn 虚拟盘标准解析器交叉核验 |
| FAT12/16/32/exFAT | BytesPerSector、BPB、FAT 表长、FSInfo、簇、校验及格式化路径是否随 4Kn 改变？ | 官方格式化行为/产物字节 + 独立文件系统解析与挂载验证；静态反编译不足以解锁实盘写入 |
| LCE、尾部镜像和保留区 | “sector” 是原生扇区数，还是固定字节偏移？不同官方代际是否不同？ | 官方生产/消费双方+4Kn 虚拟或真实正例，不能盲目缩放 |
| 元数据备份恢复 | 官方备份是否完整保留非协议原生尾部、何时全块写回？ | 本项目 EDPB 新旧互操作和原生块 roundtrip；明确这是自有功能契约，不能以官方行为替代测试 |

**明确划分三类结论：** `官方实证/可复现`、`独立解析佐证`、`仅假设`。只有前两者构成实现依据，纯假设仅允许用于测试设计、不能用于有破坏性的逻辑。每个偏离官方的决策（特别是安全修复和更严格的回滚）另列 `安全差异清单`，说明为何不复刻及如何验证与官方盘面兼容。

### 10.3 官方 4Kn 对照验收（不接触真实目标盘写入）

1. 在隔离的**4096B 原生虚拟块设备**中，只调用官方可控的制盘/注册写入链（需要隔离运行环境、记录传给 `WriteFile` 的 `(LBA, bytes, len)`）；实际器件绝不参与写盘实验。
2. 采集官方产物的完整原生块快照，与 edpcli 的**同几何/同模式/同密码/同 UID/同熵输入**模拟输出做逐字段比对；对随机字节、时间种子、填充、格式化随机值，先固定或分离再比较，不能把合理随机差异当成错误。
3. 对非确定性及官方有害泄漏尾部，比较的是**协议兼容性**而不是仿制泄漏字节：edpcli 新盘全原生块初始化，只有前512B 做标准编码，尾部不带任何栈内存；旧盘无损更新保留原尾部。
4. 两边用独立读取端验证设备身份、分区表、挂载与元数据；若没有可控官方 4Kn 写入运行时样本，写入能力记录为 `官方产物证据不足`，不能据此宣称与官方 4Kn 完全对等。
5. **每种模式、每种文件系统和每种操作分别认证**；不能凭 Mode0 成功推定 Mode1–4/Plain、FAT16 成功推定 FAT32/exFAT、只读成功推定写入成功。

## 11. 现有功能零退化：必须执行的回归契约（新增最高优先级）

### 11.1 兼容性定义

对所有**既有已支持**的 512B 操作，输入相同且随机源可控时，预期输出必须保持原始字节一致；任何有意的可见 UI/错误文案变化必须经过审核、形成快照回归，不能静默带入。对于无法稳定字节比对的流程，比较结构、加密结果、行为、性能门槛和设备状态转换。不能声称逻辑上保证“永不出现缺陷”；必须依靠可复现的基线、自动化覆盖、双路径隔离和分阶段回滚保证可验证的“不退化”。

### 11.2 合入前需要冻结的旧功能证据

| 现有功能面 | 必须冻结/回归的 512B 行为 | 现有/新增测试锚点 |
|---|---|---|
| USB 身份与设备识别 | Vendor 可为空的设备、VID/PID、onlyid、CRC、Mode0–4/Plain、未知/损坏盘分层错误 | `tests/identify_list.rs`, `tests/media_identity.rs`, `tests/platform_suite.rs`；保留 HIKSEMI 空 Vendor 正例 |
| 6656B 协议与加密 | 每个 LBA 的原始金标、前512B 解码/重编码、LBA7/12 交叉验证、LBA11 DiskSize/CHS、LCE/IIR、密码策略/多代字段 | `tests/protocol_byte_ledger.rs`, `tests/protocol_gold_crosscheck.rs`, `tests/protocol_behavior/lba11.rs`, `tests/provision_lce.rs`, `tests/provision_protocol_audit.rs` |
| 制盘/重制/Plain | 每种模式默认布局、MBR/GPT、启动/交换/保密区、容量/标签、保留区 LBA12–62、透传/需重建判定、分区密钥及格式化意图 | `tests/provision_suite.rs`, `tests/provision_generate.rs`, `tests/provision_reprovision.rs`, `tests/plain_provision.rs`, `tests/provision_validate.rs` |
| 文件系统 | 当前已验证 FAT12 读取与 FAT16/32/exFAT 检测、生成、读回、边界簇、盘型差异和已验证格式化行为 | `tests/filesystem_contract.rs`, `tests/filesystem_domain.rs`, `tests/provision_filesystem.rs`, `tests/provision_fat16.rs` |
| EDPB 备份/恢复 | 旧 `.edpb` 原样验证、查看、列出、选择、恢复；同盘/跨盘身份与 SHA 门禁、写后校验、不同几何拒绝；`仅元数据备份` 语义不变 | `tests/edpb.rs`, `tests/backup_suite.rs`, `tests/backup_metadata.rs`, `tests/atomic_write.rs`, `tests/cli_write_safety.rs` |
| `Inspect`、表格与 TUI | Raw/Decode/Mixed、搜索/`J` 跳转、进入/ESC 返回、设备/备份表格、`jk/HL/0/$`、窗口滚动和滚动条、Modal、制盘确认、进度及结果页 | `tests/inspect_suite.rs`, `tests/inspect_full_disk_acceptance.rs`, `tests/tui_suite.rs`, `tests/tui_table_layout.rs`, `tests/tui_keymap_contract.rs`, `tests/tui_operation_progress.rs` |
| 跨平台、稳定性、性能 | 512n/512e、Linux/macOS/Windows 构建、非 USB 拦截、同步/回滚、读写次数、任务阻塞、实际 CLI 行为 | `tests/platform_cli_matrix.rs`, `tests/virtual_disk_hil.rs`, `tests/plain_macos_virtual_hil.rs`, `scripts/test-benchmark.py`, `.github/workflows/{ci,virtual-disk-hil}.yml` |

**回归不得依赖手工“感觉没问题”。** 各项必须持有 `before (main@bb19ab4)` 和 `after (PR)` 的测试日志、字节差异或稳定快照；测试必须覆盖旧模式完整成功/失败路径，而非只测新增 4Kn 成功路径。

### 11.3 稳定旧路径与双轨迁移策略

1. **P0 先做基线快照、再编码。** 复制为只读基线的必须是有来源信息的测试夹具/哈希，不复制隐私 USB 全部内容入 Git。固定熵、密码和布局，冻结 512B 生成物 LBA0..12、FAT 关键数据、写入计划和旧备份验证结果。
2. **保留且保护原 512B 路径。** 第一阶段新增 `NativeBlockDevice` + `ProtocolSector512` 适配，不立即整体替换 `SectorDev` 或重写成熟 512B 的事务/FS 逻辑；仅在观察几何为 4096B 时路由到严格只读新路径，旧业务逻辑保持原处理。待跨平台/业务逐项验收后再分模块迁移，避免一次大范围侵入式重构。
3. **每阶段双跑对比。** 对 512B 夹具调用原/新 parser 或 planner，比较 `device_id/onlyid/模式/分区/LBA/容量/字段/错误码/写入计划`，对可确定输出做 byte-for-byte 比对；差异即阻断，不允许只改预期测试让失败消失。
4. **能力按几何和功能隔离。** `512B` 继续走稳定写入门禁；`4096B` 的 `Read`、`Inspect`、`Backup`、`Plan`、`VirtualWrite`、`PhysicalWrite` 分别开放。功能开关默认不解除 4Kn 实盘写禁；即使新增实现抛异常也必须安全降级成明确 `unsupported`，不得回退到 512B 错位读取。
5. **备份兼容先行。** 旧 v3 `.edpb` 的原始校验、选择、恢复规则必须维持；新 4Kn 格式先读/校验/备份，再考虑恢复写入。新旧归档 schema/版本变更必须证明旧程序不会静默误读，并通过完整只读校验和模拟恢复测试。
6. **输出/交互契约保护。** 原 TUI 键位、表格组件、排序、滚动条、Modal、进度层级和已有 API 返回值原则上不变；新增加“逻辑扇区大小”只作为同一展示模型的额外字段，不能复制出 4Kn 专属 TUI 组件。
7. **性能不退化。** 记录基线读块数、扫描耗时、备份吞吐、TUI 操作延时；禁止在旧 512B 盘因适配导致无界内存、反复探测或显著性能下降，超出基线预算必须审查并提供证据。
8. **不改变稳定安装。** `~/.local/bin/edpcli` 的稳定发行版本不在 P0–P7 自动替换；使用独立 `target/` 开发构建和虚拟 HIL。完整旧功能门禁及新功能验收通过后才允许依当前安装规范更新。

### 11.4 每次合入的硬门禁（缺一项即拒绝）

- `cargo fmt --all -- --check`、`git diff --check`、`scripts/test-fast.sh`，以及受影响域的所有定向测试与**完整**回归 `uv run --locked python scripts/test-full.py --profile full`，全特性 `cargo clippy --locked --all-targets --all-features -- -D warnings`。
- `uv run --locked python scripts/audit-redundancy.py --check`，以及对应协议字节账本、真实盘金标、备份格式/完整性、事务/故障注入、TUI 交互门禁。
- Linux、macOS、Windows CI；相关镜像/虚拟 HIL 必须跑完，不能因为 fast/full 通过就当成写盘验证。CI 明确运行新加入的各扇区大小配置，防止仅本机 macOS 通过。
- **官方对照矩阵**在该阶段涉及功能上达到已证实（或明确标记不支持/保持禁写）；对应 `before/after` 零退化矩阵全绿。
- 真实 USB HIL 仅在用户允许具体可擦写测试盘后执行，绝不使用当前尚需保留数据的 aigo U391；失败即锁死对应功能的实体写许可。
- 提供代码 review 的四个结果：**证据差异清单、回归测试记录、安全门禁记录、变更回退方案**，并通过一次干净克隆的构建验证。禁止绕过错误码、身份复核、金标或测试来实现表面绿色。

### 11.5 发现退化时的处置与可回退性

- **PR 失败：** 停在当前阶段，修复并重跑，不合并。
- **发现旧 512B 行为变化：** 先冻结 4Kn 新路径，恢复旧代码路径默认有效；对差异定位归因，补入永久回归。
- **4Kn 只读不支持某字段：** 精确提示 `unsupported_geometry/unsupported_protocol_variant`，保持识别/设备信息可用；不能伪报坏盘、不能开启写入。
- **写入回滚失败：** 维持 `EXIT_INTERMEDIATE`、锁住后续写入并要求离线恢复流程；不能静默继续制盘。
- **发布后回退：** 保留上一稳定版可安装产物与 SHA256，对项目代码小步可逆提交并保留归档兼容；已生成的 4Kn 备份仍应可校验/导出（不等于旧版本能恢复），不得要求用户删除历史备份。

## 12. P0–P10 计划附加停机条件与交付模板

在原 P0–P10 每一阶段新增交付文档（同 PR 或同模块目录）：

```text
阶段 / 对应提交 SHA：
涉及 512B 旧功能、before 证据与 after 结果：
引用的官方二进制 SHA256 + 函数/偏移/原源码锚点：
512B/4096B/异常几何用例：
字节级差异（区分受控随机与真实不一致）：
写盘能力状态（Discover/Read/Backup/Plan/VirtualWrite/PhysicalWrite）：
独立 Linux/macOS/Windows 验证：
虚拟 HIL/故障注入/恢复证明：
未解决的官方兼容疑问 / 保持禁写的理由：
回退方式及安全不变量：
审核结论 PASS / BLOCKED：
```

- P0 若未冻结 512B 旧行为基线，**不能开始 P1**；P1/P2 不得让 512B 旧设备自动进入未经验证的新写路径。
- P3/P4 在官方身份/几何证据不完整时，仍可交付只读的保守可用子集，但不能标记为全部兼容。
- P5 旧 `.edpb` 没有完整 roundtrip 和负向样本，**不能开始任何新恢复写入路径**。
- P6 未覆盖模式及 FAT16/32/exFAT 的官方产物差异，不得将该组合的写入能力设为 `PhysicalWrite`。
- P7 无全块事务/故障注入证明，**禁止进入 P8**。
- P8 必须由用户针对明确可擦写测试盘批准；当前 U391 仅允许只读诊断与备份。
- P9/P10 即使全功能完成，旧 512B 门禁或三平台 CI 失败时也不能发布。

**最终发布验收定义：** 对原来已支持的所有 512B 功能，新版本继续成功，字节/语义/错误码/交互均没有未审批的差异；对逐项认证的 4096B 功能，能在官方证据、虚拟运行时、独立读回与受控实体 HIL 之间形成闭环；任何无法证明的配置类型按安全规则明确拒绝，而不是声称已完整支持。
