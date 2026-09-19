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

### 2.4 官方 cross-platform 源码位置索引

Linux `libcemsfilesyscheck.so` 带 DWARF，可恢复原工程文件和行号。后续字段追踪
优先以这些位置作为 producer/reader 的源码锚点，再用 Windows 当前实现和实盘
交叉验证：

| 区域 | Producer | Consumer/reader | 原源码位置 |
|---|---|---|---|
| LBA4 | `CLabelManage::BuildSector4` | `CLabelManage::ReadSector4` | `diskfile.cpp:740 / 956` |
| LBA6 | `CLabelManage::BuildSector6` | `CLabelManage::ReadSector6` | `diskfile.cpp:672 / 1005` |
| LBA7 | `CLabelManage::BuildSector7` | LBA7/EDPF reader链 | `diskfile.cpp:895` |
| LBA8 | `CLabelManage::BuildSector8` | `CLabelManage::ReadSector8` | `diskfile.cpp:805 / 1102,1143` |
| LBA11 | `CLabelManage::BuildSector11` | `CLabelManage::ReadSector11` | `diskfile.cpp:783 / 1168` |
| LBA12 | `CLabelManage::BuildSector12` | `CLabelManage::ReadSector12` | `diskfile.cpp:921 / 1195` |
| LBA11随机源 | `CDataSecrity::RandBuffer256` | 作为 DataEncrypt/Decrypt KDF 输入 | `datasecrity.cpp:14` |
| LBA11加/解密 | `CDataSecrity::DataEncrypt` | `CDataSecrity::DataDecrypt` | `datasecrity.cpp:34 / 61` |

结构定义的主要 DWARF 源位置：

- `tagEdpPartionInfo`：`global/inc/edpdiskglobal.h:76`；
- `tagNewEdpPartionInfo`：`edpdiskglobal.h:101`；
- `tagEdpPartionPassInfo`：`edpdiskglobal.h:151`；
- `UsbLabelParam`：`diskfile.h:125`；
- `UsbWriteParam`：`diskfile.h:142`。

以上“源码位置”来自 DWARF，本仓库没有复制这些第三方源码；文档只记录定位信息、
反编译证据和伪代码。

### 2.5 LBA6 producer/reader 伪代码：为什么不能按固定字符串槽粗暴判完成

Linux producer：

`CLabelManage::BuildSector6(UsbWriteParam&, char*) @ diskfile.cpp:672`

机器码已恢复出的关键行为：

```text
BuildSector6(p, out):
    out = UsbMainBSec template

    if strlen(p.department) <= 63:
        memcpy(out+0x000, p.department_buffer, 0x40)
    else:
        out+0x000 = overflow_marker(0x40245E2A) + first_60_bytes
        write remaining bytes into extension area

    if strlen(p.owner) <= 31:
        memcpy(out+0x050, p.owner_buffer, 0x20)
    else:
        out+0x050 = overflow_marker(0x40245E2A) + first_28_bytes
        write remaining bytes into extension area

    memcpy(out+0x070, p.autoid, 0x10)
    memcpy(out+0x080, p.office, 0x40)
    ...

    tmp16 = zero[16]
    memcpy(tmp16, p.m_usbGSerial, 15)
    memcpy(out+0x1C0, tmp16, 16)

    tmp16 = zero[16]
    memcpy(tmp16, p.BeiZhu, 15)
    memcpy(out+0x1D0, tmp16, 16)

    u32(out+0x1F0) = p.m_encrypt

    rolling_xor(out[0x000..0x1FB])
    checksum = rol(CRC32(out[0x000..0x1FB]), 10)
    u32(out+0x1FC) = checksum
```

Consumer：

`CLabelManage::ReadSector6(char*, UsbLabelParam&) @ diskfile.cpp:1005`

reader 会识别 `0x40245E2A` 溢出 marker，并从扩展位置重建长 Dept/User；
GSerial/BeiZhu 则直接按 C 字符串读回。注意 `UsbLabelParam` **没有**
`m_encrypt` 成员；`m_encrypt` 只存在于 writer 的 `UsbWriteParam+0x258`。

因此：

- GSerial / BeiZhu 的 **C 字符串语义**可以闭合，但固定 16B 物理槽不能整体计
  COMPLETE：writer 固定复制输入对象的前 15B 再补第 16B NUL，consumer 只按
  C 字符串读取；输入对象在 NUL 后的 backing bytes 并没有稳定生成语义；
- Dept/User 虽字段含义明确，但“本槽 + overflow extension”必须作为一个整体继续追，
  不能仅看到 `0x000..0x03F` 或 `0x050..0x06F` 就把整槽算 COMPLETE；
- `m_encrypt @ sector+0x1F0` 当前只有 producer 和 22/22=1，reader 没有对应输出字段，
  仍保持 PARTIAL。

22份原始盘进一步给出了不能把两个 16B 字符串槽整体标 COMPLETE 的直接反例：

- GSerial：16/22 的 C 字符串为 `"322CA28A"`，6/22 为
  `"322CA28A-D7D144"`；前一组 **16/16 都在 NUL 后仍有非零字节**；
- BeiZhu：20/22 为空、2/22 为 GBK `"普通"`；总计 **8/22 在首个 NUL 后仍有
  非零 backing bytes**；
- 两份旧 profile（Aigo U335、SanDisk）还同时在 `+0x1E0..0x1EF`
  留有非零旧扩展，而当前 reader 完全不消费这 16B。

**LBA6 C-string slots keep opaque post-NUL tails**。因此本轮主动回撤此前对
`+0x1C0..0x1DF` 的过度 COMPLETE 认定。这里是
“字符串含义已知 + 固定槽尾未闭合”的 PARTIAL，而不是 32B 完整字段。

#### m_autoid / Autonum：字符串语义闭合，但固定 16B 槽不能整体升级

producer：

```text
UsbWriteParam::UsbWriteParam(UsbLabelParam&):
    strcpy_s(writer.m_autoid /* +0x259 */, 16,
             label.m_autoid  /* +0x258 */)

BuildSector6:
    memcpy(sector6 + 0x70, writer.m_autoid, 16)
```

consumer：

```text
ReadSector6:
    strcpy_s(label.m_autoid /* +0x258 */, 16,
             decoded_sector6 + 0x70)

BuildSector8(label):
    ELABEL += "Autonum=" + label.m_autoid + "||"
```

22 份原始实盘只读交叉：

- **22/22**：LBA6 `0x70` 起的 C 字符串 == LBA8 `Autonum=`；
- 分布：`YD000001` 14份、空串6份、`1` 2份；
- 但固定 16B 槽在第一个 NUL 之后经常保留非零字节，例如
  `YD000001\0\0 73 05 A0 B6 07 EB`；
- 这与 `UsbWriteParam(UsbLabelParam&)` 只用 `strcpy_s` 写 C 字符串、
  随后 `BuildSector6` 却固定 `memcpy 16B` 的实现吻合：NUL 后内容不属于
  `m_autoid` 的字符串语义，可能来自对象旧内容/未定义尾部。

因此这里**不增加 COMPLETE 字节数**。如果以后要把某个固定 offset 升级 COMPLETE，
必须先证明该 offset 在所有相关 profile 中都有确定 producer/consumer 语义，
不能因为字符串本身已闭合就把 NUL 后尾部当作零填充。

## 3. 严格逐字节进度

> 每个 LBA 固定 512B；总计 13 × 512 = 6656B。
>
> 完成率只统计 COMPLETE，不把 PARTIAL 计入完成。

