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
| LBA11历史容量兼容 helper | `CDisk::GetWindowsDiskSizeFromLinux` | 当前 build 无静态 caller；仅作为兼容函数存在 | `DiskInterface.cpp:1196` |

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
- **LBA6 m_encrypt current producer !SAFE gate** 已继续追到 `RegsiterUsb` 的
  current Windows 赋值点，而不再只停留在 `BuildSector6` 的落盘动作：
  `sub_100139f0(this+0x2E0 -> temp)` 构造临时 `UsbWriteParam` 后，临时对象基址
  精确为 `ebp-0x3F4`；DWARF 已知 `m_encrypt@UsbWriteParam+0x258`，因此对应
  `ebp-0x19C`。原始 PE 机器码在 `RegsiterUsb@0x1003BA94` 的相等分支把该字节
  写为 `1`，`0x1003BAC7` 的非相等分支写为 `0`；前面的5字节比较目标
  `0x100C9A90` 在 PE 中为 ASCII `!SAFE`。随后 `0x1003BAE2..0x1003BAF5`
  立即把同一临时对象传给 `BuildSector6/sub_10013FD0`，后者再把该 BYTE 扩成
  DWORD 写到 `sector6+0x1F0`。Linux `BuildSector6@diskfile.cpp:672` 独立给出
  同一落盘映射。也就是说 current producer 的 1/0 来源已经闭合，不再是
  “22/22 恰好为1”的样本推断；但 `UsbLabelParam` 没有 `m_encrypt` 成员，当前
  Windows/Linux `ReadSector6` 以及已扫 runtime 组件仍未找到最终行为 consumer，
  所以这4B继续保持 PARTIAL。

22份原始盘进一步给出了不能把两个 16B 字符串槽整体标 COMPLETE 的直接反例：

- GSerial：16/22 的 C 字符串为 `"322CA28A"`，6/22 为
  `"322CA28A-D7D144"`；前一组 **16/16 都在 NUL 后仍有非零字节**；
- BeiZhu：20/22 为空、2/22 为 GBK `"普通"`；总计 **8/22 在首个 NUL 后仍有
  非零 backing bytes**；
- 两份旧 profile（Aigo U335、SanDisk）还同时在 `+0x1E0..0x1EF`
  留有非零材料。本轮已证明它不是独立“扩展字段”，而是旧版 MBR
  partition-table underlay 的幸存片段：`+0x1DE..0x1ED` 原本是第3条
  16B MBR entry，BeiZhu 覆盖其前2B 后，`+0x1E0..0x1ED` 仍保留
  CHS/type/start/count；`+0x1EE..0x1EF` 已进入第4条 entry。

**LBA6 C-string slots have profile-dependent post-NUL backing bytes**。因此本轮主动回撤此前对
`+0x1C0..0x1DF` 的过度 COMPLETE 认定。这里是
“字符串含义已知 + 固定槽尾跨 writer profile 未闭合”的 PARTIAL，而不是
32B 完整字段。current writer 的槽尾来自输入对象 backing bytes；两份 legacy
profile 则能看到被短 C 字符串局部覆盖后的 MBR 几何残留。

#### m_autoid / Autonum：固定 16B 槽已闭合，包括非语义 post-NUL backing

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
- 后续进一步反汇编 `strcpy_s@0x1B9B0` 证明它遇到 NUL 后立即停止，
  **不会清 destination 剩余 capacity**；同时
  `UsbWriteParam(UsbLabelParam&)@0x1C362` 入口没有整体 memset，
  随后 `BuildSector6` 又固定 `memcpy 16B`，因此 NUL 后内容的 producer
  已闭合为 **writer-uninitialized backing**；
- committed originals 还保留了更强反例：同一个空 autoid 字符串至少存在
  2种不同且非零的 post-NUL backing，证明这些字节不是第二个隐藏字段。

`ReadSector6` 只解释首个 NUL 前字符串，整扇 checksum 又保护完整16B物理值。
因此这里现在**增加16B COMPLETE**；COMPLETE 表示“每个字节的存储/消费行为已知”，
并不表示 post-NUL 字节具有固定值。Provision 不需要模拟未初始化内存泄漏。

### LBA4 `+0x45/+0x46`：post-XOR wire flags 已闭合到 producer / reader 分叉

Linux DWARF 给出 restore node 的正式字段名：

```text
tagEdpPartionRestorInfoNode @ edpdiskglobal.h:220, sizeof=0x2F
  +0x2D BYTE bDataToServer
  +0x2E BYTE bConnetServer
```

两套官方 producer 独立证明这两个字段是 **post-XOR 明文覆盖字节**：

- Windows `cemsusbregsiter.dll::sub_10014550`：先把 0x2F node 复制到
  LBA4 `+0x18`，对 `+0x18..+0x1FF` 执行完整 0xF4-word rolling XOR，随后
  明确执行 `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 完成同一 0xF4-word rolling loop 后，
  `0x1D302..0x1D329` 再把 `pSerinfo+0x2D/+0x2E` 原样写回
  `buffer+0x45/+0x46`。

reader 则存在一个必须明确记录的非对称行为：

- Windows `sub_10015090` 对 `+0x18..+0x1FF` 统一 rolling 解码后，直接复制
  `decoded+0x18` 的 0x2F node，只校验 `OnlyIdXor8`；没有把物理
  `+0x45/+0x46` 恢复回来；
- Linux `CLabelManage::ReadSector4@0x1E048, diskfile.cpp:956` 同样在
  `0x1E18C..0x1E1D8` 完整 rolling，再 `memcpy(decoded+0x18, 0x2F)`，随后仅
  比较 `OnlyIdXor8 == onlyid ^ 0x88888888`；也没有 post-decode 修正。

因此必须再按 restore-node profile 区分两层语义：

1. **current SAFE6 producer-side / wire flag value**：current writer 在 rolling 后把
   `node+0x2D/+0x2E` 明文覆盖回物理 `raw[0x45]/raw[0x46]`；
2. **official ReadSector4 transformed bytes**：reader 对物理字节统一执行 rolling，
   本身不会补偿 current writer 的 post-XOR 例外；
3. **legacy restore-node profile**：现有旧实盘的 `+0x45/+0x46` 表现为普通 rolling
   密文，当前未找到对应旧 producer，不能把 current post-XOR 规则反向套用到它们。

严格 22 份 original generation reference set（21份非转换 backup + 独立
SanDisk 原始加密盘）重新逐盘复算：

- **22/22** 的 physical bytes 与 generic rolling-decoded bytes 不相等；
- **6/22 current-style**：同时满足
  `OnllyID2Nd == main onlyid && HSerialCRC[5] == 0`；physical=`00 00`，
  generic reader 输出为6组不同非零值。该组与当前 Windows producer 的
  `memset(node,0,0x2F)` + post-XOR store 精确一致；
- **14/22 legacy**：`OnllyID2Nd != main onlyid && HSerialCRC[5] != 0`，
  physical 为非零字节，generic reader 输出=`00 00`；
- **2/22 legacy 特殊 profile**（Aigo U335 rev_pmap + 独立 SanDisk）：同属
  legacy identity profile，physical 非零，generic reader 输出=`0B 00`。

新增反例进一步证明 **raw-zero/full-rolling 物理表示与 current/legacy identity 不是同一个维度**：
严格原始 Kingston `2026-08-27 17:30:24` 的 restore node 已是 current identity
（`OnllyID2Nd==main onlyid && HSerialCRC[5]==0`），但 `+0x47..0x1FB` 仍为
physical raw-zero；同盘 `17:28:57` 的14扇区只读快照与其逐字节完全一致，排除两次采集
之间临时改写。CI 夹具
`kingston_20260827_current_identity_raw_zero_lba4.bin`（LBA4 单扇区，SHA-256
`85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec`）与
`lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile` 锁定该反例。
因此后续追 raw-zero 历史 producer 时，**禁止**再用 OnllyID2Nd/HSerialCRC 代际形态作为
short/full 表示的选择条件；真实选择条件仍未定位。

这组结果证明此前两个极端模型都不对：既不能把 generic rolling 结果对所有盘都当
server flags 本值，也不能把 physical bytes 对所有盘都当 producer-side flags。
`src/inspect.rs` 现改为 profile-aware：

```text
semantic = rolling_decode(raw[0x18..])
if historical raw-zero gap:
    semantic[0x47..0x1FB] = 0
if semantic.OnllyID2Nd == main_onlyid && semantic.HSerialCRC[5] == 0:
    # current SAFE6 post-XOR wire exception
    semantic[0x45] = raw[0x45]   # bDataToServer
    semantic[0x46] = raw[0x46]   # bConnetServer
else:
    # legacy profile: keep official ReadSector4 rolling view
    keep semantic[0x45..0x47]
