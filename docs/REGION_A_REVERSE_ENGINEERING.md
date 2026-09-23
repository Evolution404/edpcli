# Region A 逆向工程总文档

本文件是 Region A 的唯一人类可读总入口。目标是把 Region A 推进到与 LBA0-LBA12 相同的工程等级：物理证据、producer/consumer、字节级 ledger、profile/evolution、代码解析器、行为测试和严格完成率必须互相一致。

## 1. 范围和硬约束

Region A 不是 device tail window。它是 LBA7 compact EDPF 中 `PartitionSize=0xC00` 的非 Boot 条目所指向的固定 6 扇区区域；三条目样本的 entry1/entry2 同指它，两条目免密码 SanDisk 仅 type4 entry1 指向它。

当前真实 Lexar：

- total sectors: 243625984
- Region A start: LBA 243623933
- range: LBA 243623933..243623938
- size: 3072B / 0xC00
- physical SHA-256: fbf45d4713664d1e68eca634e8e9c04565be8b60d9ad4e1a18b7df3c766aaa24

物理 gold 位于 `audit/region_a/gold/lexar_region_a_lba243623933.bin`。

所有 COMPLETE 必须同时具备可复现 producer、consumer 和物理正证据。静态反编译或 Unicorn 单独存在都不能把真实盘明文字段提升为 COMPLETE。

## 2. 当前严格进度

### 2.1 物理 wire 3072B

| 范围 | COMPLETE | PARTIAL | UNKNOWN |
|---|---:|---:|---:|
| Region A +0x000..+0xbff | 0 | 0 | 3072 |

含义：当前唯一严格成立的是 LBA7 指针、CHS 定位公式和 3072B 物理密文本身。尚未找到直接作用于这 0xC00 的 producer、consumer、明文结构或加密边界，因此**全部 3072B 都是 UNKNOWN**。此前把前 0x800B 计为 IIR PARTIAL 属于未证明的对象绑定，现已撤销。

机器 ledger：`audit/region_a/wire_byte_ledger.tsv`。

### 2.2 独立 IIR plaintext 参考 2048B

IIR 是一个独立研究对象：`ReadIIR/WriteIIR` 的 0x800B 表结构、AES-256-CBC 和 CRC 规则均有静态证据，但**尚未证明它与 Region A 是同一物理对象**。因此下面的 IIR plaintext ledger 只记录 IIR 自身解析进度，不计入 Region A 的 COMPLETE/PARTIAL 字节。

| 范围 | COMPLETE | PARTIAL | UNKNOWN |
|---|---:|---:|---:|
| IIR plaintext +0x000..+0x7ff | 0 | 1052 | 996 |

机器 ledger：`audit/region_a/plain_byte_ledger.tsv`。

## 3. LBA7 与 Region A 的闭环部分

匹配 Lexar LBA0-LBA12 physical gold 的 LBA7 entry1/entry2 均描述：

- start sector = 243623933
- sector size = 512
- partition size = 3072B

LBA7 compact key 字段已经单独闭环：默认密码 `0000aaaa` 可通过 old-format unwrap 得到 `key8 = dd4019e3637d390f`，且 CRC32(key8) 与 LBA7 存储值一致。

这只证明 LBA7 compact key 自洽，不证明 `key8` 就是 Region A AES key，也不证明它如何扩展成 AES 上下文。

### 3.1 Region A 物理定位算法已 COMPLETE：基于 CHS，不依赖厂商命令

2026-09-22 已用 `cemsusbregsiter.dll` 机器码和三品牌实盘 LBA7 独立交叉验证闭环制盘/注册路径的定位算法：

1. `CDiskFile::sub_10012c40` 对 `\\.\PhysicalDriveN` 调用标准 `IOCTL_DISK_GET_DRIVE_GEOMETRY (0x70000)`。
2. 官方代码用三个 64 位乘法调用计算 `Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector`，结果写入 `CDiskFile+0x30/+0x34`。
3. `CUsbRegsiter::CreateDiskFile` 把该 CHS 字节容量复制到 `this+0x6a0/+0x6a4`，sector size 写入 `this+0x6a8`。
4. `sub_10040110 @ 0x10040110` 的机器码直接执行 `m_ullSize - 0x100000 + 0x20000`，即 `CHS_bytes - 0xE0000`。
5. `CreatePartitions @ 0x1003de7a` 调用该函数，再用 `sub_10068590` 除以 sector size，把 64 位商写入**旧 0x40-stride / LBA7 表**的 entry `+0x18/+0x1c`；同时把对齐后的 `0xC00` 写入 `+0x28/+0x2c`。
6. 同一函数另建 `var_1364` 的 **0x60-stride / LBA12 新表**。`sub_10014f30(..., &var_1244, &var_1364, ..., 0x206)` 序列化新表，随后 `MountEdpPart` 明确接收 `&var_1364`，不是承载 Region A 指针的旧表。

因此通用公式是：

`RegionA_byte_offset = CHS_bytes - 0xE0000`

`RegionA_LBA = (CHS_bytes - 0xE0000) / BytesPerSector`

对 512B sector：

`RegionA_LBA = CHS_sectors - 0x700 = CHS_sectors - 1792`

三盘独立 LBA7 解码均精确匹配：

