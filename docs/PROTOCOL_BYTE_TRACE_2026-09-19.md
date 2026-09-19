# EDP LBA0–12 逐字节协议追踪总表

> 目的：把 LBA0–12 的每一个已识别字段追到“官方生产者 → 盘面字节 → 官方消费者 → 实盘验证”，并以严格完成率衡量逆向进度。
>
> 本文是长期维护的**主账本**。研究过程、历史误判和更长的证据讨论见
> `docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md`。

## 1. 完成判定：必须四证合一

字段状态只有三种：

- **COMPLETE**：字段边界、producer/生成算法、consumer/行为语义、真实原盘验证全部闭合；存在代际/profile 差异时，差异也必须解释到不会影响字段语义。
- **PARTIAL**：至少一项关键证据缺失。例如只有字段名、只有 producer、只有 consumer、只有样本规律、只有解密公式，均只能算 PARTIAL。
- **UNKNOWN**：尚不能稳定划定语义边界，或只知道“当前样本为零/固定值”。

以下内容**永远不能单独把字段升级为 COMPLETE**：

1. 22/22 样本相同、全零或固定值；
2. DWARF/变量名看起来合理；
3. 可以正确解密；
4. edpcli Provision 可以生成；
5. 只有 writer 没有 reader；
6. 只有 reader 没有 producer；
7. 免密转换结果、自生成盘或实验盘与预期一致。

### 1.1 实盘证据规则

当前原始协议参考集固定为：

- `nopwd_tool/backup` 中 **21 份原始完整备份**；
- 独立目录中的 **1 份 SanDisk 原始加密盘**；
- 合计 **22 份原始参考**。

仓库 `tests/fixtures/protocol` 保留 **7 份非免密裁剪夹具**用于 CI。它们是回归子集，不等于全量逆向样本。

**禁止把免密转换盘、自生成盘、文件名带 `_nopwd` 的夹具、或内容已经呈现免密状态的备份作为“原始 writer 协议”证据。**

## 2. 官方制盘工具链：已经确认

### 2.1 官方 GUI

官方制盘/写标签前端：

`cemssafeudisklabeltool.exe`

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemssafeudisklabeltool.exe.m`

关键证据：

- `sub_42fdb0`，反编译行 **1457–1480**：
  - 加载 `/usbtoolBusManage.dll`；
  - `QLibrary::resolve("CreateBusManageImp")`；
  - 初始化 BusManage 对象。
- Qt 元对象注册：
  - `WriteLabel`：反编译行 **15534–15539**；
  - 写标签页面 `StartSlot()`：反编译行 **13404–13407**；
  - UI 状态文本 `"WriteLabel,Please wait..."`：反编译行 **13396**。
- 服务/业务字段中存在 `labelOnlyId`：反编译行 **7542**。

伪代码：

```text
UsbLabelTool.StartSlot()
    -> show("WriteLabel,Please wait...")
    -> bus = load("usbtoolBusManage.dll").CreateBusManageImp()
    -> bus.WriteNormalULabel(label_request)
```

### 2.2 BusManage 业务层

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/usbtoolbusmanage.dll.m`

`BusManageImp::Init` 的原源码日志位置直接保留在二进制中：

- `busManageImp.cpp:0x214`：加载 `sectorManage.dll`；
- `busManageImp.cpp:0x21A`：加载 `sectorManageHd.dll`；
- `busManageImp.cpp:0x22A`：加载 `SafeUsbRegsiterCems.dll`；
- `busManageImp.cpp:0x23C`：加载 `CEMSUsbRegsiter.dll`；
- 随后 resolve `CreateNormalSectorManageImp`。

`BusManageImp::WriteNormalULabel`：

- 反编译行 **103842–103865**；
- 原源码日志位置 `busManageImp.cpp:0xC4D` 明确写出
  `"UsbToolsRegsiterUsb ERROR_REGSUCESS"`；
- vtable `+0x08` 调用注册；
- vtable `+0x0C` 调用 `ChkRegsiterUsb` 校验。

伪代码：

