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

- `+0x000..+0x7ff` 已证明是 `SectorManageImp::ReadIIR/WriteIIR` 固定读写的 IIR 主表密文，producer/consumer、读写长度、AES cipher family 和真实物理 ciphertext 均有证据；但真实设备 AES key/context 尚未闭环，因此保持 PARTIAL。
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

## 5. AES 算法边界

`sub_1800092c0/sub_180009390` 调用 `sub_18001c560()` 得到 cipher descriptor。静态注册表把该 selector 放在 `AES-192-CBC` family：

- `sub_18001c580/sub_18001c560/...` 注册为 AES-192-CBC
- 当前动态 path 返回 descriptor address `0x1801cd190`

2026-09-22 使用 `/opt/homebrew/bin/python3.14` + Unicorn 2.1.4 重新执行官方 wrapper，确认：

- `sub_180009050` 返回候选 0x20B key buffer：`8eeaa2062efa0b371076ebe510d98f18` + 16B zero
- decrypt wrapper 把该 buffer 直接作为 EVP key pointer
- observed EVP init IV 参数为 NULL

注意：NULL IV 参数不能单独证明有效 IV 一定是全零；还需闭合 EVP context 初始化状态。

更重要的是，该候选解真实 Lexar Region A 后：

- main CRC: FAIL
- 19 segment CRC: 0/19 PASS
- all_crc_ok: false

因此该 candidate/context **明确不能标记为真实盘正确解密 key**。

证据：

- `audit/region_a/evidence/derive_iir_key_official_20260922.json`
- `audit/region_a/evidence/decrypt_candidate_20260922.json`
- `audit/region_a/evidence/decrypt_candidate_crc_20260922.json`

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
5. 使用同一官方 AES-192-CBC context 重加密后，必须与 physical Region A 前 0x800B bit-exact。
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

当前最高优先级不是继续暴力扫 key，而是追官方 core-context 的真实初始化来源：

1. `SectorManageImp::Init/sub_18000b520` 到 `core_context + 0x00 string` 的完整数据流。
2. 真实客户端创建 SectorManageImp 时，是否覆盖/替换 `sub_180009050` 的默认 output。
3. ReadIIR 登录/设备打开链里 core context 是否从驱动、配置、设备标识或 session state 更新。
4. 动态追踪 `sub_180009390` 调用点上游，记录真实运行时 key buffer provenance，而不是只在 wrapper 内观察最终 pointer。
5. 修复官方 InitIIR Unicorn harness，使其真正到达 WriteDev，再用生成 plaintext/ciphertext做 producer round-trip。
6. 另开路径追 Region A 后 0x400 的读写者。

禁止把当前 `8eeaa206...` candidate、LBA7 key8、file_key 或任意扫描结果直接升级为真实 key，除非通过第 9 节全部门禁。