```

current profile 的 raw 恢复只代表已闭合的 current producer wire 语义；它没有
伪称官方 `ReadSector4` 会做同样修正。legacy profile 仍使用 reader 的 rolling
结果，因为旧 writer 尚未定位。当前已审 Windows/Linux 上层：

- Windows `ReadRestorInfo/sub_10041290` 只负责读取/重试，不修正这2B；
- `ActiveNormalUDev -> sub_1003CEB0` 不读取 `+0x2D/+0x2E`；
- `GetUpLoadInformation/sub_10039C30` 会把 restore node 暴露给上层，但本 DLL
  内只用身份材料，不读取两个 server flag；
- Linux `libcemsfilesyscheck.so` 除 BuildSector4 的两次 post-XOR store 外，没有
  对 restore-node `+0x2D/+0x2E` 的直接字段访问。

所以这 **2B 仍保持 PARTIAL，不增加 COMPLETE 计数**：current writer/wire 规则已
闭合，legacy reader/实盘边界已闭合，但旧 producer 与最终业务 consumer 仍缺失。

同时，current SAFE6 Provision 已从历史 raw-zero short representation 改为官方
current writer 的 full representation：完整 `+0x18..+0x1FF` rolling，然后再
post-XOR 写回 `+0x45/+0x46`。历史 raw-zero 实盘仅作为兼容读取 profile 保留。

## 3. 严格逐字节进度

> 每个 LBA 固定 512B；总计 13 × 512 = 6656B。
>
> 完成率只统计 COMPLETE，不把 PARTIAL 计入完成。

<!-- STRICT_PROGRESS_BEGIN -->
| LBA | COMPLETE | PARTIAL | UNKNOWN | 严格完成率 |
|---:|---:|---:|---:|---:|
| LBA0 | 104 | 408 | 0 | 20.3% |
| LBA1 | 0 | 512 | 0 | 0.0% |
| LBA2 | 0 | 512 | 0 | 0.0% |
| LBA3 | 0 | 512 | 0 | 0.0% |
| LBA4 | 36 | 476 | 0 | 7.0% |
| LBA5 | 512 | 0 | 0 | 100.0% |
| LBA6 | 419 | 93 | 0 | 81.8% |
| LBA7 | 490 | 22 | 0 | 95.7% |
| LBA8 | 476 | 36 | 0 | 93.0% |
| LBA9 | 276 | 236 | 0 | 53.9% |
| LBA10 | 424 | 88 | 0 | 82.8% |
| LBA11 | 512 | 0 | 0 | 100.0% |
| LBA12 | 442 | 70 | 0 | 86.3% |
<!-- STRICT_PROGRESS_END -->

当前总计：

- **COMPLETE：3691B / 6656B = 55.5%**
- **PARTIAL：2965B / 6656B = 44.5%**
- **UNKNOWN：0B / 6656B = 0.0%**

LBA11 已完整闭合为 512B COMPLETE。此前卡住的后半 252B 不是“某型号盘偶尔使用
CHS”的未知 profile，而是来自另一条官方 writer/reader 路径：正常注册 writer 使用
`DISK_GEOMETRY_EX.DiskSize`；`UDiskLabelRepair` 的 LBA11 检查与重写使用传统
`DISK_GEOMETRY` 计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector`
作为 `ullSize`。同一 Aigo U335 `rev_pmap` 已同时保留 CHS 与 exact DiskSize 两种真实
捕获，分别匹配这两条官方路径。

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
| LBA0 | 0x000–0x18F | PARTIAL | MBR bootstrap body | Windows `UsbMainBSec@0x100E7220` 提供正式 legacy 模板；current `CUsbRegsiter::RegsiterUsb` 在最终13扇区 `WriteSectorData` 前无条件 `memset(LBA0+0x000,0,0x190)`；`UDiskLabelRepair::CLabelRepair::ReCreate0Sector -> sub_10003960` 的 current 重建路径也清零同一0x190B；Aigo L8302 第三 profile 则由 `Netac_USB_API.dll::sub_10003880` 从 `0x1014BA58` 以 `rep movsd, ECX=0x80` 整扇复制512B MBR模板 | legacy 模板自身 BIOS/MBR bootstrap 会执行并消费其中代码/消息；current EDP 将前400B视作可清除 bootstrap。`CUsbRegsiter::UnRegsiterUsb` 有 `UsbMainBSec -> LBA0` 直接写回。主制标链已进一步恢复为 `BusManageImp::WriteLabelImp -> SafeUsbRegsiterCems.dll!GetUsbTegsiterObj -> CCEMSSafeUsbRegsiter::UsbFormat`；该 `UsbFormat` 会先按目标盘 `Costom\\sectorManage.dll` / `Costom\\usb20dll.dll` 状态和设备信息命令 `0x52` 的 `0xA2` 返回值决定是否执行密码/IIR预处理，但没有直接调用 Netac Format。随盘 `usb20dll.dll!_IF_DiskFormat` 则明确下钻到 `NewUsb20.dll!FormatExA_NetacAPI`，当前官方包未找到主制标链对 `IF_DiskFormat` 的普通符号导入/动态名称引用。因此 historical 已注册盘为何保留 legacy 模板、以及何种上层流程实际选择 Netac Format 仍未闭合 | 21份非转换 backup 的前400B只有标准 `UsbMainBSec`、current zero[400]、Aigo Netac 三类；`UsbMainBSec` 前400B SHA-256=`4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed`；Aigo/Netac 前400B SHA-256=`00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec`；curated 原盘夹具锁定前两类，Aigo 原盘与 Netac 模板逐字节一致 | 三种主体 profile 的物理边界/部分 producer 已知，但 historical profile-selection 仍缺，因此400B继续 PARTIAL；尤其不能把存在的 `IF_DiskFormat -> FormatExA_NetacAPI` 导出链误当成已证实的 current WriteLabel 调用链 |
| LBA0 | 0x190–0x19F | COMPLETE | cross-profile unowned preserve / historical-zero compatibility region | current SAFE6 `RegsiterUsb` 最终只清 `+0x000..+0x18F`，因此本16B保持 pre-read backing；Linux `BuildSector0@diskfile.cpp:625` 只重建 `+0x1BE` MBR entry，不写本区；legacy `UsbMainBSec` 本16B固定为零；Netac `sub_10003880` 整扇复制的 Aigo producer 模板在本16B同样为零 | 16-bit `UsbMainBSec` bootstrap 的直接数据引用落在 `+0x1B5/+0x1B6/+0x1B7` 和分区表/签名，不读取本区；current 注册/准入与 `UDiskLabelRepair::ReCreate0Sector/sub_10003960` 均不解释本16B，repair 新建只清前0x190和分区表，保持本区 unowned | 严格22份原始参考本16B 22/22 全零；扩展 `nopwd_tool/backup + utils/backup` 共57份完整历史快照也 57/57 全零；既有 `lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix` 门禁锁定 committed originals | COMPLETE 表示跨已知 profile 的**无业务 payload / preserve-existing**生命周期闭合，而不是规定未来盘面必须为零；遇到未知非零值应兼容保留 |
| LBA0 | 0x1A0–0x1A3 | PARTIAL | SAFE1 / legacy `SectorSize` compatibility slot | Windows `BuildSector0/sub_10013F10` 在 **SAFE1** 分支明确写 `m_nSectorSize` 到 `+0x1A0`；`UsbMainBSec` 静态模板也固定为512。Linux `CLabelManage::BuildSector0@diskfile.cpp:625` 只用 `m_nSectorSize` 计算 partition sector count，**不把它写入 +0x1A0**；current SAFE6 主链也不调用 `sub_10013F10`，只 preserve 该槽 | 已反汇编的 legacy 16-bit MBR bootstrap不读取该槽；current SAFE6 注册/repair同样未找到值相关读取 | 严格22份原始盘存在0/512双 profile（其中4份为512）；扩展57份历史完整快照为34×512、23×0，证明它不是 current/legacy 简单二分 | 字段来源已收窄为 SAFE1/legacy compatibility metadata，但 SAFE6 历史 0/512 profile selection 与独立业务 consumer 仍缺，继续 PARTIAL |
| LBA0 | 0x1A4–0x1B4 | COMPLETE | cross-profile unowned preserve / historical-zero compatibility region | 与 `+0x190..+0x19F` 相同：current SAFE6 不覆盖、Linux BuildSector0 不写；legacy `UsbMainBSec` 与 Aigo/Netac整扇模板均在本17B生成零 | 16-bit MBR bootstrap 不读取本区；current EDP 准入、partition repair 与 `ReCreate0Sector` 都只处理其它明确区域，不赋予本17B语义 | 严格22份 22/22 全零；扩展57份完整历史快照 57/57 全零；committed original 门禁已有精确零断言 | 17B 的跨 profile unowned/preserve、negative consumer 与真实盘已闭合；未来非零兼容值必须 preserve，不得机械清零 |
| LBA0 | 0x1B5–0x1B7 | COMPLETE | **LBA0 legacy MBR message-pointer bytes** | 官方 `UsbMainBSec@0x100E7220` 固定为 `2C 44 63`；`sub_10013FD0`/旧模板写路径整扇复制该模板 | 模板先把 `+0x1B..` 搬到 `0x061B` 后执行；runtime `mov al,[0x07B5/0x07B6/0x07B7]` 分别组成 `SI=0x072C/0x0744/0x0763`，指向原模板 `+0x12C/+0x144/+0x163` 三条错误消息 | 22盘严格统计：14/22=`2C 44 63`，8/22=`00 00 00`，无第三种值；CI夹具同时保留两种 profile | 三字节是 legacy MBR 错误消息指针低字节；零态表示该 legacy tail 未存在/已清空，不再当“未知随机尾巴” |
| LBA0 | 0x1B8–0x1BB | PARTIAL | standard MBR disk signature | `CreateDiskMbr` 取 `GetSystemTimePreciseAsFileTime`（fallback `GetSystemTimeAsFileTime`）→ FILETIME 转 Unix seconds → 低32位填 `CREATE_DISK_MBR.Signature` → `IOCTL_DISK_CREATE_DISK` | Windows drive-layout API 会把它作为 MBR Signature 报告；当前已审 EDP `0x70050` 调用均未发现业务逻辑读取该值 | 22/22非零，19个值；同一 onlyid 的重复备份保持不变，按LE解释与历史初始化日期吻合 | 标准字段语义与 producer 已知，但未找到 EDP 自身语义 consumer，因此按项目严格口径继续 PARTIAL |
| LBA0 | 0x1BC–0x1BD | COMPLETE | standard MBR reserved / unowned compatibility word | current SAFE6 `RegsiterUsb` 只清 `+0x000..+0x18F` 并在 `+0x1BE` 起重建分区表，因此这2B保持 pre-read backing；Linux `BuildSector0@diskfile.cpp:625` 同样不写本槽；legacy `UsbMainBSec` 与 Aigo/Netac 整扇模板都在此显式携带 `00 00` | 16-bit `UsbMainBSec` bootstrap 的已确认尾部引用不包含 `+0x1BC/+0x1BD`；current 注册/准入、`UDiskLabelRepair::ReCreate0Sector` 与已审 `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 路径均不把这2B作为业务字段读取，分区语义从 `+0x1BE` 开始 | 严格22份原始参考 22/22=`00 00`；扩展 `nopwd_tool/backup + utils/backup` 的57份完整历史快照同样 57/57=`00 00`，且相邻 disk signature 明确多值，排除“整段尾部碰巧固定”的误判 | 2B 的跨已知 profile producer/preserve 生命周期、negative semantic consumer 与实盘均闭合；COMPLETE 表示该 reserved word 当前无业务 payload，未来未知非零值应兼容 preserve，不得机械清零 |
| LBA0 | 0x1BE–0x1FD | COMPLETE | 4×MBR partition entry | `UsbMainBSec` 模板；SAPF 恢复项也直接写回此处 | `UDiskLabelRepair.dll::Repair0Sector` 直接恢复该 64B 区域 | 22/22 可按标准 MBR 解码 | 分区表边界和消费闭合 |
| LBA0 | 0x1FE–0x1FF | COMPLETE | MBR 55AA | 官方模板直接写 `55 AA` | MBR 校验/修复链检查签名 | 22/22 | 完成 |
| LBA1 | 0x000–0x1FF | PARTIAL | optional GPT_Header profile | Linux官方 `CLabelManage::BuildSector1_Gpt@diskfile.cpp:1458` 构造完整512B `GPT_Header`，计算 partition-table CRC 与 header CRC | Windows `IsAllowRegisterCommonLabel/sub_1002ab70` 在 protective MBR 命中后，以 sector_size 跳到 LBA1，检查 `EFI PART` 与 `header_lba@+0x18==1` | 22/22原始 SAFE6 参考整扇全零；另只读扫描本地 `u_disk` 下3896个大于13扇、低于1GiB的候选文件，LBA1 `+0x00` 均无 `EFI PART`，仍缺正向 GPT 实盘 | producer/consumer/结构已知，但当前真实参考与扩展本地捕获均未启用 GPT profile，因此不升 COMPLETE |
| LBA2 | 0x000–0x1FF | PARTIAL | optional GPT partition-entry sector | Linux官方 `BuildSector2_Gpt@diskfile.cpp:1493` 生成128B `GPT_Partition` entry（type GUID/partition GUID/start/end/attr/name） | Windows GPT parser 从 `metadata+2*sector_size` 即 LBA2 起，按每扇4个×128B entry解析；注册检查可连续解析多扇 | 22/22原始 SAFE6 参考整扇全零；上述3896个扩展候选没有任何 LBA1 GPT header，故也没有可采信的对应 LBA2 正例 | GPT table用途和entry边界已知，但 profile 的实际已注册盘样本缺失，因此保持PARTIAL |
| LBA3 | 0x000–0x1FF | PARTIAL | **Phison MP/FW manufacturing metadata sector, EDP-opaque** | Windows `CUsbRegsiter::RegsiterUsb` 先读完整13扇区，SAFE6注册链没有任何 LBA3 builder，最终整段13扇区写回，因此 EDP producer 行为是 preserve-existing；Linux `libcemsfilesyscheck.so` 同样不存在 `BuildSector3`。外部独立资料把尾部 `"this is mp mark"` 收敛到 **Phison** 制造生态；进一步交叉 `MPALL_F1_9000_v372_0B.exe`、`mpall_f1_7f00_dl07_v503_0a.exe` 与 `UPTool_Ver2093.exe` 的静态结果，跨版本均出现 `CBaseController::WriteF2Mark`，3.72 还出现 `CU32SSBaseContoller::WriteF2Mark` 与 `F1-F2 MARK`，因此 producer 已从泛化 MPALL/FW 收敛到明确的 **F2-mark writer / WriteF2Mark 函数族** | 当前 Windows 注册/登录/修复组件与 Linux `CLabelManage` 均未找到 `ReadSector3` 或 LBA3 payload 解析路径；这是 EDP 侧“忽略内容”的负证据。公开 MPALL 静态结果仍未暴露 `WriteF2Mark` 内部的 sector offset、512B staging layout 或 `+0x001/+0x020..027/+0x1F0` 的逐字段 store，也未取得 controller firmware consumer | 22份原始参考：21/22全零；唯一 strict Kingston 非零 profile 为 `+0x001=01`、`+0x020..027=b5 7e 9c 45 00 80 00 14`、`+0x1F0..1FF="this is mp mark\\0"`。扩展60份历史备份又出现第二种非零 Kingston profile：同样 `+0x001=01` 与尾 marker，但 `+0x020..027=a8 82 a4 22 00 20 02 16`；另有同型号全零 LBA3。strict 非零盘为 Kingston DataTraveler 3.0 `0951:1666`、121110528×512=`62008590336B`；公开同 identity/capacity 记录同时存在 PS2307 与 PS2309 | 至少两种非零 payload + zero profile，制造家族与 producer 函数族都已收窄；但 `WriteF2Mark` exact write layout、字段定义及 firmware consumer 未闭合。相同 VID/PID/model/capacity 不能锁死具体 controller，因此512B继续全部 PARTIAL |
| LBA4 | 0x000–0x017 | COMPLETE | `$$$onlyid$$$` clear header | 当前注册 writer 根据 main onlyid 格式化 | 识别/解码链从此恢复 onlyid | 22/22 | 完成 |
| LBA4 | 0x018–0x01B | COMPLETE | OnlyIdXor8 | current writer: `main_onlyid ^ 0x88888888` | restore-info 读取该字段 | 22盘 current/legacy 可解 | 完成 |
| LBA4 | 0x01C–0x01F | PARTIAL | OnllyID2Nd | **LBA4 current writer machine-code node layout**：Windows PE `RegsiterUsb@0x1003BBD1..0x1003BBD7` 直接执行 `node+0x04 = object+0x698`；后者已闭合为本次注册 main onlyid | Windows `sub_10015090` / Linux `ReadSector4` 解密后把完整 0x2F node 返回，但当前只强校验 `node+0x00`，未发现对第二 ID 的独立行为判断 | 6/22 current-style 样本 `OnllyID2Nd==main onlyid && HSerialCRC=0`；其余16份旧 profile 第二ID不同 | current producer 已闭合，legacy producer/consumer 仍缺失，保持PARTIAL |
| LBA4 | 0x020–0x033 | PARTIAL | HSerialCRC[5] | Windows PE `RegsiterUsb@0x1003BBF0..0x1003BC79` 精确把 `object+0x558/+55C/+560/+564/+568` 写入 `node+0x08..+0x1B`；`this+0x2E0` 已闭合为内嵌 `UsbLabelParam`，故这些地址正是 `HDOnlySerial[5]@+0x278`。Windows `sub_10047690` 与后续 `sub_100139F0` 两层 current 参数构造/拷贝均跳过这20B；Linux `UsbLabelParam()` 整体清零，`UsbWriteParam(UsbLabelParam&)` 同样不复制 HDOnlySerial，`BuildSector4` 只序列化已构造 node、不计算 HSerial | restore-info reader 只保留/返回这 5×DWORD，当前未找到其业务判断 | 严格22份：14份固定 `1D29,7B,4DD,79,7C`、6份全零、2份高熵；并且 22/22 满足 `OnllyID2Nd==main` iff `HSerialCRC==0` | current zero producer 已跨 Windows/Linux 闭合；legacy 非零值的上游注入算法/consumer 未找到，继续 PARTIAL，禁止解释成目标U盘唯一序列 |
| LBA4 | 0x034–0x044 | PARTIAL | SingleUsbFlg / MyHardinfo / NewLabFlag / Version / 4 sector bytes | current Windows machine code在 node 清零后明确写 SingleUsbFlg=0、NewLabFlag=`LLGB`、Version=1、sector tuple=`08 04 0C 01`；MyHardinfo当前路径没有同等稳定赋值来源 | Windows/Linux reader 均复制完整node；目前未找到这些字段各自的最终行为 consumer | 22/22：Single=0、NewLabFlag=LLGB、Version=1、sector tuple一致；MyHardinfo明显分profile | 常量观察+producer不足以替代业务consumer；继续PARTIAL |
| LBA4 | 0x045–0x046 | PARTIAL | `bDataToServer` / `bConnetServer` profile-dependent wire flags | Linux DWARF恢复正式字段名；Windows `sub_10014550` 与 Linux `BuildSector4@0x1D08E` 的 **current** writer 均先完成 full rolling，再把 node `+0x2D/+0x2E` 原样覆盖回物理 `LBA4+0x45/+0x46` | Windows `sub_10015090` / Linux `ReadSector4@0x1E048` 都不会补偿 current post-XOR 例外：统一 rolling 后复制0x2F node，仅校验 OnlyIdXor8；当前未找到最终业务 consumer | 严格22盘：6份 current identity profile (`OnllyID2Nd==main && HSerialCRC==0`) physical=`00 00`、generic非零；16份 legacy identity profile physical非零，generic为14×`00 00`+2×`0B 00` | inspect 必须 profile-aware：current profile恢复physical post-XOR bytes，legacy保留official rolling-reader视图；legacy旧producer和最终consumer仍缺，因此2B保持PARTIAL |
| LBA4 | 0x047–0x1FB | PARTIAL | restore-node 后 backing/gap；raw-zero 与 rolling-encrypted-zero 两种物理表示 | Windows current `sub_10014550` 与 Linux `BuildSector4@diskfile.cpp:741` 都只把 0x2F restore node 写到 `+0x18..+0x46`；non-null node 分支随后把 rolling XOR 扩展到 `+0x18..+0x1FF`，因此会连同这437B已有 backing 一起变换；Windows 同函数的 `arg0==NULL` 分支明确跳过 node copy/rolling loop，可保留既有 raw gap。两端 builder 都**没有显式把这437B清零** | Linux `ReadSector4@diskfile.cpp:957` 会对 `+0x18..+0x1FF` 执行同一 rolling XOR，但最终只 `memcpy(decoded+0x18, 0x2F)` 给 restore-node 输出并校验 `OnlyIdXor8`，不返回/解释 `+0x47..+0x1FB`；Windows同类识别链也只消费 restore node / onlyid锚点 | 严格22份：18份（17 backup + 独立SanDisk）为物理 raw-zero gap，4份为几乎全非零 rolling 形态；4/4 rolling 形态按 onlyid key 解码后437B全零，raw-zero形态按区域规则保持语义零；仓库CI同时保留两种物理表示并断言 semantic gap 全零 | 已闭合物理边界、两种 current 可解释的存储/变换行为、reader negative semantic consumer 和22盘零语义；但历史 raw-zero 初始 producer及“为何选择/保留哪种表示”未闭合，且 full builder 会变换已有 backing 而非主动清零，因此严格保持 PARTIAL |
| LBA4 | 0x1FC–0x1FF | COMPLETE | trailing LLGB | current writer 继续 rolling key schedule 写 LLGB | reader 作为尾锚点校验 | 22盘可验证 | 完成 |
| LBA5 | 0x000–0x1FF | COMPLETE | opaque preserve / write-protection probe scratch sector | `CUsbRegsiter::RegsiterUsb` 先读取既有 LBA0–12；后续 builder 只重建其它明确扇区，LBA5 不被覆盖，最终随13扇区整体写回；即 producer 语义是 preserve existing bytes | 两版 `EdpDiskCtrl` 的唯一 `base+5` raw-sector consumer 都是：读取整扇→原样写回同一扇区→仅检查 `WriteFile` 是否以 `ERROR_WRITE_PROTECT(0x13)` 失败；`UserLogin` 据此进入只读使用状态，完全不解析内容 | 22/22原始参考整扇512B全零，SHA-256均为 `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；7份原始CI夹具继续锁定 | COMPLETE 表示“整区用途和无payload语义闭合”；全零只是当前实盘状态，不是协议规定，非零内容也应原样保留 |
| LBA6 | 0x000–0x03E | COMPLETE | Dept 主槽前63B：short C-string/backing 或 long marker + Dept[0..58] | Linux `UsbWriteParam(UsbLabelParam&)@0x1C362` 对 department 调 `strcpy_s(dst+0x40,0xBC,src+0x40)`；copy-constructor 不预清对象且自带 `strcpy_s@0x1B9B0` 复制到NUL即停，所以 short profile 的 NUL 后字节是 writer-uninitialized backing。Linux `BuildSector6@0x1CAAC` 在 `strlen<=63` 时固定 memcpy 完整64B；在 long profile 时先清64B临时槽，写 marker `0x40245E2A`，再把 Dept 前60B 放到 marker 后，其中 `+0x04..+0x3E` 正好是 Dept[0..58] | Windows/Linux `ReadSector6`：short profile 按 C-string 读取；long profile 识别 marker 后把 inline prefix 与 LBA9+0x80 continuation 重组。当前/legacy 两种 long reader 均共享 marker + Dept[0..58] 这63B，接缝差异只发生在最后1B `+0x3F` | committed originals 的 short profile 至少3份，LBA6 Dept C-string 与 LBA8 `Dept=` 一致，且在 `+0x00..+0x3E` 内保留真实 post-NUL 非零 backing。严格原始 current Kingston join60 与 legacy Lexar join59 的解密 LBA6 `+0x00..+0x3E` **63B逐字节完全相同**；回归 `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 锁定这一点 | 前63B的两种动态状态均闭合：short = C-string + writer-uninitialized backing；long = marker + Dept[0..58]。已知历史 join59 分叉不触及这63B，因此本段升 COMPLETE |
| LBA6 | 0x03F | PARTIAL | long-Dept inline final byte / short-slot backing | current Linux/Windows/vrvaud long writer 在此写 Dept[59]，当前76B Dept 原盘值为 GBK trail `A8`；short profile 则只是固定64B槽的最后一个 backing byte | long reader 用此字节区分接缝：非零则 continuation 接 Dept[60]；为0则按 legacy compatibility 分支从 Dept[59] 覆盖并接 LBA9 continuation | strict-original current Kingston join60 在 `+0x3F=A8`，strict-original Lexar join59 在同一完整 Dept 上 `+0x3F=00`，且两盘前63B完全一致 | 这是当前已知 join60/join59 唯一 LBA6 主槽分叉字节；legacy join59 producer/选择条件尚未定位，因此单独保留1B PARTIAL |
| LBA6 | 0x040–0x04F | COMPLETE | UsbMainBSec static template material | Windows `sub_10013FD0` 先从 `UsbMainBSec@0x100E7220` 复制整扇；Linux `BuildSector6@0x1CAAC` 同样从 `UsbMainBSec@0x22BB40` 复制 sector_size；本16B没有后续 overlay | Windows `sub_100152A0` 与 Linux `ReadSector6@0x1E2CC` 都在字段解析前对 raw `+0x000..0x1FB` 计算并校验 SAFE6 checksum，不匹配即拒绝；字段 parser 不另解释本段 | 严格22份原始盘解密后22/22精确等于官方模板 `f0 ac 3c 00 74 fc bb 07 00 b4 0e cd 10 eb f2 88`；CI含独立SanDisk锁定 | fixed producer + whole-sector integrity consumer + real-device evidence，无已知 profile 分叉，16B COMPLETE |
| LBA6 | 0x050–0x06F | PARTIAL | User/Owner fixed 32B slot | Windows/Linux `BuildSector6` 在 owner 长度<32时固定复制 `UsbWriteParam.m_usbowner[0..32]`；长 owner 在本槽写 `0x40245E2A` marker + 前28B，continuation 落到 LBA9+0x100 | Windows/Linux `ReadSector6` 检查 marker；普通路径按C字符串恢复 `UsbLabelParam.m_usbowner`，长值从 LBA9 continuation 重组 | committed原始夹具的 LBA6 C-string 与 LBA8 `User=` 一致；实盘在首个NUL后存在非零 backing bytes | 32B物理边界和字符串/overflow语义闭合，但 post-NUL backing 不具稳定字段语义，整体PARTIAL |
| LBA6 | 0x070–0x07F | COMPLETE | `m_autoid[16]` / Autonum fixed slot, including nonsemantic post-NUL backing | Linux DWARF 定义 `UsbWriteParam.m_autoid char[16]@+0x259`；`UsbWriteParam(UsbLabelParam&)@0x1C362` 调自带 `strcpy_s@0x1B9B0`，该实现只复制到首个 NUL、**不清剩余 capacity**，且构造器入口没有先 memset 整对象；`BuildSector6` 随后固定 memcpy 完整16B 到 `LBA6+0x70` | `ReadSector6` 只用 `strcpy_s(...,16,decoded+0x70)` 消费首个 NUL 前的 C-string；`BuildSector8` 将该字符串序列化为 `Autonum=`；同时 LBA6 前508B checksum 覆盖并保护包括 post-NUL backing 在内的全部物理字节 | 22/22 LBA6 C-string 与 LBA8 Autonum 相同；committed originals 中同一个空字符串至少出现2种不同且非零的 post-NUL backing，直接证明尾字节不属于隐藏字符串语义 | 16B 的逐字节行为已闭合：前缀是 C-string，NUL 后是明确的 **writer-uninitialized backing**；值不固定是协议实现行为本身，不是未知字段，因此整槽 COMPLETE |
| LBA6 | 0x080–0x0BF | COMPLETE | `m_UsbOffice[64]` fixed slot, including nonsemantic post-NUL backing | Linux DWARF 定义 `UsbWriteParam.m_UsbOffice char[64]@+0x198`；copy-constructor 用同一个不清尾 `strcpy_s@0x1B9B0` 写该数组且不预清对象；Windows/Linux `BuildSector6` 再固定复制完整64B到 `out+0x80` | Linux `ReadSector6@0x1E6A3..` 明确 `strcpy_s(UsbLabelParam.m_UsbOffice,64,decoded+0x80)`，只解释首个 NUL 前字符串；Windows reader 同构；整扇 checksum 仍覆盖这64B的全部物理值 | 22份原始盘存在空/非空 Office；committed originals 中同一个空 Office 字符串至少出现3种不同且非零的 post-NUL backing profile，排除隐藏字段/固定 padding 解释 | 64B 槽同样是 **writer-uninitialized backing**：producer bug、C-string consumer、完整性消费和多实盘 profile 均闭合；post-NUL 值允许不稳定但无第二业务字段语义，故整槽 COMPLETE |
| LBA6 | 0x0C0–0x0FF | COMPLETE | UsbMainBSec static bootstrap/template material | Windows/Linux BuildSector6 均先整扇复制官方 `UsbMainBSec`，本64B后续无字段覆盖 | 两端 ReadSector6 的 SAFE6 checksum 在字段解析前覆盖整个前508B；本64B无独立业务字段读取 | 严格22份原始盘22/22逐字节等于官方模板；CI含独立SanDisk精确锁定 | fixed producer + checksum consumer + 22盘闭合，64B COMPLETE |
| LBA6 | 0x100–0x103 | PARTIAL | `CLabelManage::m_crcUsbID[0]` = CRC32(device_id) | Linux DWARF 明确 `m_crcUsbID[2]@CLabelManage+0x24`；Linux ctor/`Init` 都执行 `CRC32(0,m_strUID.c_str(),m_strUID.length()) -> +0x24`；Windows `sub_10013D20/sub_10013B80` 同构；两端 `BuildSector6` 都把该数组8B复制到扇区 +0x100 | 同一个运行时成员 `m_crcUsbID[0]` 是 LBA7 rolling-XOR 与 LBA8/LBA12 加解密的4B key；但 current `ReadSector6` 不读取持久化在 LBA6 的这份副本，Windows current CheckLabel 的物理副本检查又位于不可达 legacy fallback | 严格22份含独立SanDisk：22/22 `u32(+0x100)==CRC32(device_id)` 且全部非零；CI门禁 `lba6_crc_usb_id_pair_is_device_id_crc_and_doubled_guard` | 字段名、producer、值算法、同源运行时用途均闭合；但缺对“LBA6这4B副本”的活跃 consumer，严格口径仍 PARTIAL |
| LBA6 | 0x104–0x107 | PARTIAL | `CLabelManage::m_crcUsbID[1]` = 2 × CRC32(device_id) mod 2^32 | Linux ctor/`Init` 直接 `m_crcUsbID[1]=m_crcUsbID[0]*2`；Windows两套构造路径同样 `object+0x48=object+0x44<<1`；`BuildSector6` 连续复制8B | Windows `CheckLabel/sub_100152A0`、`cemsudisk`、`vrvaud_c` 都保留 `+0x100!=0 && +0x104==(+0x100<<1)` 的 legacy 一致性校验，并可映射到 `ERROR_USBVERSIONNOMATCH(11)`/`ERROR_SYSLABELMISTMATCH(13)`；但三处 current 构建均被恒真 `if(1)`/机器码 `mov 1; test; je` 隔离，当前不可达 | 严格22份：22/22 `u32(+0x104)==u32(+0x100).wrapping_mul(2)`；独立SanDisk同样吻合 | 可以命名为 doubled CRC guard / legacy compatibility check material，但当前没有活跃 consumer，保持 PARTIAL，不把死代码当 COMPLETE 证据 |
| LBA6 | 0x108–0x187 | COMPLETE | UsbMainBSec static bootstrap/message template material | Windows/Linux BuildSector6 先复制官方 `UsbMainBSec`；本128B没有任何字段 overlay | Windows/Linux ReadSector6 均先校验覆盖前508B的 SAFE6 checksum；字段 parser 不读取本段 | 严格22份22/22等于官方模板，包含 `Invalid partition table` / `Error loading operating system` / `Missing operating system` legacy message material；CI含独立SanDisk锁定 | 128B fixed template material 的 producer、完整性 consumer、实盘闭合，COMPLETE |
| LBA6 | 0x188–0x1BF | COMPLETE | `m_usbLabel[64]` first 56B physical slot, including nonsemantic post-NUL backing | Linux DWARF fixes `UsbLabelParam/UsbWriteParam.m_usbLabel@+0x218`; `UsbWriteParam(UsbLabelParam&)@0x1C362` uses built-in `strcpy_s(...,64,...)`, and this copy-constructor does **not** memset the 0x299B destination object first. The built-in `strcpy_s@0x1B9B0` returns immediately after copying the first NUL, so destination bytes after NUL retain prior backing. Windows `sub_10013FD0` / Linux `BuildSector6` then fixed-copy the first 0x38=56B of that array to `out+0x188` | Linux `ReadSector6@0x1E84B..` constructs a C++ string from `decoded+0x188` and writes it back to `UsbLabelParam.m_usbLabel[64]`; Windows reader isomorphic. `BuildSector8` serializes the same logical value as ELABEL `Label=`; LBA6 checksum covers every physical byte of the 56B slot | committed originals all decode to the same business value `江苏电力!SAFE6`, yet the bytes after its NUL form **at least 3 distinct and all-nonzero backing profiles**; regression `lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries` locks this. LBA6 C-string and LBA8 `Label=` remain equal in every committed original | Entire 56B behavior is closed: prefix is the C-string, bytes after NUL are **writer-uninitialized backing** copied from the 64B source array, not hidden fields or fixed padding. Future nonzero backing is valid compatibility data; canonical provisioning may zero it deterministically and recompute checksum |
| LBA6 | 0x1C0–0x1CF | PARTIAL | m_usbGSerial C-string slot + profile-dependent post-NUL backing bytes | current Windows/Linux `BuildSector6` 都先清零临时16B，再固定复制输入对象前15B；输入对象 NUL 后字节可被一并带入。legacy 两盘则显示短 GSerial 覆盖了旧 MBR entry1/2 的一部分，NUL 后 surviving bytes 继续落在旧 MBR 几何位置 | Linux `ReadSector6` 只按C字符串匹配 GSerial，NUL 后不消费 | 22盘中16份短值 `322CA28A` 全部在NUL后仍有非零字节；6份长值为 `322CA28A-D7D144`；两份 legacy 盘的 `u32@+0x1CA=20417` 恰落在 entry1 sector_count 位置 | 字符串语义闭合，但固定槽尾跨 writer profile 语义不同，整16B仍不能 COMPLETE |
| LBA6 | 0x1D0–0x1DF | PARTIAL | BeiZhu C-string slot + profile-dependent post-NUL backing bytes | current Windows/Linux writer 同样固定复制输入对象前15B；legacy 两盘中 GBK“普通”+NUL 后的 surviving bytes 与后续 `+0x1E0` 连成旧 MBR entry2/entry3 几何 | Linux `ReadSector6` 只以 C 字符串读回 BeiZhu | 22盘：20空、2份GBK“普通”；8/22首个NUL后仍有非零字节；两份 legacy 盘的 `start_lba@+0x1D6`/`sector_count@+0x1DA` 对应旧 entry2 几何 | 字符串语义闭合；legacy underlay 已识别，但 current/legacy 固定槽物理语义不同，整16B仍 PARTIAL |
| LBA6 | 0x1E0–0x1EF | PARTIAL | current-template zero / legacy MBR partition-table fragment | current Windows/Linux `BuildSector6` 都从静态 `UsbMainBSec` 起步且不显式覆盖此区；当前模板这里为16B零。legacy 两盘表明旧 writer/profile 曾以动态 MBR table 作为 underlay：第3条 MBR entry 起于 `+0x1DE`，BeiZhu 覆盖前2B 后，`+0x1E0..0x1ED` 仍保留 start-CHS尾、`type=0x07`、end-CHS、start_lba、sector_count；`+0x1EE..0x1EF` 是 entry4 前2B | current `ReadSector6` 无业务读取；`UDiskLabelRepair` 虽有真实 MBR repair/check consumer，但其 SAFE6 判断直接读取 LBA12 并从 sector9/backup 恢复 sector0，未发现直接读取该 LBA6 fragment | 严格22份：20/22 为零且这20份仍全部存在 LBA12 type4；仅 Aigo+SanDisk 2/22 非零，2/2 的 `type/start_lba/sector_count` 都与同盘 LBA12 type4 精确对应。Aigo/SanDisk 分别为 `c1 ff 07 ef ff ff 1c a8 7d 0e e3 f4 27 00 00 00` / `c1 ff 07 ef ff ff b2 8a 05 0e 77 3c 4c 00 00 00` | 已从“opaque extension”纠正为 legacy MBR-layout 残片；current-zero producer闭合、legacy结构语义和实盘交叉已闭合，但旧 writer 与直接 consumer 仍缺，因此不增加 COMPLETE |
| LBA6 | 0x1F0–0x1F3 | PARTIAL | m_encrypt | **LBA6 m_encrypt current producer !SAFE gate**：DWARF 定位 `UsbWriteParam+0x258`；Windows `RegsiterUsb` 临时对象=`ebp-0x3F4`，故该字段=`ebp-0x19C`；PE `0x1003BA94/0x1003BAC7` 在5字节 `!SAFE` 比较的相等/不等分支分别写1/0，随后 `0x1003BAF5 -> BuildSector6`；Windows/Linux BuildSector6 都扩成 DWORD 写 `+0x1F0` | `UsbLabelParam` 无 `m_encrypt` 成员；Windows/Linux ReadSector6 与当前已扫 runtime 组件仍未找到该字段的最终行为读取 | 22/22=1 | current producer 1/0来源已闭合；仍缺真实行为 consumer，严格标准下继续 PARTIAL |
| LBA6 | 0x1F4–0x1FB | COMPLETE | UsbMainBSec static zero tail before checksum | Windows/Linux BuildSector6 都由 `UsbMainBSec` 初始化，字段 overlay 最后只写到 `+0x1F3`，故8B保持模板零 | 两端 ReadSector6 的 checksum 覆盖到 `+0x1FB`；字段 parser无独立读取 | 严格22份22/22解密为8B零，官方模板同样为零；CI含独立SanDisk锁定 | explicit template-zero producer + checksum consumer +实盘，8B COMPLETE |
| LBA6 | 0x1FC–0x1FF | COMPLETE | SAFE6 checksum | writer 对前508B计算 checksum | reader/inspect 校验 | 22/22 校验通过 | 完成 |
| LBA7 | 0x000–0x0BF | PARTIAL | 3×64B packed EDPF 区 | Windows old-table writer/runtime；Linux natural ABI 仅作字段名参考 | 多处 reader/登录/挂载 | 22盘均按0x40 stride成立 | 逐字段状态见详细审计；不能用 Linux 0x48 natural stride 解析物理 LBA7 |
| LBA7 | entry0 +0x010–+0x013 | COMPLETE | entry0 `NeedDisturb` MBR scramble/descramble gate | Windows `CreatePartitions` 对 entry0 显式写入调用者传入的 `NeedDisturb=1`；old/new ABI converter 双向保留该DWORD | 两版 Windows `vrvaud_c` 的 `NewCheckDisTurbUsb(*)` 直接检查 packed entry0 `NeedDisturb@+0x10 != 0`；`SetProtect` 在该检查链后调用 `sub_1006ff80 -> sub_1006e580`，把 LBA0 `+0x1BE..+0x1FD` 的64B MBR表替换为静态 scramble 表；`UnsetProtect -> sub_1006ffd0 -> sub_1006e9b0` 则从 LBA2 读整扇恢复到 LBA0 并刷新磁盘属性 | 严格22份 original real-device：22/22 entry0 `NeedDisturb=1`；另有真实免密 SanDisk 两条-entry profile 同样 entry0=1 | 字段不是泛化“防篡改”位，而是驱动侧是否进入系统可见 MBR 分区表 scramble/descramble 流程的门控；静态扰动表只有一条 type=0x04、start_lba=66、sector_count=1 的占位 entry。entry1/entry2 的同名字段仍未找到独立 consumer |
| LBA7 | 每条entry +0x038–+0x03F | COMPLETE | 8B legacy wrapped file-key | Windows `sub_10028DB0` 以 `fold32(password)` 对两个32位 half 做对称 XOR 包装；`sub_100125B0` 映射回 old 0x40 entry；`SavePartionSector/sub_10028580 -> sub_10010FC0` 写 LBA7 | `sub_10026050` 对 v0x0064 固定解包8B，随后以 `sub_10038840` 计算 CRC32 并比较同 entry `FileKeyCRC(+0x34)`；改密后反向重包 | 22份 original real-device 中全部28条非零 type2/type4 legacy entry 独立复算 28/28 PASS；默认 `fold32("0000aaaa")=0x91919191` | **LBA7 v0x0064 packed legacy file-key wrapping** 已闭合；FileKeyCRC 4B此前已经计入 COMPLETE，本轮仅新增3×8B=24B，禁止重复计数 |
| LBA7 | 0x0CA | COMPLETE | pass-info `bNoUsbChkPasSafe` | current Windows `CreatePartitions/sub_1003DB50` 明确从制标请求写 `tail+0x0A`；`WriteNormalULabel/sub_10046E80` 又把 `UsbWriteParam+0x7EC` 传入该请求字段 | 独立 Linux 官方 `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接执行 `cmp byte [pass+0x0A],1; setne showparam+0x03`，随后 `CreateSafe6TmpPolicyFile@0x407D50` 将结果纳入 CRC/加密 SAFE6 policy；独立 `EdpEDiskBack::Safe6PolicyFile::GetSafe6Policy@0x4100B0` 与 `linuxedpedisk::Safe6PolicyFile::GetSafe6Policy@0x41EAA0` 解密并恢复该 policy/runtime 参数 | 严格22份原始盘：18×0、4×1；22/22 LBA7/LBA12 同盘取值一致；CI `pass_info_no_usb_safe_flag_varies_and_matches_between_lba7_and_lba12` 锁定0/1双值与跨扇区一致性 | 字段行为闭合到“值等于1时将 SAFE6 show-policy byte +3 清零，否则置1”，不是仅 opaque round-trip；producer、真实行为 consumer、跨独立客户端传递和实盘双值证据齐全 |
| LBA7 | 0x0CC–0x0CD | PARTIAL | pass-info `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` | current `CUsbRegsiter::CreatePartitions/sub_1003DB50` 机器码 `0x1003DC16..0x1003DC26` 先把完整14B pass-info 显式清零；后续字段 store 最远只到 `pass+0x0A`（`0x1003E78A..0x1003E790`），因此 `+0x0C/+0x0D` 是明确的 current writer-owned zero，而不是未初始化字节。LBA7 builder 随后原样接收该14B结构 | current/旧版 Windows `EdpEDiskCtrl`、Linux `libcemsfilesyscheck.so`、`checkdiskback`/SAFE6 policy 路径均未发现这2B的值相关读取。两代 `vrvaud_c` 虽存在独立策略项 `BackupPromptInfo` 并按其首字节分支，但其磁盘 old-table 全局只承接 `0xC0 = 3×0x40` packed entries、明确不含14B pass-info tail，且未发现任何数据流把该策略项连接到 `+0x0C/+0x0D` | 严格22份 LBA7/LBA12 均为0 | 正式字段边界与 current-zero producer 已闭合，并排除了名称相近的 `BackupPromptInfo` 误接；但单位/取值域、历史非零 producer/profile 和真实业务 consumer仍缺，继续 PARTIAL |
| LBA7 | 0x0CE–0x1FF | COMPLETE | packed old-table post-table writer-zero region | Windows `edpediskctrl.dll::sub_10010FC0` 先以 `sub_1004D110(...,0,0xFFF)` 明确 memset staging，随后只复制 `0xC0` packed table + `0x0E` pass-info，再对完整512B rolling并写 LBA7；`sub_1004D110` 机器码已复核为 memset 等价实现 | Windows `ReadPartionInfoExEx/sub_10010B40` 解密完整512B，但成功后只复制 `0xC0` table 和 `0x0E` pass-info，完全不返回/解释 `0x0CE..0x1FF`；Linux natural-ABI builder也独立采用“整块清零→写结构→整扇rolling”的同原则，但其表尾在0xE6，只作原则佐证、不用于覆盖Windows物理offset | 严格22份原始生成参考（21 non-converted backup + 独立SanDisk）逐盘解密：22/22 `0x0CE..0x1FF == zero[306]`；CI门禁 `lba7_post_table_plaintext_is_zero_through_sector_end` 锁定 committed original subset | 306B 的 producer零来源、negative consumer、物理边界和原盘均闭合；这里的 COMPLETE 表示 writer-owned zero region，不是靠“样本碰巧全零”推断 |
| LBA8 | 0x000–0x003 | COMPLETE | LLGB magic | Windows/Linux `BuildSector8` | reader 先检查 LLGB | 22/22 | 完成 |
| LBA8 | 0x004–0x007 | COMPLETE | logical length | writer=`0x80+strlen(ELABEL)` | decoder决定动态加密前缀 | 22/22吻合 | 完成 |
| LBA8 | 0x008–0x00B | COMPLETE | ToolVersion[4] | Windows `sub_100148d0` 与 Linux `BuildSector8@diskfile.cpp:805` 都写固定字节 `01 00 00 01` | `ReadSector8(UsbLabelParam&)` 的语义 parser 不读取该版本戳；`ReadSector8(BYTE*)` 仅把完整解密扇区原样导出 | 22/22原始盘=`01 00 00 01`；CI原始夹具锁定 | 4B writer、reader行为、实盘一致，无已知 profile 分叉 |
| LBA8 | 0x00C–0x00F | COMPLETE | Labversion | Windows/Linux writer 都固定写 `0x00000222` | 语义 parser 跳过该 DWORD；raw reader 仅原样导出 | 22/22原始盘=`0x222`；CI原始夹具锁定 | 4B 标签版本戳闭合 |
| LBA8 | 0x010–0x013 | COMPLETE | writeTime | Windows writer 调 `GetTickCount()`；Linux `CLabelManage::GetTickCount@0x1FBAA` 用 `clock_gettime(CLOCK_MONOTONIC)` 转为毫秒并截为32位 | 两个官方 reader 都不把该值用于标签解析/准入；raw reader 只导出原值 | 22/22原始盘均非零且跨标签变化；CI夹具保持多值反例 | 不是墙钟时间，而是制标时单调时钟毫秒计数（32位回绕） |
| LBA8 | 0x014–0x017 | PARTIAL | HDSerialInfo | `tagEdpUsbLableInfo.HDSerialInfo@+0x14`。current Windows/Linux BuildSector8 从零初始化 header 得到0；另取得并哈希核验 2020 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`），其可达 ELABEL writer `sub_10007DF0` 先调 `UsbTools.dll` ordinal4，结果为0才 fallback ordinal3，并把返回DWORD写到临时LLGB结构 `+0x14`。本机 `UsbTools.dll` 导出表已精确证明 ordinal4=`EDP_DiskNumber`、ordinal3=`EDP_DeviceNumber`；两者分别是跳到 `DeviceNumber.dll` ordinal3/ordinal1 的纯 thunk，不能把两层 DLL 的 ordinal 数字混为一谈。`EDP_DiskNumber`：依次读取 `PhysicalDrive0..3` ATA IDENTIFY words10..19 的20B serial，每16-bit word交换字节、裁剪首尾ASCII空格，失败/20B全零跳过，其余无分隔拼接后做 reflected CRC32(poly `0xEDB88320`, initial=0)。`EDP_DeviceNumber` 复用同一 disk-serial stream，再调用 network helper mode=1；该分支仅收非零 MAC，并排除描述同时命中 `VIRTUAL`+`VMWARE` 的 VMware virtual adapter，按接受顺序追加 `MACAddress<i>=<12位大写无分隔MAC>\r\n`，最后若至少1条再追加 `MACCount=<N>\r\n`，随后对 `serial_blob || mac_blob` 做同一 CRC32 | 注册侧 Windows/Linux semantic ReadSector8 不读取该DWORD；current runtime `sub_10016260` 会把 `+0x14` 保存到标签结构。新增独立 2020 `EdpEDiskCtrl.dll` v3.6.10.18（MD5 `95a06e0d466ba40a7d5c0e6a409e2114`）同样完整复制 `+0x14`，但整DLL不导入 `DeviceNumber.dll`；其对象后续只直接读取 `this+0x1728..+0x1734` 的早期版本/标志字段，唯一 `this+0x173C` 引用反而把该地址作为0x104B路径缓冲区覆盖后再 `CreateFileA`，未发现按原DWORD值消费或整块搬运到其它比较路径 | current identity profile 全0；legacy identity profile全非零，且旧值按物理盘族成组；committed-original 回归现显式断言 legacy `HDSerialInfo!=0`。2020 writer证明官方确有“宿主磁盘/主机身份 CRC32 -> 非零HDSerialInfo” producer family | 2020 producer family 的 primary/fallback 两条算法现均闭合，但字段仍保持PARTIAL：该 writer 同时生成非零 `UsbOnlyInfo`，与16份 strict legacy 原盘“HDSerialInfo非零但UsbOnlyInfo全零”不是同一 exact generation profile；更早 writer/profile selection 仍未定位，不能用近邻代际强行闭合 |
| LBA8 | 0x018–0x01D | COMPLETE | `MacInfo[6]` reserved/unused MAC slot | Linux DWARF 正式定义 `MacInfo unsigned char[6]@+0x18`；Windows/Linux BuildSector8 都先清零完整 header，Linux 再把仍为0的 DWORD+WORD写到+0x18，current producer明确为6B零 | Windows `sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 均从 ElabOffset 解析 ELABEL，不读取 MacInfo；raw reader仅 opaque 导出，不赋予业务语义 | 严格22份原始盘跨 current/legacy identity **22/22均为6B零**；CI `lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty` 现对全部 profile 锁定 MacInfo=0 | 官方字段边界、显式零 producer、negative semantic consumer 与跨代实盘均闭合，无已知 profile 分叉，6B COMPLETE |
| LBA8 | 0x01E–0x03D | PARTIAL | UsbOnlyInfo[32] | current Windows `RegsiterUsb -> sub_100148d0` 明确以 main onlyid 执行 `sprintf("%08x%08x", onlyid,0)` 并写32B槽；Linux BuildSector8同构。2020 `CEMSUsbRegsiter.dll::sub_10007DF0` 又证明存在过渡 producer：直接在 LLGB `+0x1E` 执行同一 `%08x%08x`，第一DWORD来自调用参数，第二DWORD就是上述 `EDP_DiskNumber`/fallback结果，因此该代会把非零 `HDSerialInfo` 同时编码进 UsbOnlyInfo 第二半 | semantic reader跳过该槽，raw/runtime reader只复制到结构；2020 runtime 未找到该32B的值相关最终行为读点 | 6/6 current identity匹配 `format("%08x%08x", main_onlyid_bits,0)`；16/16 strict legacy槽全零；2020 writer提供了独立的“第二DWORD非零”官方中间代际 producer，但它不匹配这16份 legacy wire profile | current 与2020过渡 producer都已知，反而更明确证明至少存在三代 profile；最早 `UsbOnlyInfo==zero[32]` 的 producer/选择条件仍未定位，因此32B继续PARTIAL |
| LBA8 | 0x03E–0x03F | COMPLETE | ElabOffset | `BuildSector8@diskfile.cpp:805` 写 `0x0080`；官方结构 `tagEdpUsbLableInfo.ElabOffset@edpdiskglobal.h:413` | `ReadSector8@diskfile.cpp:1102` 读取 WORD 并用 `decoded+ElabOffset` 构造 ELABEL 字符串 | 22/22原始盘=0x80，且22/22都指向 `<ELABEL>`；CI真实夹具锁定 | 2B 寻址语义、producer、consumer、实盘全部闭合 |
| LBA8 | 0x040–0x07F | COMPLETE | Reserverd[64] | Windows `sub_100148d0` 与 Linux `BuildSector8` 都先零初始化整个 header，再把未被其它赋值覆盖的 64B 原样复制到该区 | `ReadSector8(UsbLabelParam&)` 直接越过该区定位 `ElabOffset` 指向的 ELABEL；raw reader 仅原样导出，不赋予业务语义 | 22/22原始盘解密后64B全零；CI原始夹具锁定 | 官方结构名、零初始化 producer、negative consumer 和实盘全部闭合为 reserved-zero 区 |
| LBA8 | 0x080–0x1FF | COMPLETE | **LBA8 dynamic ELABEL + encrypted backing + preserved tail** | Windows `sub_100148d0` 与 Linux `BuildSector8@0x1D602` 独立同构：都只把 17-key ELABEL+NUL 写到 `+0x80`，按 `(logical_len / 16 + 1) * 16` 对现有 LBA8 **原地**加密；两函数都不清 caller output。Windows `RegsiterUsb` 在此之前已先读完整13扇区到 `var_500`，再把旧 `LBA8` 指针直接传入 writer，因此 ELABEL NUL 后到 `encrypted_len` 的字节属于既有 backing、会随前缀一起加密，`encrypted_len..0x1FF` 则完全 preserve-existing | Windows 注册侧 `cemsusbregsiter.dll::sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 独立只回填同一 7-key 集合：`registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit`。但运行时 `out_raw_data/EdpEDiskCtrl.dll::sub_10016260` 是更完整的 consumer：`runtime EdpEDiskCtrl reader parses all 17 ELABEL keys`，将 `GLab/Indus/Orgcd/Org/Unit/Dept/User/Alarm/Autonum/Label/Rmark/VOL0/1/2/VOLC0/1/2` 全部写入 `tagEdpUsbLableInfo` 对应槽。NUL 后块内 backing 和块外 tail 不赋予字段语义；inspect 的 nonzero in-block backing、nonzero physical tail、16B-aligned extra-block 三个回归锁定兼容读取边界 | 严格22份原始盘：`logical_end=0x148..0x183`、encrypted prefix=`0x150..0x190`，22/22 正文均为同一17-key顺序且十个当前 compatibility 键为空；committed original 门禁同时覆盖多个动态长度，并验证当前实盘 `ELABEL NUL 后到 encrypted_len 的字节属于既有 backing` 的观测值目前为零。所有观测物理 tail 也为零，但两者的零值都不是协议要求 | 384B 的动态状态机已逐类闭合：正文/终止NUL由writer拥有；块内剩余字节为 encrypted preserved backing；块外为 unencrypted preserved tail。7个核心键有注册侧 Windows/Linux 双 consumer，运行时 EdpEDiskCtrl 又对全部17键提供完整结构 consumer；实盘无已知 body/profile 分叉。因此整段384B升 COMPLETE；这不要求 backing/tail 恒零，未来非零值必须按边界原样保留 |
| LBA9 | 0x000–0x003 | COMPLETE | EETU magic | `CUsbRegsiter::SetTempUse` 构造 `EETU`；`WriteTempUseInfo` 可运行时回写 | `ReadTempUseInfo` 必须校验 EETU magic | 20个非零LBA9原始样本 | 完成 |
| LBA9 | 0x004–0x00B | COMPLETE | ullBTime | Windows `SetTempUse` 从开始时间字符串解析为64位值；空/短字符串保持0 | Linux `CheckTempUse` 与 `time(NULL)` 比较；非零且 now < ullBTime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 开始时间下界语义闭合，0表示不启用该下界 |
| LBA9 | 0x00C–0x013 | COMPLETE | ullETime | Windows `SetTempUse` 从结束时间字符串解析为64位值；空/短字符串保持0 | `CheckTempUse` 与 `time(NULL)` 比较；非零且 now > ullETime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 结束时间上界语义闭合，0表示不启用该上界 |
| LBA9 | 0x014–0x017 | COMPLETE | useCount | `BusManageImp::WriteNormalULabel` 普通模式从请求 `+0x947` 取次数；特殊 OutManage-off 模式明确写 `0xFFFFFFFF`；`CUsbRegsiter::SetTempUse` 再将 request+0x40 原样写 EETU+0x14 | Linux `CheckTempUse`：`0xFFFFFFFF` 不递减/不回写；0=次数耗尽；其它正值减1并 `WriteTempUseInfo` 回写 | 20/20原始EETU=0xFFFFFFFF；真实CI夹具回归 | 4B 次数控制及无限次数哨兵完全闭合 |
| LBA9 | 0x018–0x07D | COMPLETE | **EETU reverse[0..101] writer-uninitialized opaque backing** | Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 正式定义 `reverse[104]@+0x18`。Windows `CUsbRegsiter::SetTempUse` 先清零 EETU magic 后124B，再固定 `memcpy(EETU+0x18, request+0x44, 0x66)`。继续回溯 `BusManageImp::WriteNormalULabel/sub_100A99F0` 原始机器码确认：SetTempUse 请求 `&var_BD4` 只写 `begin[32]`、`end[32]`、`useCount@+0x40`；此前 `sub_1009BAD0` 的 `ECX=&var_9EC` 只清 `ebp-0x9EC..-0x56`，并不覆盖位于 `ebp-0xBD4` 的请求对象，因此 `request+0x44..+0xA9` 102B 是明确的 writer-uninitialized caller backing，而非 reserved-zero | 当前 Windows runtime `ReadTempUseInfo/sub_10013490` 解密并把完整0x80 EETU缓存到 `CEdpDiskControl+0x1076`；行为代码 `GetTempUseInfo/CheckTmpUse` 只直接读取 `ullBTime/ullETime/useCount`（显式对象引用截止 `+0x108A`，reverse 从 `+0x108E` 开始无独立读点）。登录递减次数后 `WriteTempUseInfo/sub_10013770` 将完整0x80重新加密写回，所以 **runtime preserves the full 0x80 EETU while only consuming time/useCount**。Linux `CheckTempUse` 同样不读取 reverse | 20/20原始非零LBA9的 EETU `reverse[104]` 全零；committed original 门禁 `real_eetu_temp_use_limits_match_the_official_unlimited_profile` 锁定该观察，扩展历史只读扫描也未发现非零 reverse | 前102B的“值不稳定/当前恰零”本身不是未知字段：正式边界、writer-uninitialized来源、runtime透明保存/negative semantic consumer与原始实盘均闭合，按与 LBA6/LBA8 backing 相同口径升 COMPLETE。未来若出现非零 reverse 必须原样保留，不得清零或赋予隐藏字段语义 |
| LBA9 | 0x07E–0x07F | COMPLETE | reverse[102..103] zero tail | `SetTempUse` 对 EETU +0x04..+0x7F 先整体清零，随后从 +0x18 只覆盖0x66B，即最后覆盖到 +0x7D；因此 +0x7E/+0x7F 在所有 current writer 路径都保留显式零初始化 | Linux `CheckTempUse` 只读取 ullBTime/ullETime/useCount，对 reverse[104] 完全无业务读取；运行时回写只修改 useCount 并保留其余字节 | 20/20原始EETU均为 `00 00`；CI门禁 `lba9_eetu_final_two_reverse_bytes_are_writer_zero_padding` | 2B 满足 explicit-zero producer + negative consumer + real-device evidence，可严格升 COMPLETE；不得把前102B一起升级 |
| LBA9 | 0x100–0x103 | COMPLETE | SAPF magic | 旧writer恢复模板 | `UDiskLabelRepair::Repair0Sector` | 14样本 | 完成 |
| LBA9 | 0x104–0x113 | COMPLETE | MBR恢复entry | writer保存16B entry | repair直接写回 LBA0 0x1BE | 14/14 | 完成 |
| LBA9 | 0x114–0x11F | PARTIAL | SAPF decoded trailing/backing 12B / long-User continuation overlap | 历史 SAPF producer 尚未定位；SAPF reader `sub_10008550` 固定对 `+0x100..+0x11F` 32B 全部 `^0x88` 解码。另一方面 Windows `BuildSector6/sub_10013FD0`、Linux `BuildSector6@0x1CAAC` 与 `vrvaud_c::sub_10118ED0` 均在 User 长度>=32 时把 marker+前28B 写到 LBA6+0x50，并把 `User[28..NUL]` 写入 LBA9+0x100，因此该区存在与 SAPF 重叠的 current long-User profile | SAPF repair 的已确认行为只用 magic 与前16B MBR恢复项；`ReadSector6` 在 User marker 命中时又会从 LBA9+0x100 复制完整0x80B回 `UsbLabelParam.m_usbowner[27/28..]`，支持 legacy join=27 与 current join=28 | 14份真实SAPF中该12B至少5种 decoded profile；严格22盘未出现 User>=32 的正向 continuation 样本 | SAPF 与 long-User continuation 是同一物理区的不同 profile。long-User current producer/consumer已知但缺实盘；SAPF尾12B又缺历史 producer/独立语义，故继续PARTIAL |
| LBA9 | 0x120–0x17F | PARTIAL | post-SAPF region / long-User continuation remainder | Windows/Linux/vrvaud 三套 current `BuildSector6` 在 User>=32 时可由 `User[28..NUL]` 连续写到 LBA9+0x100..，最大可覆盖到+0x17F；若不触发该长User profile，则注册路径保留既有 backing。运行时 `SetTempUse` 与 `SetPassInfoEx` 不覆盖这96B | `ReadSector6` 的 User-marker 分支会把 LBA9+0x100 起完整0x80B复制回 owner continuation；SAPF reader只到+0x11F，不解释+0x120..+0x17F | 严格22盘没有长User正向样本；14/14 SAPF盘该96B全零，独立SanDisk亦零 | 旧“current纯preserve/ignore”结论已纠正；这里是可选 long-User continuation + otherwise preserve 的多profile区。缺真实长User样本且可能承接其它历史 backing，继续PARTIAL |
| LBA9 | 0x180–0x183 | COMPLETE | EPPE magic | `SetPassInfoEx` | `ReadPassExInfo` | 6样本 | 完成 |
| LBA9 | 0x184–0x187 | COMPLETE | minimum password length | writer限制6..19 | `ReadMinPassLenInfo` 返回该DWORD | 6/6=8 | 完成 |
| LBA9 | 0x188–0x1FF | COMPLETE | **EPPE writer-owned zero tail** | PE机器码 `SetPassInfoEx/sub_1003ADD0`：先校验输入DWORD为6..19，再对 EPPE `+0x04..+0x7F` 124B整体清零，写 magic，随后明确 `EPPE+0x04=*arg0`；因此 `+0x08..+0x7F` 120B 在 current writer 中为显式零 | 正式注册侧 `CUsbRegsiter::GetPassInfoEx` 解密并校验 EPPE 后只执行 `*out = *(decoded+0x04)`；独立 `modfilesyscheck::ReadMinPassLenInfo` 同样只消费 magic/+0x04。两套 `EdpDiskCtrl` 虽保留可搬运完整0x80B的 `ReadPassExInfo/GetPassExInfo` compatibility helper，但其外层对象由唯一两个 DLL export（Create/Release）创建，current factory vtable `0x1008021c` 不包含该 helper，反编译交叉引用也仅见实现/相邻 thunk，未形成 current 产品语义消费路径 | 严格22份中6份EPPE；6/6 minPassLen=8 且解密后120B全零；CI门禁 `real_eppe_samples_keep_the_current_writer_zero_tail` | 120B 的 current producer、两个独立 semantic reader 的 negative consumer、当前公开接口边界与原始实盘均闭合，按 writer-owned zero region 升 COMPLETE。COMPLETE 不授权清洗未知历史非零 profile：兼容读取若未来遇到非零 tail 应保留/报告，而不是据此臆造业务字段 |
| LBA9 | 0x080–0x0FF | PARTIAL | long-Dept continuation slot (current join=60 / legacy join=59) | Windows `BuildSector6/sub_10013FD0`、Linux `BuildSector6@0x1CAAC` 与 `vrvaud_c::sub_10118ED0` 三套 current producer一致：Dept长度>=64时在 LBA6+0写 `0x40245E2A + Dept前60B`，再把 `Dept[60..NUL]` 写入 LBA9+0x80；若未触发长Dept则该builder不覆盖此区 | Windows `CheckLabel/sub_100152A0`、Linux `ReadSector6@0x1E2CC` 与 `cemsudisk` reader 都识别同一 marker，并支持两种接缝：inline[59]!=0时 continuation 写回 Dept[60..]；inline[59]==0时从 Dept[59..] 覆盖，以修复 legacy GBK split profile | 严格22盘恰有8盘 marker+continuation：4盘 join=60、continuation含NUL共17B；4盘 join=59、continuation含NUL共18B；两组均重建为相同76B合法GBK Dept。CI `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 锁定两种真实profile | current producer、双profile官方consumer及22盘实证已闭合；但现有三套writer都只能生成join=60，4份join=59原盘的旧producer仍未定位，按严格规则整128B继续PARTIAL，不得再称 opaque backing |
| LBA10 | 0x000–0x003 | COMPLETE | EESI magic | Set EESI writer | Get EESI reader | 1个SanDisk样本 | 完成 |
| LBA10 | 0x004–0x007 | COMPLETE | **UsbSuspensionWnd lifecycle/control flag** | 两套独立 `EdpEDisk.exe`（SHA-256 `cfa13177...` / `dc71c300...`）启动时都先将完整0x80B EESI缓冲清零并显式写 `+0x04=1` 后调用 vtable `+0x20 GetEdpEdiskSetInfo`；同两套程序的卷标设置对话框 `IDOK` handler zero-initializes the full 0x80B EESI payload，只填 `+0x08/+0x18` 两个卷标，再经 vtable `+0x24 SetEdpEdiskSetInfo` 保存，因此该 producer 路径明确写 `+0x04=0`；底层 setter 只强制 magic，其余DWORD原样落盘 | 两套程序均加载 `UsbSuspensionWnd.dll` 的 `Show/Destroy/SetParentWnd`：读回 EESI 后 `+0x04==0` 路径调用 `Destroy`；自动登录成功后 `+0x04!=0` 且 suspension-window helper 已初始化时，进入刷新/`Show` 链；`UserLogin` 自身不把该DWORD当卷标开关 | 唯一启用 EESI 的原始 SanDisk 实盘解密值为1；21份其它原始盘无 EESI；旧两代 DLL 无 EESI 路径，与“该控制字段属于后续 EESI profile”一致 | 4B边界、0/1官方 producer、值相关 `UsbSuspensionWnd` 行为 consumer、原始实盘均闭合；字段按可观察行为保守命名，不臆造原始 C++ 成员名 |
| LBA10 | 0x008–0x017 | COMPLETE | Share/type2 volume label | `SetEdpEdiskSetInfo` 原样复制调用者结构前0x80并加密写入；默认 reader 初始化为GBK“交换区” | `CEdpDiskControl::UserLogin` 将该槽赋给本地 string；type2 分支直接把其 `c_str()` 传给 `SetVolumeLabelA` | 22份原始参考中唯一启用EESI的SanDisk实盘为GBK“交换区”；已加入512B原始证据夹具 | 16B字段语义、writer、consumer、实盘闭合 |
| LBA10 | 0x018–0x027 | COMPLETE | Encrypt/type4 volume label | 同上；默认 reader 初始化为GBK“保密区” | `UserLogin` type4 分支直接把该槽对应 string 的 `c_str()` 传给 `SetVolumeLabelA` | 唯一启用SanDisk实盘为GBK“保密区”；另一版 `out_raw_data/EdpEDiskCtrl.dll` 同构复核 | 16B字段完整闭合 |
| LBA10 | 0x028–0x07F | PARTIAL | EESI uninterpreted round-trip payload | current 与另一版 `EdpEDiskCtrl` 的 Get 都解密并向调用者返回完整0x80B；Set 都把调用者完整0x80B（除强制magic）原样加密写入；两套独立 `EdpEDisk.exe` 的卷标设置 `IDOK` producer 又都先把完整0x80B清零、只填 `+0x08/+0x18`，因此该官方 UI producer 明确给这88B写零 | current `UserLogin` 只消费 +0x08/+0x18；两套 `EdpEDisk.exe::OnInitDialog` 外部 getter caller 也只消费 `+0x04/+0x08/+0x18`；设置对话框虽经 vtable +0x24 调用 setter，但没有读取/赋予这88B任何业务值 | 严格22份生成参考中的 SanDisk EESI 解密后88B全零；另有2026-08-04 Netac OnlyDisk 历史真实捕获，LBA6 CRC 与 LBA7/LBA12 EDPF 自洽，独立 device_id 解密后这88B同样全零；后者暂不并入22份严格生成参考 | 已补上真实外部 Get/Set caller、current UI 零来源与第二个历史 EESI profile，但底层 API 明确允许这88B round-trip 任意调用者数据，仍缺正式字段划分、非零 profile 与值相关 consumer，保持PARTIAL |
| LBA10 | 0x080–0x1FF | COMPLETE | cross-generation unowned preserve/ignore physical tail | 两个独立 EESI build（`ydcc/edpediskctrl.dll` 与 `out_raw_data/EdpEDiskCtrl.dll`）的 setter 都执行同一 read-modify-write：先读完整0x200B LBA10，只以新 EESI 密文覆盖前0x80B，再将后0x180B原样写回；更老 `VRV/edp` 与 `cems/Edp` 两代 DLL 连 EESI magic/Get/Set 路径都不存在，因此同样没有该 tail producer | 两个 EESI getter 都只解密/返回前0x80B，完全不暴露后384B；UserLogin 也只消费前0x80中的卷标；旧两代无 EESI reader，当前收集的其它产品组件未发现 LBA10 tail 入口 | committed originals 的 LBA10 全零 profile + 独立 SanDisk EESI 的 tail384B 全零；额外对本地历史语料按 LBA6 CRC guard + LBA12 EDPF 双重过滤得到58份有效 EDP 前部快照，58/58 tail全零，其中2份独立 EESI 正例同样 tail全零 | COMPLETE 指“该384B跨已知代际均不属于 EESI payload，writer unowned/preserve、reader ignore”的存储行为已闭合，**不**表示协议要求其值恒零；若未来遇到非零 tail，兼容实现必须原样保留 |
| LBA11 | 0x000–0x003 | COMPLETE | DRKB magic | `CDataSecrity::RandBuffer256` 先写 DRKB | `ReadSector11` 首先校验 DRKB | 22/22 | 完成 |
| LBA11 | 0x004–0x0FF | COMPLETE | random252 | `RandBuffer256`: `srand(time(NULL)); rand()%255` 共252B | `DataEncrypt/DataDecrypt` 将整个 DRKB块纳入 CRC32 密钥输入 | 22/22；均无0xFF；7 CI夹具回归 | 每字节都是密钥扰动材料，来源和消费闭合 |
| LBA11 | 0x100–0x103 | COMPLETE | PDKB magic（解密后） | `BuildSector11` 构造 PDKB plaintext | `ReadSector11` 解密后必须校验 PDKB | 22/22 | 完成 |
| LBA11 | 0x104–0x1FF | COMPLETE | 加密的 UID + zero fill | 正常注册 writer：`cemsusbregsiter.dll::sub_10014720` 以 `DISK_GEOMETRY_EX.DiskSize` 为 `ullSize`；repair writer：`UDiskLabelRepair.dll::CLabelRepair::Repair -> sub_10008950(ReWrite11Sector) -> sub_10003A40`，其 `disk_info+0x30/+0x34` 由 `IOCTL_DISK_GET_DRIVE_GEOMETRY(0x70000)` 返回的 `DISK_GEOMETRY` 经 `sub_10019EF0` 64-bit multiply 计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector` 后生成 LBA11 | 正常 consumer：Windows/Linux `ReadSector11` 以 exact DiskSize 解密；repair consumer：`CLabelRepair::Repair -> sub_10008820 -> sub_10003BD0` 用同一 CHS `disk_info+0x30/+0x34` 校验 LBA11，失败才进入 ReWrite11Sector | 严格22份：21/22 exact DiskSize，1/22 Aigo U335 rev_pmap 为 CHS；另有同一 Aigo rev_pmap 的独立真实 exact-size LBA11 捕获，证明 profile 取决于 writer 路径而非硬件；22/22 解密后 UID 正确且 UID 后全零 | 两种已观测 wire profile 的 producer、consumer、容量算法和实盘均闭合；因此后半252B升级 COMPLETE |
| LBA12 | 0x000–0x11F | PARTIAL | 3×96B EDPF | Windows/Linux writer | 登录/挂载/兼容链大量消费 | 22盘 | 字段逐项状态见详细审计 |
| LBA12 | 0x010–0x013 | COMPLETE | entry0.NeedDisturb compatibility gate | `CUsbRegsiter::CreatePartitions` 写入 entry0；Linux `edpdiskglobal.h:82` 定义字段 | `vrvaud_c::NewCheckDisTurbUsb(*)` fallback 在 `Format.cpp:0x3CE/0x380` 直接以该 DWORD 非零判 success | 22/22原始盘=1；20个entry0 type1、2个type2；7 CI夹具锁定 | 完成的是 entry0 兼容门控行为；其它 entry 的 NeedDisturb 不随之升级 |
| LBA12 | 每条entry +0x048–+0x057 | COMPLETE | `EncryptFileKey32[16]` cross-generation compatibility slot | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`；current packed writer 后续只写 wrapped16 `+0x38..47` 与 mode `+0x58`，所以该16B保持显式零。旧72B `tagEdpPartionInfo` **根本没有**该槽；Linux checker 的 old→new `GetPartionFromOld` 也只把旧8B key搬到 natural `+0x40`，不填 natural `+0x50 EncryptFileKey32[16]` | 104B checker DWARF正式命名 `EncryptFileKey32[16]@+0x50`，但 `DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0` 都不读取它；packed `libedpedisk.so` 会在按值构造时结构缓存完整96B，但严格按 `PartitionHeader` 符号边界审计，映射到对象 `+0x88/+0x90` 的两个QWORD只在构造器写入，后续没有值相关读取；正对照 wrapped-key 起点 object `+0x78` 被 SMS4/AES128/OldEdp decrypt 实际消费。Windows UserLogin/改密同样只消费 `+0x38..47/+0x58` | 严格22份原始盘全部现存 EDPF entry 共66条，`+0x48..57` **66/66全零**；CI `lba12_encrypt_file_key32_compatibility_slots_are_zero_in_original_entries` 锁定 | **LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure**：旧ABI无槽、新ABI正式保留名字、current producer显式零、跨Windows/Linux只结构搬运不参与算法、原始实盘全零。COMPLETE 表示“兼容槽生命周期/无当前业务语义”闭合，不把它误称 Reserved，也不禁止未来其它ABI结构性携带非零值 |
| LBA12 | 每条entry +0x059–+0x05F | COMPLETE | packed Reserved[7] | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`，后续只写至 +0x58；Linux DWARF正式字段名 `Reserved[7]` | Windows UserLogin/改密只消费 wrapped16 与 +0x58；Linux decrypt/改密同样不消费 Reserved | 22盘66/66 entry全零；CI原始夹具锁定 | **LBA12 packed Reserved[7] producer/negative-consumer closure**；与前面的 `EncryptFileKey32[16]` compatibility slot 分开建模 |
| LBA12 | 0x12A | COMPLETE | pass-info `bNoUsbChkPasSafe` | 与 LBA7 同一 current CreatePartitions 请求输入，LBA12 builder 保存同一 pass-info 字节 | `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接比较该字段并生成 SAFE6 show-policy byte +3；policy 经 `CreateSafe6TmpPolicyFile` 加密后被 `EdpEDiskBack` 与 `linuxedpedisk` 两套 `Safe6PolicyFile::GetSafe6Policy` 恢复 | 严格22份18×0+4×1，且22/22与同盘 LBA7 +0x0A相同；CI锁定双值/一致性 | 与 LBA7 同一逻辑字段、同一 producer/consumer 链，1B COMPLETE |
| LBA12 | 0x12C–0x12D | PARTIAL | pass-info `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` | 与 LBA7 共用同一 current `CreatePartitions` pass-info：`0x1003DC16..0x1003DC26` 显式清零完整14B，后续 store 只到 `+0x0A`；在写 LBA12 前只把 Version 改成 `0x0206`，`+0x0C/+0x0D` 仍保持 writer-owned zero | 已审 Windows/Linux/`checkdiskback` policy 链均未找到这2B的业务读取；两代 `vrvaud_c::BackupPromptInfo` 是独立 policy 来源，old-table 全局仅为0xC0 packed EDPF entries，不含 pass-info tail，当前无数据流可把两者等同 | 22/22为0，且22/22与同盘 LBA7 对应两字节一致 | current-zero producer与跨LBA复制边界已闭合；仍缺单位/取值域、历史非零 producer/profile 与最终 consumer，严格保持 PARTIAL |
| LBA12 | 0x12E–0x16F | COMPLETE | post-table zero initialized padding | writer整块零初始化且不覆写 | 主reader不消费该区 | 22/22解密为零 | producer+negative consumer+实盘闭合 |
| LBA12 | 0x170–0x1FF | COMPLETE | post-table zero initialized padding / continuous-cipher tail | Windows current `sub_10014F30` 分配 `sector_size+1` 后整块清零，v0x206 只复制 `0x120+0x0E=0x12E` 结构字节，随后加密整扇；Linux `BuildSector12` 同样先把 `sector_size+1` 全零再只复制表/表尾并整扇加密 | Windows `sub_100160B0` 固定解密0x200B，但只复制 `0x120+0x0E` 返回；Linux主reader同样只解释表/表尾，不消费 post-table 区 | 严格22份 + 独立SanDisk 解密后 `0x12E..0x1FF` 全零；既有连续密文门禁同时证明 `0x170..` 不是RAW尾 | 与 `0x12E..0x16F` 同属一个 post-table zero padding 区；此前 PARTIAL 行是主表 stale 状态，总进度表早已把这144B计入 LBA12 的393B COMPLETE，因此本次只纠账、不重复增加总数 |
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

