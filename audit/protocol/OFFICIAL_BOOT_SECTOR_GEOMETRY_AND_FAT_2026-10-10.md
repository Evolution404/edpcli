# 官方制盘：启动区逻辑扇区几何和 FAT 类型取证（2026-10-10）

## 证据边界

本报告直接核验本地官方 Windows 二进制和原配置；未执行 Windows `fmifs.dll!FormatEx`，没有把自己生成的 FAT12 元数据冒充官方新写盘金标。

- GUI：`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemssafeudisklabeltool.exe`，SHA-256 `1b1ddfb92298f2860dfa82952557139d57e87daa6e387507f4bbd9ab18c27427`。
- 制盘 DLL：`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/cemsusbregsiter.dll`，SHA-256 `122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb`。
- GUI 配置：`/Users/zhangyuxi/Desktop/u_disk/VRV/cems/ydcc/res/usblabeltool/cfg/usbtoolcfg.ini`，[GLOBAL] `bootSize=10`，SHA-256 `59c374305cab0847e9237523762a2c9b2825f0bf9a3595e3a7576f00c1ef9495`。
- GUI `sub_48E8B0` 通过 Qt `QSettings::value` 读取 `GLOBAL/bootSize`。DLL 原机码 `0x10046D77..0x10046D8A`：从请求 `+0x7DC` 取 type1 启动区 MiB，执行 `mul 0x100000` 得到64位字节数；type2/type4 在 `0x10046D9C..0x10046DD4` 同理使用 `+0x7E0/+0x7E4`。
- DLL `CUsbRegsiter::CreatePartitions/sub_1003DB50`：`LBA63` 为第一个分区的原生起点；取真实逻辑 `SectorSize`；type1 的有效字节长度来自 `[bootMiB * 1048576] - [63 * SectorSize]`，后续条目从前段结束字节位置按 `SectorSize` 换算 LBA。参见 `docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md` 第5.9.4节与对应本地原厂 DLL 机器码。
- DLL `CUsbRegsiter::OnFormatDisk/sub_10045710` 运行时加载 `fmifs.dll` 并解析 `FormatEx`，`FormatDisk` 普通 type1 路径给此函数传入 `L"FAT"`（反编译原调用行：`sub_10045710(...,u"FAT",...)`；dll.m 约L81895）。官方应用层**没有固定传 FAT12 或 FAT16**。FAT 文件系统族的生成细节由所调用的 Windows 格式化组件按设备卷几何完成，不宜用媒体类型名猜测具体 FAT 版本。

## 当前官方默认 bootSize=10 下的几何

| 逻辑块 B | 首分区开始 LBA | 结束 LBA（独占） | 启动区长度（原生块） | 启动区实际字节数 | 证据等级 |
|---:|---:|---:|---:|---:|---|
| 512 | 63 | 20480 | 20417 | 10453504 | 官方原始 512B 样本及公式吻合，FAT16 样本 |
| 1024 | 63 | 10240 | 10177 | 10421248 | 由机器码按原生单位计算，未见官方 1024B 格式化金标 |
| 2048 | 63 | 5120 | 5057 | 10356736 | 由机器码按原生单位计算，未见官方 2048B 格式化金标 |
| 4096 | 63 | 2560 | 2497 | 10227712 | U391 原始 4Kn 实盘只读样本直接验证；FAT12 |

这些行共同拥有 **10MiB 的绝对结束字节位置**，不是 10MiB 的实际 type1 分区大小。公式对 10MiB 可整除的块大小为：`boot_blocks = boot_mib * 1048576 / logical_sector_bytes - 63`；不整除时官方边界舍入、分区有效性及 FAT 支持需另行验证，不允许机械套用。LBA63 是“63 个原生逻辑块”，不是固定 32256 字节。

## FAT12/FAT16 的原厂行为证据

- U391 官方 4Kn Mode0 原盘：LBA63、2497个4096B原生块，BPB `BytesPerSector=4096`、`SectorsPerCluster=1`、`Reserved=1`、`NumFATs=2`、`RootEntCnt=512`、`FATSz16=1`，数据簇 `(2497-1-2-4)/1=2490`，因此按 FAT 规范归类 FAT12。该数据来自已存档的 U391 *只读*盘面；**未据此声称原盘的 MBR type 字节已被独立确认**，而不是本项目新生成的虚拟盘。
- 原始512B盘样本：type1 从63起、长度20417，MBR type=0x0E、识别为FAT16；不可仅由容量推断簇规格，仍应检查 BPB。
- 微软 FAT 规范 1.03 第15页规定实际类型依 data cluster count 决定：簇数 <4085 → FAT12；簇数在4085..65524 → FAT16；再以上是 FAT32。来源：`https://www.cs.fsu.edu/~cop4610t/assignments/project3/spec/fatspec.pdf`。格式化器如何选择每簇扇区数仍与其实现有关，不能光凭逻辑块宽度推断所有输出类型。
- 证据边界：**已证实当前官方 DLL 调用的是通用 FAT；U391 真实4096B样本使用 FAT12**。尚未在 Windows 中使用该 DLL + 原生4Kn块设备执行 `FormatEx` 的端到端制盘，所以不声称所有发布版/OS版本都会选择完全相同的 BPB 字节序列，也不声称1024B/2048B默认最终为某一种 FAT 类型。

## 模式特例和 edpcli 正确实现建议

- 官方 `part=0`（mode0）与`part=3`（mode3）含独立 type1 启动区；`part=1`（mode1）只有 type2 + type4，没有独立启动区，前部“二合一”卷走交换区分支；`part=2`（mode2）有 type1 兼容保留条目，当前 DLL 的 `sub_10046E80` 特殊写入 `0x7E00` 字节，不是标准 `bootSize=10` 的 FAT 启动区，禁止强行套统一公式。
- 统一原生几何规划；起点63、目标结束边界在字节单位上计算后转换为原生 LBA，协议字段记录真实逻辑块尺寸；依分区 FAT metadata 规模、每簇扇区数和簇数量判断 FAT12/FAT16；文件系统格式化器分别生成完整原生逻辑块；对系统格式化能力不支持的几何或选项明确拒绝，而非扩大默认启动区或暗改设备几何。
- 新功能须保留当前 512B 正向字节金标，建立 4Kn 原厂只读 BPB 几何回归；执行真实 Windows 官方 `FormatEx` 前，4Kn FAT12 的具体引导代码、卷标、FAT Reserved 内容等只可标为“兼容实现”，不写成逐字节复刻完成。
