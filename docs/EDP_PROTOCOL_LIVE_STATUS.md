# EDP Protocol Live Status

> 这是 LBA0-LBA12 逆向的短状态页；完整证据与推理只维护在
> `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md`，本页不作为第二份协议总文档。

更新时间：2026-09-22

## 严格进度

- COMPLETE：6513 / 6656 B = 97.9%
- PARTIAL：143 B
- UNKNOWN：0 B
- 本轮进入时仓库 HEAD：`63b76da1fed351f1e7bbe45f0f12f5c22765e3dc`
- 中途另一工作流先在 `1c281cb9bf4a9d92f8fc27198120d53db6466da9` 将真实免密 SanDisk
  flag 表示歧义保守降级；本轮随后补齐 v19 historical writer 的 exact-512 virtual
  reconstruction 与 2021 repair 边界证据，LBA4 `+0x046` 重新满足 COMPLETE 门禁。
  LBA4 `0x020..0x033` 已按 caller-owned HSerial identity vector 字段级生命周期闭环。

## 最新结论：LBA3 按 EDP preserve-only 生命周期闭环

- current Windows `RegsiterUsb` 先读 LBA0-LBA12 到同一 staging image、没有 LBA3 builder，最终整段写回；Linux 独立没有 `BuildSector3/ReadSector3`，因此 current EDP 对 LBA3 的规则是 preserve/ignore。
- historical v19.11.4.1 已新增可重放 seek 审计：`scripts/protocol/audit_v19_lba3_preserve.py` 固定 DLL SHA-256，枚举31个 `SetFilePointer` 调用点，可恢复的固定 sector multiplier 精确为 `{1,2,4,6,7,8,12}`，不存在3；SAFE6 `virtual_56` 也不直接调用 generic absolute-seek wrapper。
- 2021 `UDiskLabelRepair.dll` 的 repair/backup 只复制 LBA4-LBA12 共9扇区；现有 edpcli apply 也只写 `0/6/7/9/12`，且 atomic-write 回归已经锁定未列入扇区逐字节不动。
- 两个独立真实非零 LBA3 profile 的动态前40B不同，但共同保留 marker-tail；这证明不能把 LBA3 规定为零或固定模板。Phison F2 INFO 与 host LBA3 的直接等同已被反证，厂商内部 bit 继续保持 opaque。
- 因此 LBA3 512B 现在按 **manufacturer-owned opaque MP metadata / EDP preserve-only sector** 计入 COMPLETE；exact Phison serializer/firmware 私有含义仍是 manufacturer provenance，不再作为 EDP 字节闭环 blocker。

## 最新结论：LBA0 三种 bootstrap profile 已按盘面语义闭环

- 20份 general census 的 LBA0 前400B被严格穷尽为三类：8份显式 `zero[400]`、11份 `UsbMainBSec`、1份 Netac MBR；真实免密 SanDisk 属于 `UsbMainBSec`，不存在第四种 wire profile。
- 从 current `CEMSUsbRegsiter.dll@VA 0x100E7220` 提取的 first-party `UsbMainBSec` 前400B SHA-256=`4eeee8d52f8b58e...`，与对应物理 profile逐字节一致；从 `Netac_USB_API.dll` 1.3.1.16（SHA-256=`b12a249a...`）`VA 0x1014BA58` 提取的模板前400B SHA-256=`00863071fd5db2f4...`，与 Aigo L8302 唯一 Netac profile逐字节一致。两份静态 producer prefix 已提交为 clean-clone fixture。
- current SAFE6 对同一区域显式清零，构成第三个 absent-zero profile；两种非零 profile 都是完整的16-bit MBR bootstrap payload，EDP 对该区域按 bootstrap/opaque 处理。
- historical 上游“为什么选择 UsbMainBSec / Netac / zero”仍是调用链 provenance 问题，但它只选择三个已闭合 producer state之一，不再制造未解释的盘面字节。因此原剩余370B升 COMPLETE，LBA0 达到512/512 COMPLETE。

## 最新结论：LBA4 MyHardinfo / LBA8 host identity 与 UsbOnlyInfo 已闭环