本轮又把范围扩到 `nopwd_tool/backup` 与 `utils/backup` 两个目录的60份 `.bin`
历史/真实快照做只读 census；该扩展集合包含历史/转换状态，只用于 profile 发现，
不改变上述22份 strict generation reference 的计数。非零 LBA3 只有3份，并形成
**两个不同的 MP payload profile**：2026-08-03 两份逐字节相同的 Kingston 快照
使用 `+0x020..027=a8 82 a4 22 00 20 02 16`；strict 2026-09-03 Kingston 使用
`+0x020..027=b5 7e 9c 45 00 80 00 14`。两类都保持 `+0x001=01` 和
`+0x1F0..1FF="this is mp mark\\0"`，而同型号其它快照还存在整扇全零 profile。

新增历史 profile 的原始 LBA3 SHA-256 为
`a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e`；仓库夹具
`tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex` 逐字节锁定。
回归 `lba3_mp_marker_has_multiple_real_historical_payload_profiles` 明确要求两个
marker profile 的中间8B不同，防止把任一单盘的值错升成全局固定模板。

因此不能把 `this is mp mark` 单独建模成 EDP 字段，也不能把 21 个零样本解释成
“协议规定全零”，也不能把 `+0x020..+0x027` 建模成单一固定常量。LBA3 整扇从
UNKNOWN 移到 PARTIAL；在找到厂商 MP producer、
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