<!-- STRICT_PROGRESS_BEGIN -->
| LBA | COMPLETE | PARTIAL | UNKNOWN | 严格完成率 |
|---:|---:|---:|---:|---:|
| LBA0 | 66 | 446 | 0 | 12.9% |
| LBA1 | 0 | 512 | 0 | 0.0% |
| LBA2 | 0 | 512 | 0 | 0.0% |
| LBA3 | 0 | 512 | 0 | 0.0% |
| LBA4 | 36 | 39 | 437 | 7.0% |
| LBA5 | 512 | 0 | 0 | 100.0% |
| LBA6 | 4 | 156 | 352 | 0.8% |
| LBA7 | 155 | 51 | 306 | 30.3% |
| LBA8 | 86 | 324 | 102 | 16.8% |
| LBA9 | 52 | 104 | 356 | 10.2% |
| LBA10 | 36 | 4 | 472 | 7.0% |
| LBA11 | 260 | 252 | 0 | 50.8% |
| LBA12 | 372 | 140 | 0 | 72.7% |
<!-- STRICT_PROGRESS_END -->

当前总计：

- **COMPLETE：1579B / 6656B = 23.7%**
- **PARTIAL：3052B / 6656B = 45.9%**
- **UNKNOWN：2025B / 6656B = 30.4%**

LBA11 本轮从 8B COMPLETE 提升到 260B COMPLETE。没有因为“能解开第二半扇”就把其余 252B 也冒进标完成：旧 Aigo U335 `rev_pmap` 为什么选择 CHS 容量参与密钥，而其它 21 份使用 DiskSize，上游选择逻辑尚未闭合。

### 3.1 Linux DWARF 原源码索引

`libcemsfilesyscheck.so` 带有可用 DWARF。以下位置不是按函数名猜测，而是
DWARF 中恢复出的原始声明文件/行号与本地函数地址：

| 功能 | 本地地址 | 原始源码位置 |
|---|---:|---|
| `CLabelManage::BuildSector6` | `0x1CAAC` | `/mnt/git/cross_platform/src/global/src/diskfile.cpp:672` |
| `CLabelManage::BuildSector4` | `0x1D08E` | `diskfile.cpp:740` |
| `CLabelManage::BuildSector11` | `0x1D350` | `diskfile.cpp:783` |
| `CLabelManage::BuildSector8` | `0x1D602` | `diskfile.cpp:805` |
| `CLabelManage::BuildSector7` | `0x1DCDA` | `diskfile.cpp:895` |
| `CLabelManage::BuildSector12` | `0x1DEB8` | `diskfile.cpp:921` |
| `CLabelManage::ReadSector4` | `0x1E048` | `diskfile.cpp:956` |
| `CLabelManage::ReadSector6` | `0x1E2CC` | `diskfile.cpp:1005` |
| `CLabelManage::ReadSector8(UsbLabelParam&)` | `0x1E994` | `diskfile.cpp:1102` |
| `CLabelManage::ReadSector8(BYTE*)` | `0x1F0F0` | `diskfile.cpp:1143` |
| `CLabelManage::ReadSector11` | `0x1F220` | `diskfile.cpp:1168` |
| `CLabelManage::ReadSector12` | `0x1F426` | `diskfile.cpp:1195` |
| `CLabelManage` 构造器 | `0x1C538` | `diskfile.cpp:576` |
| `CDataSecrity::RandBuffer256` | `0x20204` | `/mnt/git/cross_platform/src/global/src/datasecrity.cpp:14` |
| `CDataSecrity::DataEncrypt` | `0x202D4` | `datasecrity.cpp:34` |
| `CDataSecrity::DataDecrypt` | `0x20476` | `datasecrity.cpp:61` |

这些路径是二进制编译时记录的原始源码位置；本机没有对应原厂源码正文。
审计把它们作为函数来源定位证据，不把 DWARF 行号误写成“已取得源码”。

## 4. 字段证据账本

表中 COMPLETE 行必须同时有 producer、consumer、实盘验证。CI 会解析本表，缺任一列即失败。

