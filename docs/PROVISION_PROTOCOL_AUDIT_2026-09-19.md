# 新 U 盘 Provision 协议审计（Phase 0）

日期：2026-09-19
范围：仓库中 23 份真实设备 LBA0–13 备份；本阶段只读，不对物理 raw disk 写入。

## 结论

1. onlyid 不是 device_id / VID / PID / 容量的确定函数。
   - 同一 Netac 0dd8:2005、相同容量、相同 device_id 的真实样本存在多个不同 onlyid。
   - 样本同时存在大于 i32::MAX 的十进制文本和负数文本。
   - 因此 Provision 第一版必须显式接收 onlyid，内部按 32 位位模式解释；在没有新的可验证证据前禁止发明自动生成算法。

2. LBA12 0x170..0x200 的 144B 不是 donor 随机保留区。
   - 对全部已提交真实备份逐字验证：
     tail == a7f0_full(144B zero, CRC32(device_id), initial_counter=0x170)。
   - 因此新盘可由目标 device_id 纯生成该区域，不复制 donor。

3. 保留扇区策略已经有真实样本证据。
   - LBA1、2、5、10、13：全部 23 份样本均为全零。
   - LBA3：22 份为全零；唯一非零样本带 Kingston 制造标记 this is mp mark，同型号另一真实样本仍为全零。
   - Provision canonical profile 将 LBA1、2、3、5、10、13 生成为全零，不复制厂商制造私有标记。

4. LBA8 的 GLAB canonical 值在所有可解码样本中一致：
   322CA28A-D7D1448B-DCE2CED9。
   - User / Dept 是动态业务字段。
   - Autonum 在历史样本存在不同世代，因此作为 profile 字段处理，不从 donor 复制。

5. LBA9 在历史原盘存在 EETU/SAPF 与全零两种合法形态；现有 apply 的最终免密形态会清零 LBA9。
   - 新盘 Provision 的第一阶段目标与现有免密产品形态一致，canonical profile 采用全零 LBA9。
   - 不尝试复制厂商/旧版本 SAPF 的未知附加字段。

6. LBA6、LBA7、LBA8、LBA11、LBA12 已有 decoder 足够作为生成后的反向验证器。
   - LBA6 checksum、device CRC；
   - LBA7 rolling-XOR/EDPF；
   - LBA8 LLGB/User/Dept；
   - LBA11 PDKB + 目标 device_id；
   - LBA12 A6B0/EDPF + 144B tail。

## 尚不能猜测的材料

- LBA4 中除 onlyid、已知 LLGB 常量之外的生成期动态字节；
- EDPF 的每盘密钥材料中尚未证明的随机/摘要生成关系；
- LBA11 前 256B random。

这些内容不得从当前插入 donor 盘复制，也不得以全零替代。实现中把它们显式建模为
ProvisionEntropy / ProvisionProfile 材料；纯 builder 只消费已经验证的输入。
其中 LBA11 random 明确作为每次 provision 的熵输入。若后续逆向得到确定生成公式，
再用新的金标测试替换显式材料。

## Phase 0 门禁

tests/provision_protocol_audit.rs 固化以下事实：

- canonical reserved sectors 的真实样本证据；
- 同硬件身份存在不同 onlyid；
- LBA12 tail 不是全局常量；
- 全部真实样本的 LBA12 tail 均可由目标 device_id 纯生成；
- canonical GLAB 在真实样本中的一致性。

Phase 1 以后不得绕过这些门禁，也不得把未知区域重新退化为 donor copy。