补充历史只读证据：`/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`
记录 `device_id="disk&ven_netac&prod_onlydisk&rev_0000"`，整份6656B快照
SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`。
该捕获的 LBA6 `crcUsbID`、LBA7/LBA12 EDPF 都与同一 device_id 自洽；LBA10
独立解密再次得到 `EESI/+0x04=1/交换区/保密区`，且 `+0x28..0x7F` 88B全零。
仓库测试夹具 `netac_onlydisk_20260804_lba10_head.hex` 锁定其前0x80密文。
由于这份历史快照尚未完成与主22份参考同等级的原始生成来源链审计，它只作为
额外真实 profile 证据，**不**扩大22份严格生成参考计数，也不据此把88B升级 COMPLETE。

仓库新增原始证据夹具：

`tests/fixtures/protocol_evidence/sandisk_ultra_usb_3_0_lba10.bin`

SHA-256：
`240d04e7c97d300c5081f793d72850d49acbf5408bc0d8cf32de8eef7a5e8f02`

`+0x04..0x07` 本轮继续追到 `EdpEDisk.exe` 的真实外部调用者后已经闭合。
此前“未找到外部 vtable Get/Set caller”的结论需要废弃：

- `out_raw_data/EdpEDisk.exe`（SHA-256
  `cfa1317775801381b6ca51f13857d1e52506ff48ac4df94d7b9f91f742d3e4a1`）与
  `VRV/cems/ydcc/edpedisk.exe`（SHA-256
  `dc71c30041c4fe9fab277737116216502e9f6a610630a1a70b125c59441e1fd1`）
  在 `CEdpDiskDlg::OnInitDialog` 都先清零对象内 `0x80` 字节 EESI 缓冲，
  显式写 `buffer+0x04=1`，再经接口 vtable `+0x20` 调
  `GetEdpEdiskSetInfo(buffer)`；
- 同两套程序的卷标设置对话框 `IDOK` handler 则先清零完整 `0x80B` EESI，
  只把两个编辑框写入 `+0x08/+0x18`，然后经 vtable `+0x24` 调
  `SetEdpEdiskSetInfo`。因此这条官方 producer 明确把 `+0x04` 写成0，
  同时也明确把 `+0x28..0x7F` 写成0；
- 两套程序都加载 `UsbSuspensionWnd.dll` 并解析 `Show`、`Destroy`、
  `SetParentWnd`。重新读回 EESI 后，`+0x04==0` 的路径会调用 `Destroy`；
  自动登录成功后，`+0x04!=0` 且 helper 初始化成功时会进入
  suspension-window 的刷新/`Show` 链；
- 因而 `+0x04` 不是卷标文本开关，也不是固定常量。它是一个有明确0/1
  producer 和值相关 consumer 的 **UsbSuspensionWnd lifecycle/control flag**。
  这里按可观察行为命名，不宣称恢复了原厂 C++ 成员名。

原始实盘侧，唯一启用 EESI 的 SanDisk 样本该 DWORD=1；其余21份原始盘没有
EESI。结合上述两套独立官方 producer/consumer，`+0x04..0x07` 4B 可从
PARTIAL 升为 COMPLETE。

因此：

- `LBA10 +0x04..0x07` 4B → COMPLETE；
- `LBA10 +0x08..0x17` 16B → COMPLETE；
- `LBA10 +0x18..0x27` 16B → COMPLETE；
- `+0x28..0x7F` 88B → PARTIAL：已知完整 round-trip 边界、官方 UI 零来源和
  negative consumer，但仍缺字段划分、非零 profile 与值相关业务 consumer；
- `+0x80..0x1FF` 384B → COMPLETE：按后续跨代审计已闭合为
  cross-generation unowned preserve/ignore physical tail，不能再按 padding 分析。

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

CI 中的原始完整夹具也有多份 EETU，回归会逐盘解密并固定前三项。
`reverse[104]` 需要进一步拆开，不能再整体归一：

- `SetTempUse` 先把 magic 之后的 `0x7C` 字节全部清零；
- 随后 `copy(EETU.reverse, request+0x44, 0x66)` 只覆盖 reverse 的前102B，
  即 LBA9 `+0x18..+0x7D`；
- 最后2B `+0x7E..+0x7F` 从未被覆盖，因此始终保留 writer 的显式零初始化；
- 对 `BusManageImp::WriteNormalULabel` 的真实机器码做栈区扫描后，上游临时请求
  只显式写 begin/end/useCount；在调用 `SetTempUse` 前没有对
  `tempUse+0x44..+0xA9` 这102B做整体初始化。故前102B即使当前20/20为零，
  也不能解释成协议固定零 padding。后续进一步恢复 `sub_1009BAD0` 的真实 thiscall
  参数后，已确认该 memset 清的是 `&var_9EC` 另一对象，完全不覆盖 `&var_BD4`
  SetTempUse 请求，因此这102B的 producer 已从“疑似未初始化”闭合为明确的
  writer-uninitialized backing。

Linux `CheckTempUse` 对整个 reverse[104] 都不读取，只消费时间窗和 useCount；
后续 Windows runtime 审计又确认 `ReadTempUseInfo` 整0x80缓存、
`GetTempUseInfo/CheckTmpUse` 的显式读点只到 useCount，`WriteTempUseInfo` 再整0x80
透明写回。因此当前最终结论为：

- `reverse[0..101] / LBA9 +0x18..+0x7D`：**writer-uninitialized opaque backing**，
  producer + transparent-preserve/negative semantic consumer + real-device profile 已闭合，
  后续升级 COMPLETE；
- `reverse[102..103] / LBA9 +0x7E..+0x7F`：
  **explicit zero-init producer + negative consumer + 20/20 real-device zero**
  三条证据闭合，升级 COMPLETE。

本轮因此从 PARTIAL 升级：

- `+0x04..0x0B`：8B；
- `+0x0C..0x13`：8B；
- `+0x14..0x17`：4B；
- `+0x7E..0x7F`：2B；
- 此处记录的是最初阶段累计 **22B COMPLETE**；后续 reverse 前102B补齐 producer/runtime
  透明保存证据后，EETU 本段又新增102B COMPLETE，最终计数以主账本为准。

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

此前唯一缺口是 Aigo U335 `rev_pmap / onlyid=1987718388` 为什么使用 CHS 容量。
本轮已从独立官方 repair 组件闭合这条路径：

```text
UDiskLabelRepair.dll::CLabelRepair::Repair
  -> sub_10008820                 # Check LBA11
       -> sub_10003BD0            # 用 disk_info+0x30/+0x34 解密 PDKB
  -> if check fails:
       sub_10008950               # ReWrite11Sector
         -> sub_10003A40          # 重新生成 DRKB/PDKB LBA11