<!-- FIELD_LEDGER_BEGIN -->
| LBA | 范围 | 状态 | 字段/区域 | Producer 证据 | Consumer 证据 | 实盘验证 | 当前结论 |
|---|---|---|---|---|---|---|---|
| LBA0 | 0x000–0x1BD | PARTIAL | MBR bootstrap | Windows `UsbMainBSec` 静态模板存在 | BIOS/MBR 标准启动语义已知，但 EDP 为什么选该 bootstrap 未闭合 | 22盘均可解析 MBR | 不计完成 |
| LBA0 | 0x1BE–0x1FD | COMPLETE | 4×MBR partition entry | `UsbMainBSec` 模板；SAPF 恢复项也直接写回此处 | `UDiskLabelRepair.dll::Repair0Sector` 直接恢复该 64B 区域 | 22/22 可按标准 MBR 解码 | 分区表边界和消费闭合 |
| LBA0 | 0x1FE–0x1FF | COMPLETE | MBR 55AA | 官方模板直接写 `55 AA` | MBR 校验/修复链检查签名 | 22/22 | 完成 |
| LBA1 | 0x000–0x1FF | PARTIAL | optional GPT_Header profile | Linux官方 `CLabelManage::BuildSector1_Gpt@diskfile.cpp:1458` 构造完整512B `GPT_Header`，计算 partition-table CRC 与 header CRC | Windows `IsAllowRegisterCommonLabel/sub_1002ab70` 在 protective MBR 命中后，以 sector_size 跳到 LBA1，检查 `EFI PART` 与 `header_lba@+0x18==1` | 22/22原始 SAFE6 参考整扇全零；缺正向 GPT 实盘 | producer/consumer/结构已知，但当前真实参考未启用 GPT profile，因此不升 COMPLETE |
| LBA2 | 0x000–0x1FF | PARTIAL | optional GPT partition-entry sector | Linux官方 `BuildSector2_Gpt@diskfile.cpp:1493` 生成128B `GPT_Partition` entry（type GUID/partition GUID/start/end/attr/name） | Windows GPT parser 从 `metadata+2*sector_size` 即 LBA2 起，按每扇4个×128B entry解析；注册检查可连续解析多扇 | 22/22原始 SAFE6 参考整扇全零；缺正向 GPT 实盘 | GPT table用途和entry边界已知，但 profile 的实际已注册盘样本缺失，因此保持PARTIAL |
| LBA3 | 0x000–0x1FF | PARTIAL | LBA3 opaque manufacturer/MP sector | Windows `CUsbRegsiter::RegsiterUsb` 先读完整13扇区，SAFE6注册链没有任何 LBA3 builder，最终整段13扇区写回，因此 LBA3 的 EDP producer 行为是 preserve-existing；Linux `libcemsfilesyscheck.so` 符号/实现同样不存在 `BuildSector3` | 当前 Windows 注册/登录/修复组件与 Linux `CLabelManage` 均未找到 `ReadSector3` 或 LBA3 payload 解析路径；这是 EDP 侧“忽略内容”的负证据，不等于已找到厂商固件消费者 | 22份原始参考独立复核：21/22全零；唯一 Kingston 非零盘在 `+0x001=01`、`+0x020..027=b5 7e 9c 45 00 80 00 14`、`+0x1F0..1FF="this is mp mark\\0"` 有内容；同 VID/PID 的另一 Kingston 原盘整扇为零 | 整扇边界和 EDP preserve/ignore 行为已确定，因此从 UNKNOWN 降为 PARTIAL；厂商 MP 工具真正 producer、字段定义及固件侧 consumer 未闭合，禁止升 COMPLETE |
| LBA4 | 0x000–0x017 | COMPLETE | `$$$onlyid$$$` clear header | 当前注册 writer 根据 main onlyid 格式化 | 识别/解码链从此恢复 onlyid | 22/22 | 完成 |
| LBA4 | 0x018–0x01B | COMPLETE | OnlyIdXor8 | current writer: `main_onlyid ^ 0x88888888` | restore-info 读取该字段 | 22盘 current/legacy 可解 | 完成 |
| LBA4 | 0x01C–0x01F | PARTIAL | OnllyID2Nd | current writer = main onlyid；旧 profile 来源未闭合 | restore-info reader 读取 | 22盘存在 3 类 profile | 旧 profile 未闭合 |
| LBA4 | 0x020–0x033 | PARTIAL | HSerialCRC[5] | current writer 为 0；旧 writer 上游未找到 | restore-info reader 保留此 5×DWORD | 14固定组/6零组/2高熵组 | 不得解释成目标U盘唯一序列 |
| LBA4 | 0x034–0x046 | PARTIAL | restore-info 其余字段 | Windows/Linux 结构边界已恢复 | 部分 reader 已知 | 22盘可解 | 尚有字段缺消费语义 |
| LBA4 | 0x047–0x1FB | UNKNOWN | short/full form 扩展区 | short current writer 不写此区 | full-form 历史消费者未闭合 | current short form 为物理零 | 不能按零认完成 |
| LBA4 | 0x1FC–0x1FF | COMPLETE | trailing LLGB | current writer 继续 rolling key schedule 写 LLGB | reader 作为尾锚点校验 | 22盘可验证 | 完成 |
| LBA5 | 0x000–0x1FF | COMPLETE | opaque preserve / write-protection probe scratch sector | `CUsbRegsiter::RegsiterUsb` 先读取既有 LBA0–12；后续 builder 只重建其它明确扇区，LBA5 不被覆盖，最终随13扇区整体写回；即 producer 语义是 preserve existing bytes | 两版 `EdpDiskCtrl` 的唯一 `base+5` raw-sector consumer 都是：读取整扇→原样写回同一扇区→仅检查 `WriteFile` 是否以 `ERROR_WRITE_PROTECT(0x13)` 失败；`UserLogin` 据此进入只读使用状态，完全不解析内容 | 22/22原始参考整扇512B全零，SHA-256均为 `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；7份原始CI夹具继续锁定 | COMPLETE 表示“整区用途和无payload语义闭合”；全零只是当前实盘状态，不是协议规定，非零内容也应原样保留 |
| LBA6 | 0x000–0x03F | PARTIAL | Dept slot | `BuildSector6` 从 UsbWriteParam/UsbLabelParam 写入 | `ReadSector6` 取回 | 多盘真实部门字段可解析 | 上游业务来源明确，所有字节语义仍未逐个闭合 |
| LBA6 | 0x050–0x05F | PARTIAL | User slot | writer 固定槽写入 | reader 取回 | 多盘真实姓名可解析 | 槽边界明确 |
| LBA6 | 0x070–0x07F | PARTIAL | m_autoid / Autonum fixed copy slot | `BuildSector6@diskfile.cpp:672` 固定复制 writer `m_autoid[16]` | `ReadSector6@diskfile.cpp:1005` 以 C 字符串复制到 `UsbLabelParam.m_autoid`；`BuildSector8` 再序列化为 `Autonum=` | 22/22 LBA6 C-string 与 LBA8 Autonum 完全相同；但 NUL 后真实槽尾大量非零 | 字符串语义已闭合，固定槽尾不是协议零 padding，整16B仍不能算 COMPLETE |
| LBA6 | 0x100–0x107 | PARTIAL | device-id CRC材料 | writer 写 CRC32 及派生值 | inspect/reader 可验证 | 22盘可交叉 | 第二DWORD业务语义未闭合 |
| LBA6 | 0x1C0–0x1CF | PARTIAL | m_usbGSerial C-string slot + opaque post-NUL backing bytes | Windows/Linux `BuildSector6` 都先清零临时16B，再固定复制输入对象前15B；输入对象 NUL 后字节可被一并带入 | Linux `ReadSector6` 只按C字符串匹配 GSerial，NUL 后不消费 | 22盘中16份短值 `322CA28A` 全部在NUL后仍有非零字节；6份长值为 `322CA28A-D7D144` | 字符串语义闭合，但物理16B槽尾 producer/consumer 不闭合 |
| LBA6 | 0x1D0–0x1DF | PARTIAL | BeiZhu C-string slot + opaque post-NUL backing bytes | Windows/Linux writer 同样固定复制输入对象前15B再补末字节NUL | Linux `ReadSector6` 只以 C 字符串读回 BeiZhu | 22盘：20空、2份GBK“普通”；8/22首个NUL后仍有非零字节 | 字符串语义闭合，不得把剩余物理字节当 padding/字段 |
| LBA6 | 0x1E0–0x1EF | PARTIAL | current-template zero / legacy opaque extension | current `BuildSector6` 不显式覆盖；Linux `UsbMainBSec@0x22BB40` 模板对应 `+0x1E0..0x1EF` 为16B零 | current `ReadSector6` 无业务读取 | 20/22原始盘为零；2份旧profile非零，Aigo=`c1 ff 07 ef ff ff 1c a8 7d 0e e3 f4 27 00 00 00`，SanDisk=`c1 ff 07 ef ff ff b2 8a 05 0e 77 3c 4c 00 00 00` | current profile边界已知，但旧 producer/consumer 未找到，保持PARTIAL |
| LBA6 | 0x1F0–0x1F3 | PARTIAL | m_encrypt | 官方 writer 字段名/写入已知 | 最终行为消费者未完全闭合 | 22/22=1 | 固定值不足以完成 |
| LBA6 | 0x1FC–0x1FF | COMPLETE | SAFE6 checksum | writer 对前508B计算 checksum | reader/inspect 校验 | 22/22 校验通过 | 完成 |
| LBA7 | 0x000–0x0BF | PARTIAL | 3×64B EDPF 区 | BuildSector7/注册 writer | 多处 reader/登录/挂载 | 22盘 | 逐字段状态见详细审计 |
| LBA7 | 0x0C0–0x0CD | PARTIAL | pass-info | writer/reader 14B结构已恢复 | 部分字段有行为消费者 | 22盘 LBA7/LBA12 同步 | +0A/+0C/+0D未闭合 |
| LBA7 | 0x0CE–0x1FF | UNKNOWN | 表后区域 | 待查 | 待查 | 多数为固定/零 | 未闭合 |
| LBA8 | 0x000–0x003 | COMPLETE | LLGB magic | Windows/Linux `BuildSector8` | reader 先检查 LLGB | 22/22 | 完成 |
| LBA8 | 0x004–0x007 | COMPLETE | logical length | writer=`0x80+strlen(ELABEL)` | decoder决定动态加密前缀 | 22/22吻合 | 完成 |
| LBA8 | 0x008–0x00B | COMPLETE | ToolVersion[4] | Windows `sub_100148d0` 与 Linux `BuildSector8@diskfile.cpp:805` 都写固定字节 `01 00 00 01` | `ReadSector8(UsbLabelParam&)` 的语义 parser 不读取该版本戳；`ReadSector8(BYTE*)` 仅把完整解密扇区原样导出 | 22/22原始盘=`01 00 00 01`；CI原始夹具锁定 | 4B writer、reader行为、实盘一致，无已知 profile 分叉 |
| LBA8 | 0x00C–0x00F | COMPLETE | Labversion | Windows/Linux writer 都固定写 `0x00000222` | 语义 parser 跳过该 DWORD；raw reader 仅原样导出 | 22/22原始盘=`0x222`；CI原始夹具锁定 | 4B 标签版本戳闭合 |
| LBA8 | 0x010–0x013 | COMPLETE | writeTime | Windows writer 调 `GetTickCount()`；Linux `CLabelManage::GetTickCount@0x1FBAA` 用 `clock_gettime(CLOCK_MONOTONIC)` 转为毫秒并截为32位 | 两个官方 reader 都不把该值用于标签解析/准入；raw reader 只导出原值 | 22/22原始盘均非零且跨标签变化；CI夹具保持多值反例 | 不是墙钟时间，而是制标时单调时钟毫秒计数（32位回绕） |
| LBA8 | 0x014–0x03D | PARTIAL | HDSerialInfo / MacInfo[6] / UsbOnlyInfo[32] | 当前 Windows/Linux writer 对 HDSerialInfo/MacInfo 写零初始化值，并以 `%08x%08x` 生成 UsbOnlyInfo；历史盘存在非零/空 profile | 当前语义 parser 不消费这些字段；其它历史消费者未闭合 | 22盘 HDSerialInfo 与 UsbOnlyInfo 存在多 profile；MacInfo 22/22为零 | 已知当前 writer 不足以解释历史值，整体保持PARTIAL |
| LBA8 | 0x03E–0x03F | COMPLETE | ElabOffset | `BuildSector8@diskfile.cpp:805` 写 `0x0080`；官方结构 `tagEdpUsbLableInfo.ElabOffset@edpdiskglobal.h:413` | `ReadSector8@diskfile.cpp:1102` 读取 WORD 并用 `decoded+ElabOffset` 构造 ELABEL 字符串 | 22/22原始盘=0x80，且22/22都指向 `<ELABEL>`；CI真实夹具锁定 | 2B 寻址语义、producer、consumer、实盘全部闭合 |
| LBA8 | 0x040–0x07F | COMPLETE | Reserverd[64] | Windows `sub_100148d0` 与 Linux `BuildSector8` 都先零初始化整个 header，再把未被其它赋值覆盖的 64B 原样复制到该区 | `ReadSector8(UsbLabelParam&)` 直接越过该区定位 `ElabOffset` 指向的 ELABEL；raw reader 仅原样导出，不赋予业务语义 | 22/22原始盘解密后64B全零；CI原始夹具锁定 | 官方结构名、零初始化 producer、negative consumer 和实盘全部闭合为 reserved-zero 区 |
| LBA8 | 0x080–logical_end | PARTIAL | 17-key ELABEL | Windows/Linux 同一模板 writer | User/Dept等部分下游已知 | 22/22含17键 | 每个键最终业务消费未全部闭合 |
| LBA8 | tail | UNKNOWN | 物理零区 | writer只写加密前缀 | 无读取语义 | 22盘为零 | 不把 padding 猜成协议字段 |
| LBA9 | 0x000–0x003 | COMPLETE | EETU magic | `CUsbRegsiter::SetTempUse` 构造 `EETU`；`WriteTempUseInfo` 可运行时回写 | `ReadTempUseInfo` 必须校验 EETU magic | 20个非零LBA9原始样本 | 完成 |
| LBA9 | 0x004–0x00B | COMPLETE | ullBTime | Windows `SetTempUse` 从开始时间字符串解析为64位值；空/短字符串保持0 | Linux `CheckTempUse` 与 `time(NULL)` 比较；非零且 now < ullBTime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 开始时间下界语义闭合，0表示不启用该下界 |
| LBA9 | 0x00C–0x013 | COMPLETE | ullETime | Windows `SetTempUse` 从结束时间字符串解析为64位值；空/短字符串保持0 | `CheckTempUse` 与 `time(NULL)` 比较；非零且 now > ullETime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 结束时间上界语义闭合，0表示不启用该上界 |
| LBA9 | 0x014–0x017 | COMPLETE | useCount | `BusManageImp::WriteNormalULabel` 普通模式从请求 `+0x947` 取次数；特殊 OutManage-off 模式明确写 `0xFFFFFFFF`；`CUsbRegsiter::SetTempUse` 再将 request+0x40 原样写 EETU+0x14 | Linux `CheckTempUse`：`0xFFFFFFFF` 不递减/不回写；0=次数耗尽；其它正值减1并 `WriteTempUseInfo` 回写 | 20/20原始EETU=0xFFFFFFFF；真实CI夹具回归 | 4B 次数控制及无限次数哨兵完全闭合 |
| LBA9 | 0x018–0x07F | PARTIAL | reverse[104] | Windows `SetTempUse` 从 request+0x44 固定复制0x66B，并保留结构剩余字节 | 当前 Linux `CheckTempUse` 不消费该区；其它消费者未闭合 | 20/20原始EETU该区为零 | producer边界已知，但全零不能替代业务语义 |
| LBA9 | 0x100–0x103 | COMPLETE | SAPF magic | 旧writer恢复模板 | `UDiskLabelRepair::Repair0Sector` | 14样本 | 完成 |
| LBA9 | 0x104–0x113 | COMPLETE | MBR恢复entry | writer保存16B entry | repair直接写回 LBA0 0x1BE | 14/14 | 完成 |
| LBA9 | 0x180–0x183 | COMPLETE | EPPE magic | `SetPassInfoEx` | `ReadPassExInfo` | 6样本 | 完成 |
| LBA9 | 0x184–0x187 | COMPLETE | minimum password length | writer限制6..19 | `ReadMinPassLenInfo` 返回该DWORD | 6/6=8 | 完成 |
| LBA9 | 其余 | UNKNOWN | 未闭合区域 | 待查 | 待查 | 多profile | 未完成 |
| LBA10 | 0x000–0x003 | COMPLETE | EESI magic | Set EESI writer | Get EESI reader | 1个SanDisk样本 | 完成 |
| LBA10 | 0x004–0x007 | PARTIAL | EESI +0x04 | writer原样保存API输入DWORD；reader默认=1并返回 | 独立行为消费者仍未闭合 | 唯一启用实盘=1 | 暂不命名具体业务语义 |
| LBA10 | 0x008–0x017 | COMPLETE | Share/type2 volume label | `SetEdpEdiskSetInfo` 原样复制调用者结构前0x80并加密写入；默认 reader 初始化为GBK“交换区” | `CEdpDiskControl::UserLogin` 将该槽赋给本地 string；type2 分支直接把其 `c_str()` 传给 `SetVolumeLabelA` | 22份原始参考中唯一启用EESI的SanDisk实盘为GBK“交换区”；已加入512B原始证据夹具 | 16B字段语义、writer、consumer、实盘闭合 |
| LBA10 | 0x018–0x027 | COMPLETE | Encrypt/type4 volume label | 同上；默认 reader 初始化为GBK“保密区” | `UserLogin` type4 分支直接把该槽对应 string 的 `c_str()` 传给 `SetVolumeLabelA` | 唯一启用SanDisk实盘为GBK“保密区”；另一版 `out_raw_data/EdpEDiskCtrl.dll` 同构复核 | 16B字段完整闭合 |
| LBA10 | 0x028–0x1FF | UNKNOWN | 保留/其它 | 待查 | 待查 | 当前样本多零 | 未完成 |
| LBA11 | 0x000–0x003 | COMPLETE | DRKB magic | `CDataSecrity::RandBuffer256` 先写 DRKB | `ReadSector11` 首先校验 DRKB | 22/22 | 完成 |
| LBA11 | 0x004–0x0FF | COMPLETE | random252 | `RandBuffer256`: `srand(time(NULL)); rand()%255` 共252B | `DataEncrypt/DataDecrypt` 将整个 DRKB块纳入 CRC32 密钥输入 | 22/22；均无0xFF；7 CI夹具回归 | 每字节都是密钥扰动材料，来源和消费闭合 |
| LBA11 | 0x100–0x103 | COMPLETE | PDKB magic（解密后） | `BuildSector11` 构造 PDKB plaintext | `ReadSector11` 解密后必须校验 PDKB | 22/22 | 完成 |
| LBA11 | 0x104–0x1FF | PARTIAL | 加密的 UID + zero fill | producer: PDKB+4 = `m_strUID`; key=CRC32(DRKB256+VID4+PID4+ullSize8) | consumer: `ReadSector11` 解密并把 PDKB+4 返回 `strDPBack` | 22/22 UID正确；21 DiskSize + 1 CHS | 旧rev_pmap为什么选CHS的上游决策未闭合，因此保守PARTIAL |
| LBA12 | 0x000–0x11F | PARTIAL | 3×96B EDPF | Windows/Linux writer | 登录/挂载/兼容链大量消费 | 22盘 | 字段逐项状态见详细审计 |
| LBA12 | 0x010–0x013 | COMPLETE | entry0.NeedDisturb compatibility gate | `CUsbRegsiter::CreatePartitions` 写入 entry0；Linux `edpdiskglobal.h:82` 定义字段 | `vrvaud_c::NewCheckDisTurbUsb(*)` fallback 在 `Format.cpp:0x3CE/0x380` 直接以该 DWORD 非零判 success | 22/22原始盘=1；20个entry0 type1、2个type2；7 CI夹具锁定 | 完成的是 entry0 兼容门控行为；其它 entry 的 NeedDisturb 不随之升级 |
| LBA12 | 0x120–0x12D | PARTIAL | pass-info | writer/reader结构闭合 | 部分字段消费闭合 | 22盘 | 仍有+0A/+0C/+0D |
| LBA12 | 0x12E–0x16F | COMPLETE | post-table zero initialized padding | writer整块零初始化且不覆写 | 主reader不消费该区 | 22/22解密为零 | producer+negative consumer+实盘闭合 |
| LBA12 | 0x170–0x1FF | PARTIAL | continuous-cipher zero plaintext tail | 整扇A6B0 writer | 当前主reader无结构消费 | 22/22解密为零 | 密码学边界已知，但历史用途仍保守PARTIAL |
<!-- FIELD_LEDGER_END -->

### 4.0 LBA3：EDP 只保留的外部制造/MP 扇区

这一扇区此前只因为 21/22 全零、1 份带 `this is mp mark` 而记为 UNKNOWN。
本轮没有沿用旧文档结论，而是重新从官方写链、官方 reader 集合和 22 份原始盘三条线核对。

Windows 当前注册入口
`CUsbRegsiter::RegsiterUsb / sub_1003b560 @ usbregsiter.cpp:0x915..0xA04`
先调用 `ReadSectorData(..., count=0x0D)` 把 LBA0–12 整段读入 staging buffer。
SAFE6 分支随后明确重建 LBA4、LBA6、LBA8、LBA11，并由分区/EDPF helper
处理其它协议扇区；整个函数没有 LBA3 builder。最后仍以同一个 staging buffer
调用 `WriteSectorData(..., count=0x0D)`。所以对 LBA3 而言，官方 EDP 注册 writer
不是“生成零扇区”，而是：

```text
read existing LBA3
    -> no EDP mutation
    -> write the same LBA3 bytes back with the 13-sector batch