- v19.11.4.1 的 LBA4 SAFE6 writer 与 LBA8 LLGB writer 独立调用 `EDP_DiskNumber`，返回0才 fallback `EDP_DeviceNumber`，分别写 `MyHardinfo@node+0x1D` 与 `HDSerialInfo@+0x14`；current profile 两处都为0。
- strict originals 逐盘 22/22 两副本完全相等；非零集合为 `A017AD78/A68BAE08/8B4613F5/2AB0E33C`，其中 `A68BAE08` 跨 Lexar 与 Aigo 不同目标U盘出现，符合 host identity 而不是目标U盘唯一身份。
- current `EdpEDiskCtrl::UpLoadBackupInfo` 的 JSON 模板直接硬编码 `myHardinfo=0`，historical restore 又只消费 `OnllyID2Nd`，因此上层业务对该 DWORD 是 structural-preserve / semantic-ignore。
- `UsbOnlyInfo[0..15]` 现按 optional compatibility identity text 建模：current producer 写 `"%08x%08x" = main onlyid + 0`；v19 transitional producer 写相同16字符格式、第二DWORD为 host-hardinfo；strict legacy originals 保持16B全零，代表 absent profile。
- registration semantic reader跳过该槽，current/v19 runtime只结构保存且无值相关业务分支；因此三种已观测 profile 的 producer/absent-state、consumer 和 physical evidence 已闭合。最早把该槽留空的 manufacturing EXE 未取得，但不再构成该16B字段语义缺口。MyHardinfo/HDSerialInfo 两处各4B与 UsbOnlyInfo 16B 均为 COMPLETE。

## 最新结论：LBA10 EESI 正向物理 profile 已闭环

- 新增用途受限 physical evidence：`audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin`，完整6656B SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`；它不加入19+1 general census。
- 原始 companion metadata 记录 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`、CRC32=`5088ee37`、size=6656、MD5=`db17edf8246ad55e9800b36701afd8e4`。
- 原采集工具 `make_big_boot.py` SHA-256=`d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd`；其 `read_lba()` 使用 `O_RDONLY/pread`，`--apply` 主流程先 `backup()` 落盘13扇快照，之后才二次确认、unmount，并进入 `O_RDWR/pwrite`。因此该文件是这次操作的写前真实物理状态，不是修改器合成输出。
- 对该完整捕获重放：`CRC32(device_id)=0x5088EE37`，LBA10 前0x80解密为 `EESI`、flag=1、GBK“交换区”、GBK“保密区”、后续88B全零；与独立旧 SanDisk EESI 正例一致。
- `S-EESI-361018` 已经给出 magic、flag 0/1、两个卷标和88B caller-owned extension 的 producer/consumer 生命周期。补上允许的真实正向 physical profile 后，LBA10 `0x000..0x07F` 128B 从 PARTIAL 升为 COMPLETE；general 19+1 census 仍保持 LBA10 20/20 zero，不改变其它字段统计口径。

## 最新结论：LBA4 HSerial / HDSerial 字段级闭环

2020 `busManage.dll` 的 `ReadUsbHserialsInfo` 调用 ABI 已闭合到可复核地址：

- `fcn.10010730` 在 `0x100108C3` 调历史 CEMSUsbRegsiter 接口 `vtable+0x2C`；
- v19.11.4.1 `ISUdiskRegsiterObj` vtable=`0x1019DB54`，`+0x2C` 精确对应
  `virtual_44@0x100054A0`；
- 第一个输出是从 LBA4/两份尾部镜像读取并 rolling decode 得到的完整 0x2F-byte
  restore node；HSerialCRC[5] 位于该 node `+0x08..+0x1B`，属于盘面持久化数据；
- reader 成功后才单独调用 `UsbTools` ordinal3=`EDP_DeviceNumber`、
  ordinal4=`EDP_DiskNumber`，将结果写到另外两个 4B 输出；
- 2020 caller 不消费这两个 scalar 输出，只把 0x2F node 传给 `vtable+0x44`；该槽在
  v19.11.4.1 精确对应 `RestoreRegsiterUsb/virtual_68@0x1000E650`，恢复端只取
  `node+0x04 OnllyID2Nd` 作为恢复密钥。
- 历史写入链已继续闭合：`request+0x150..+0x160 -> object+0x2488..+0x2498 -> node+0x08..+0x1B -> fcn.10006090 -> LBA4`；五个非零 DWORD 在进入 request 之前的生成算法仍未知，但这是 caller value-generation provenance，不是这20B盘面字段的含义或传输缺口。
- `fcn.10008800` 会把 LBA4-LBA12 共9扇区原样备份到 `disk_end-0x80000`；该地址与第三 restore-node reader `fcn.1000DB90` 精确一致。