```

`disk_info+0x30/+0x34` 的来源为：

```text
sub_10002A20
  -> sub_10002FF0
       DeviceIoControl(IOCTL_DISK_GET_DRIVE_GEOMETRY = 0x70000,
                       out_size = 0x18)
  -> DISK_GEOMETRY:
       Cylinders            @ +0x08/+0x0C (u64 in enclosing object)
       TracksPerCylinder    @ +0x14
       SectorsPerTrack      @ +0x18
       BytesPerSector       @ +0x1C
  -> sub_10019EF0 64-bit multiply helper
  -> capacity = Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector
  -> disk_info+0x30/+0x34
```

`sub_10019EF0` 的机器码已复核为标准 64-bit multiply helper：返回 `EDX:EAX`，
不是业务函数。对常见 255/63/512 geometry，这正是此前实盘复算得到的 CHS-floor
容量。也就是说，**CHS 不是注册 writer 的隐式分支，而是 repair writer/reader 的
明确容量来源**。

### 5.5 22份原始实盘验证

只读重算结果：

- 21/21 原始完整备份均成功恢复 PDKB + 正确 device_id；
- 独立 SanDisk 原始加密盘成功恢复
  `disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00`；
- 总计 **22/22**；
- 21/22 使用 `DiskSize`；
- 1/22（Aigo U335 `rev_pmap`）使用 CHS 取整容量；
- 22/22 第一半扇都是 `DRKB + 252B`，随机区没有出现 `0xFF`，符合 `rand()%255`。

同一 Aigo U335 `rev_pmap` 的辅助真实采集同时存在 CHS 与 exact DiskSize 两种
LBA11，进一步证明差异来自 writer path，而不是硬件身份本身。至此两种已观测
profile 都具备 producer、consumer、容量算法和真实盘验证，因此：

- LBA11 `0x000..0x0FF`：COMPLETE；
- `0x100..0x103`：COMPLETE；
- `0x104..0x1FF`：由 PARTIAL 升级 COMPLETE；
- **LBA11 整扇 512B = 100% COMPLETE**。

## 6. LBA7 / LBA12 EDPF 字段 producer-consumer 图

字段名称来自 Linux DWARF，但这里存在一个必须显式区分的 ABI 分叉：

`/mnt/git/.../global/inc/edpdiskglobal.h:76`

```text
Linux libcemsfilesyscheck.so natural tagEdpPartionInfo (sizeof=0x48)
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
+0x34..+0x37 alignment hole
+0x38 FileKeyCRC
+0x40 EncryptFileKey     (u64)
```

但 **Windows 真实物理 LBA7 不是这个 72B natural ABI**，而是去掉对齐洞后的
64B packed ABI：

```text
Windows physical LBA7 packed entry (stride=0x40)
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
+0x38 EncryptFileKey     (8B legacy wrapped key)
```

**LBA7 packed 64-byte ABI versus Linux natural 72-byte ABI** 已由三条独立证据锁定：

- `libcemsfilesyscheck.so::BuildSector7@0x1DCDA` 的 natural build 复制
  `0xD8 = 3*0x48`，其 pass-info 位于 `+0xD8`；
- Windows `cemsusbregsiter.dll::sub_10016490` 明确按 `0x40` 读取旧表并逐字段
  扩展到 `0x60` runtime table；`edpediskctrl.dll::sub_100125B0` 做反向
  `0x60 -> 0x40` 映射；
- 22份 original real-device：按 `0x40` stride，21/22 为3条 EDPF、1份已知
  中间态为2条，且 `+0xC0` 的14B pass-info 22/22 可恢复合法版本；
  按 `0x48` stride 则 22/22 都无法得到三条连续 EDPF，`+0xD8` 也无一得到合法
  pass-info。CI 原始夹具另外锁死 `0x40` 三条 entry 与 `+0xC0` 表尾。

这里的“22份”继续特指**原始生成协议参考集**。另有
`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4`：
它是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的设备快照，
不是 edpcli 自生成/免密转换产物。本轮把它作为独立第23份真实行为/profile
样本纳入复核，但不拿它替代22份原始生成参考。该盘 LBA7 明确为：

```text
entry0: Version=0, PartionCount=2, PartionType=2, NeedDisturb=1, NeedEncrypt=1
entry1: Version=0, PartionCount=2, PartionType=4, NeedDisturb=1, NeedEncrypt=1
pass-info Version=0x0064
```

因此真实产品确实存在两条 entry 的 LBA7 profile；此前22份参考中的 Netac
`onlyid=949028302 @ 17:24:33` 仍因同 onlyid 前后只有 LBA7 被改动而作为
该扇区的局部实验态降权，但不能再把“LBA7=2”本身视为实验态特征。
此外，`no_password_disk4/info/disk4_info.json` 中旧解析结果
`"ver": 2` 是把 `PartionCount@+0x08` 错标成 Version；按物理 packed ABI
重新解码后两个 entry 的 `Version@+0x04` 都为0。仓库
`protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex` 回归门禁
专门拦截这类字段错位。该新增样本不改变当前 COMPLETE/PARTIAL 字节计数。

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

### 6.0 entry Version 与 entry1/entry2 NeedDisturb：producer 已闭合到赋值来源，consumer 仍缺

本轮直接验证 Windows 官方 PE 机器码：

- \`cemsusbregsiter.dll::sub_10016490\` 按 \`0x40 -> 0x60\` 遍历三条 old entry，
  明确复制 \`Version@+0x04\` 与 \`NeedDisturb@+0x10\`；
- \`edpediskctrl.dll::sub_100125B0\` 的 \`0x60 -> 0x40\` 反向转换也逐条复制二者；
- 因此它们确实属于物理 old ABI 字段，而不是 padding。

当前 writer \`CUsbRegsiter::CreatePartitions/sub_1003DB50\` 的真实赋值序列是：

\`\`\`text
memset(old_table, 0, 3 * 0x40)

# entry Version
entry0.Version = 0        # 三条都没有 +0x04 覆盖写
entry1.Version = 0
entry2.Version = 0

# NeedDisturb
caller passes need_disturb = 1
entry0.NeedDisturb = need_disturb
entry1.NeedDisturb = need_disturb
entry2.NeedDisturb = 0    # 没有覆盖写，保留 memset
\`\`\`

这与原始三分区实盘的 \`1/1/0\` 完全一致，也解释了为何不能把 NeedDisturb
理解成 \`PartionType\` 的函数：新增真实免密 SanDisk 的两条表中，
type4 位于 entry1，因此其 NeedDisturb=1；标准三分区 type4 位于 entry2，
NeedDisturb=0。回归测试
\`lba7_need_disturb_is_not_a_partition_type_invariant\`
固定这一 profile/位置差异。

consumer 继续向下追踪后的边界：

- 两版官方 \`vrvaud_c\` 都把完整 packed old table 读入缓冲；
- \`NewCheckDisTurbUsb\` / \`NewCheckDisTurbUsbEx\` 实际只检查
  entry0 \`NeedDisturb@+0x10\`；
- 本轮把两版全局 table 的 ABI 又按机器码/地址重新锁定：ydcc build
  `0x1020BF40`、Win10 build `0x10172520` 都先 memset **0xC0**，随后承接
  3×0x40 packed old table；两版 `sub_*C5D0` 都以 `(i << 6)+base+0x0C`
  遍历 PartionType，明确 stride=0x40，不是0x60 runtime；
- 对这两个0xC0全局区做完整静态 xref：两版都只有
  `entry0 NeedDisturb@base+0x10` 的行为读取；entry1/entry2 `+0x10`
  和三条 `Version@+0x04` 均无直接 xref。动态循环也只读取 `PartionType@+0x0C`；
- Linux \`CLabelManage::GetPartionFromOld\` 对 Version/NeedDisturb 只是 ABI 搬运；
- Linux \`PartitionHeader\` 体系会携带整条 new entry，但已扫描的解密/校验方法没有
  发现这两个字段参与行为分支；
- 旧 Windows \`EdpEDiskCtrl\` 读取 old LBA7 后，实际协议代际仍由
  14B pass-info Version 决定，不依赖 entry \`Version@+0x04\`。

实盘方面，22份原始生成参考的全部有效 entry 与新增真实免密 SanDisk 两条 entry
均为 \`Version@+0x04=0\`。这足以闭合“当前 writer 为什么是0”和字段物理边界，
但按本项目 COMPLETE 标准，没有业务 consumer 就不升级：
entry Version 与 entry1/entry2 NeedDisturb 继续 PARTIAL。
\`lba7_entry_version_is_not_partition_count_across_real_profiles\`
同时锁死 \`+0x04 Version\` / \`+0x08 PartionCount\` 的边界，防止旧解析错误复发。

因此当前剩余20B不能因为“跨两版都没有读取”而当作 Reserved/zero COMPLETE：
Version/NeedDisturb 都是正式 ABI 字段，converter 也会保留其值；负 xref 只能收紧
当前组件的“不消费”边界，不能证明其它历史组件永远忽略它们。

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

### 6.2 LBA7 v0x0064 +0x38..+0x3F：8B legacy file-key wrapping 完整闭合

Windows 运行时 `edpediskctrl.dll::sub_10026050` 是旧表 key 的直接 consumer 和
改密 producer。对目标 entry，它依据 pass-info/version 决定 key 长度：

```text
key_len = 8
if pass_info.Version == 0x0206:
    key_len = 16