```

Linux 当前 `libcemsfilesyscheck.so` 提供
`BuildSector0/4/6/7/8/11/12`、`BuildSector0/1/2_Gpt` 以及
`ReadSector4/6/8/11/12`，独立不存在 `BuildSector3` / `ReadSector3`。
Windows 当前注册、登录和修复组件也未发现 LBA3 payload 解析路径。
因此当前可闭合的是 **EDP preserve + EDP 不解析**；厂商量产工具或控制器固件
是否消费该扇区，仍属于缺失证据。

22 份原始生成参考重新逐字节统计：

- 21/22：LBA3 512B 全零；
- 1/22：Kingston DataTraveler 3.0 非零；
- 该唯一非零扇区并不只是尾部字符串：
  - `+0x001 = 0x01`；
  - `+0x020..+0x027 = b5 7e 9c 45 00 80 00 14`；
  - `+0x1F0..+0x1FF = "this is mp mark\\0"`；
  - 其它字节为零；
- 同 VID/PID 的另一份 Kingston 原始盘 LBA3 仍为全零。

因此不能把 `this is mp mark` 单独建模成 EDP 字段，也不能把 21 个零样本解释成
“协议规定全零”。LBA3 整扇从 UNKNOWN 移到 PARTIAL；在找到厂商 MP producer、
字段格式和实际 consumer 前，512B 中没有任何字节计入 COMPLETE。

### 4.1 LBA8 ElabOffset：2B 完整闭环

官方结构来自 Linux DWARF：

`tagEdpUsbLableInfo @ edpdiskglobal.h:403`

```text
+0x00 Flag
+0x04 cbSize
+0x08 ToolVersion[4]
+0x0C Labversion
+0x10 writeTime
+0x14 HDSerialInfo
+0x18 MacInfo[6]
+0x1E UsbOnlyInfo[32]
+0x3E ElabOffset   // WORD
+0x40 Reserverd[64]
+0x80 UsbLabel / ELABEL storage begins
```

producer：

`CLabelManage::BuildSector8 @ diskfile.cpp:805`

```text
EightSecInfo = zero_initialized()
EightSecInfo.Flag = "LLGB"
EightSecInfo.ElabOffset = 0x80
...
copy ELABEL string to byte[0x80]
```

consumer：

`CLabelManage::ReadSector8 @ diskfile.cpp:1102`

```text
decrypt(sector8)
if header.Flag != "LLGB":
    return error

