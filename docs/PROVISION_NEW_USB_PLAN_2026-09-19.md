# 新 U 盘 Provisioning 实施计划（2026-09-19）

## 1. 目标与边界

基线：`v2.1.0` / `main@97600fd175ff2151b9cd6d2757741163148e4368`。

新增“把普通全新 USB 生成成 EDP/cems 元数据盘”的能力。第一阶段目标严格限定为：

- 识别一块普通 USB 整盘；
- 根据目标盘自身硬件身份、容量和用户输入生成完整 **LBA0–13（14 * 512B）** 元数据镜像；
- 支持离线 plan / image / verify；
- 最终支持经过完整安全链后写入目标 USB；
- CLI 与 TUI 共用同一 application/service 和协议 builder；
- **不自动格式化数据区，不创建/重建文件系统，不写 LBA14 及之后区域。**

这与现有 `apply` 不同：`apply` 是对已有 cems 盘做保守转换，依赖原盘 LBA0–13 的 type4、reserved bytes、LBA12 尾部 144B 等材料；Provision 必须能够从经过验证的 canonical profile + 目标盘动态身份重新构造。

预计正式发布版本：v2.2.0。实现阶段不要提前 bump 版本。

## 2. 产品接口

CLI 规划：

```text
edpcli provision plan  --disk <selector> [metadata options]
edpcli provision image --disk <selector> --output <file> [metadata options]
edpcli provision verify --image <file>
edpcli provision write --disk <selector> [metadata options]
```

建议的元数据输入至少包括：

- User
- Dept
- Label（如协议确认需要）
- onlyid：先按 Phase 0 审计结论决定自动生成算法；结论未锁定前允许显式输入但不得随意发明算法

TUI 规划：

- Device dashboard 对“普通 USB / 非 cems 盘”提供 `p` Provision；
- command palette 增加 `:provision`；
- 向导先展示目标硬件身份、device_id、onlyid、User/Dept、容量和 14 扇区变更摘要；
- 真正写盘必须输入完整确认词 `PROVISION`，不能复用单字符确认。

## 3. 架构

新增纯领域层，禁止 builder 直接读写真盘：

```text
HardwareProbe + user metadata
        ↓
ProvisionSpec
        ↓
ProvisionProfile
        ↓
pure builders
        ↓
ProvisionImage [LBA0..13]
        ↓
ProvisionValidator
        ↓
application/provision service
        ↓
CLI / TUI
        ↓
shared write safety service
```

建议模块：

```text
src/application/provision.rs
src/provision/mod.rs
src/provision/spec.rs
src/provision/profile.rs
src/provision/generate.rs
src/provision/validate.rs
```

前端不得直接依赖 platform/raw write。

## 4. Phase 0 — 协议样本审计（先做，不写盘）

允许连接 Mac。优先使用现有真实备份和只读设备采样，建立测试 fixture，不直接修改物理 U 盘。

必须回答：

1. LBA0–13 各字节在不同厂商/容量/设备间哪些是常量，哪些与身份或容量相关；
2. onlyid 的真实取值范围、正负、重复率、与 device_id/VID/PID/序列/容量是否存在确定关系；
3. LBA4 的完整构造规则和 rolling-XOR 区域；
4. LBA6 SAFE6 中 Label/User/Serial/device CRC/注册位/模板值/校验和的真实生成规则；
5. LBA7/LBA12 EDPF entry 的 canonical defaults、type2/type4 布局与终止符；
6. LBA8 LLGB 的完整编码、User/Dept 等字段边界；
7. LBA9 新盘应为 zero 还是 canonical SAPF；
8. LBA11 PDKB 的 random、VID/PID、容量和 device_id 构造闭环；
9. LBA12 未加密尾部 144B 与其他 reserved bytes 是否可固定为 canonical profile；
10. LBA1/2/3/5/10/13 的真实策略。

禁止为了赶进度把未知区域直接全零或复制 donor 盘身份。

状态：Phase 0 已完成。审计结论与可重复门禁见
docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md 和
tests/provision_protocol_audit.rs。关键结论是：onlyid 暂不自动生成；LBA12 tail
已证明可由目标 device_id 纯生成；canonical reserved sectors 已锁定；未知生成期
材料必须进入显式 profile/entropy，禁止 donor copy 或臆造清零。

## 5. Phase 1 — contract tests + ProvisionSpec/Profile

测试先行，先让旧代码失败。

锁定：

- Provision 只能面向 USB whole-disk；
- 系统盘/非 USB 必须拒绝；
- builder 不访问磁盘、不执行命令；
- `device_id` 必须来自目标盘硬件 probe；
- 无法可靠得到 Vendor/Product/Transport 等必要身份时 fail-closed；
- image 固定为 7168B；
- reserved/canonical profile 有显式版本；
- User/Dept/onlyid 输入边界；
- CLI v2 既有命令不回归。

## 6. Phase 2 — LBA builders

优先拆出并复用已有协议原语。

### LBA0