因此已经**直接排除**“DeviceNumber/HDSerialCRC 单 DWORD 就是 LBA4 HSerialCRC[5] 20B”这一等价关系。字段本身现按 **caller-owned `HSerialCRC[5]` identity vector** 闭合：current zero profile、v19 五DWORD注入 ABI、三镜像 historical reader、value-ignore restore consumer 与 strict/nonzero physical profiles均已闭合；`probe_lba4_v19_writer.py` 又保留真实免密 SanDisk 的非零20B输入并让官方 v19 writer 产生 512/512 bit-exact LBA4。更早 caller 如何计算五个DWORD仍保留为 provenance 研究问题，但不再阻止这20B的 COMPLETE。

## 当前阻塞点与下一步

本机可见的 `/private/tmp/ijinshan_edp` 三件套与已审组件 SHA-256 完全重复：BusManage 仍是 2020 build，CEMSUsbRegsiter 仍是 v19.11.4.1，RegManage 仍是 v20.1.2.2；Spotlight 也未发现更早的 `busmanage.dll/cemsusbregsiter.dll`。更早 caller 的 HSerial 数值生成算法仍可继续追，但已从字节闭环 blocker 降为 provenance 开放问题。

LBA4 `bDataToServer@+0x045` 已按 caller-owned compatibility BYTE 闭环：v19
official writer 注入逻辑值 `0B` 时只改变 wire `+0x45`；strict Aigo 的
`wire=64 7A` 又按正式 rolling 精确解为 `node=0B 00`，reader/restore 无值相关
consumer。当前真正剩余的字节 blocker 只剩 LBA6/LBA9：继续追 join59 同代 writer ABI
与 legacy MBR underlay producer。

### 2026-09-22：v19 LBA6 BeiZhu 32B 槽审计纠错

- 新增 `scripts/protocol/audit_v19_lba6_beizhu_slot.py`，固定 v19.11.4.1 DLL SHA-256，直接锁定 `BuildSector6@0x10006370`、`ReadSector6@0x10006AC0`、共享 `strcpy_s@0x101473FE` 及六个 reader callsite。
- 旧文档“v19 overlay 不触及 `+0x1E0..+0x1ED`”是错误的：v19 writer 在 `0x10006648..0x10006656` 把 `sector+0x1D0` 作为 **cap=32 的 BeiZhu C-string 槽**；reader 对称地从 `+0x1D0` 用同一 cap=32 C-string helper 返回。
- 但这不是“完整32B raw copy”：`strcpy_s` 在首个NUL即停止。真实 writer caller `0x1000B16C..0x1000B226` 先把完整32B arg8局部清零，再复制 `object+0x2620` 的 BeiZhu；pinned `UsbMainBSec@0x101BA790` `+0x1D0..+0x1F3` 也全部为0。因此 v19 对短 BeiZhu 只能生成本14B为0，**不能生成** Aigo/SanDisk 两份 nonzero MBR underlay。
- consumer 侧已进一步闭合：第一组 caller 完全不再读取 arg8；第二组成功路径读取的是另一字符串；第三组唯一后续使用是把 arg8 再以 `strcpy_s(cap=16)` 写入对象，NUL 后 `+0x10..+0x1D` 无比较、分支、哈希或字段提取。
- 因而 LBA6 `+0x1E0..+0x1ED` 的 blocker 现在只剩 **exact earlier legacy MBR-underlay producer/profile-selection**。在该 producer 取得前仍保持 PARTIAL；LBA4 `+0x045` 后续独立闭环使全局严格进度更新为 6513/6656=97.9%，这里仍没有为了完成率降低本14B门槛。

## 最新结论：LBA6/LBA9 join59

- `cems/Edp/fileophook.dll` x86 SHA-256=`db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65`，
  `fileophook64.dll` x64 SHA-256=`93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9`；
  两者均为 2022-12-13 build，版本 `8.1.2211.2811 / 1.0.0.11`，不能冒充2019 writer。
- x86 `fcn.10022F80` 与 x64 `fcn.180026D70` 都固定执行 join59：marker 后先取60B prefix，
  再把 LBA9 continuation 覆盖到 prefix `+0x3B`，没有 current 的 prefix[59] 分支。
- 但两个架构的唯一 live `\\.\\PhysicalDrive%u` raw path——x86 `fcn.10026280` 与
  x64 `fcn.18002B880`——都只以 `GENERIC_READ` 打开盘并调用 `ReadFile`；marker 在两个
  二进制也都只有一个 `cmp` 命中。因此这两份
  CEMS2.0-lineage FileOpHook 是 join59 reader 证据，**不是**缺失的 join59 producer。