offset = u16(header + 0x3E)
elabel = C_string(header + offset)
parse ELABEL key/value pairs
```

原始实盘验证：

- 22/22：`ElabOffset == 0x0080`；
- 22/22：`decoded[ElabOffset..]` 以 `<ELABEL>` 开始；
- CI 真实夹具新增同一断言。

因此 `LBA8 +0x3E..+0x3F` 2B 从 PARTIAL 升级 COMPLETE。
其它头字段即便已有官方名称，也不会因为与它相邻而自动升级。

#### LBA8 static version/writeTime/reserved header

本轮继续对同一个 `tagEdpUsbLableInfo` 逐字段追踪，而不是把整个 header 一次性
标成“已知”。Windows `sub_100148d0` 与 Linux
`CLabelManage::BuildSector8@0x1D602` 的实际写序列一致：

```text
+0x08 ToolVersion[4] = 01 00 00 01
+0x0C Labversion     = 0x00000222
+0x10 writeTime      = monotonic milliseconds, low 32 bits
+0x40 Reserverd[64]  = zero-initialized
```

`writeTime` 的来源已独立闭合：

- Windows `sub_10016610` 只有一次 `GetTickCount()` 调用并直接返回；
- Linux `CLabelManage::GetTickCount@0x1FBAA` 调
  `clock_gettime(CLOCK_MONOTONIC)`，计算
  `tv_sec * 1000 + tv_nsec / 1_000_000`，返回低 32 位。

所以它不是 Unix 时间戳，也不是制盘日期，而是**制标进程所在系统自启动后的
单调时钟毫秒值**，发生 32 位回绕是协议允许的自然结果。

consumer 侧也重新核对：

- `ReadSector8(char*, UsbLabelParam&)` 解密后只检查 `LLGB`，
  读取 `+0x3E ElabOffset`，随后从该 offset 解析 ELABEL；
  不读取 ToolVersion、Labversion、writeTime 或 Reserved；
- `ReadSector8(char*, BYTE*)` 在检查 LLGB 后只是把完整 512B 解密结果
  `memcpy` 给调用者，并返回 `+0x04 cbSize`，同样不解释这些字段。

22 份原始参考重新解密统计：

- ToolVersion：22/22 = `01 00 00 01`；
- Labversion：22/22 = `0x222`；
- writeTime：22/22 非零，且跨独立标签存在多值；
- Reserved[64]：22/22 全零。

因此上述 76B 可从 PARTIAL 升 COMPLETE。相邻的
`HDSerialInfo/MacInfo/UsbOnlyInfo` **没有跟着升级**：当前 writer 虽可解释
其当前写法，但 22 盘已经出现历史 profile 差异，尤其
`HDSerialInfo` 非零组和 `UsbOnlyInfo` 空/16位十六进制串并存；旧 producer
尚未闭合。

### 4.2 LBA10 两个 16B 字段：交换区/保密区卷标

旧分析曾把这两个槽解释成“数值/时间戳候选”。重新验证后该解释应废弃。

当前 Windows reader `GetEdpEdiskSetInfo -> sub_1000f930` 在输出结构初始化时：

```text
out + 0x04 = 1
out + 0x08 = "交换区"   // GBK
out + 0x18 = "保密区"   // GBK
```

若 LBA10 存在有效 EESI，则 reader 只解密前 `0x80`，并用实盘结构覆盖这份默认值。

writer `SetEdpEdiskSetInfo -> sub_1000fc70`：

```text
input->magic = "EESI"
plain80 = input[0x00..0x7F]
cipher80 = A6B0(plain80, CRC32(device_id))
read sector10
replace sector10[0x00..0x7F] = cipher80
write sector10
```

因此 `+0x08/+0x18` 的 producer 来源就是 API 调用者提供的两个固定 16B
文本字段，而不是 writer 内部再派生的数据。

更关键的是 `CEdpDiskControl::UserLogin` 的实际汇编数据流：

```text
GetEdpEdiskSetInfo(&info)