从目标容量和布局生成 MBR。第一阶段只生成 metadata 约定的分区表；不得格式化 LBA63 后的数据区。

### LBA4

建立独立 builder：

- `$$$<labelOnlyId>$$$`
- onlyid 派生 K0
- 0x18 之后 rolling XOR
- LLGB/其他常量按 Phase 0 结论生成

### LBA6 SAFE6

由 plaintext builder 构造后：

```text
plaintext[0..508]
→ rolling XOR(K0=0x4DAA)
→ lba6_checksum(ciphertext[0..508])
→ 512B
```

必须通过现有 decoder/inspect round-trip。

### LBA7 / LBA12 EDPF

抽象统一的 `EdpfEntry` / table builder，避免 provision 再复制 convert 逻辑。

LBA7：按 `CRC32(device_id)` 派生 K0 做 rolling XOR。

LBA12：前 368B 使用现有 a7f0/A6B0 变体；尾部 144B 使用 Phase 0 锁定的 canonical profile，不能随意清零。

### LBA8 LLGB

新增结构化 builder，至少覆盖 Dept/User 以及 Phase 0 确认的字段。生成后必须能被现有 `ownership_from_lba8` 反向解析为同值。

### LBA11 PDKB

按目标盘真实：

- VID
- PID
- 容量
- 256B random

生成 key，构造 `PDKB + device_id`，加密后必须能被现有 decoder 反向得到目标 device_id。

### 其他 LBA

LBA1/2/3/5/9/10/13 只允许使用 Phase 0 已验证的 canonical policy。

## 7. Phase 3 — 全量离线 Validator

`ProvisionImage` 生成后，在任何写盘前必须离线自证：

- 长度精确 7168B；
- MBR/布局一致；
- LBA4 onlyid round-trip；
- SAFE6 checksum 与 device CRC 一致；
- LBA7/LBA12 解密后 EDPF 结构一致；
- LBA8 Dept/User round-trip；
- LBA11 PDKB/device_id round-trip；
- `identify / inspect / metainfo` 能识别生成镜像；
- 免密/状态判断与目标设计一致；
- 所有 reserved bytes 与 profile fixture 精确匹配。

## 8. Phase 4 — CLI dry-run

先只实现不写盘的：

- `provision plan`
- `provision image`
- `provision verify`

要求普通用户权限下可完成纯 image verify；硬件探测需要权限时复用现有 elevation 机制。

生成 image 应能在没有真实盘写入的情况下用于测试与审计。

## 9. Phase 5 — 安全写入

复用并必要时泛化现有 application write service，禁止 provision 自己调用 raw write。

安全链：

```text
system disk guard
→ USB whole-disk guard
→ 确认当前为普通/目标 USB
→ native selector pinning
→ 读取并备份原 LBA0–13
→ ProvisionImage 离线 validate
→ 用户输入 PROVISION
→ prepare_write / lock / unmount
→ reopen
→ capacity + VID/PID + selector identity recheck
→ atomic write LBA0–13（LBA0 最后）
→ sync
→ readback 14 sectors bit-for-bit
→ protocol-level verify
→ failure rollback 原 LBA0–13
```

普通 U 盘原本的数据可能被新的 MBR 隐藏，因此 UI/CLI 必须明确这是破坏性 metadata 初始化。

**未经用户再次明确指定某块测试物理 U 盘，不要在 Mac 上对真实 raw disk 执行 provision write。** Mac 在 Phase 0–4 可用于样本分析、只读探测、编译和测试。

## 10. Phase 6 — TUI Provision Wizard

- 普通 USB 行显示“可 Provision”；
- `p` / `:provision`；
- 分步编辑 User / Dept / onlyid 等；
- 预览 device_id、容量、LBA0–13 结构；
- 权限提升后保持 native selector pin；
- 提权后再次确认 `PROVISION`；
- critical operation 中 q/Esc/Ctrl-C 延迟退出；
- 完成后后台重新扫描并显示为新 EDP/cems 盘。

## 11. Phase 7 — HIL / 回归

必须扩展虚拟磁盘 HIL：

### Linux loop

普通磁盘 sentinel LBA0–13
→ provision
→ readback
→ list/info/inspect/verify
→ restore 原 14 sectors
→ sentinel 完整恢复

### Windows VHD

同样覆盖 lock/dismount/reopen/write/readback/rollback。

必须继续保持：

- macOS/Linux/Windows × arm64/x86_64 Rust CI 全绿；
- CLI v2 与 TUI v2.1 行为无回归；
- 不提高/绕过现有安全门禁；
- 不新增系统盘或物理 raw device 测试旁路。

## 12. 实施规则

- 测试先行；
- 小 commit、及时 push；
- 禁止 reset/clean 覆盖 WIP；
- 不复制 CLI/TUI 两套业务逻辑；
- 不从 donor 盘复制 onlyid/device_id/User/Dept/VID/PID/容量派生材料；
- 协议未知处先审计、加 fixture 和文档，再实现；
- 正式 v2.2.0 发布前才升级版本。