| 设备 | CHS sectors | 公式 LBA | LBA7 entry1/2 StartSector | size |
|---|---:|---:|---:|---:|
| Lexar | 243625725 | 243623933 | 243623933 | 0xC00 |
| aigo | 245746305 | 245744513 | 245744513 | 0xC00 |
| SanDisk | 240252075 | 240250283 | 240250283 | 0xC00 |

这同时修正旧的 `total_sectors - 2051` 表述：该式只在 Lexar 上因 `physical_total - CHS = 259 sectors` 而数值成立，不是官方通用算法。官方还另外取得 real size，但 Region A locator 明确使用 `DISK_GEOMETRY` 的 CHS 乘积。

证据：`audit/region_a/evidence/region_a_locator_algorithm_20260922.json`。

注意：这里 COMPLETE 的只有 **Region A 物理定位**，不包含其数据语义。

### 3.2 已排除：`MountEdpPart -> EdpMountFile` 不是 Region A consumer

`CreatePartitions` 同时维护两张不同表：`var_1244` 为 3×0x40 的 legacy/LBA7 表，Region A 的 CHS-0x700 指针写在这里；`var_1364` 为 3×0x60 的 new/LBA12 表，`MountEdpPart` 迭代的正是后者。当前 Lexar 更能直接交叉验证：LBA7 entry1/2 都指向 243623933，而 LBA12 type2/type4 的真实数据分区起点分别是 20480 和 231424000。

因此 `EdpMountFile/EdpEDisk64.sys` 的分区加密、file_key、SM4 profile **不能作为 Region A 的 producer/consumer 或解密证据**。机器证据：`audit/region_a/evidence/mount_not_region_a_20260922.json`。

## 4. 独立 IIR 候选（尚未与 Region A 绑定）

静态证据来自：

`~/Desktop/u_disk/VRV/cems/ydcc/sectormanage64.dll`

SHA-256：

`5fa0823b3f2698ae2c93b3d27fbb0e09e380cf119c7d78ceb73683d45ec6a9bc`

关键函数：

- `sub_180010fc0` / ReadIIR
- `sub_180012100` / WriteIIR
- `sub_18000f4e0` / InitIIRTable
- `sub_180013b80` / WriteIIRPar
- `sub_1800092c0` / encrypt wrapper
- `sub_180009390` / decrypt wrapper
- `sub_18001c560` / AES cipher descriptor selector

ReadIIR 和 WriteIIR 都使用 device tree node `+0x18` 计算物理位置，并固定处理 0x800B。这些事实只证明 IIR 自身结构；在缺少直接物理地址实测前，不把这 0x800B 映射到 Region A，也不存在所谓“Region A 后 0x400B 是 IIR 尾部”的前提。

### 4.1 物理地址链已闭环到 PartInfo[2]，最后一个 runtime 值仍缺

2026-09-22 进一步把地址来源向下追到 `netac_usb_api64.dll`：

1. `GetPartInfoAllA_NetacAPI` 通过控制器命令 `0x06FE` 取得 PartInfo 表；每个条目 24B。
2. `GetPartSectorNumAllA_NetacAPI` 明确逐项复制每个 24B 条目的**第一个 u32**，因此该字段语义是 sector number。
3. `langkeudisk64!GetDevInfo` 遍历4个 PartInfo 条目，把 `sector_num << 9` 依次写入4个64位局部值；第3个条目 `PartInfo[2]` 成为 `GetDevInfo` 输出 `+0x18`。
4. `sectormanage64!GetHardInfoImp` 把 `GetDevInfo.out+0x18` 写入 device-tree node `+0x18`。
5. `ReadIIR/WriteIIR` 从该 node 读取 `+0x18`，再减 `0x20000` bytes（256 sectors）得到实际0x800B IIR读写位置。

因此静态公式已经是：

`IIR_LBA = PartInfo[2].sector_num - 256`

对当前 Lexar，若 IIR 与 LBA7 Region A 起点同址，则必须满足：

- `PartInfo[2].sector_num = 243624189`
- `243624189 - 256 = 243623933`
- 同时 `243624189 = CHS - 1536`

当前**尚未抓到这块 Lexar 的真实 `0x06FE` 控制器响应**，所以 `243624189` 仍是由物理 Region A 反推的待验证值，不能算 runtime physical evidence。旧文档把 `node+0x18` 称为“总扇区数”已经被静态代码否定：它实际来自 `PartInfo[2].sector_num * 512`。

机器证据：`audit/region_a/evidence/iir_address_chain_20260922.json`。

### 4.2 `0x06FE` PartInfo 传输和解码规则已静态闭环

`netac_usb_api64.dll!GetPartInfoAllA_NetacAPI` 的底层链已经继续闭环：

1. 上层命令字 `0x06FE` 以小端写入 12B SCSI CDB，因此 CDB 为 `FE 06 00 00 00 00 00 00 00 00 00 00`。
2. `sub_180003b80 -> sub_180003990` 固定申请 0x200B 数据区；该调用使用 direction=1，底层完成后才把数据区复制回调用者，因此是 **device-to-host DATA-IN**，不是写盘命令。
3. 原始 512B 响应不能直接解释。官方随后用固定 32B ASCII key `1234567890abcdefFEDCBA!@#$%^&*()` 初始化 `sub_180001040`：block size=16、key size=32、rounds=14。
4. `sub_180002650` 把 512B 按 16B 独立分块，逐块调用 `sub_1800021c0`，没有 CBC/stream chaining state，因此该响应解码算法是 **AES-256-ECB**。
5. 解码后 byte `+0x04` 是 partition count，24B PartInfo entries 从 `+0x08` 开始；所以 `PartInfo[2].sector_num` 位于解码后 `+0x38`，其第一个 u32 应直接实测为 `243624189 / 0x0E8568FD` 才能关闭当前 Lexar 的同址绑定。

