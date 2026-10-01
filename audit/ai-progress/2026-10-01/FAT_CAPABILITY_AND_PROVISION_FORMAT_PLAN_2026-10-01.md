# FAT 能力补齐与制盘格式治理计划（2026-10-01）

## 1. 范围

本轮不新增任何 FilesystemKind。保持 FAT12 / FAT16 / FAT32 / exFAT / NTFS。
目标是优先补齐 FAT 系列能力，并把制盘可选格式正式收敛为 FAT16 / FAT32 / exFAT。NTFS 暂缓。

## 2. 当前与目标能力矩阵

| 文件系统 | 当前 Detect | Metadata | Format | Verify | Analyze | 本轮制盘可选 |
|---|---:|---:|---:|---:|---:|---:|
| FAT12 | ✓ | ✓ | × | × | ×→✓ | × |
| FAT16 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| FAT32 | ✓ | ✓ | ×→✓ | ×→✓ | ×→✓ | ✓ |
| exFAT | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| NTFS | ✓ | ✓ | × | × | × | × |

## 3. FAT16

保持完整 driver：detect、matches_geometry、read_metadata、validate_format_request、expected_format_metadata、build_format_plan、verify_format、analyze。

### 3.1 OEM Name 治理
当前 FAT16 formatter 会在 boot sector +0x03..+0x0A 写入 `EDPCLI  `。这是磁盘真实内容，不是 Inspect 显示 bug。

处理原则：
1. Inspect Raw/ASCII 不隐藏该内容。
2. 从 formatter 根源移除产品名痕迹。
3. 先读取真实官方 EDP FAT16 样本该字段。
4. 官方样本是什么优先复刻什么；跨版本有差异则记录兼容策略。
5. 加 fixture/regression，禁止 formatter 再写 EDPCLI。
6. 不允许无证据改成全零或任意 vendor string。

### 3.2 FAT16 几何
FAT16 仍可出现在制盘选项中，但如果当前容量无法合法表示为 FAT16，必须在执行前明确拒绝，不得静默切 FAT32/exFAT。

## 4. FAT32：本轮核心

目标把 FAT32 补成与 FAT16/exFAT 同等级的一等 driver。

### 4.1 matches_geometry
至少校验：512B logical sector、total sectors、partition offset/hidden sectors、cluster count >= 65525、reserved sectors、FAT copies、FAT size、root cluster、FSInfo/backup boot 引用。

### 4.2 read_metadata
补齐 Volume Serial、Volume Label，并在需要时从 root directory volume-label entry 回退读取。卷标编码策略应尽量复用 FAT family 公共工具。

### 4.3 validate_format_request
校验 filesystem==FAT32、label、serial、sector size、geometry，并拒绝落入 FAT12/FAT16 cluster count 的几何。

### 4.4 build_format_plan
必须生成可被主流 OS 正常识别和挂载的空 FAT32，至少包含：boot sector、FSInfo、backup boot、backup FSInfo、FAT #1、FAT #2（如 copies=2）、root cluster、Volume Label entry、55 AA、volume serial、hidden sectors/partition offset、FAT32 type text、稀疏写入计划。

核心 formatter 不依赖外部 mkfs。

### 4.5 verify_format
读回校验 boot signature、BPB geometry、hidden sectors、total sectors、FAT size、root cluster、FSInfo signatures、backup boot、serial、label，并确认 driver detect 仍为 FAT32。

### 4.6 analyze
复用 `filesystem/analysis/fat.rs`，禁止为 FAT32 再写一套目录遍历器。目标是 FAT12/FAT16/FAT32 共享同一个 FAT family analyzer。

## 5. FAT12

本轮只补 Analyze=true，并复用 FAT analyzer。暂不实现 formatter，也不进入 Provision 可选列表。

## 6. exFAT

保持 Detect / Metadata / Format / Verify / Analyze。只做统一 provision list/capability gate 所需适配，不重写 formatter。

## 7. NTFS

本轮冻结 Detect / Metadata。不做 Analyze / Format / Verify / Provision selection，后续单独规划。

## 8. Provision 可选格式

建立单一支持列表，唯一产品策略为：FAT16 / FAT32 / exFAT。EDP 与 Plain 共用，不允许各页面复制 match。

