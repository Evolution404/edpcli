# Provision / LCE / FAT HIL 收口记录（2026-10-01）

## 1. 结论

本轮 Provision / LCE / FAT 写盘链已经完成虚拟盘与实体 USB 收口。实现提交基线：

```text
e3ab720 fix(provision): close real-disk HIL gaps
```

最终验证证明：

- `verify_lce_readback()` 位于协议事务写入之后、任何文件系统格式化之前；
- 实体 USB mode0 制盘成功，LBA0-12、分区几何与 LCE 读回均通过；
- 同一次实体盘制盘实际格式化并读回验证 FAT16、FAT32、exFAT；
- macOS Virtual Disk HIL、FAT16/FAT32 原生挂载 HIL、Plain exFAT Virtual HIL 均通过；
- 另一块实体 mode0 盘的 6-sector LCE 也独立解码为同一 gold plaintext；
- HIL 过程发现的两个生产缺口已经修复并增加回归门禁。

## 2. 虚拟磁盘 / 文件系统 HIL

### 2.1 FAT16

执行：

```text
scripts/hil-fat16-macos.sh
```

结果：

```text
fsck_msdos: PASS
Volume 启动区: native mount PASS
file write/read/unmount/remount/read: PASS
FAT16 native mount, label and file round-trip: PASS
```

### 2.2 FAT32

执行：

```text
scripts/hil-fat32-macos.sh
```

结果：

```text
fsck_msdos: PASS
Volume FAT32HIL: label PASS
file write/read/unmount/remount/read: PASS
FAT32 native mount, label and file round-trip: PASS
```

### 2.3 Plain exFAT Virtual Disk

执行：

```text
scripts/ci/macos-plain-virtual-disk-hil.sh
```

结果：

```text
macos_plain_virtual_disk_provisions_and_reidentifies ... ok
macOS Plain virtual-disk HIL PASS
```

### 2.4 Official Virtual Disk

`tests/virtual_disk_hil.rs` 已允许在 macOS `ci-virtual-disk` 条件下运行。使用 256MiB disposable CRawDiskImage：

```text
EDPCLI_VIRTUAL_DISK_PATH=/dev/rdiskN
EDPCLI_VIRTUAL_DISK_SECTORS=524288
cargo test --features ci-virtual-disk --test virtual_disk_hil -- --ignored --nocapture
```

结果：

```text
running 1 test
raw_virtual_disk_atomic_roundtrip_and_restore ... ok
1 passed; 0 failed
```

macOS 平台门禁同时证明目标必须为 WholeDisk + Virtual + BusProtocol=Disk Image，不能把该入口用于真实 USB。

## 3. 实体 USB 制盘 HIL

目标盘：

```text
disk5
VID:PID       3535:6300
total_sectors 15728640
capacity      8.05GB
serial SHA256 be6ae8e336d8d4686eb67d6525ddb8bb0deb0058f66504f30ff9c0ff30aad4bf
source        mode1
source device_id disk&ven_aigo&prod_u335&rev_1100
```

目标 mode0：

```text
Boot    FAT16  label=启动区
Share   FAT32  label=交换区
Encrypt exFAT  label=保密区
```

实际目标几何：

```text
Boot    LBA63       .. 20479      20417 sectors
Share   LBA20480    .. 13627454   13606975 sectors
Encrypt LBA13627455 .. 15725842   2098388 sectors
LCE     LBA15725843 .. 15725848   6 sectors
```

制盘前自动备份：

```text
disk5_15728640_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_1100_onlyid3638853809_20261001_184453.edpb
```

正式结果：

```text
制盘：成功，协议与几何读回验证通过。
格式化：✓ 启动区，读回验证通过
格式化：✓ 交换区，读回验证通过
格式化：✓ 保密区，读回验证通过
```

写后重新扫描：

```text
disk5  8.05GB  3535:6300
mode0 · 缺省三分区
device_id=disk&ven_aigo&prod_u335
```

文件系统读回：

```text
LBA63       -> 物理明文 FAT16
LBA20480    -> SM4 mode2 密文，解密后 FAT32
LBA13627455 -> SM4 mode2 密文，解密后 exFAT
```