```text
WriteNormalULabel(request):
    reg = CEMSUsbRegsiter interface
    ret = reg.UsbToolsRegsiterUsb(mapped_request)
    if ret != success:
        return error
    if reg.ChkRegsiterUsb(mapped_request) != valid:
        return verify_error
    return success
```

### 2.3 CEMSUsbRegsiter：真正的 LBA0–12 producer

反编译文件：

`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll.m`

`CUsbRegsiter::RegsiterUsb`：

- 反编译函数 `sub_1003b560`，起始行 **71006**；
- 原源码日志位置 `usbregsiter.cpp:0x915..0xA04`；
- SAFE6 分支中明确调用：
  - `sub_10014550(..., sector4)` → LBA4；
  - `sub_10013fd0(..., sector6)` → LBA6；
  - `sub_100148d0(..., sector8)` → LBA8；
  - `sub_10014720(..., sector11)` → LBA11；
  - 其它 helper 生成剩余标签扇区；
- 最后 `WriteSectorData(..., count=0x0D)` 一次写 **13 个扇区，即 LBA0–12**；
- 写失败日志原源码位置 `usbregsiter.cpp:0x9B3`。

这条调用链是后续字段 producer 追踪的根。

## 3. 严格逐字节进度

> 每个 LBA 固定 512B；总计 13 × 512 = 6656B。
>
> 完成率只统计 COMPLETE，不把 PARTIAL 计入完成。

<!-- STRICT_PROGRESS_BEGIN -->
| LBA | COMPLETE | PARTIAL | UNKNOWN | 严格完成率 |
|---:|---:|---:|---:|---:|
| LBA0 | 66 | 446 | 0 | 12.9% |
| LBA1 | 0 | 0 | 512 | 0.0% |
| LBA2 | 0 | 0 | 512 | 0.0% |
| LBA3 | 0 | 0 | 512 | 0.0% |
| LBA4 | 36 | 39 | 437 | 7.0% |
| LBA5 | 0 | 0 | 512 | 0.0% |
| LBA6 | 36 | 124 | 352 | 7.0% |
| LBA7 | 155 | 51 | 306 | 30.3% |
| LBA8 | 8 | 402 | 102 | 1.6% |
| LBA9 | 32 | 124 | 356 | 6.2% |
| LBA10 | 4 | 36 | 472 | 0.8% |
| LBA11 | 260 | 252 | 0 | 50.8% |
| LBA12 | 368 | 144 | 0 | 71.9% |
<!-- STRICT_PROGRESS_END -->

当前总计：

- **COMPLETE：965B / 6656B = 14.5%**
- **PARTIAL：1618B / 6656B = 24.3%**
- **UNKNOWN：4073B / 6656B = 61.2%**

LBA11 本轮从 8B COMPLETE 提升到 260B COMPLETE。没有因为“能解开第二半扇”就把其余 252B 也冒进标完成：旧 Aigo U335 `rev_pmap` 为什么选择 CHS 容量参与密钥，而其它 21 份使用 DiskSize，上游选择逻辑尚未闭合。

## 4. 字段证据账本

表中 COMPLETE 行必须同时有 producer、consumer、实盘验证。CI 会解析本表，缺任一列即失败。

