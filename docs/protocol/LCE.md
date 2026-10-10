# LCE（LBA7 兼容扩展区）— 最终结论

状态：2026-10-10。本页仅保留经过原厂二进制、实体样本或回归测试支持的最终结果；不再以日期分批叠加逆向过程。机器结论：`audit/protocol/lba7_compatibility/evidence/lce_final_result_20261010.json`。原有物理字节账本、金标、实盘采集仍位于 `audit/protocol/lba7_compatibility/`。

## 1. 结论及证据等级

| 对象 | 判定 | 证据边界 |
| --- | --- | --- |
| LBA7 写入端/指针生成（类型与四模式） | 已证实 | 2026 原厂注册链到 `CreatePartitions` |
| 物理位置/大小、512B 设备 3072B 负载和 zero8 加解密 | 已证实 | 真实 Lexar/SanDisk 密文逐字节闭环 |
| 驱动物理读写映射与历史 LBA7 消费 | 已证实（特定旧版路径） | LBA7 回退、`EdpMountFile`、后端物理字节偏移 |
| 原厂 Windows 原生块读改写 | 已证实，但仅对 LBA0 MBR | 2022/2026 两代原厂机器码的 48B 表项替换 |
| Linux 读取原生 4Kn 扇区与算法限制 | 已证实（静态代码） | `BLKSSZGET`、512B 密码子单元和不支持 AES_CROSS 的模式判断 |
| 上层业务触发：首次写入 LCE 3072B 对象 | **OPEN**（未证明） | 文件中有镜像、驱动可写不等于业务实际写入 |
| 4Kn 原生块末尾 1024B 的归属和厂家生成 | **未证明** | 无法凭 3072B 历史负载推导完整 4096B |
| 真实 4Kn 新制盘、独立消费者、重插读写 | **未验收** | 静态与合成测试不能替代实体兼容测试 |

**最重要的最终边界：** 512B 旧版 LCE 的 3072B 对象可以按已验证规则复现；4Kn 的 LBA7 指针可由来源读取验证，但 **1024B 尾部仍未知，不能声称“加密全零”是原厂规则**。

## 2. 历史 LCE 的实际结构

- LCE 是项目定义的“LBA7 兼容扩展区”，不是 IIR，也不是当前 LBA12 的普通 type4 保密分区。
- `PartionType`：`1` 启动、`2` 交换/二合一、`4` 保密。原厂旧表的模式对应：Mode0 `[1,2,4]`，Mode1 `[2,4]`，Mode2 `[1,4]`，Mode3 `[1,2]`。Mode2 的 type1 兼容条目有 `0x7E00` 字节特殊规则，不能当作正常用户可见启动区。
- 原厂一方写入链：`cemssafeudisklabeltool_orig.exe` → `usbtoolbusmanage.dll` → `cemsusbregsiter.dll` → `CUsbRegsiter::CreatePartitions` → LBA7。后续旧版表条目保留各自类型，但可共同指向 CHS 尾部的同一 `0xC00` 字节兼容物理对象；**type2 不是 type4 的别名**；固定物理扩展区**并不专属于 type4**。
- 512B 真实 Lexar/SanDisk 的密文经旧版 `zero8` EDPSECDISK 变换及物理后端字节偏移修正，可精确恢复/重建 3072B FAT16 兼容明文（含“旧客户端需升级”提示）。原始证据位于 `audit/protocol/lba7_compatibility/byte_ledger.tsv`、`gold/`、`live_captures/`。
- 旧 `EdpEDiskCtrl` 可以在新标签不适用时回退 LBA7 旧表，经 `EdpMountFile` 建立虚拟卷并将已挂载的虚拟 I/O 映射到后端字节偏移。这证明“旧消费者能访问该对象”，不证明所有时期都存在相同的首次写入行为。IIR 保持独立模块和独立证据链。

## 3. 2022/2026 Windows 原厂二进制：最终确认与排除