shareLabel   = string(info + 0x08)
encryptLabel = string(info + 0x18)

if current_partition.type == 2:
    SetVolumeLabelA(drive, shareLabel.c_str())

if current_partition.type == 4:
    SetVolumeLabelA(drive, encryptLabel.c_str())
```

当前 build 中汇编可直接看到：

- `info+0x08 -> std::string @ ebp-0x74`；
- type2 分支将 `ebp-0x74.c_str()` 传给 `SetVolumeLabelA`；
- `info+0x18 -> std::string @ ebp-0x54`；
- type4 分支将 `ebp-0x54.c_str()` 传给 `SetVolumeLabelA`。

另一版 `out_raw_data/EdpEDiskCtrl.dll` 也存在同构 reader/writer 与
type2/type4 `SetVolumeLabelA` 路径，排除单版本偶然行为。

22份原始参考只读验证：

- 21/22：LBA10 全零，表示 EESI 功能未启用；
- 1/22：独立 SanDisk 原始加密盘存在有效 EESI；
- 该样本：
  - `+0x08..0x17 = "交换区" + NUL/zero fill`；
  - `+0x18..0x27 = "保密区" + NUL/zero fill`；
  - `+0x28..0x7F = 0`，但此事实仍不能把后续区域升级为 padding。

仓库新增原始证据夹具：

`tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba10.bin`

SHA-256：
`240d04e7c97d300c5081f793d72850d49acbf5408bc0d8cf32de8eef7a5e8f02`

因此：

- `LBA10 +0x08..0x17` 16B → COMPLETE；
- `LBA10 +0x18..0x27` 16B → COMPLETE；
- `+0x04..0x07` 仍保持 PARTIAL；
- `+0x28..0x1FF` 仍保持 UNKNOWN。

### 4.3 LBA9 EETU：时间窗口 + 使用次数 20B 完整闭环

旧分析只观察到 `+0x14 = FFFFFFFF`，一度把它当成未知 flag/固定值候选。
重新从 DWARF、Windows producer、Linux consumer 和原始实盘四条线交叉后，
这段已经可以精确命名。

Linux DWARF 官方结构：

`tagEdpEDiskTmpUse @ edpdiskglobal.h:481`

```text
size = 0x80
+0x00 flag       int
+0x04 ullBTime   uint64_t
+0x0C ullETime   uint64_t
+0x14 useCount   uint32_t
+0x18 reverse    char[104]
```

#### Producer：Windows `CUsbRegsiter::SetTempUse`

当前 Windows 二进制的真实机器码（不是反编译器猜测）显示：

```text
request:
    +0x00 begin-time string
    +0x20 end-time string
    +0x40 use count
    +0x44 extra/reverse material

EETU = zero[0x80]
EETU.flag = "EETU"

if begin-time string is present:
    EETU.ullBTime = parse_time(begin)
else:
    EETU.ullBTime = 0

if end-time string is present:
    EETU.ullETime = parse_time(end)
else:
    EETU.ullETime = 0

EETU.useCount = request.useCount
copy(EETU.reverse, request+0x44, 0x66)

encrypt EETU[0x00..0x7F] with device-id CRC key
read-modify-write LBA9 first 0x80
```

日志中保留原源码位置：

- `CUsbRegsiter::SetTempUse`：`usbregsiter.cpp:0x6DF`；
- 时间解析日志：`usbregsiter.cpp:0x702`；
- 写入结束：`usbregsiter.cpp:0x70F`；
- 实际 LBA9 读写 helper 路径：`usbregsiter.cpp:0x735..0x743`。

#### useCount 的上游来源

`BusManageImp::WriteNormalULabel` 的机器码把临时使用本地请求结构的
`+0x40` 明确初始化为：

```text
if OutManage switch is off:
    tempUse.useCount = 0xFFFFFFFF
    begin = default-empty
    end   = default-empty
else:
    tempUse.useCount = request + 0x947
    begin = request + 0x907
    end   = request + 0x927

SetTempUse(&tempUse)
```

另一条 `BusManageImp::GenloginCfg` 路径同样使用
`0xFFFFFFFF` 与正常请求次数二选一，进一步排除偶然常量。

#### Consumer：Linux `CheckTempUse`

Linux 运行时直接消费同一结构：

```text
info = ReadTempUseInfo()
now = time(NULL)

if info.useCount == 0xFFFFFFFF:
    # 无限次数，不递减，也不回写
elif info.useCount == 0:
    return COUNT_EXHAUSTED
else:
    info.useCount -= 1
    WriteTempUseInfo(info)

if info.ullBTime != 0 or info.ullETime != 0:
    if info.ullBTime != 0 and now < info.ullBTime:
        return OUTSIDE_TIME_WINDOW
    if info.ullETime != 0 and now > info.ullETime:
        return OUTSIDE_TIME_WINDOW

return OK
```

因此：

- `ullBTime` 是临时使用开始时间下界；
- `ullETime` 是临时使用结束时间上界；
- 两者都为0时不启用时间限制；
- `useCount=0xFFFFFFFF` 是**无限次数哨兵**；
- `useCount=0` 表示次数耗尽；
- 其它正值每次成功检查都会减1并写回。

#### 22份原始实盘验证

全量只读复核：

- 22份原始参考中，20份存在 EETU，2份 LBA9 该区全零；
- 20/20 EETU：
  - `ullBTime = 0`；
  - `ullETime = 0`；
  - `useCount = 0xFFFFFFFF`；
  - `reverse[104]` 当前实盘均为0。

CI 中的原始完整夹具也有多份 EETU，新增回归会逐盘解密并固定前三项。
`reverse[104]` 即使20/20为零，也**不升级 COMPLETE**：官方 producer 明确允许
从请求复制0x66B附加数据，而当前只缺最终业务 consumer。

本轮因此从 PARTIAL 升级：

- `+0x04..0x0B`：8B；
- `+0x0C..0x13`：8B；
- `+0x14..0x17`：4B；
- 合计 **20B**。

### 4.4 LBA5：512B 整区是 opaque write-protection probe scratch sector

旧分析文档曾把 LBA5 称作“写保护探测牺牲扇区”。这次没有沿用旧结论，
而是从当前/另一版运行时、当前注册 writer 和 22 份原始实盘重新验证。

#### Consumer：两版 EdpDiskCtrl 同构

当前版本：

`/VRV/cems/ydcc/edpediskctrl.dll.m::sub_10012490`

另一版：

`/out_raw_data/EdpEDiskCtrl.dll.m::sub_10015270`

两者逻辑完全同构：

```text
seek((metadata_base + 5) * sector_size)
read(one_sector, scratch)