<!-- FIELD_LEDGER_BEGIN -->
| LBA | 范围 | 状态 | 字段/区域 | Producer 证据 | Consumer 证据 | 实盘验证 | 当前结论 |
|---|---|---|---|---|---|---|---|
| LBA0 | 0x000–0x1BD | PARTIAL | MBR bootstrap | Windows `UsbMainBSec` 静态模板存在 | BIOS/MBR 标准启动语义已知，但 EDP 为什么选该 bootstrap 未闭合 | 22盘均可解析 MBR | 不计完成 |
| LBA0 | 0x1BE–0x1FD | COMPLETE | 4×MBR partition entry | `UsbMainBSec` 模板；SAPF 恢复项也直接写回此处 | `UDiskLabelRepair.dll::Repair0Sector` 直接恢复该 64B 区域 | 22/22 可按标准 MBR 解码 | 分区表边界和消费闭合 |
| LBA0 | 0x1FE–0x1FF | COMPLETE | MBR 55AA | 官方模板直接写 `55 AA` | MBR 校验/修复链检查签名 | 22/22 | 完成 |
| LBA1 | 0x000–0x1FF | UNKNOWN | 未知/当前多为零 | 待查 | 待查 | 当前参考多为零 | 全零不等于完成 |
| LBA2 | 0x000–0x1FF | UNKNOWN | 未知/当前多为零 | 待查 | 待查 | 当前参考多为零 | 全零不等于完成 |
| LBA3 | 0x000–0x1FF | UNKNOWN | 厂商制造标记/空 | 待查 | 待查 | 21零 + 1 Kingston `this is mp mark` | 用途未闭合 |
| LBA4 | 0x000–0x017 | COMPLETE | `$$$onlyid$$$` clear header | 当前注册 writer 根据 main onlyid 格式化 | 识别/解码链从此恢复 onlyid | 22/22 | 完成 |
| LBA4 | 0x018–0x01B | COMPLETE | OnlyIdXor8 | current writer: `main_onlyid ^ 0x88888888` | restore-info 读取该字段 | 22盘 current/legacy 可解 | 完成 |
| LBA4 | 0x01C–0x01F | PARTIAL | OnllyID2Nd | current writer = main onlyid；旧 profile 来源未闭合 | restore-info reader 读取 | 22盘存在 3 类 profile | 旧 profile 未闭合 |
| LBA4 | 0x020–0x033 | PARTIAL | HSerialCRC[5] | current writer 为 0；旧 writer 上游未找到 | restore-info reader 保留此 5×DWORD | 14固定组/6零组/2高熵组 | 不得解释成目标U盘唯一序列 |
| LBA4 | 0x034–0x046 | PARTIAL | restore-info 其余字段 | Windows/Linux 结构边界已恢复 | 部分 reader 已知 | 22盘可解 | 尚有字段缺消费语义 |
| LBA4 | 0x047–0x1FB | UNKNOWN | short/full form 扩展区 | short current writer 不写此区 | full-form 历史消费者未闭合 | current short form 为物理零 | 不能按零认完成 |
| LBA4 | 0x1FC–0x1FF | COMPLETE | trailing LLGB | current writer 继续 rolling key schedule 写 LLGB | reader 作为尾锚点校验 | 22盘可验证 | 完成 |
| LBA5 | 0x000–0x1FF | UNKNOWN | 未知/当前多为零 | 待查 | 待查 | 当前参考多为零 | 全零不等于完成 |
| LBA6 | 0x000–0x03F | PARTIAL | Dept slot | `BuildSector6` 从 UsbWriteParam/UsbLabelParam 写入 | `ReadSector6` 取回 | 多盘真实部门字段可解析 | 上游业务来源明确，所有字节语义仍未逐个闭合 |
| LBA6 | 0x050–0x05F | PARTIAL | User slot | writer 固定槽写入 | reader 取回 | 多盘真实姓名可解析 | 槽边界明确 |
| LBA6 | 0x070–0x077 | PARTIAL | Autonum | writer 写入 | reader/下游部分闭合 | 22盘有多个 profile | 仍 PARTIAL |
| LBA6 | 0x100–0x107 | PARTIAL | device-id CRC材料 | writer 写 CRC32 及派生值 | inspect/reader 可验证 | 22盘可交叉 | 第二DWORD业务语义未闭合 |
| LBA6 | 0x1C0–0x1CF | COMPLETE | m_usbGSerial | Windows/Linux `BuildSector6` 固定复制 15B+NUL | Linux `ReadSector6` 按C字符串匹配 GSerial | 22盘与 LBA8 GLab 前缀交叉 | 完成 |
| LBA6 | 0x1D0–0x1DF | COMPLETE | BeiZhu | Windows/Linux writer 固定复制 15B+NUL | Linux `ReadSector6` 直接读回 BeiZhu | 22盘 | 完成 |
| LBA6 | 0x1E0–0x1EF | PARTIAL | 模板/旧版扩展 | current writer不显式覆盖 | current reader无业务读取 | 20零/2旧版非零 | 旧 profile 未闭合 |
| LBA6 | 0x1F0–0x1F3 | PARTIAL | m_encrypt | 官方 writer 字段名/写入已知 | 最终行为消费者未完全闭合 | 22/22=1 | 固定值不足以完成 |
| LBA6 | 0x1FC–0x1FF | COMPLETE | SAFE6 checksum | writer 对前508B计算 checksum | reader/inspect 校验 | 22/22 校验通过 | 完成 |
| LBA7 | 0x000–0x0BF | PARTIAL | 3×64B EDPF 区 | BuildSector7/注册 writer | 多处 reader/登录/挂载 | 22盘 | 逐字段状态见详细审计 |
| LBA7 | 0x0C0–0x0CD | PARTIAL | pass-info | writer/reader 14B结构已恢复 | 部分字段有行为消费者 | 22盘 LBA7/LBA12 同步 | +0A/+0C/+0D未闭合 |
| LBA7 | 0x0CE–0x1FF | UNKNOWN | 表后区域 | 待查 | 待查 | 多数为固定/零 | 未闭合 |
| LBA8 | 0x000–0x003 | COMPLETE | LLGB magic | Windows/Linux `BuildSector8` | reader 先检查 LLGB | 22/22 | 完成 |
| LBA8 | 0x004–0x007 | COMPLETE | logical length | writer=`0x80+strlen(ELABEL)` | decoder决定动态加密前缀 | 22/22吻合 | 完成 |
| LBA8 | 0x008–0x07F | PARTIAL | LLGB header | writer 多个动态/固定字段 | consumer未逐字段闭合 | 22盘存在稳定结构 | 不计完成 |
| LBA8 | 0x080–logical_end | PARTIAL | 17-key ELABEL | Windows/Linux 同一模板 writer | User/Dept等部分下游已知 | 22/22含17键 | 每个键最终业务消费未全部闭合 |
| LBA8 | tail | UNKNOWN | 物理零区 | writer只写加密前缀 | 无读取语义 | 22盘为零 | 不把 padding 猜成协议字段 |
| LBA9 | 0x000–0x003 | COMPLETE | EETU magic | WriteTempUseInfo | ReadTempUseInfo | 20个非零LBA9样本 | magic完成，payload仍未知 |
| LBA9 | 0x004–0x07F | PARTIAL | EETU payload | writer读改写 | reader存在 | 20/20同形 | `+0x14=FFFFFFFF`语义未闭合 |
| LBA9 | 0x100–0x103 | COMPLETE | SAPF magic | 旧writer恢复模板 | `UDiskLabelRepair::Repair0Sector` | 14样本 | 完成 |
| LBA9 | 0x104–0x113 | COMPLETE | MBR恢复entry | writer保存16B entry | repair直接写回 LBA0 0x1BE | 14/14 | 完成 |
| LBA9 | 0x180–0x183 | COMPLETE | EPPE magic | `SetPassInfoEx` | `ReadPassExInfo` | 6样本 | 完成 |
| LBA9 | 0x184–0x187 | COMPLETE | minimum password length | writer限制6..19 | `ReadMinPassLenInfo` 返回该DWORD | 6/6=8 | 完成 |
| LBA9 | 其余 | UNKNOWN | 未闭合区域 | 待查 | 待查 | 多profile | 未完成 |
| LBA10 | 0x000–0x003 | COMPLETE | EESI magic | Set EESI writer | Get EESI reader | 1个SanDisk样本 | 完成 |
| LBA10 | 0x004–0x027 | PARTIAL | EESI字段/两文本槽 | writer已恢复 | UserLogin读取部分文本 | 1样本 | 最终业务作用未全部闭合 |
| LBA10 | 0x028–0x1FF | UNKNOWN | 保留/其它 | 待查 | 待查 | 当前样本多零 | 未完成 |
| LBA11 | 0x000–0x003 | COMPLETE | DRKB magic | `CDataSecrity::RandBuffer256` 先写 DRKB | `ReadSector11` 首先校验 DRKB | 22/22 | 完成 |
| LBA11 | 0x004–0x0FF | COMPLETE | random252 | `RandBuffer256`: `srand(time(NULL)); rand()%255` 共252B | `DataEncrypt/DataDecrypt` 将整个 DRKB块纳入 CRC32 密钥输入 | 22/22；均无0xFF；7 CI夹具回归 | 每字节都是密钥扰动材料，来源和消费闭合 |
| LBA11 | 0x100–0x103 | COMPLETE | PDKB magic（解密后） | `BuildSector11` 构造 PDKB plaintext | `ReadSector11` 解密后必须校验 PDKB | 22/22 | 完成 |
| LBA11 | 0x104–0x1FF | PARTIAL | 加密的 UID + zero fill | producer: PDKB+4 = `m_strUID`; key=CRC32(DRKB256+VID4+PID4+ullSize8) | consumer: `ReadSector11` 解密并把 PDKB+4 返回 `strDPBack` | 22/22 UID正确；21 DiskSize + 1 CHS | 旧rev_pmap为什么选CHS的上游决策未闭合，因此保守PARTIAL |
| LBA12 | 0x000–0x11F | PARTIAL | 3×96B EDPF | Windows/Linux writer | 登录/挂载/兼容链大量消费 | 22盘 | 字段逐项状态见详细审计 |
| LBA12 | 0x120–0x12D | PARTIAL | pass-info | writer/reader结构闭合 | 部分字段消费闭合 | 22盘 | 仍有+0A/+0C/+0D |
| LBA12 | 0x12E–0x16F | COMPLETE | post-table zero initialized padding | writer整块零初始化且不覆写 | 主reader不消费该区 | 22/22解密为零 | producer+negative consumer+实盘闭合 |
| LBA12 | 0x170–0x1FF | PARTIAL | continuous-cipher zero plaintext tail | 整扇A6B0 writer | 当前主reader无结构消费 | 22/22解密为零 | 密码学边界已知，但历史用途仍保守PARTIAL |
<!-- FIELD_LEDGER_END -->

