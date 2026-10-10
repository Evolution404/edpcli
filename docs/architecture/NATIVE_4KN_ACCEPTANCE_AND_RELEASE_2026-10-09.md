# 原生4Kn制盘验收与发布门禁

日期：2026-10-09。此文档用于分清 GitHub 离线验收与之后的实体测试，不授权对 U391 写入。

## P0：只读证据与来源一致性

- 输入：可信来源标识、原生扇区宽度、总扇区数、原生LBA0–12协议快照、LBA7定位的LCE完整原生块。
- 路由：`EvidenceSource` 与统一只读 `SectorReader`；身份和协议快照由来源对象提供，不接受用户单独拼接不相关的密钥记录或LCE缓冲区。
- 校验：总容量和扇区宽度一致；13块快照读取全字节相等；LBA7/LBA12成对解析、所有分区几何、MBR签名与首分区、LCE指针全部匹配；读LCE失败即拒绝。
- 证据：合成读取器覆盖512B/4096B，正确来源、截断、快照漂移、缺失LCE、错误身份/总容量与错误MBR。
- 限制：可信身份取决于上游采集者及硬件观测；同源读取不能证明对抗性伪造者、真实设备固件状态或LCE密文真实性。

## P1：厂家原生4Kn生产者独立金标

| 待证明对象 | 通过门槛 | 当前处理 |
|---|---|---|
| 原生LBA0–12 | 厂家写入程序或真实厂家盘的完整4096B原始字节及版本定位 | 保留未知3584B，不能伪造生成 |
| LBA7/LCE | **采用edpcli自定义规范**：前3072B兼容镜像 + 后1024B零明文，4096B整体zero8+A7F0加密；原厂同字节一致性不作为门禁 | 验证指针、4096B生成/解密/完整回读和实际目标端可用性 |
| Mode2非整块区域 | 说明历史`0x7E00`长度在4Kn下的真正表达方式 | 仅在离线4096B新盘候选上舍入到`0x8000`，不声明原厂认证 |
| 身份/密码域 | 真实生成者记录、真实消费者接受、合法原始密钥校验闭环 | 合成材料可测试，不能称原生官方金标 |
| Windows/Linux消费者 | 对应驱动/用户态版本与4096B真实读取证明 | 不由离线金标推断跨平台挂载 |

P1无法靠离线自行猜测补齐：必须取得独立生产者或来源证据，证据不足就维持只读状态。

## P2：GitHub可执行的整链隔离验收

- 验证矩阵：四种官方布局 × 512/4096B扇区 × 三种明确已证明的数据加密方式。每条记录必须同时区分 `NeedEncrypt`（协议）和 `physically_encrypted`（实际物理变换）。
- 读取闭环：预备完整原生来源，按安全范围形成离线块计划，验证MBR最后提交；独立原生字节读取器回读13个协议块及LCE，逐字节比较未知尾部。
- 分区数据：在普通文件虚拟盘中只对**确实物理加密**的分区执行对应加密，再独立解密、检查FAT/exFAT元数据或测试文件内容；不能把没有原生新盘生产金标的盘面字节冒称官方出厂映像。
- 负向门禁：错误身份/算法、未知扇区宽度、几何越界/重叠、块不完整、CRC失败、协议快照漂移、LCE缺失、MBR过早有效或写后回读字节变化必须拒绝。
- CI门槛：`cargo fmt --all -- --check`、非实体完整测试、`cargo clippy --locked --all-targets --all-features -- -D warnings`、协议基线与冗余审计、独立跨平台虚拟磁盘工作流全部成功。不得将取消、跳过的必要步骤计为通过。

## P3：以后实体介质验收（本轮不执行）

仅对**另外准备的可牺牲测试盘**开展。必须有显式设备身份、目标端授权、全部原始区段备份与离线校验、风险确认、可恢复介质；不允许仅以`disk4`等可变系统编号判断目标。