if read succeeded:
    seek((metadata_base + 5) * sector_size)
    ok = write(one_sector, scratch)   // 写回刚刚读到的完全相同字节
    if !ok && GetLastError() == ERROR_WRITE_PROTECT /* 0x13 */:
        return WRITE_PROTECTED

return NOT_WRITE_PROTECTED
```

当前 `CEdpDiskControl::UserLogin` 的调用语境进一步确认该返回值用途：

```text
if ProbeSector5WriteProtection():
    global_read_only = 1
    log("UDisk Write-protected and can only read-only use!")
else:
    global_read_only = 0
```

也就是说 consumer 只关心“能否把**同一批字节**写回”，从未解析 LBA5
任何 offset、magic、flag 或 checksum。

#### Producer：注册 writer 保留既有 LBA5

`CUsbRegsiter::RegsiterUsb` 的当前 writer 流程是：

```text
buffer = zero[13 * sector_size]
ReadSectorData(existing LBA0..12, buffer)

rebuild known sectors:
    LBA4  <- BuildSector4
    LBA6  <- BuildSector6
    LBA7  <- CreatePartitions / BuildSector7 path
    LBA8  <- BuildSector8
    LBA11 <- BuildSector11
    LBA12 <- CreatePartitions / BuildSector12 path
    ...

# no builder targets base + 5 * sector_size

WriteSectorData(buffer, count=13)
```

额外审计：

- `CreatePartitions` 的真实目标是 `base + 7*sector_size` 与
  `base + 12*sector_size`；
- `BakupUsbSec` 只把当前 metadata 复制到磁盘尾部备份区，不修改内存 LBA5；
- SAFE1 builder 使用独立临时缓冲区，不存在 `base+5` 写入；
- 对当前与另一版 `EdpDiskCtrl` 的 raw-sector 引用搜索，`base+5`
  都只出现于上述“读后原样写回”probe。

因此官方 writer 对 LBA5 的规则不是“必须写零”，而是**preserve existing bytes**。

#### 22份原始实盘

只读全量复核：

- 22/22：LBA5 512B 全零；
- 22/22：整扇 SHA-256 都是
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；
- 但这只证明当前原始参考的实际内容，不把“零”提升为协议固定值。

因此本账本把整个 LBA5 的 **512B** 标为 COMPLETE，含义非常具体：

> LBA5 没有字段级 payload；整扇内容对协议是 opaque bytes，注册流程原样保留，
> 运行时仅把它作为可安全执行“读→同字节写回”的写保护探测 scratch 区。

如果未来发现非零原始 LBA5，这个结论不会失效：只要 writer 仍 preserve、
probe 仍原样写回，非零内容同样符合协议。CI 对当前原始夹具的“全零”断言只用于
防止样本集被悄悄替换，不把全零编码成生成规则。

### 4.5 LBA1/LBA2：官方 GPT profile 已恢复，但当前22盘没有正向样本

旧文档把 LBA1/LBA2 简写成“保留/全零”。重新验证后，这个描述不完整。

Linux 官方库直接保留三套 GPT builder：

```text
CLabelManage::BuildSector0_Gpt @ diskfile.cpp:1445
CLabelManage::BuildSector1_Gpt @ diskfile.cpp:1458
CLabelManage::BuildSector2_Gpt @ diskfile.cpp:1493
```

DWARF 恢复出的核心结构：

```text
GPT_Header       size = 0x200 (512B)
  +0x00 signature[8]       "EFI PART"
  +0x08 version
  +0x0C headersize
  +0x10 headercrc32
  +0x14 reserve
  +0x18 header_lba
  +0x20 backup_lba
  +0x28 pation_first_lba
  +0x30 pation_last_lba
  +0x38 guid[16]
  +0x48 pation_table_first
  +0x50 pation_table_entries
  +0x54 pation_table_size
  +0x58 pation_table_crc
  +0x5C notuse[420]

GPT_Partition    size = 0x80 (128B)
  +0x00 pationtype[16]
  +0x10 pationid[16]
  +0x20 pation_start
  +0x28 pation_end
  +0x30 pation_attr
  +0x38 pation_name[72]
```

`BuildSector1_Gpt` 的机器码明确执行：

```text
header = gpt_header template
header.backup_lba = total_lba - 1
header.pation_last_lba = backup_lba - 0x21
header.pation_table_crc = CRC32(partition_table_bytes)

tmp = header[0x00..0x5B]
tmp.headercrc32 = 0
header.headercrc32 = CRC32(tmp)

copy full 512B header to LBA1 output
```

`BuildSector2_Gpt` 则按128B GPT entry模板写 type GUID、partition GUID、
起止 LBA 等字段。

#### Windows consumer 重新验证

`CUsbRegsiter::IsAllowRegisterCommonLabel -> sub_1002ab70`：

```text
inspect LBA0 protective MBR
if first partition start == 1 and size == 0xFFFFFFFF:
    lba1 = metadata + sector_size
    require lba1[0:8] == "EFI PART"
    require u64(lba1+0x18) == 1
    return GPT
```

随后 GPT 分支把 `metadata + 2*sector_size`，也就是 LBA2，交给
`sub_1002b2f0`。该 parser 的双层循环为：

```text
for sector in 0..sector_count:
    base = lba2 + sector * sector_size
    for entry in 0..4:
        p = base + entry * 0x80
        if type_guid is nonzero/recognized:
            consume pation_start @ +0x20
            consume pation_end   @ +0x28
            consume pation_attr  @ +0x30
```

因此 LBA1/LBA2 不是“永远没用的保留零扇区”，而是存在正式 GPT profile。

#### 实盘限制

22份当前原始 SAFE6 参考复核：

- LBA1：22/22 整扇全零；
- LBA2：22/22 整扇全零；
- 两者 SHA-256 均为零扇区
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`。

这只能说明当前参考盘没有启用 GPT metadata profile，不能反证官方 GPT builder。
由于缺少至少一份**真实、原始、已启用 GPT profile 的实盘**，本账本把：

- LBA1：512B UNKNOWN → **512B PARTIAL**；
- LBA2：512B UNKNOWN → **512B PARTIAL**；
- COMPLETE 不增加。

这也是“旧文档可参考但必须重验”的典型例子：原来的“全零/保留”观察本身没错，
但遗漏了官方可选 profile。

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

### 5.3 Windows 独立 producer / consumer

Windows `cemsusbregsiter.dll` 存在与 Linux 完全独立、但公式一致的实现：

| 角色 | 函数 | 地址 | 反编译位置 |
|---|---|---:|---:|
| LBA11 builder | `sub_10014720` | `0x10014720` | `cemsusbregsiter.dll.m` 约 L78343 |
| DRKB/random producer | `sub_10002B90` | `0x10002B90` | 约 L82081 |
| KDF/encrypt | `sub_10002C30` | `0x10002C30` | 约 L82112 |
| LBA11 reader | `sub_10015F00` | `0x10015F00` | 约 L93391 |

关键证据：

- `sub_10002B90` 固定写 `0x424B5244 == "DRKB"`，随后循环生成
  `rand()%0xFF`；
- `sub_10014720` 构造 `0x424B4450 == "PDKB"`，并把 UID C-string
  写到明文 `+0x04`；
- `sub_10002C30` 把
  `DRKB256 || VID4 || PID4 || size8` 作为 CRC32 输入，再用 CRC
  的 4B little-endian 作为后半 256B 加密 key；