1. 两版 `vrvaud_c.dll` 均可检出同一历史 3072B FAT16 警告镜像，也存在物理写入 API 与外部磁盘升级调用链。但静态直接寻址/相关代码审计**未建立**“这个警告镜像缓冲区 → 原厂首次 LCE 写请求 → 目标偏移 → 真实回读”的闭环；被排除的只是特定静态直接引用方式，间接调用仍不能断言不存在。
2. `cemsusbregsiter.dll` 中注册到 `fmifs.dll!FormatEx` 是普通分区文件系统格式化链；`EdpEDiskEx.dll` 的会话 INI 写入属于挂载配置。这两条均不能充当历史 LCE 首次写入者。
3. 2022/2026 原厂内嵌 `EdpEDiskEx` 挂载 ABI、配套驱动虚拟写入后端映射已有机器码实证；仍不能逆向推断是哪一个业务流程真实写入 3072B 警告对象。
4. **原厂 4Kn 相关正例但不是 LCE：** 两版 `vrvaud_c.dll` 均从 LBA0 读取一个动态原生逻辑扇区，仅替换 MBR `[462:510]` 的 48B 分区表项，再写回完整原生块。它支持“原厂部分路径会保留 4Kn 块未知尾部”，但不能推导 LCE 3072B→4096B 的填充算法。

上述验证由 SHA 绑定的脚本复现：`scripts/protocol/audit_oem_lce_format_producer.py`、`audit_vrvaud_writer_provenance.py`、`audit_lce_upgrade_notice_image.py`、`audit_vrvaud_mbr_rmw.py`、`audit_edpediskex_mount_ini.py`、`audit_embedded_edpediskex_versions.py`、`audit_oem_embedded_driver_mapping.py`、`audit_vrvaud_disk_upgrade_launch.py`、`audit_vrvaud_lce_template_xrefs.py`。

## 4. Linux 与 U391：不能混为一个故障原因

- 原厂 Linux `libedpedisk.so` 使用 `BLKSSZGET` 获取真实逻辑扇区大小，部分元数据访问及读写按此大小计算，**不是所有路径都错误地固定 512B**。
- `libsectorManage.so` 部分元数据加密读写按 512B 对齐；内核卷加密也存在 512B 密码子单元。这可能是合法密码分组设计，尚未证明其导致真实 4Kn 访问失败。
- 所检查 Linux `Volume::GetPartitionHeader` 分区头实现只接纳模式 1（AES）和 2（SMS4），未处理模式 3（AES_CROSS）。这是独立于扇区大小的兼容性限制。证据脚本：`scripts/protocol/audit_linux_client_4kn_mount_boundary.py`。
- 先前 U391 默认密码/SM4 转换仅生成**离线元数据候选**，无法把已有 AES_CROSS 保密密文自动变成 SM4，也没有证明原厂 4Kn LCE 生产者或 Linux 消费者能接受候选。该探索性生成示例不纳入最终受支持的制盘接口，源码可从 Git 历史复原。

## 5. 对 edpcli 的最终约束

1. 512B 历史兼容对象只使用 `src/protocol/lba7.rs` 与 `lba7_compat.rs` 的统一解析/序列化及字节账本；不另建第二套。
2. 4Kn 读取按真实设备报告的逻辑扇区宽度、LBA7 指针、身份和完整原生回读执行。未知尾部必须明确保留或拒绝；绝不静默写零、重复历史 FAT16 镜像或冒称官方等价。
3. 原有 3072B 解密正确不等于新制造 4096B 块正确，也不等于挂载/密码域互转成功。
4. AES_CROSS 的 Linux 不支持是独立算法问题，不要归类为 4Kn 几何缺陷。
5. 若要提高未闭环问题的证据等级，**仅需新增两类独立证据**：厂家可信 4Kn 完整金标（LBA7/LCE 原生块）与一次可归因的实际 LCE 写入轨迹（源缓冲区、长度、目标、重插读回）。缺少它们时不再反复增加猜测性笔记。

## 6. 可重复验证

本次使用本机原始 Windows 2022/2026 PE 与原厂 Linux ELF 进行只读 SHA/指令级审计，`33/33` 测试全部通过、`0` 跳过、`0` USB 实盘写入：

`uv run --locked --group protocol python -m unittest discover -s scripts/protocol/tests -p 'test_audit_*.py' -v`

测试会通过 `EDP_OEM_*` 环境变量读取外置样本；缺少原厂样本时部分用例会显式跳过，不得将这种情况称为原厂完整复验通过。原有已验证证据保留；9 个逐次调查 JSON 被单一最终结论 JSON 替代，详细原厂机器数据可用 SHA 绑定复验脚本重新生成。