机器证据：`audit/region_a/evidence/partinfo_transport_crypto_20260922.json`。

这里 COMPLETE 的只是**IIR/Netac profile 的命令传输和响应解码规则**，不代表该控制器协议适用于当前 Lexar。按此前实盘结论，当前 Lexar 不识别这条 `FE 06` 路径，因此后续不再把“继续抓 FE06”作为 Region A 主线。IIR 与 Region A 是否同址仍是未证明假设；这组静态证据不给 Region A wire ledger 贡献任何 PARTIAL 字节。

### 4.3 `sectorInfo` 上传链已明确排除为 Region A

2026-09-22 对运行日志、`EdpEDiskCtrl.dll` 静态实现和已有独立解码器交叉复核后，可以关闭此前“`sectorInfo` 可能就是 Region A 上报数据”的候选：

- `CEdpDiskControl::UpLoadBackupInfo` 确实把 `sectorInfo` 放入上传 JSON；79 条日志样本的 Base64 解码长度为 672B（52 条）或 688B（27 条）。
- `ReadOrgSector/sub_10013140` 在未显式传入相对扇区时把 `var_12` 设为 8，再按 `(base + 8) * sector_size` 定位并读取 512B；这条 producer 路径锚定的是 LBA8 类数据，而不是磁盘尾部 Region A。
- `analyze/scripts/decode_sectorinfo.py` 独立复核 9 个 `sectorInfo` 样本，9/9 都还原出 `LLGB + <ELABEL>` 的 LBA8 明文；其后追加 160B 或 176B EDPF 元数据。
- 因此这条上传链的真实结构是“LBA8 明文 + EDPF 备份元数据”，与 0xC00 Region A 物理块不是同一对象。

结论：**`sectorInfo` 不能再作为 Region A 上传服务器的证据。** 是否存在另一条真正上传 Region A 的网络链仍然是开放问题。

机器证据：`audit/region_a/evidence/sectorinfo_upload_log_20260922.json`。
### 4.5 `cemsusbregsiter` 只闭环 Region A 指针生产，未发现 payload I/O

2026-09-22 对当前 `cemsusbregsiter.dll` 的 `CDiskFile` 扇区 I/O 调用图做了完整枚举：`sub_100136c0` 是 `ReadSectorData`，`sub_10013810` 是 `WriteSectorData`。`CreatePartitions/sub_1003db50` 确实通过 `sub_10040110` 计算 `CHS_bytes - 0xE0000`，并把换算后的 Region A sector 写进 legacy EDPF entry；但其后 `sub_10014da0` 只是把 3×0x40 legacy 表序列化到 LBA7，本身不按 entry 的 `StartSector` 做物理 I/O。

对该 DLL 内全部直接 `ReadSectorData/WriteSectorData` 调用逐项检查后，没有发现以 Region A `CHS-0x700` 为目标、长度为 6 sectors 的读写。`BakupUsbSec/sub_10040940` 写的是另一组 front-label backup sectors，`sub_10013e40` 只构造内存结构，也都不是 Region A payload I/O。

因此当前严格结论是：**`cemsusbregsiter` 已闭环 Region A 指针 metadata producer，但未闭环 Region A 0xC00 payload producer/consumer。** 真正 payload 路径必须继续到其他模块、其他版本或该 DLL 之外的 I/O 机制查找。

机器证据：`audit/region_a/evidence/cemsusbregsiter_no_region_a_io_20260922.json`。

### 4.6 三盘物理证据进一步把 Region A 与静态 IIR 候选窗口分离

2026-09-23 对同一次只读采集体系中的三块真实盘重新交叉检查。`capture_disk.py` 明确把两个位置独立采集：

- Region A：`CHS_bytes - 0xE0000`，长度 `0xC00 / 3072B`。
- 静态 IIR 候选窗口：`CHS_bytes - 0x40000`，长度 `0x800 / 2048B`。

三盘结果完全不同：

- Lexar Region A SHA-256=`fbf45d...aaa24`，3068/3072 字节非零；IIR 候选窗口 2048B 全零。
- aigo Region A SHA-256=`996d23...f138`，3058/3072 字节非零；IIR 候选窗口 2048B 全零。
- SanDisk Region A SHA-256=`e51e95...298a`，3067/3072 字节非零；IIR 候选窗口 2048B 全零。
- 三个 IIR 候选窗口 SHA-256 均为全零 2048B 的 `e5a00aa9991ac8a5ee3109844d84a55583bd20572ad3ffcd42792f3c36b183ad`。

因此在这三块实盘上，**LBA7 指向的 Region A 与 CHS-0x40000 的静态 IIR 候选窗口是两个不同物理对象**。这进一步支持当前原则：除非后续取得 runtime PartInfo 地址直接等于 Region A 的正证据，否则不得把 Region A 当作 IIR。

机器证据：`audit/region_a/evidence/region_a_vs_iir_three_disk_20260923.json`。

### 4.7 `edpediskctrl!ReadEncryptPartionInfoEx` 已排除为 Region A consumer

