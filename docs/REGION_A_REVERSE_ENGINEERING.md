# Region A 逆向工程总文档

本文件是 Region A 的唯一人类可读总入口。目标是把 Region A 推进到与 LBA0-LBA12 相同的工程等级：物理证据、producer/consumer、字节级 ledger、profile/evolution、代码解析器、行为测试和严格完成率必须互相一致。

## 1. 范围和硬约束

Region A 不是 device tail window。它是 LBA7 compact EDPF entry1/entry2 指向的固定 6 扇区区域。

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
| Region A +0x000..+0xbff | 0 | 2048 | 1024 |

含义：

- `+0x000..+0x7ff` 与 `SectorManageImp::ReadIIR/WriteIIR` 的**静态结构关系高度收敛但尚未完成物理绑定**：0x800B 读写长度、AES-256-CBC、device-tree 地址链和真实 Region A ciphertext 均有证据；但仍缺当前 Lexar 控制器 `0x06FE/GetPartInfoAll` 的 `PartInfo[2].sector_num` 实测值，因此保持 PARTIAL，禁止写成“已证明同址”。
- `+0x800..+0xbff` 只有真实物理密文，主 `ReadIIR/WriteIIR` 不覆盖，producer/consumer 未定位，因此 UNKNOWN。

机器 ledger：`audit/region_a/wire_byte_ledger.tsv`。

### 2.2 条件 IIR plaintext 2048B

这里的“plaintext”仅表示：如果获得正确 Region A AES 上下文后，`ReadIIR` 解出的 0x800B 表结构。当前真实 Lexar 尚未获得 CRC 闭合的 plaintext，因此任何字段都不能算 physical COMPLETE。

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
5. `CreatePartitions @ 0x1003de7a` 调用该函数，再用 `sub_10068590` 除以 sector size，把 64 位商写入 EDPF entry `+0x18/+0x1c`；同时把对齐后的 `0xC00` 写入 `+0x28/+0x2c`。
6. `MountEdpPart` 原样复制这些字段到 `m_Partion.StartSector/PartitionSize`，并调用 `EdpEDisk.dll!EdpMountFile`。

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

注意：这里 COMPLETE 的是 **cemsusbregsiter 制盘/注册路径的 Region A 物理定位**；它不自动把 `sectormanage64::ReadIIR` 的 PartInfo 地址链提升为同址 COMPLETE，后者仍按 §4.1 的边界保持 PARTIAL。

## 4. IIR producer / consumer

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

ReadIIR 和 WriteIIR 都使用 device tree node `+0x18` 计算物理位置，并固定处理 0x800B。后 0x400B 尚未由这条主路径解释。

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

这里 COMPLETE 的只是**命令传输和响应解码规则**。真实 Lexar 的 512B `FE 06` DATA-IN 尚未成功捕获，因此 §4.1 的物理同址状态仍保持 PARTIAL。

## 5. AES 算法、默认 Init key 与版本 profile

`sub_1800092c0/sub_180009390` 调用 `sub_18001c560()` 得到 `EVP_CIPHER` descriptor。早期仅依据 OpenSSL 注册字符串曾误判为 AES-192-CBC；2026-09-22 已用 descriptor 本体纠正。

动态路径返回 `0x1801cd190`。该结构的关键字段为：

- NID = 427 (`0x1ab`)
- block size = 16
- key length = 32
- IV length = 16
- flags = `0x1002`

因此 Region A 主 IIR wrapper 实际使用 **AES-256-CBC**。另外，使用完整 32 字节 ASCII key 和 zero IV 的标准 OpenSSL AES-256-CBC 解密，与官方 wrapper 前 2032 字节输出 bit-exact，证明当前 fresh EVP context 中 observed NULL-IV 参数的有效行为就是 zero IV。证据见 `audit/region_a/evidence/cipher_descriptor_aes256_20260922.json`。

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

### 5.2 两类默认 key 都不能解当前真实 Lexar

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

因此**两个已知默认 Init key profile 都被真实物理 ciphertext 明确排除**。这把未解问题进一步收窄为：真实制盘/运行链存在不同的 key provenance、对象状态或尚未定位的 producer profile，而不是 AES mode、IV 或简单 hex 大小写问题。

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

代码当前故意**没有**提供 real Region A decrypt API，因为真实 AES key/context 尚未闭环。

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

## 9. 真实解密 COMPLETE 门禁

真实 Lexar `+0x000..+0x7ff` 只有同时满足下列条件才能从 PARTIAL 升级为 COMPLETE：

1. key/context producer 来源从官方调用链闭合，不能靠猜测、字典扫描或硬编码物理 plaintext。
2. 对 physical gold 解密后，main CRC 通过。
3. 同一 plaintext 的 19 个 segment CRC 全通过。
4. plaintext 的关键容量/状态字段与同一物理盘 LBA7/LBA12/设备几何交叉一致。
5. 使用闭环后的真实官方 AES-256-CBC key/context 重加密后，必须与 physical Region A 前 0x800B bit-exact。
6. 解密实现和静态/动态证据必须进入仓库测试，不依赖聊天记录。

只有满足 1-6，才允许声明“Region A 前 0x800 解密完成”。

## 10. Region A 整体 100% 门禁

即使前 0x800 解密完成，整个 0xC00 仍不能宣布 100%。还必须对 `+0x800..+0xbff` 找到：

- producer
- consumer
- 数据结构/算法
- 至少一个 physical positive
- 行为测试

在此之前后 0x400 始终 UNKNOWN。

## 11. 下一步研究顺序

当前最高优先级已经从“继续猜 key”收敛为物理绑定、producer profile 和尾部用途三条线：

1. 使用已经闭环的 `FE 06 + 512B DATA-IN + AES-256-ECB` 规则，只读捕获当前 Lexar 的真实 PartInfo 响应；直接验证 `PartInfo[2].sector_num == 243624189`。
2. 当前 x64 `Init -> core+0x00 -> ReadIIR/WriteIIR/BackupIIR` 直接数据流已闭环；后续重点转为寻找**历史 producer/profile**，以及有证据时再追间接内存覆盖，而不是继续假设存在未见的 `SetCoreKey` API。
3. 动态追踪真实 Windows 客户端的 `sub_180009390` 调用点仍有价值，用于观测 runtime key buffer 是否确实等于当前 Init default key。
4. 修复官方 InitIIR Unicorn harness，使其真正到达 WriteDev，再用生成 plaintext/ciphertext做 producer round-trip。
5. 独立追 Region A `+0x800..+0xbff` 的 producer/consumer，不把这 0x400B 强行归入主 IIR。

禁止把当前 `8eeaa206...` candidate、LBA7 key8、file_key 或任意扫描结果直接升级为真实 key，除非通过第 9 节全部门禁。