## 5. LBA11 完整 producer / consumer 追踪

### 5.1 Producer：Linux 官方实现

二进制：

`libcemsfilesyscheck.so`

DWARF 原源码：

- `/mnt/git/cross_platform/src/global/src/diskfile.cpp:783`
  - `CLabelManage::BuildSector11(const char *pVid, const char *pPid, ULONGLONG ullSize, char *buffer)`
- `/mnt/git/cross_platform/src/global/inc/datasecrity.h:25`
  - `CDataSecrity::RandBuffer256`
- `datasecrity.h:41`
  - `CDataSecrity::DataEncrypt`

函数地址：

- `BuildSector11 = 0x1D350..0x1D601`
- `RandBuffer256 = 0x20204..0x202D3`
- `DataEncrypt = 0x202D4..0x20474`

已恢复伪代码：

```text
BuildSector11(pVid, pPid, ullSize, out512):
    drkb = byte[256]
    len = 0x104
    RandBuffer256(drkb, len)
        # 实际返回 len = 0x100
        # drkb[0:4] = "DRKB"
        # srand(time(NULL))
        # for i in 4..255:
        #     drkb[i] = rand() % 255

    pdkb_plain = zero[256]
    pdkb_plain[0:4] = "PDKB"
    memcpy(pdkb_plain+4, m_strUID.c_str(), m_strUID.length())

    key_crc = CRC32(
        drkb[0:256]
        || pVid[0:4]
        || pPid[0:4]
        || little_endian_u64(ullSize)
    )

    pdkb_cipher = Encrypt(key_crc_le32, pdkb_plain[0:256])

    out512[0x000:0x100] = drkb
    out512[0x100:0x200] = pdkb_cipher
```