1. 首先只读取证：精确容量、原生扇区大小、供应商/设备身份、版本、官方目标系统与数据路径（软件或安全模块）。
2. 在可信来源和已认证金标均齐全时，另经授权打开单一测试盘写入门禁；先使用最小范围可回滚试验，再扩大到全模式验收。
3. 按阶段注入模拟失败、拔插、进程终止与电源中断；每次核对原始备份、事务日志、真实重插识别与可恢复性。普通文件虚拟回滚测试不代替此阶段。
4. 官方目标端独立验收每种模式和算法：分区识别、密码域、文件系统挂载、文件读写、重新插入后完整性检查。
5. 证据包括测试盘身份、协议原始块哈希、版本、失败/恢复过程、读后哈希及独立目标端结果。缺失任一项都不解除其他设备的保护。

**U391本轮仅执行只读取证。用户已明确允许丢弃原有数据，但尚未通过4Kn实体写入、设备锁定与故障恢复门禁，不得仅因已授权而绕过安全机制。**

## P4：仓库收口和发布条件

- 原生只读功能、生成器实验和真实写入权限分别测试、分别标记，不能将一个`feature`开关同时授予全部权限。
- 先合并 GitHub CI 全通过的离线PR到4Kn开发分支，再审计`main`差异。当前正式发布基线为2.6.0，开发版本为2.7.0-alpha.0。
- 版本命名和安装验证另遵循仓库现有规范；Mac离线时**不宣称已本地安装**。仅靠GitHub CI成功不意味着可以开放原生4Kn实体制盘。
- 实体能力的release判定必须满足P1厂家原生协议金标、P2数据整链、P3独立测试盘认证及目标系统可用性。未满足的能力保持硬性拒绝。

## 2026-10-10 实施状态：持久化镜像复读验收，不解除实体门禁

- P2进一步完成：`native_image::verify_native_virtual_image` 从**独立重开的普通文件句柄**核验完整文件容量及每个已写入512B/4096B原生块（包含LBA0、全部协议块、LCE完整密文尾部及每个已格式化元数据块），拒绝设备路径/符号链接、错误块几何、重复LBA、丢失或变更字节及截断文件。仍不验证无所属的稀疏自由空间，也不证明电源中断后真实USB持久状态。
- `export_native_plain_image`（Plain、全新512B/4Kn EDP及来源重放共用）现要求写后sync，再新开文件核验；Unix上以文件dev/ino确认被校验的是`create_new`创建的同一个文件，失败只删除身份仍相同的本次新建文件，避免误删并发替换文件。
- 历史512B `export_native_virtual_image` 原子发布路径同样先验证私有候选文件的完整写集，再执行原子重命名；损坏镜像不得抢先替换旧的正式镜像。
- 512B实体格式化又新增`verify_authored_metadata_readback`：原格式化事务第一次同步/读回后，再独立`sync`和逐扇区读取全部物理密文/明文格式化元数据；只有所有实际块严格等于经FileKey验证的物理格式化计划才允许使用相应明文计划进行FAT/exFAT结构分析。负例证明普通区和加密区的**非引导元数据**在首次回读之后受损都会失败。稀疏空闲/未写入扇区仍不是验证范围，不能宣称全盘安全清零。
- 回归：512B、4Kn普通盘格式化文件的再次打开/完整读回、单一数据块损坏、错误MBR、恶意重复LBA、容量截断、符号链接拒绝；四模式4Kn LCE末尾4096B块的最后1字节改动检测；原512B路径不改变写入语义。
- 另行核验OEM证据分支`b378f09a`只认证历史虚拟卷挂载→条件IRP→驱动ZwWriteFile传输路径，**没有**给出新4Kn LCE生产者、1024B所有权或任何实物生产写入金标。Mode2兼容预留区在新4Kn虚拟候选中上舍入到0x8000B，但不可称为厂家认证格式。
- **用户2026-10-10确定的LCE规则优先**：前3072B历史FAT16兼容镜像，后1024B零明文，zero8+A7F0加密全部4096B，物理起点按原生LBA*4096计。不得再为LCE设置寻找原厂生产者、末尾归属、原厂字节金标的解禁前置条件；如果原厂消费者不兼容，应单独记录兼容性限制。其余协议/身份可信性及用户数据保全不受该决策改变。
- P1尚未具备证明：新4Kn镜像的其它原生协议尾部及原厂软件消费端真实挂载证据。P3仍阻断：独立**可牺牲**4Kn测试盘的备份、身份锁定、断电/拔插、各模式/算法文件读写与目标端验收。**不得仅凭P2成功取消`NativeEdpLayoutPlan::may_write=false`或旧`ObservedDeviceGeometry::writable_protocol_sectors`对4Kn的拦截。**
## 2026-10-10 当前介质的原生备份写前比对