继续追 `edpediskctrl.dll` 后，`ReadEncryptPartionInfoEx/sub_100128d0` 的真实寻址链已经闭环：

1. `InitDiskInfo/sub_10021e20` 调用 `sub_10031120`，把返回值保存为全局 label base。
2. `sub_10031120` 的日志名是 `CDiskFunc::GetGptIndex`；它只读取前 13 个扇区来判断 MBR/GPT label 基址。
3. `sub_100128d0` 在相对扇区为 0 时默认设为 `0x0c`，随后按 `(label_base + relative_sector) * sector_size` 做 `SetFilePointer + ReadFile`。
4. 这条地址计算没有读取 legacy LBA7 entry 的 `StartSector@+0x18/+0x1c`。

所以虽然该函数也会解密并校验 `EDPF` magic，它消费的是前部 label/LBA12-family metadata，不是 CHS-0x700 的 Region A payload。`UDiskLabelRepair.dll` 中同名解析路径也表现为对已读 label buffer 的结构解密，未发现按 legacy StartSector 再跳转 6 sectors 的证据。

机器证据：`audit/region_a/evidence/edpediskctrl_front_label_not_region_a_20260923.json`。

### 4.8 免密码 SanDisk 的旧「Region A 全零」采集位点错误

历史 `no_password_disk4/raw/region_a_at_e53720000.bin` 确实是 3072B 全零，但它采集于 **physical_total_bytes - 0xE0000**，对应 LBA `120174848`。该盘 CHS 容量是 `120166200` sectors，解码后的 LBA7 type4 entry 指向 **CHS_bytes - 0xE0000**，即 LBA `120164408` / 字节偏移 `0xe53207000`。两个位置相差 `10440` sectors。旧文件的零值不能归因给 Region A。

该两条目 profile 还有一个约束：LBA7 type2 指向实际 Share 起点 LBA63，而 type4 指向 CHS 定位的 0xC00 区域。因此「LBA7 type2/type4 总是同址指向 Region A」也不是通用规律。机器证据：`audit/region_a/evidence/no_password_sandisk_mislocated_capture_20260923.json`。

2026-09-23 对用户重新插入的同一 SanDisk (`disk5`) 进行了只读重采集。当前 LBA7 原始 512B 与 2026-08-23 旧快照逐字节一致。LBA7 type4 指向的真正 Region A `LBA120164408..120164413` 为独立的六扇区高熵块：SHA-256=`aaeffbba440e553c2eb47accac54552c9b53af9d4951f728ac700a2e81aca0a0`，3062/3072B 非零，熵约 7.937 bit/B；前后各四扇区全零。旧错误位点仍为 3072B 全零并与旧文件逐字节一致。采集原始数据与方法保存在 `audit/region_a/live_captures/sandisk_nopwd_20260923/` 和 `scripts/protocol/capture_region_a_readonly.py`，机器结论在 `audit/region_a/evidence/sandisk_nopwd_region_a_live_20260923.json`。这证明该两条目免密码 profile 仍有真实 Region A payload，但其明文与消费机制仍未知。

### 4.9 旧版 `EdpEDiskCtrl.dll` 的条件挂载路径

离线驱动变换复算脚本：`scripts/protocol/probe_legacy_region_a_driver.py`（只读输入文件，不访问物理盘）。

对历史版本 `u_disk/VRV/edp/EdpEDiskCtrl.dll`（与当前 `cemsusbregsiter` 挂载路径不同）核查出一条**仅在新版标签读取失败时**启用的旧标签路径。`InitDiskInfo/sub_10018d90` 首先调用 `sub_10011530` 读取 LBA12 family 新标签；失败后调用 `sub_1000f520` 从物理 LBA7 读取 512B 旧标签，再经 `sub_10010850` 将三个 `0x40` entry 逐个复制到 `0x60` 运行时 entry，保留 `NeedEncrypt@+0x14`、`StartSector@+0x18`、`PartitionSize@+0x28`、CRC 与 wrapped8；运行时表版本置 `0x64`。这条分支的存在由反汇编 `0x10018df5..0x10018e75` 确认。

在同一 DLL 的 `UserLogin/sub_10018ee0`，按 type 匹配运行时 `0x60` entry 后，`0x10019889` 从所选 entry 的 `+0x14/+0x18/+0x28/+0x34/+0x58` 复制挂载参数，随后 `0x10019b12` 调用动态解析的 `EdpMountFile`。版本非 `0x206` 时，登录校验使用 8B key，驱动 `EdpEDisk64.sys` 的读写分支也按非 `0x206` 走 8B key 算法。因而**若旧标签回退被触发、对应 type 成功登录且挂载成功**，LBA7 中指向 Region A 的 entry 会把它作为 3072B backing extent 提交给虚拟磁盘驱动。这是目前找到的第一条有物理 Region A 指针参与的条件消费链，不能推广成当前客户端正常登录必读 Region A。

免密 SanDisk 的旧表恰好提供可复核实例：type2 指向 LBA63 的普通 Share，type4 指向 Region A LBA120164408、大小 3072B、`NeedEncrypt=1`。type4 的 wrapped8=`b5355e2c582ed090` 用旧口令 `0000aaaa` 解得 8B key=`24a4cfbdc9bf4101`，bare CRC32=`9d13ad66` 与 entry 相符；新版 LBA12 type4 则指向 LBA117611865、大小 1299594240B、`EncryptMode=2`。这说明旧回退和新版正常路径选择的是不同的物理对象与密钥分支。旧回退可挂载性的静态证据**尚未证明**用户曾在该盘上触发回退、Windows 能识别此 3KB 虚拟设备中的文件系统，或 Region A 明文内容的真正业务作用。机器证据见 `audit/region_a/evidence/legacy_edpediskctrl_region_a_fallback_20260923.json`。