`CLabelManage` DWARF：

- `m_strUID @ +0x10`；
- 构造函数参数官方名 `pUID`；
- `CDiskReader::InitDisk` 将 `m_strUid @ +0x248` 传给
  `CLabelManage::Init(..., pUID, ...)`。

### 5.2 Consumer：Linux 官方实现

DWARF 原源码：

`/mnt/git/cross_platform/src/global/src/diskfile.cpp:1168`

函数：

`CLabelManage::ReadSector11 = 0x1F220..0x1F424`

伪代码：

```text
ReadSector11(pVid, pPid, ullSize, sector512, outUid):
    drkb = sector512[0x000:0x100]
    cipher = sector512[0x100:0x200]

    if drkb.magic != "DRKB":
        return ERR_DRKB

    key_crc = CRC32(
        drkb
        || pVid[0:4]
        || pPid[0:4]
        || little_endian_u64(ullSize)
    )

    plain = Decrypt(key_crc_le32, cipher)

    if plain.magic != "PDKB":
        return ERR_PDKB

    outUid = C_string(plain + 4)
    return OK
```

### 5.3 22份原始实盘验证

只读重算结果：

- 21/21 原始完整备份均成功恢复 PDKB + 正确 device_id；
- 独立 SanDisk 原始加密盘成功恢复
  `disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00`；