- 扩大到 Desktop + `/private/tmp` 的 PE marker 扫描以及 `VRV.zip` 归档检查仍没有得到
  新的 earlier writer；2024 `EdpEDiskCtrl.dll` 新命中仍只是兼容 reader。
- current `cemsudisk.dll` 的真实 join 选择现已精确恢复并由
  `scripts/protocol/audit_join59_selector.py` 固定：marker 后先复制60B inline Dept，
  然后直接测试这60B的最后1B **Dept[59]**；该字节为0时 continuation 写到输出
  `+0x7B`（覆盖 Dept[59]，join59），非0时写到 `+0x7C`（join60）。
- 同一审计证明 current `CEMSUsbRegsiter::BuildSector6` 只有
  `strlen(Dept)>=64` 才进 long 分支，并固定复制60B、从 `Dept[60]` 取 continuation；
  所以 current writer 被直接排除为 join59 producer。这一新证据关闭的是 reader
  profile-selection 规则，不是历史 producer；LBA4 `+0x045` 后续独立闭环后全局严格完成率为6513/6656=97.9%。

所以 LBA6 `+0x03F` 与 LBA9 `+0x080..+0x0FF` 保持 PARTIAL；当前 blocker 已进一步缩成
“取得独立的同代 `safeudisklabeltool/cemsusbregsiter` writer，并直接看到 Dept[59] 被置 NUL、
continuation 从 Dept[59] 开始的 producer/选择条件”。没有该 writer 前不得增加 COMPLETE。

### 2026-09-22：legacy producer 部署边界继续收窄

- 新增 `scripts/protocol/audit_legacy_labeltool_boundary.py`，固定旧
  `cems/Edp/cemsudisk.dll` SHA-256
  `49d9a63c0ddcf9de05add21af2ab01128db29e03729c1cba642067446a431ecd`
  与版本 `8.1.2205.3015`。该控制层同时保留
  `SafeUDiskLabelTool\\cemsSafeUdiskLabelTool.exe`、
  `setLabelSafeUDisk/cleanLabelSafeUDisk/initPwdSafeUDisk` 和
  `safeUdiskRegManage` 标记，说明同代终端把安全U盘注册/变更明确委托给独立
  SafeUDiskLabelTool，而不是由 endpoint FileOpHook reader 承担。
- 这与 2022-12 的 x86/x64 FileOpHook “join59 positive reader + raw PhysicalDrive
  read-only”证据组成同一部署边界：**缺失的 join59 writer 应继续在独立
  SafeUDiskLabelTool/CEMSUsbRegsiter 家族中找，不应再回头把终端 reader 当 producer。**
- 新增 scripts/protocol/audit_ydcc_2025_update_provenance.py，固定本机
  VUpdateReplace.log SHA-256=dfc74465... 与 emsstore_decrypted.db
  SHA-256=10704b28...。数据库 Product_audit 证明 ydcc 2.10.0
  于 **2025-05-13 13:19:15** 首次安装、2026-04-30 更新；UTF-16LE 更新日志又直接
  保留 base\\8.1.2502.2116\\ydcc\\cemsusbregsiter.dll 与 .dll.zip 路径。
  2026 替换流程还把当时运行中的旧 DLL 备份到
  base\\8.1.2604.0917.bk\\ydcc\\cemsusbregsiter.dll 后再替换/清理。
- 因此 missing producer 不再只是“某个未知中间版本”：**2025 的
  8.1.2502.2116 / ydcc 2.10.0 CEMSUsbRegsiter 是已证实存在且随后被删除的精确
  acquisition target**。目前仍未恢复其 DLL bytes/hash，所以该证据只缩小版本窗口，
  不把 LBA6/LBA9 的143B PARTIAL 升级。