这证明 CLI 参数、密码域计划、实际文件系统写入器、加密分区变换与读回分析器使用同一语义。

## 4. LCE 实体盘证据

### 4.1 新制 disk5

最终 LBA7：

```text
Entry[1] start=15725843 size=3072
Entry[2] start=15725843 size=3072
official mode=0
```

连续 6 sector 解码 SHA-256：

```text
0  4b6b7fb9805ad36f95c306cb942a3c5ca65bc8bda1d6d02e2f938f4785f7a563
1  759e623cd0bf2624fa5dba43fe9f6e5a725342265a6fa56a81b53e5f093f521e
2  ac44e9d57f6aba0ee85579c31c9031bd140d134fb66eb0270a15f98e57f5aa5e
3  951245527a378454ebea343f97c72bda8c35e782bb7b0813fe48e832e3b4853b
4  0d46462457e6d0af9858582acc6280579ff3efa433d1439677f214d6a00b31f1
5  12ad265769d733414ca4a62d39ff266a61a644c10f1458645078a6fe768d58ab
```

与 `audit/protocol/lba7_compatibility/gold/lba7_compat_plain_zero8.bin` 六个 sector 的 SHA-256 逐项完全一致；gold 整体：

```text
size=3072
SHA-256=386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c
```

### 4.2 第二实体样本 disk4

现有 125.83GB mode0：

```text
VID:PID 2bdf:0300
LCE start=245744513
Entry[1]/Entry[2] size=3072
```

其 6 个解码 sector SHA-256 与上述 gold 六项再次逐项一致。该盘用于第二独立物理样本佐证，本轮未破坏其现有 mode0 内容。

## 5. HIL 发现并修复的生产缺口

### 5.1 重建时显式文件系统被旧分区几何覆盖

实体盘计划最初指定：

```text
--boot-fs fat16
--share-fs fat32
--encrypt-fs exfat
```

但计划曾错误显示 Share=exFAT。根因是 `prepare_target_provision()` 先读取用户 `FormatOptions`，随后无条件用 `target_plan.partitions[].geometry.filesystem` 覆盖，导致 Rebuild 仍继承旧来源文件系统。

修复后规则：

- Rebuild：目标文件系统必须使用当前显式请求；
- `Preserve` / `Rewrap`：继续保留已验证来源文件系统。

新增单测：

```text
rebuild_uses_explicit_requested_filesystems_instead_of_stale_source_geometry
preserved_partition_keeps_verified_source_filesystem
```

实体 disk5 最终实际读回 FAT16/FAT32/exFAT，闭环证明修复有效。

### 5.2 带版本号旧版 device_id 与规范写入 device_id 被误判换盘

disk5 来源协议实际使用：

```text
disk&ven_aigo&prod_u335&rev_1100
```

当前第一方写入使用的规范 device_id：

```text
disk&ven_aigo&prod_u335
```

二者都是同一当前硬件探测可生成的合法候选。原写前复核要求字符串绝对相等，因此在提权/重开后会误报：

```text
设备在选择/确认期间发生变化
```

修复后：

- 来源协议解析优先使用真实观察到的 protocol device_id；
- 新写入继续使用当前规范写盘 `device_id`；
- 写前 identity recheck 允许 expected/actual 都属于当前同一硬件候选集的 short/revision variant；
- `onlyid`、`serial`、VID:PID、分区几何、协议快照等既有 fail-closed 身份锁定门禁不放宽。

新增回归：

```text
observed_protocol_device_id_wins_for_existing_edp_source_parsing
edp_identity_recheck_accepts_short_and_revision_candidates_for_same_hardware
```

## 6. 最终软件门禁

在上述修复后重新执行：

```text
cargo fmt --all
git diff --check
scripts/test-fast.sh
python3 scripts/test-full.py --profile full
```

结果：

```text
fast: failures=0
full: suites=8 artifacts=10 failures=0
full duration=10.65s
```

## 7. 收口判定

`PROVISION_BACKUP_GOVERNANCE_2026-10-01.md` 中 P0-P4 的实现与本轮 HIL 门禁均已完成。

剩余的长期工作仅属于未来功能扩展或新的真实样本覆盖，不再属于本计划的未完成项。