以同一份 `EdpEDisk64.sys` 在 Unicorn 中调用旧版 8B key 分支 `sub_13160`，并用 `sub_13450` 对结果回加密，已在 16B 零块及完整 3072B Region A 上验证逐字节 round-trip。对免密 SanDisk 使用上述 CRC 闭合的 key8，解后 SHA-256=`5e3620...a2ac7`、熵约 7.936 bit/B；对 Lexar 使用其已验证 key8=`dd4019e3637d390f`，解后 SHA-256=`ead2c923...436960a`、熵约 7.939 bit/B。两份输出均无 `EDPF`/`FAT`/`NTFS`/`LLGB` 标识。这只排除“用此驱动旧 8B key 分支解后直接得到普通可识别磁盘格式”的简单模型；不排除双层加密、另一种 key/模式或随机内容。历史 `u_disk/analyze/mock_newlabel/decrypt_region_a_driver.py` 使用了不符合该驱动调用方式的分离输入/输出缓冲，先前由它产生的候选明文不应用来判定 Region A。

### 4.10 已定位的条件物理写入点与现存 payload 来源的界限

旧版驱动 `EdpEDisk64.sys` 的写 IRP 路径提供了一个**可达 Region A 的物理写入点**：`0x120f2` 先检查虚拟写入 `offset + length <= PartitionSize`；`0x1214d..0x12163` 计算并保存 `backing_offset + virtual_offset`；若启用加密且版本非 `0x206`，`0x122aa` 选择 8B key 的 `sub_13450`；最后 `0x12300..0x1232e` 把数据、长度及保存的物理偏移传给 `ZwWriteFile`。结合上节的旧 LBA7 回退，如果 type4 的 3072B 分区成功挂载，主机向其虚拟偏移 `0..0xBFF` 写数据，驱动就会写入真实 Region A 六扇区。这是**写入能力和条件代码路径**，尚无执行轨迹证明四块采样盘的现存高熵内容由它产生。

写入者筛查还覆盖了 `cemsusbregsiter.dll` 的直接 `WriteFile`/`SetFilePointer` 调用：所见物理盘或逻辑盘写入分别用于前部标签、备份、NTFS 初始化及包装器；`CreatePartitions` 中另外三处 `CHS−0xE0000` 运算只填入 `SetPartInfoNew` 条目。安装包中的 `safeusbregsitercems.dll`、`usbtools.dll`、`usbshformat.exe`、Netac API 与 Linux 客户端也未出现可直接绑定为 `CHS−0xE0000` 六扇区写入的正证据。静态未命中不等于排除间接/动态地址流。现存 Region A 的**首次生成点仍未定位**，不可把驱动的通用 `ZwWriteFile` 等同于已证实的历史 producer。细节记录在 `audit/region_a/evidence/legacy_region_a_conditional_write_20260923.json`。

### 4.11 旧驱动 8B key 分支已绑定 A6B0/EDPSECDISK 密码族；counter-tweak 语义澄清

2026-09-23 用 Unicorn 对 `EdpEDisk64.sys` `sub_13160/sub_13450` 做结构实验后，旧 8B key 分支的密码学轮廓已经精确化：

1. 驱动 0x191d0 处 key 表为 ASCII `EDPSECDISK200709`（16B），与历史 `analyze/last/decrypt_tail.py` 从 `EdpEDiskCtrl.dll sub_10001de0` 提取的 A6B0 常量逐字节一致；`.sys.old`（2023-11）也含同一常量。
2. 算法是 **counter-tweaked AES-128 变体**：`expanded[i] = key[i % keylen] ^ KEY_TABLE[i]`，标准 AES-128 44 字扩展，每个 16B 块把 8 字节小端 counter（= 缓冲区内字节偏移，逐块 +16）注入全部轮密钥（`cb[(wi*4+bi)%8]`）。**不是 ECB，也不是 CBC**：单块解密（counter=0）与全缓冲中同块解密（counter=0x50）结果不同；两段相同明文加密结果不同。
3. 因此此前 4.9 的解密尝试语义是"counter 从 Region A 偏移 0 起步"，这是条件挂载下虚拟偏移 0 的正确语义；解后高熵的负结论仍然有效，但**任何未来重试都必须显式选择 counter seed**，而不是寻找 IV。
4. key 候选矩阵（免密 SanDisk，驱动精确语义）：key8、wrapped8、全零、反转 key8、expanded16（key8^KEY_TABLE）、md5(key8) —— 全部 round-trip 精确、熵 7.93-7.94、无任何 magic。这与历史 `COMPREHENSIVE_ANALYSIS_20260611.md` 第五节已排除 A6B0/XOR/SM4/AES-128/192-CBC/A7F0/3-SBOX/XOR-0x88 的清单互相印证。
5. 四份物理密文（lexar/aigo/sandisk3/sandisk_nopwd）统计：每份 192 块内零重复；任意两份之间零共享块；同位置字节相等仅 0.3-0.5%（随机巧合水平）。排除"跨盘同 key + 同明文模板"。
6. `EDPSECDISK` ASCII 标记可直接做跨二进制搜索，已命中 **Linux 客户端全套**（`libedpedisk.so`、`linuxedpedisk`、`cemsudiskcallerproxy`、`checkdiskback`、`EdpEDiskBack`、`libcemsfilesyscheck.so`）及 `EdpEDiskEx.dll`、`UDiskLabelRepair.dll` 等；Linux 侧为 ELF、比签名驱动更易反汇编，且从未按 CHS-0xE0000/0xC00 语义筛查过。