- 进一步审计此前未纳入账本的 `VUpdateService.log`（SHA-256=`5ec3b53e...`）：更新服务在 2026-04-30 升级前仍把本机 CEMS/ydcc 本地基线报告为 `8.1.2502.2116`，随后切换服务版本到 `8.1.2604.0917`；同一日志还记录新版 `cemsusbregsiter.dll` 通过版本化 `ydcc/cemsusbregsiter.dll.zip` 单文件下载，并附 CRC/size 元数据。由此可确认 2025 generation 不只是数据库历史行，而是升级前实际运行基线；旧 DLL bytes/hash 仍未恢复，因此仍不提升143B。
- 新增 `scripts/protocol/audit_vupdate_metadata_crypto.py`：固定 `VUpdateReplace.exe` SHA-256=`9b110457...` 与四份本地加密更新元数据，直接复现 `0x4275F0/0x427670` 的 32 轮 TEA-family 解密（delta=`0x7E69AC4E`，sum0=`0xCD3589C0`，K=`27BDB886/F03E934F/993BA3AE/D0AAE945`）。`UpdateProductInfo.xml.data` 解密后明确同时记录 `LocalVersionBase=8.1.2502.2116` 与 `ServiceVersionBase=8.1.2604.0917`；`UpgradeIndex.xml.enc(.base)` 与 `UpdateProductFileRecord.xml.data.bk` 均只保留 2026 新索引，其中新版 `ydcc/cemsusbregsiter.dll.zip` 的 MD5/ZIP CRC/size 为 `2ABA574551E59550B0D4FB50E8BECD27 / F9FE2852 / 533081`。这同时证明 `.base` 不是旧 2025 索引，不能从现存元数据倒推出旧 ZIP 的 CRC/size。
- 新增 `scripts/protocol/audit_legacy_lba6_mbr_underlay.py`：clean-clone 直接固定 Aigo rev_pmap 完整金标（SHA-256=`150d705e...`）与独立 SanDisk 原始 LBA6 fixture（SHA-256=`16dfa230...`），按 SAFE6 rolling 重放 `+0x1DE..+0x1ED` 的完整16B标准 MBR entry。Aigo 的账本14B为 `c1ff07efffff1ca87d0ee3f42700`，SanDisk 为 `c1ff07efffffb28a050e773c4c00`；二者前6B相同、后8B随盘变化，而真实免密 SanDisk 完整金标同一 underlay 为16B全零，证明这里是 profile-dependent 动态分区几何而非固定常量。可选本机 PE 扫描又覆盖 `/Users/zhangyuxi/Desktop/u_disk/VRV/cems` + `/private/tmp` 共1745个 PE，公共前缀 `c1ff07efffff` 为0命中；该负证据只用于继续排除已捕获组件内嵌固定模板，不作为升级门槛。exact historical copy site/profile selector 仍缺，因此14B保持 PARTIAL。
- 新增 `scripts/protocol/audit_netac_mbr_entry_boundary.py`，把此前只闭合 LBA0 bootstrap 的 Netac 1.3.1.16 formatter 继续向内追到 `FormatExA_NetacAPI@0x1000A030 -> fcn.100040A0@0x1000A205 -> fcn.10003880@0x100040EB`。`fcn.10003880` 两条 MBR 初始化分支都会先复制 `0x1014BA58` 的完整512B模板，再明确执行 `memset(output+0x1CE,0,0x30)`；`memset@0x100FE5A0` 的参数/写循环也已直接固定。随后可见动态 patch 只落在第一条 entry 的 `+0x1BE/+0x1C2/+0x1C6/+0x1CA`，而模板自身 `+0x1CE..+0x1FD` 也全零。因此**当前捕获的 Netac MBR 初始化路径被直接排除为 nonzero `+0x1DE` entry3 underlay 的来源**；搜索空间进一步缩为更老 formatter 或 EDP 对既有 MBR entry3 的复制/复用，仍不提升14B。
- LBA6 legacy entry3 的 CHS 也完成字段级解码：Aigo 与 SanDisk 两份 nonzero
  实盘均为 start `C=1023,H=0,S=1`、type `0x07`、end
  `C=1023,H=239,S=63`，后8B又分别与本盘 LBA12 type4 的 StartSector /
  PartionSize/512 精确相等。因此 14B 的**字段含义已经全部确定**；它继续
  PARTIAL 只因为 exact historical snapshot producer/profile selector 尚未取得。

## 最新结论：真实免密 SanDisk 的 LBA4 flag 表示

`tests/protocol_gold_crosscheck.rs` 对当前19份加密金标重算：HSerial=6份零、12份
固定tuple、1份其它非零；Dept=12短/3 join59/4 join60；非零MBR fragment只有1份。
join59样本与非零fragment样本不重合，因此历史writer四类指纹不能作为必须同时命中的
筛选条件；这也不证明它们一定来自不同writer。