正向循环：FAT16 → FAT32 → exFAT → FAT16。
反向循环：FAT16 ← FAT32 ← exFAT。

默认保持不变：
- 启动区 FAT16。
- 交换区 exFAT。
- 保密区 exFAT。
- Plain 保持既定默认，只增加 FAT32 可选。

## 9. Capability Gate

UI 支持列表是产品策略，driver capability 是运行时事实。FAT32 开放到 UI 前必须满足：driver 存在、format=true、verify_format=true，并通过 formatter/verify tests。

建议测试 invariant：对所有 Provision 支持 FS，driver 必须存在且 format/verify_format 都为 true。

不得先把 FAT32 放进 UI，再在确认页或执行阶段报 format unsupported。

## 10. EDP / Plain 共用链路

统一：FilesystemKind → DriverRegistry → Provision 支持列表 → TUI/CLI form → validation → build_empty_filesystem → verify_format。

必须防止：
- EDP 有 FAT32、Plain 没有；
- Plain 能选 FAT32、EDP validation 拒绝；
- 表单能选、确认页错误；
- CLI 能选、TUI 不能选。

## 11. Partition Type

选择 FAT32 时由 domain 的 `visible_mbr_partition_type(...)` 统一决定 MBR type，TUI 禁止硬编码。exFAT 继续按 0x07 等现有 domain 规则执行。

## 12. Validation UX

当用户选择一个支持的文件系统，但当前 geometry 不合法时，错误必须明确到：哪个分区、哪个 FS、哪项几何约束不满足。确认页在执行前完成全部 filesystem geometry validation。

## 13. 测试

### FAT32 driver
- detect 正样本。
- reject FAT12/FAT16/exFAT。
- geometry match。
- stale hidden sector rejection。
- build empty FAT32。
- serial / label。
- FSInfo / backup boot。
- FAT copies / root cluster。
- readback verify。
- analyze empty filesystem。
- analyze sample directory。
- corrupt boot / FSInfo / backup。
- capacity boundary。

### FAT12 analyze
- detect。
- shared analyzer。
- empty。
- files/dirs。
- invalid chain。
- duplicate/loop protection。

### FAT16 OEM
- formatter 不再写 EDPCLI。
- 与官方 sample OEM field 一致。
- Detect/metadata/verify 无回归。

### Provision matrix
| Role | FAT16 | FAT32 | exFAT |
|---|---:|---:|---:|
| Boot | ✓ | ✓ | ✓ |
| Share | ✓ | ✓ | ✓ |
| Encrypt | ✓ | ✓ | ✓ |
| Plain | ✓ | ✓ | ✓ |

✓ 表示允许选择并进入 geometry validation；具体容量仍可合法拒绝 FAT16/FAT32。

### UI
- forward/reverse cycle。
- EDP/Plain 一致。
- 默认值不变。
- unsupported kind 不泄漏。
- capability invariant。

## 14. 实施阶段

F1 FAT32 domain 完整化：geometry、metadata、formatter、verify、tests。
F2 FAT family analyzer 收口：FAT32 analyze、FAT12 analyze、shared FAT parser、capability flags。
F3 FAT16 OEM 治理：官方样本取证、formatter 修改、regression fixture。
P1 Provision 三种 FS：单一支持列表、EDP/Plain 共用、forward/reverse、validation、confirmation/result。
P2 全矩阵测试：Domain、Application、TUI、CLI、Virtual Disk、必要时真实 USB/HIL。

## 15. 完成定义

1. 不新增 FilesystemKind。
2. FAT12 Analyze 可用。
3. FAT16 完整能力保持。
4. FAT16 新格式化结果不再写 EDPCLI OEM Name。
5. FAT32 达到 Detect/Metadata/Format/Verify/Analyze 全能力。
6. exFAT 无回归。
7. NTFS 保持 Detect/Metadata，不进入制盘。
8. EDP 与 Plain 只提供 FAT16/FAT32/exFAT。
9. 默认 Boot=FAT16、Share/Encrypt=exFAT 不变。
10. UI 与 domain 共享单一支持列表。
11. FAT32 开放 UI 前已通过 formatter/verify tests。
12. geometry 不合法时在执行前明确拒绝并给出具体原因。