plain_key = unwrap(password, entry.EncryptMode, entry.wrapped_key)
if CRC32_bare(plain_key[0:key_len]) != entry.FileKeyCRC:
    reject
```

对 v0x0064 legacy table，转换后的 `EncryptMode=0`。`sub_10028AB0` 的 mode0
分支调用 `sub_10011290 + sub_10011450`。继续拆机器码可得：

- `sub_10011290(password)`：按 little-endian 4B chunk 求和；不足4B的尾部
  以零补齐后再加，结果按 u32 回绕；
- `sub_1005CEF0` 是 unsigned 64-bit right-shift helper；
- `sub_1004EF80` 是 unsigned 64-bit multiply helper；
- 化简编译器 helper 后，`sub_10011450` 对两个32位 half 的实际变换为：

```text
K = fold32(password)
plain_lo = wrapped_lo XOR K
plain_hi = wrapped_hi XOR K
```

逆向写回 `sub_10028DB0` 使用同一对称变换：

```text
wrapped_lo = plain_lo XOR K
wrapped_hi = plain_hi XOR K
```

默认密码的两个独立值：

```text
CRC32_bare("0000aaaa") = 0x0429735D
fold32("0000aaaa")     = 0x91919191
```

持久化链：

```text
CEdpDiskControl::ChangePwd / sub_100269A0
  -> sub_10026050
       -> sub_10028DB0          # 生成新的 wrapped8
       -> runtime entry +0x34   # FileKeyCRC
       -> runtime entry +0x38   # wrapped8
       -> if version == 0x64:
            sub_100125B0        # 0x60 runtime -> 0x40 packed
  -> CEdpDiskControl::SavePartionSector / sub_10028580
       -> version == 0x64:
            sub_10010FC0
              memcpy old_table[0xC0]
              append pass-info[0x0E]
              rolling-XOR whole LBA7
              WriteFile(sector 7)