真实免密 SanDisk 完整金标 LBA4：main=`794661040`，second=`0x4A32BA39`、HSerial非零，
物理 flags=`00 00`，current official `ReadSector4` rolling-reader view=`D4 D9`；guard、
LLGB、Version=1、sector tuple=`08 04 0C 01` 全部通过。旧 dec 文件的零 flags 来自
逐字节 raw-zero 强制归零的错误 decoder，继续禁止作为独立证据。

本轮新增四条关键闭环：

- v19.11.4.1 `CEMSUsbRegsiter.dll` SHA-256
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814` 的
  `fcn.10006090` 在 rolling 后于 `0x10006249..0x10006257` 同样执行
  `node+0x2D/+0x2E -> physical+0x45/+0x46`。其 SAFE6 node 同时允许从 request/object
  注入 nonzero HSerial，所以 **legacy identity 不能再作为 rolling-wire flag 的判据**；
- `scripts/protocol/probe_lba4_reader.py` 隔离执行 current official
  `ReadSector4@RVA 0x15090`，固定 current DLL hash 与 authentic gold hash，实际执行到
  `0x15295`、返回0、无 emulator exception，复现 wire=`0000` -> reader=`d4d9`。
  onlyid 的 K0=`0xBFED`，两个位置的 rolling key byte正好为 `D4/D9`；
- `scripts/protocol/probe_lba4_v19_writer.py` 固定 v19 DLL 与 authentic gold hash，保留
  second=`0x4A32BA39` 和原20B nonzero HSerial，只把 node flags 设为 `0000`，把所有
  PhysicalDrive I/O 重定向到内存后原生执行 `fcn.10006090@RVA 0x06090`。结果 `ret=1`、
  无 emulator exception、唯一一次 LBA4 write=`offset 2048,size 512`，输出 SHA-256
  `c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8` 与真实 SanDisk
  LBA4 **512/512完全一致**；
- historical `UDiskLabelRepair.dll` 2021-12-08 build，SHA-256
  `f5e6ddbb4e3097c9968296b43627543ecacdc24b174e52f8f049b289d7264efc`：
  `RepairSafe6Label@0x10008B20` / `RewriteSafe6BakLabel@0x100094E0` 只在 LBA4-LBA12
  与 `disk_end-0x80000` 间整块复制9扇区；`fcn.10006DA0` 独立 reader 做完整 rolling、
  copy 0x2F node、只校验 OnlyIdXor8，没有单独 patch flag 的路径。

因此 `inspect` 已删除 `second==main && HSerial==0` 的 representation heuristic：
`decoded` 始终保持官方 reader view，flag 字段同时展示 wire/reader，producer-side 值
必须按 writer provenance 判断；**不会为了 COMPLETE 把 reader 的 D9 清成0**。
`bConnetServer@+0x046` 现按 producer-side dormant-zero compatibility byte 重新闭合：
current Windows/Linux 与 v19.11.4.1 direct producer 都保持 node+0x2E=0；historical
rolling-form reader view 为0；真实免密 wire=00/reader=D9 又已由 v19 official writer
以 producer-side zero 精确重建整扇；跨代 reader/restore 均无值相关业务分支。
物理盘当年的 exact manufacturing EXE 仍未知，但这不再构成本1B语义缺口。

新增纵向实盘约束：同一 Aigo U335 `onlyid=1987718388` 的原始加密备份与后续免密快照
虽然 LBA0/6/7/11/12 已改变，但 LBA4 512/512 完全相同；因此该盘的 `reader=0B 00`、
高熵 HSerial 和 MyHardinfo 都不是免密转换产生。Linux DWARF 同时证明 restore-node 指针
在 `libcemsfilesyscheck.so` 内只进入 BuildSector4/ReadSector4，后者除 OnlyIdXor8 外无字段级
判断；这进一步把 `+0x045` 缺口限定到更老 producer 和其它上层 consumer，状态仍为 PARTIAL。

剩余 blocker 已不再包括 HSerialCRC[5]、LBA3 或 `bDataToServer@+0x045`；只剩 LBA6 `+0x03F/+0x1E0..+0x1ED`、LBA9 `+0x080..+0x0FF` 的既有 PARTIAL 字节。完整证据见主文档第1.3节。

## 固定门禁

任何新增 COMPLETE 必须同时满足现有 source + consumer + physical gold/model 门禁，并通过
`scripts/protocol/audit_baseline.py`、`protocol_byte_ledger`、
`protocol_documentation_contract`、格式检查与 `git diff --check`。推断、命名相似、相邻字段
相关性或单个反编译变量名都不能增加 COMPLETE 数量。