推论：现有负证据一致指向两种剩余假设——(a) Region A 使用尚未推导的 key 派生（counter seed 也可能不是 0），(b) Region A 明文本身就是高熵 keybag/证书类材料，即使解密正确也不会出现 magic 或熵下降。两者都要求先闭环 producer/consumer，不能靠猜 key 突破。

机器证据：`audit/region_a/evidence/edpsecdisk_cipher_profile_20260923.json`。

### 4.12 Linux 客户端筛查：跨平台回退 consumer 确认，Linux producer 不存在

2026-09-23 对 Linux 客户端全套 6 个 x86-64 ELF（`libedpedisk.so`、`linuxedpedisk`、`cemsudiskcallerproxy`、`checkdiskback`、`EdpEDiskBack`、`libcemsfilesyscheck.so`，全部未 strip、两个带 DWARF）做了可执行段级筛查：

1. **±0xE0000 立即数在全部可执行段中零命中**（同时搜了正/补码两种编码；裸 dword 命中全部位于 OpenSSL AES 代码或非执行段）。Linux 客户端不计算 Region A 物理地址，**不存在 Linux 侧 Region A 指针 producer**。`checkdiskback` 仅一处 ReadSectorData、零写；`EdpEDiskBack` 无裸盘 I/O。
2. **跨平台回退 consumer 确认**：`libedpedisk.so!Edpedisk::Volume::GetEdpUsbTage()`（0x42f80）V1 布局失败后用 V2 列表（含旧 LBA7 解析器 `EdpDiskLayoutOldTageIndex`），成功则 `GetPartionFromOld` 把 3×0x40 旧条目逐字段转成 0x60 运行时条目，并把运行时版本置 **0x64** —— 与 Windows 旧版 `EdpEDiskCtrl` 回退（4.9）完全对应。该条件路径是产品级跨平台设计，不再只是单平台静态发现。
3. **密码学分类学闭环**（均来自带符号反汇编）：
   - `CipherEDPAES`（16/16）→ `aes_128_Decrypt(offset=cipher+0x50, buf, schedKey, rounds)`；EDP 模式把 `startSector*sectorSize` 写入每个 cipher 的 +0x50 再**逐 cipher 顺序套用（支持多层堆叠）**。
   - `CipherEDPSIMPLE`（2/2）：数据面 = 全缓冲 16 位 XOR 滚动、每字 -1；单块版只保护 buffer+0x18 起的 24B（恰为旧条目 StartSector/SectorSize/PartionSize 三个字段）。
   - `CipherEDPSIMPLEKEY`（4/4）：单块同样只保护 +0x18..+0x2f，16 位 key 步进 +0xFF。
   - 模式：EDP + XTS；`PartitionHeaderOldEdp` 用 EDPSIMPLEKEY + 密码加法和校验（`GetUserPassLong` = 按 4 字节块累加）+ 条目 `EncryptFileKey` 8B。
4. **DWARF 权威条目布局**：旧 72B / 新 104B（新增 EncryptFileKey16/32、EncryptMode@+0x60；`Volume::GetPartitionHeader` 按 entry+0x58 的 EncryptMode 分发 header 类，2→Sms4）。
5. **多层假设暴力测试为负**：`region_a_stacked_brute.py` 对 [AES(key8)→SIMPLE] 双层与 SIMPLE 单层、K0 全 65536 空间、线性/二次两种步进、magic/零串 oracle，两盘均无结构（唯一 BKDP 候选因上下文仍高熵判为巧合）。未测：SIMPLE→AES 加密顺序。

机器证据：`audit/region_a/evidence/linux_client_region_a_screening_20260923.json`。

## 5. AES 算法、默认 Init key 与版本 profile

`sub_1800092c0/sub_180009390` 调用 `sub_18001c560()` 得到 `EVP_CIPHER` descriptor。早期仅依据 OpenSSL 注册字符串曾误判为 AES-192-CBC；2026-09-22 已用 descriptor 本体纠正。

动态路径返回 `0x1801cd190`。该结构的关键字段为：

- NID = 427 (`0x1ab`)
- block size = 16
- key length = 32
- IV length = 16
- flags = `0x1002`

因此 IIR wrapper 实际使用 **AES-256-CBC**。另外，使用完整 32 字节 ASCII key 和 zero IV 的标准 OpenSSL AES-256-CBC 解密，与官方 wrapper 前 2032 字节输出 bit-exact，证明当前 fresh EVP context 中 observed NULL-IV 参数的有效行为就是 zero IV。证据见 `audit/region_a/evidence/cipher_descriptor_aes256_20260922.json`。

### 5.1 默认 Init key 的精确生成规则

`SectorManageImp::Init` 构造固定 32B 输入。它不是普通 C 字符串，而是包含嵌入 NUL：

`76 72 76 44 6c 6c 00 38 30 48 33 54 37 33 34 57 47 4e 44 4d 4b 50 59 50 4d 38 30 45 59 58 31 00`