```

22份 original real-device 的只读独立复算覆盖全部非零 legacy 加密 entry：

- 共 **28条** type2/type4 entry 同时具有非零 `FileKeyCRC + wrapped8`；
- 28/28 的 `UserKeyCRC == 0x0429735D`；
- 每条分别执行 `wrapped_lo/hi XOR 0x91919191` 得到8B file-key；
- **28/28** 都满足
  `CRC32_bare(unwrapped_file_key8) == entry.FileKeyCRC`；
- current-style 对应字段为零，与 newer profile 分界一致。

因此每条 packed entry 的 `+0x38..+0x3F` 8B 可由 PARTIAL 升为 COMPLETE，
三个 entry 共新增 **24B COMPLETE**。`+0x34..+0x37 FileKeyCRC` 早已因 CRC
consumer 链计入 COMPLETE，本轮不重复增加4B/entry。

CI 回归分别锁定物理 LBA7 必须使用0x40 packed stride，以及 committed original
fixtures 的默认密码 legacy wrapped8 必须能按上述算法解包并通过 FileKeyCRC。

### 6.2.1 LBA7 `0x0CE..0x1FF`：306B post-table zero region 完整闭合

Windows 主物理 old-table writer `edpediskctrl.dll::sub_10010FC0` 已回到实际实现复核：

```text
staging[0..0xFFF] = 0                         # sub_1004D110 == memset
staging[0x000..0x0BF] = packed_table[0xC0]
staging[0x0C0..0x0CD] = pass_info[0x0E]
rolling_xor(staging[0x000..0x1FF])
WriteFile(LBA7, 0x200)
```

`sub_1004D110` 不是根据命名猜测：其机器码按 `arg2` 长度逐字节/`rep stosd`
填充 `arg1`，语义就是 memset。`sub_10010FC0` 在任何 table/tail copy 之前把
staging 清零，并且最后一个结构写入恰好结束在 `0x0CE`，所以 plaintext
`0x0CE..0x1FF` 的306B有明确的 **writer-zero producer**。

对应 Windows consumer `ReadPartionInfoExEx/sub_10010B40`：

1. 从 LBA7 读取完整 sector；
2. 对前512B执行完整 rolling 解码；
3. magic 合法后只向调用者复制 `decoded[0x000..0x0BF]` 的0xC0 table；
4. 再复制 `decoded[0x0C0..0x0CD]` 的0x0E pass-info；
5. **从不返回或解释 `decoded[0x0CE..0x1FF]`。**

Linux `BuildSector7@0x1DCDA` 也独立执行“约2KB staging 先清零→复制 natural
3×0x48 table +0x0E tail→对完整512B rolling”。因为 Linux natural ABI 的尾端是
`0x0E6`，这条证据只用于确认同源 builder 的未用区域零初始化原则，**不能**把
Linux `0x0E6` 偏移混成 Windows packed `0x0CE` 的物理边界。

严格22份 original generation reference set 再逐盘独立复算：

- 22/22 LBA7 均按各自 `CRC32(device_id)` 派生 rolling key 恢复 `EDPF`；
- **22/22 `decoded[0x0CE..0x200] == zero[306]`**；
- 独立 SanDisk original 同样为完整306B零，无 legacy 非零反例。

新增 CI 门禁 `lba7_post_table_plaintext_is_zero_through_sector_end`。因此这306B
满足字段/区域边界、官方 producer、negative consumer、原始实盘四证合一，
由 **UNKNOWN -> COMPLETE**。LBA7 严格状态随之变为：

```text
490 COMPLETE / 22 PARTIAL / 0 UNKNOWN = 95.7%
```

post-table 审计当时新增306B COMPLETE；后续又闭合 pass-info
`bNoUsbChkPasSafe(+0x0A)` 1B。当前 Version、entry1/entry2 NeedDisturb 和
BackupPromptPeriod 两字节仍保持 PARTIAL，不能被相邻区域的闭合带着升级。

### 6.3 LBA12 +0x38..+0x47：v0x0206 默认密码的 mode2 wrapping 已闭合，但整字段仍 PARTIAL

本轮对 packed entry 的 16B wrapped file-key 做了重新独立审计，
不再沿用旧脚本结论。

Windows producer：

```text
CreatePartitions
  -> generate file_key16
  -> entry.FileKeyCRC = CRC32_bare(file_key16)
  -> entry.UserKeyCRC = CRC32_bare(original_password)
  -> if original_password == "0000aaaa":
         sub_10040400(password)
         # 替换为隐藏10B字符串
  -> md5 = MD5(effective_password)
  -> if EncryptMode == 2:
         if GLOBAL/oldSM4 == "1":
             alternate mode2 implementation
         else:
             standard SM4-ECB(file_key16, md5)
  -> entry.wrapped16 = ciphertext