- `sub_10015F00` 先检查 `DRKB`，按同一 KDF 解密后再检查 `PDKB`。

伪代码：

```text
BuildSector11_Windows(vid4, pid4, disk_size, uid):
    rnd[0:4] = "DRKB"
    for i in 4..255:
        rnd[i] = rand() % 255

    plain = zero[256]
    plain[0:4] = "PDKB"
    copy_c_string(plain+4, uid)

    crc = CRC32(rnd || vid4 || pid4 || LE64(disk_size))
    out[0:256] = rnd
    out[256:512] = A7F0(plain, LE32(crc), 0)
```

### 5.4 Windows 当前 writer 的容量来源

`cemsusbregsiter.dll.m::sub_10019780`（约 L83945）直接读取物理盘容量：

```text
GetPhysicalDiskSize(disk_number):
    path = "\\\\.\\PHYSICALDRIVE%d"
    h = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE)
    geometry = DeviceIoControl(
        h,
        IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,   // 0x700A0
        out_size = 0x28
    )
    return geometry.DiskSize                // output +0x18, uint64
```

调用链已连通：

```text
sub_100186C0 enumerate USB interface
    -> sub_10018C40 resolve disk number
    -> sub_10019780(PHYSICALDRIVE#, &DiskSize)
    -> disk-info local +0xB0/+0xB4
    -> sub_10017EB0 copies disk-info object
    -> RegsiterUsb/sub_1003B560
    -> sub_10018480 copies disk-info to local
    -> local var_A8/var_A4
    -> sub_10014720(..., size8, ...)
```

因此**当前 Windows writer 的 LBA11 KDF 容量输入就是物理
`DISK_GEOMETRY_EX.DiskSize`**，不是 CHS 推导值。

历史 profile 仍有一个明确缺口：22 份原始样本中的 Aigo U335
`rev_pmap / onlyid=1987718388` 只有使用

```text
floor(DiskSize / (255*63*512)) * (255*63*512)
```

才能恢复 PDKB。当前 DLL 中存在 `sub_100184C0`，其常量
`0x7D8200 == 255*63*512`，算法形态与 CHS 容量换算一致；但在当前 build
没有发现有效 caller。因此只能证明“历史样本确实使用过 CHS profile”，尚不能证明
旧 writer **何时/为什么**选择它。

### 5.5 22份原始实盘验证

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

## 6. LBA7 / LBA12 EDPF 字段 producer-consumer 图

官方结构定义来自 Linux DWARF：

`/mnt/git/.../global/inc/edpdiskglobal.h:76`

```text
tagEdpPartionInfo
+0x00 Flag
+0x04 Version
+0x08 PartionCount
+0x0C PartionType
+0x10 NeedDisturb
+0x14 NeedEncrypt
+0x18 StartSector        (u64)
+0x20 SectorSize         (u64)
+0x28 PartionSize        (u64)
+0x30 UserKeyCRC
+0x34 FileKeyCRC
+0x38 wrapped key material...
```

Linux producer/reader 原源码位置：

- `CLabelManage::BuildSector7` → `diskfile.cpp:895`；
- `CLabelManage::BuildSector12` → `diskfile.cpp:921`；
- `CLabelManage::ReadSector12` → `diskfile.cpp:1195`。

Windows 当前 producer：

- `cemsusbregsiter.dll::CUsbRegsiter::CreatePartitions`
  （反编译 `sub_1003db50`，本地约 L120709）；
- 日志保留的原源码位置：
  `usbregsiter.cpp:0xE13..0xE15`；
- 该函数按 `0x60` stride 构造 packed LBA12 entry。

核心伪代码：

```text
for each partition:
    entry.Flag          = "EDPF"
    entry.PartionCount  = total_count
    entry.PartionType   = type
    entry.NeedDisturb   = profile value
    entry.NeedEncrypt   = profile value
    entry.StartSector   = calculated_start
    entry.SectorSize    = sector_size
    entry.PartionSize   = calculated_bytes
    entry.UserKeyCRC    = CRC32(password)
    entry.FileKeyCRC    = CRC32(file_key)
    entry.wrappedKey    = wrap(file_key, password, mode)
    entry.EncryptMode   = mode
```

### 6.1 entry0 NeedDisturb：4B 已完整闭合

Producer：

- Windows `CreatePartitions` 生成 packed entry；
- Linux 官方字段名为 `NeedDisturb @ +0x10`。

Consumer：

- `vrvaud_c.m::NewCheckDisTurbUsb`，原日志位置
  `Format.cpp:0x3CE`；
- `ReadPartionInfoExNew` 成功后：

```text
if (packed_entry0.NeedDisturb != 0):
    *is_new_tag_disk = 1
    success = 1
```

- `NewCheckDisTurbUsbEx` 在 `Format.cpp:0x380` 有同一判断；
- Windows 10 另一套 `vrvaud_c` 也独立存在同构判断。

原始实盘只读复核：

- 22/22：`entry0.NeedDisturb == 1`；
- 20/22：entry0 type=1；
- 2/22：entry0 type=2；
- 22/22 pass-info version=`0x0206`。

因此这 **4B** 可升级 COMPLETE。注意完成的是“旧兼容识别路径的非零门控”
这一具体行为，不是对字段名作“扰码/防篡改”等词义扩张；entry1/entry2
仍需分别追 consumer，不能因为同名字段而自动升级。

## 7. 代码与测试门禁

### 7.1 CI 实盘子集

`tests/provision_protocol_audit.rs` 必须持续验证仓库中的真实原盘夹具：

- 只统计非免密夹具；
- LBA11 必须是 DRKB；
- random252 不得出现 writer 不可能产生的 `0xFF`；
- 使用 ASCII VID/PID；
- DiskSize/CHS 至少有一种必须解出 PDKB；
- PDKB+4 必须与备份 device_id 一致；
- 数字 little-endian VID/PID 必须不能误解成功。

### 7.2 文档契约测试

`tests/protocol_documentation_contract.rs` 负责拦截文档口径回退：

1. LBA0–12 必须各自合计 512B；
2. 总字节必须 6656B；
3. COMPLETE 总量不得低于当前基线；
4. UNKNOWN 不得高于当前基线；
5. 每一条 COMPLETE 字段必须填写 producer、consumer、实盘验证；
6. 主文档必须保留官方制盘工具链；
7. 主文档必须明确禁止把“样本全零/只有字段名/能生成”当完成。

## 8. 后续提升顺序

按“最可能把 PARTIAL 转成 COMPLETE”的收益排序：

1. **LBA11**：追 Aigo U335 `rev_pmap` 为何传入 CHS 容量；闭合后可再提升 252B。
2. **LBA7 / LBA12**：逐个追 EDPF 中 Version、NeedDisturb 其它 entry、wrapped key8、pass-info 剩余字段。
3. **LBA4**：继续寻找旧 `HSerialCRC[5]` 的真正 producer。
4. **LBA6**：追 `0x1E0..0x1EF` 两份旧格式非零扩展来源。
5. **LBA8**：为 17-key ELABEL 的每个业务字段找到最终消费者。
6. **LBA9/10**：继续追 EETU `reverse[104]`、EESI `+0x04` 及
   `+0x28..` 未闭合区；EETU 时间/次数控制和两个16B EESI卷标槽已经完成。
7. **LBA0/1/2/3**：继续从官方 `RegsiterUsb` 的模板/读取路径向前追；
   LBA5 的 opaque write-protection probe 用途已经闭合，不再作为未知扇区。

## 9. 操作安全边界

本审计阶段：

- 可以读取本地反编译文件、二进制、历史原始备份；
- 可以修改 edpcli 代码、测试、文档；
- 可以生成内存/文件中的模拟 LBA0–12；
- **禁止对真实物理 raw USB 执行写入**，除非用户再次明确授权。