即显示为 `vrvDll\0` + 原固定常量后半段 + 末尾 NUL。

对这 32B 做标准 MD5：

`8eeaa2062efa0b371076ebe510d98f18`

`sub_180015c80`/对应 x86 helper 随后把 16B digest 格式化成 32 个 ASCII hex 字符，最后才复制进 core context 的第一个 string。

历史版本存在大小写 profile：

- 2021 x86 `sectormanage.dll`: `%02X`，得到 `8EEAA2062EFA0B371076EBE510D98F18`
- 2021/2025 x64 `sectormanage64.dll`: `%02x`，得到 `8eeaa2062efa0b371076ebe510d98f18`
- 2026 x86 `sectormanage.dll`: 机器码引用 `0x101a6d98`，该地址是 `%02x`，同样得到小写 key

机器证据见 `audit/region_a/evidence/init_key_derivation_20260922.json` 和 `init_key_profiles_20260922.json`。

### 5.2 两类默认 key 只排除了“Region A 前 0x800 = IIR”的组合假设

用实际小写 Init final key 走官方 `sub_180009390` wrapper：

- cipher = AES-256-CBC
- key = `8eeaa2062efa0b371076ebe510d98f18` 的 32 个 ASCII 字节
- effective IV = 16B zero
- main CRC = FAIL
- 19 segment CRC = 0/19 PASS

再用 2021 x86 大写 key：

- key = `8EEAA2062EFA0B371076EBE510D98F18`
- main CRC = FAIL
- 19 segment CRC = 0/19 PASS

严格结论仅是：**如果**把 Region A 前 0x800B 当作 IIR ciphertext，则 lower/upper 两种默认 Init key 都不能产生满足 IIR CRC 的明文。因此只排除了“Region A[:0x800] = IIR 且使用这两类默认 key”这一组合假设；不能据此声称已经排除了 Region A 的真实 key，也不能用它缩小 Region A 自身的 crypto profile。

证据：

- `audit/region_a/evidence/decrypt_init_key_candidate_20260922.json`
- `audit/region_a/evidence/decrypt_init_key_candidate_crc_20260922.json`
- `audit/region_a/evidence/decrypt_uppercase_key_candidate_crc_20260922.json`

旧的 raw MD5 intermediate (`8eeaa206...` + zero-filled output buffer) 解密尝试继续保留为 negative history，但它不是 `Init` 最终写入 core context 的 key，不能再作为主候选。

### 5.3 当前 x64 core-key 写入边界进一步收窄

对同一份 SHA-256=`5fa0823b...` 的当前 x64 `sectormanage64.dll` 继续做数据流和 bounded write-reference 审计：

- `SectorManageImp::Init/sub_18000b520` 把最终 32B ASCII hex key 写入 `SectorManageImp+0x08` 所持 shared core object 的**第一个 `std::string`**。
- `ReadIIR/sub_180010fc0` 直接读取这个首字符串并作为 arg4 传给 `sub_180009390` decrypt wrapper。
- `WriteIIR/sub_180012100` 和 `BackupIIR` 同样直接读取这个首字符串并作为 key 传给 `sub_1800092c0` encrypt wrapper。
- 在覆盖当前 `SectorManageImp` 业务实现的反编译代码区内，枚举 MSVC `std::string` SSO/heap 分支及随后的 payload 写入后，**只发现 `Init` 这一处直接 core-key payload writer**；公开业务方法列表中也没有 `SetCoreKey/LoadCoreKey` 一类接口。

因此，当前 x64 runtime 存在“另一个显式业务 API 在 ReadIIR 前覆盖 key”的假设已明显变弱。更合理的剩余边界是：物理 Lexar 并非由这一代/这一 profile 的默认 Init key 生产，存在历史 producer/profile，或存在当前 bounded scan 没有覆盖到的间接内存变更。

这仍然**不能**升级为“运行时绝无覆盖”。机器证据 `audit/region_a/evidence/core_key_provenance_20260922.json` 明确保留 indirect mutation 和 historical producer 两个限制。

## 6. IIR plaintext 已知布局

当前字段映射来自 `WriteIIRPar`、`ReadIIR`、`WriteIIR`、`GetHardInfoImp`、`SetPasswd`、`CheckPasswd`、`ChangeDataKey` 等静态 producer/consumer。

主要区间：

- `+0x020..+0x02f`: 四组大端扇区数/容量边界
- `+0x054..+0x093`: 两个 0x20B device/hardware info slots
- `+0x094..+0x193`: 四个 0x40B key/password/data-key related slots
- `+0x194..+0x213`: 两个 0x40B password digest slots
- `+0x214..+0x219`: retry/status bytes
- `+0x21c..+0x2db`: device/flag/extended slots
- `+0x2dc..+0x3da`: 0xffB variable info area
- `+0x3dc..+0x3e3`: node status mirrors
- `+0x3e4..+0x3fb`: short tail metadata
- `+0x3fe..+0x3ff`: inverted CRC16 of `+0x000..+0x3e3`
- `+0x400..+0x425`: 19 inverted per-field CRC16 values
- `+0x426..+0x464`: answer/auth metadata plus three 20B digest slots

完整无重叠状态见 `audit/region_a/plain_byte_ledger.tsv`。

## 7. CRC 规则已进入产品代码

`src/protocol/region_a.rs` 现在实现：