- 总计 **22/22**；
- 21/22 使用 `DiskSize`；
- 1/22（Aigo U335 `rev_pmap`）使用 CHS 取整容量；
- 22/22 第一半扇都是 `DRKB + 252B`，随机区没有出现 `0xFF`，符合 `rand()%255`。

因此：

- LBA11 `0x000..0x0FF` 可以升级为 COMPLETE；
- `0x100..0x103` 的 PDKB magic 可以升级 COMPLETE；
- `0x104..0x1FF` 暂留 PARTIAL，唯一主要缺口是旧 `rev_pmap`
  profile 选择 CHS 容量的上游决策来源。

## 6. 代码与测试门禁

### 6.1 CI 实盘子集

`tests/provision_protocol_audit.rs` 必须持续验证仓库中的真实原盘夹具：

- 只统计非免密夹具；
- LBA11 必须是 DRKB；
- random252 不得出现 writer 不可能产生的 `0xFF`；
- 使用 ASCII VID/PID；
- DiskSize/CHS 至少有一种必须解出 PDKB；
- PDKB+4 必须与备份 device_id 一致；
- 数字 little-endian VID/PID 必须不能误解成功。

### 6.2 文档契约测试

`tests/protocol_documentation_contract.rs` 负责拦截文档口径回退：

1. LBA0–12 必须各自合计 512B；
2. 总字节必须 6656B；
3. COMPLETE 总量不得低于当前基线；
4. UNKNOWN 不得高于当前基线；
5. 每一条 COMPLETE 字段必须填写 producer、consumer、实盘验证；
6. 主文档必须保留官方制盘工具链；
7. 主文档必须明确禁止把“样本全零/只有字段名/能生成”当完成。

## 7. 后续提升顺序

按“最可能把 PARTIAL 转成 COMPLETE”的收益排序：

1. **LBA11**：追 Aigo U335 `rev_pmap` 为何传入 CHS 容量；闭合后可再提升 252B。
2. **LBA7 / LBA12**：逐个追 EDPF 中 Version、NeedDisturb 其它 entry、wrapped key8、pass-info 剩余字段。
3. **LBA4**：继续寻找旧 `HSerialCRC[5]` 的真正 producer。
4. **LBA6**：追 `0x1E0..0x1EF` 两份旧格式非零扩展来源。
5. **LBA8**：为 17-key ELABEL 的每个业务字段找到最终消费者。
6. **LBA9/10**：继续追 EETU、EESI 文本的最终策略作用。
7. **LBA0/1/2/3/5**：从官方 `RegsiterUsb` 的 BuildSafe6Label/模板初始化向前追，避免仅凭全零样本猜用途。

## 8. 操作安全边界

本审计阶段：

- 可以读取本地反编译文件、二进制、历史原始备份；
- 可以修改 edpcli 代码、测试、文档；
- 可以生成内存/文件中的模拟 LBA0–12；
- **禁止对真实物理 raw USB 执行写入**，除非用户再次明确授权。