- 修复备份列表及 TUI 备份工作区统一的容量解释：使用 `BackupMeta::capacity_bytes()`，优先可信 EDPB v4 的原生容量字节，缺失时按已知逻辑扇区大小计算，历史512B保留兼容兜底。U391 真实备份现在正确显示 **255.94GB** 而非31.99GB。
- 新增只读命令：`edpcli provision verify-source --disk N --backup /path/to/source.edpb`。验证EDPB v4、设备标识、容量、4096B逻辑扇区及全部保存的完整原生块，包括LBA0..12、LCE和分区首块。任何漂移、缺块、截断、设备不符都拒绝。
- 本地 U391 实测：备份EDPB v4 `62486528*4096=255944818688B`，**17个4096B完整块**逐字节读回一致，包括LCE LBA62476561。命令未卸载、未锁盘、未尝试写入。若将来进入实体事务，必须在独占锁盘后再次复核，不允许将此只读结果当成写入授权。
- 剩余必要工作：512B专用 `SectorDev` 的实体事务必须扩展为经过授权和真实测试的 `NativeBlockDevice` 原生4Kn物理端口，保留强制备份、交换盘防护、回读与可恢复故障测试；随后独立目标端验收，不能以镜像或当前来源一致性替代实际写盘成功。

## 2026-10-10 — Native recovery and Mode0→Mode1 offline stage

- `native_journal_recovery`: bounded strict WAL decoder (v1/SHA-256/device identity/native geometry/LBA uniqueness/state), fail-closed on corrupted, missing, committed or previously recovered journals. An interrupted WAL may only be restored through a previously authorised `NativeWriteLocked` session. Recovery and in-process rollback both restore LBA0 last; physical power-loss atomicity remains **unproven**.
- `native_transaction`: the journal's initial full-block snapshot is now the **single rollback baseline** for the write transaction, avoiding a second prewrite snapshot that could disagree with durable WAL evidence.
- `native_image::plan_native_4kn_mode0_to_mode1`: pure, regular-file-oriented Mode0→Mode1 plan covering **full combined plaintext exFAT metadata**, not merely protocol LBA0–12; original type4 extent/key material and all unowned 4Kn protocol tails remain unchanged. MBR first 446 bytes remain source-owned and LBA0 commits last.
- `plan_verified_native_4kn_mode0_to_mode1`: independently re-reads the 13 full source 4Kn protocol blocks plus LBA7-pointed LCE from one read-only `SectorReader`, rejects source drift and short blocks before planning.
- Virtual tests cover successful conversion, new exFAT metadata, preserved type4 records/unknown tails/LCE, broken EDPF, missing forced combined format, invalid source LCE, interrupted WAL, wrong hardware pin/geometry, and corrupted log digest.
- Scope limit: these pure builders **do not** activate the physical 4Kn CLI/TUI commit branch, provide a full device rescan after unplug/replug or authorise manual `dd`. Firmware interoperability and on-device HIL remain unverified. The existing 512B-only `prepare_provision_on_disk` gate is unchanged.