- reflected CRC16, initial 0xffff, polynomial 0xa001
- stored value = bitwise inverted CRC16
- main CRC at +0x3fe over first 0x3e4 bytes
- 19 个 segment CRC definitions
- four big-endian partition-size fields

`tests/region_a.rs` 使用 synthetic plaintext 验证：

- 合法 IIR 的 main + 19 segment CRC 全通过
- data-key slot 单字节篡改会同时击穿对应 segment CRC 和 main CRC
- 非 0x800B plaintext fail closed

代码中的 CRC/字段解析仅是独立 IIR helper。当前故意**没有**提供 Region A decrypt API，因为 Region A 的 producer/consumer 和实际 crypto boundary 都尚未识别。

## 8. 官方 InitIIR runner 当前不能算 producer 正证据

2026-09-22 复跑 `run_init_iir_on_image.py`：

- `write_dev_called = false`
- 出现 Unicorn unmapped write/fetch
- generated ciphertext 全零
- plaintext 全零
- main CRC fail
- segment CRC 0/19

因此历史 runner 目前只能作为 negative harness evidence，不能作为 official virtual producer positive。

证据：

- `audit/region_a/evidence/init_iir_runner_failure_20260922.json`
- `audit/region_a/evidence/init_iir_runner_failure_20260922.txt`

## 9. Region A 字节 COMPLETE 门禁

任意 Region A 字节或连续区间只有同时满足下列条件，才允许从 UNKNOWN 升级：

1. 找到直接以 LBA7 Region A StartSector/0xC00 extent 为目标的 producer 或 consumer，不能靠地址数值相似推定对象相同。
2. 闭环该路径的实际字节范围、结构和读写方向。
3. 若存在加密，必须从该路径闭环算法、key/IV/tweak provenance，并对 physical gold 做解密与重加密 bit-exact 验证。
4. 至少一个真实物理样本提供字段/行为正证据；跨品牌结论必须有相应 profile 证据。
5. 结论进入机器 ledger、测试和人类文档，三者严格一致。

IIR、LBA12 分区挂载、`sectorInfo` 上传链目前都不满足第 1 条，因此不能给 Region A 的任何字节提升状态。

## 10. Region A 整体 100% 门禁

整个 `+0x000..+0xbff` 共 3072B 必须无缝覆盖，并对每个区间闭环 producer、consumer、数据结构、crypto（如有）、physical positive 与行为测试。当前严格状态仍为 **0 COMPLETE / 0 PARTIAL / 3072 UNKNOWN**。

## 11. 下一步研究顺序

1. **Linux 客户端 producer/consumer 筛查（新增，最高优先）**：对 `libedpedisk.so`、`linuxedpedisk`、`cemsudiskcallerproxy`、`checkdiskback`、`EdpEDiskBack`、`libcemsfilesyscheck.so` 按 `0xE0000` 减法序列（`2D 00 00 0E 00` / `81 E9 00 00 0E 00`）、`0xC00`/`1792`/`0x700` 常量和 6 扇区 I/O 模式反汇编搜索；ELF 无驱动签名负担，任一命中即同时给出跨平台 producer/consumer 与确切 key 派生。`EDPSECDISK` 标记命中处优先核对是否为 A6B0 同族 counter-tweak 实现。
2. 复现旧版 `EdpEDiskCtrl` 在新版标签失效时的 type4 登录和 `EdpMountFile` 参数，确认旧驱动是否实际接受并读取 `0xC00` Region A；优先用离线镜像或隔离环境，不修改实盘标签。若挂载成功，观察 3KB 虚拟设备的读 I/O 语义（counter seed、是否从 0 开始），并让 OS 文件系统识别失败/成功本身成为格式正证据。
3. 找 Region A 六扇区的真正 producer，追 `CreatePartitions` 写出 LBA7 指针之后的初始化/恢复流程；仅有旧版条件挂载 consumer 不能解释 payload 来源。追加方向：ydcc 2025 更新链（`audit_ydcc_update_wire`、vupdate metadata crypto）是否携带写入 CHS-0xE0000 的 0xC00 blob。
4. crypto 侧并行穷举仍有界进行：counter seed 取 {0, 绝对 LBA*16, PartitionSize 派生}，key 取 {device_id CRC32（如 lexar `fbeeba6b`）、file_key `1a28e58c...`、LBA12 entry 派生 key}；并用结构 oracle（解后块重复、CRC16/CRC32 自洽扫描、跨盘同位置明文相等）替代 magic 检测，以覆盖"明文本身高熵"的 keybag 假设。
5. 在 `u_disk` 历史版本、日志和 DLL/SYS 中按 `0xC00` 长度、CHS-0x700 地址和 old-format entry 语义交叉搜索 producer/consumer；当前重点转向旧注册/兼容组件，而不是 current `edpediskctrl` 前部 label 路径。
6. IIR 继续保留为独立旁证；三盘 CHS-0x40000 候选窗口已经与 Region A 物理分离，除非获得直接 runtime physical address 绑定，否则不再把 FE06、AES-256-CBC、IIR CRC 或 Init key 用作 Region A 主线。
7. 当前 `cemsusbregsiter!MountEdpPart/EdpMountFile`、`sectorInfo`、current `edpediskctrl!ReadEncryptPartionInfoEx` 已是负证据；旧版 `EdpEDiskCtrl` 的 **LBA12 失败回退**是单独的条件路径，不得与当前正常挂载混同。