```

其中 **LBA12 v0x0206 hidden default-password file-key wrapping** 的
默认密码替换过程已经从 Windows PE 机器码重新恢复：

```text
sub_10040400:
  seed32      = 468b46088b4e048bd02bd13bd37f2183
                c1098d3c003bf97f028bf98b065750e8
  key1[i]     = seed[i] XOR seed[i+16]
  key2[i]     = seed[i] + seed[i+16]  (u8 wrapping)
  table96     = A6B0_DECRYPT(obfuscated_table96, key1)
  index16     = A6B0_DECRYPT(obfuscated_index16, key2)
  password[i] = table96[index16[i]], i=0..9
```

独立重算结果：

```text
key1 = 8782cb348b75fdf4d2a028b0d528716b
key2 = 0794d3448b89fd0ad2b6cac6d9d6716b
index[0..10] = 38 17 51 4d 0c 05 26 16 2d 0d
effective_password = "LtSWi[2f)j"
MD5(effective_password) = 548b072cba7f104d88a446556cc3c432
```

这个字符串不是从旧文档复制，而是本轮直接由当前 DLL 的机器码常量和
已验证 A6B0 算法重算得到。用相反的 A7F0 方向时 index 会越界，
也提供了负向验证。

Windows `sub_100036e0` 所走的 mode2 算法可由以下常量/轮函数确认
为标准 SM4：

- FK =
  `A3B1BAC6 56AA3350 677D9197 B27022DC`；
- CK 从 `00070E15 1C232A31 383F464D ...` 顺序展开；
- S-box 与标准 SM4 S-box 逐字节一致；
- key schedule T' 使用 rot13/rot23；
- round T 使用 rot2/rot10/rot18/rot24；
- 总计32轮。

Linux consumer：

```text
CDiskReader::DecryptFileKey
  -> default input "0000aaaa"
  -> version == 0x0206 && password == default:
         GetIniString(password)
  -> AlgorithmSpace::fileKey_Decrypt
       mode2:
         MD5(effective_password)
         MC_KKSMS4::DecryptBuffer(wrapped16)
  -> CDiskReader::CheckFileKeyCrc
       CRC32(file_key16) == entry.FileKeyCRC
```

22份 original real-device reference set 的只读重算：

- 共 66 条 packed entry；
- 44 条 type2/type4 为 EncryptMode=2；
- 其中 43 条 `UserKeyCRC=0x0429735D`；
- 使用 OpenSSL 3.6.3 标准 SM4-ECB +
  `MD5("LtSWi[2f)j")` 独立解包：**43/43** FileKeyCRC 校验通过；
- 唯一非默认 type2 的同盘 type4 仍是默认密码；
  从 type4 独立解出共享 file-key
  `eadd58009f9abe0625a1f1f779d4c98b`，
  CRC=`F7EEA980`，同时精确匹配 type2/type4 保存的 FileKeyCRC；
- 两条 wrapped16 不同，排除“直接复制同一 wrapped material”。

所以当前实盘使用的
`v0x0206 + mode2 + oldSM4!="1"` 分支已经闭环。

随后继续追完 **LBA12 alternate wrapping-mode algorithm map** 后，
可以把“其它算法分支未知”这一缺口进一步消掉：

| EncryptMode | Windows writer | Windows reader | Linux当前build | 当前结论 |
|---:|---|---|---|---|
| 1 | `sub_10001190`: A7F0, key=MD5(password) | `sub_100384E0`: A6B0 inverse | `fileKey_Decrypt case1 -> Decrypt` | 算法双向闭合；22盘正向样本0 |
| 2 | `sub_100036E0` 或 `sub_10011010`: 标准SM4-ECB | `sub_10028AB0 case2`: 标准SM4 decrypt | `MC_KKSMS4::DecryptBuffer` | 44条真实entry；已闭合 |
| 3 | `sub_1000FC10`: AES-128-ECB | `sub_1002F670`: AES-128 inverse；CRC失败后强制mode1重试 | 当前 `fileKey_Decrypt` 无case3 | 算法/兼容行为闭合；22盘正向样本0 |

mode1 的关键固定关系：

```text
key = MD5(effective_password)
A7F0 key material = key XOR "EDPSECDISK200709"
wrapped16 = A7F0(file_key16, key, counter=0)

reader:
file_key16 = A6B0(wrapped16, key, counter=0)
CRC32(file_key16) == FileKeyCRC
```

mode3 则是：

```text
key = MD5(effective_password)
wrapped16 = AES-128-ECB-ENC(file_key16, key)

reader:
file_key16 = AES-128-ECB-DEC(wrapped16, key)
CRC32(file_key16) == FileKeyCRC
```

Windows `UserLogin` 还明确实现 mode3 历史 fallback：第一次 mode3 解包
CRC失败后，以 mode1 重解同一 wrapped16；第二次 CRC 成功则接受。
对应日志分别为 `EncryptMode == eEncryptAESOPENSSL` 与
`dwKeyCrcOld == m_epiNewInfos[nIndex].FileKeyCRC`。

`oldSM4=="1"` 也不再视为独立 wire profile。该分支
`sub_10011010` 与默认 `sub_100036E0` 均逐常量/轮函数对应标准SM4：
同一 S-box、FK、CK、32轮和同一 block 输入输出语义；Windows mode2
reader 只有一个标准SM4解包分支且完全不读取 `oldSM4` 配置。
所以它只是实现选择，不改变盘面格式。

因此 `+0x38..+0x47` 整体仍记 **PARTIAL**，但剩余原因已收缩为：
**22份原始盘没有 mode1/mode3 的正向样本**。当前44条加密entry全部mode2。
严格完成度统计仍不增加 16B×3，避免用静态算法闭合替代真实盘证据。

扩展历史语料也按正确 device-id 重跑，而不是把缺身份时的 A6B0 RAW 输出
误当解码结果：`utils/backup` 的 `.meta.json` 侧车提供 device-id，
`nopwd_tool/backup` 则由文件名携带 device-id。两组共得到52份可验证 EDPF
捕获，按整份前部快照 SHA-256 去重后为33份；其中25份 mode tuple 为
`0,2,2`，8份为 `2,2,0`，仍然没有 mode1/mode3。该扩展扫描包含历史/转换
状态，只用作“本地语料尚无正样本”的负证据，不并入22份严格原始盘计数，
也不提高完成度。

### 6.3 LBA12 +0x48..+0x5F：扩展 key-material 与 Reserved[7] 必须分开

两个 Linux 组件使用不同 ABI：`libedpedisk.so` runtime 的
`tagNewEdpPartionInfo`/header size 明确为 `0x60=96B`；
`libcemsfilesyscheck.so` 的 DWARF 则是 `sizeof=0x68=104B`，
其中 `+0x50 EncryptFileKey32[16]`、`+0x60 EncryptMode`、
`+0x61 Reserved[7]`。同名 C 结构不能按偏移直接混用。

对 Windows 96B packed runtime：

- `CreatePartitions / sub_1003DB50` 首先执行
  `memset(var_1364, 0, 0x120)`，一次清零完整3×96B entry；
- 后续 wrapped16 只写 `+0x38..+0x47`，EncryptMode 写 `+0x58`；
- `UserLogin` 从每条 entry `+0x38` 只复制16B，并从 `+0x58` 读取 mode；
- `sub_10026050` 改密码同样只读写16B `+0x38..+0x47`；
- Linux `libedpedisk.so::SetPartitionNewPass @ 0x52520` 对
  version=0x0206 也只更新16B `+0x38`。

22份 original real-device reference set 的66条 EDPF entry 重新统计：
`+0x59..+0x5F Reserved[7]` 为 **66/66 全零**。
这7B同时具备正式字段名、writer零来源、negative consumer 和实盘闭环，
因此三个 entry 共 **21B PARTIAL -> COMPLETE**。

相邻 `+0x48..+0x57` 后续又完成了单独的跨代闭环，不能再停留在
“存在字段名，所以用途未知”的阶段：

- 旧 `tagEdpPartionInfo` 是 **72B ABI**，只到 `EncryptFileKey@+0x40`，
  **old 72-byte ABI has no EncryptFileKey32 slot**；
- 104B checker ABI 才正式增加 `EncryptFileKey32[16]@+0x50`；
  `GetPartionFromOld` 把旧72B entry 转成104B时不填该数组；
- `CDiskReader::DecryptFileKey` 只读 natural `+0x40..+0x4F` 的16B主 wrapped key
  和 `+0x60 EncryptMode`；`CheckFileKeyCrc`、`ReadFileSysSector0`、
  `DecryptFileSysSector0` 同样没有 `+0x50` 值相关读取；
- packed 主挂载库 `libedpedisk.so` 构造 `PartitionHeader` 时会按值缓存完整96B，
  所以 packed `+0x48..+0x57` 会结构性进入对象 `+0x88/+0x90`。但按
  `PartitionHeader` 符号边界逐函数审计，这两个对象QWORD除构造写入外没有任何
  算法读取；正对照 packed wrapped-key `+0x38..+0x47` 映射到 object `+0x78/+0x80`，
  其中 `+0x78` 被 SMS4/AES128/OldEdp 三套解密路径实际取址并按16B消费；
- Windows current `CreatePartitions` 先把3×96B整表清零，后续不覆写 packed
  `+0x48..+0x57`；Windows UserLogin/改密也只使用主 wrapped16 与 EncryptMode；
- 22份 original real-device reference set 的66条现存 entry 中，该16B **66/66全零**。

因此这不是 Reserved[7] 的一部分，也不是当前第二把活动 file key；它是正式保留名
`EncryptFileKey32[16]` 的 **cross-generation compatibility slot**。旧ABI无槽，
新ABI保留并可随结构搬运，但当前已知算法链不消费；current packed producer显式零。
这满足本项目对“结构缓存但无语义消费”区域的 COMPLETE 标准，三个 entry 共
**48B PARTIAL -> COMPLETE**。未来若其它独立ABI出现非零该槽，兼容实现应结构保留，
不能因为当前66/66为零而强制清零。

**LBA12 EncryptFileKey32 compatibility slot structural-cache / negative-semantic-consumer closure**
与 `Reserved[7]` 继续作为两个独立门禁，避免再次混成“23B zero padding”。

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

1. **LBA7 / LBA12**：LBA7 legacy wrapped key8 已闭合；继续逐个追 EDPF 中 Version、NeedDisturb 其它 entry、LBA12 非实盘 mode1/mode3 分支和 pass-info 剩余字段。
2. **LBA4**：继续寻找旧 `HSerialCRC[5]` 的真正 producer。
3. **LBA6**：继续追 legacy MBR-underlay writer：已知 `0x1E0..0x1ED`
   是第3条 MBR entry 幸存区且与 LBA12 type4 对齐，下一步需要找到旧版
   “动态 MBR table -> BuildSector6” producer 或直接读取该 fragment 的 consumer。
4. **LBA8 header residual**：动态 `+0x080..0x1FF` 已按注册侧7-key reader + 运行时17-key reader + encrypted backing/preserve tail 完整闭合；继续追 `HDSerialInfo@+0x14` 与 `UsbOnlyInfo[32]@+0x1E` 的 legacy producer/consumer。
5. **LBA9/10**：继续追 EETU `reverse[104]`、EESI `+0x04` 及
   `+0x28..` 未闭合区；EETU 时间/次数控制和两个16B EESI卷标槽已经完成。
6. **LBA0/1/2/3**：继续从官方 `RegsiterUsb` 的模板/读取路径向前追；
   LBA5 的 opaque write-protection probe 用途已经闭合，不再作为未知扇区。

## 9. 操作安全边界

本审计阶段：

- 可以读取本地反编译文件、二进制、历史原始备份；
- 可以修改 edpcli 代码、测试、文档；
- 可以生成内存/文件中的模拟 LBA0–12；
- **禁止对真实物理 raw USB 执行写入**，除非用户再次明确授权。
