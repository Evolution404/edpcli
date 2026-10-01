# Inspect 界面与语义治理计划（2026-10-01）

## 1. 目标与硬原则

1. Inspect 只展示可证明事实，不用 Raw/Decoded 数值是否相同推断协议语义。
2. 左侧结构树负责“位置与层级”；右上对象摘要负责“结论”；底部字段/证据负责“依据”。
3. 删除 Browser 顶部 `业务字段 / 原始字段 / Hex` 三 Tab；Business 融入对象摘要，RawFields 融入字段/证据，Hex 仅作为下一级扇区检查。
4. Mixed 同时表达 FieldStatus 与 Decode provenance：Known/Unknown/Reserved/Preserved 保留，Decode 使用独立视觉层。
5. 设备 Inspect 与备份 Inspect 共用 Loading、错误处理、Browser 与扇区检查。
6. Raw 必须忠实展示真实字节，不能为了隐藏实现痕迹过滤 Hex/ASCII。

## 2. 当前技术债

### 2.1 全屏 Loading
`AdvancedInspectStage::Running` 当前直接占满 Inspect content area。目标改成来源 Devices/Backups 页面上的全局居中 modal；成功后才真正进入 Inspect，失败留在来源页。

### 2.2 顶部三 Tab
当前 Business / RawFields / Hex 已与结构树、对象摘要、字段表、Sector Inspector、Raw/Decode/Mixed 重叠，形成多余导航层。删除用户侧三 Tab，并清理对应死状态/死 renderer。

### 2.3 右上与底部重复
当前“对象快照”和“字段详情 / Evidence”重复输出 profile、backing、partition、notes、parser 信息。必须改成“摘要=结论、证据=依据”。

### 2.4 Mixed 判断错误
当前通过 `decoded[index] != raw[index]` 判断着色。正确语义是“该 byte 是否实际经过 Decode”。即使 Raw=00、Decoded=00，只要属于真实 Decode range，就必须显示 Decoded。

## 3. 目标信息架构

### 一级：Inspect Browser
- 左：结构树。
- 右上：对象摘要。
- 底部：字段 / 证据。
- 不再存在全局 Business / RawFields / Hex Tab。

### 二级：扇区检查
在 Browser 选中可打开 Sector 后按 Enter：
- 左侧 Hex + ASCII。
- Raw / Decode / Mixed。
- 右侧字节 / 字段。
- Esc 返回 Browser，并保持原 LBA/节点。

## 4. Loading 生命周期

目标状态机：
Devices / Backups → 创建 Running request → 后台 worker → 来源页保持显示但冻结输入 → 居中 Loading modal → 成功后 push navigation 并进入 Inspect Browser；失败则保持来源页并显示 Error modal。

### Loading modal
- 设备与备份共用一个 renderer。
- 全局居中，不随 pane 偏移。
- 中间使用 `animation::spinner_glyph(state.animation_frame())`。
- 遵守 Full / Reduced / Off animation setting。
- 主文案：正在建立全盘结构树；次文案：正在读取协议上下文；状态：只读分析。
- Running 时背景 j/k/h/l/Tab/Enter/J/search 均不生效。
- worker 没有 cancellation token 前不提供假“Esc 取消”。

### Loading 失败
禁止进入 `Browser + result=None`。失败后留在原 Devices/Backups，并显示居中错误 modal。

## 5. Browser 三块职责

### 左侧结构树
只负责整盘层级、Protocol/Partition/LCE/Reserved/Free/Tail、LBA、范围、展开折叠和当前选择。禁止放 Raw dump、复杂 notes 和大段 field value。

### 右上对象摘要
固定只回答四类问题：对象身份、状态、关键结论、来源。禁止完整 Raw 和完整字段清单。

### 底部字段 / 证据
默认表格列：`状态 | Offset | 长度 | 字段 | 值`。
FieldStatus 至少：Known、Unknown、Reserved、Preserved。
选中 Field 后展示：Raw、Decoded、Range、Decode Range、Source、Notes、Children。

## 6. 节点内容矩阵

| 左侧对象 | 右上对象摘要 | 底部字段/证据 |
|---|---|---|
| 整盘 | 容量、盘型、协议状态、区域数量 | 区域/分区表 |
| 协议区 | LBA0–12 完整性、异常数、Decode 可用性 | LBA 列表及状态 |
| 单个 LBA | LBA 角色、512B、关键结论、校验状态 | Field 表 |
| Partition/Region | 起止 LBA、容量、文件系统、加密/明文 | 几何与文件系统证据 |
| Field | 名称、值、状态 | Raw/Decoded/Range/Source |
| Unknown | 范围、长度、Unknown | Raw 摘要/Hash |
| Reserved | 范围、Reserved 原因 | 范围与保留定义 |
| Preserved | 范围、Preserve 策略 | 原样保留依据 |

## 7. Mixed 完整语义

Mixed 不是 diff view。正式定义：Mixed 显示 decoded 内容；前景表达 FieldStatus；独立视觉层表达 Decode provenance；Raw 与 Decoded 是否相等不参与 DecodeStatus 判断。

### 三个正交维度
FieldStatus：None / Known / Unknown / Reserved / Preserved。
DecodeStatus：Plain / Decoded / Unavailable。
InteractionStatus：Normal / Cursor / SearchMatch。

例如 `Unknown + Decoded + Cursor` 完全合法。

### 颜色职责
- FieldStatus 用前景色。
- Unknown 与 Reserved 必须视觉区分：Unknown=未知语义；Reserved=已知保留。
- DecodeStatus 使用独立低饱和背景/强调层；Unavailable 使用 warning overlay。
- Cursor/SearchMatch 最后叠加，但不能洗掉 FieldStatus 与 DecodeStatus。
- 禁止继续使用竞争式 `if/else-if` 决定单一颜色。

建议形成一个 style composer：FieldStatus → DecodeStatus → InteractionStatus。

## 8. Decode provenance 数据模型

禁止：
- `raw != decoded` 推断 Decoded。
- Known 推断 Decoded。
- Reserved 推断 Plain。

由真正执行协议解码/解密的 adapter/parser 显式返回 Decode coverage。优先使用 range，例如 `+0x040..+0x080`、`+0x100..+0x180`。

同时记录 availability：Available / NotRequired / Unavailable(reason)。如一个 Sector 存在部分可用、部分不可用，应支持 range 级 provenance。

Sector cache 必须保存 raw、decoded、decoded ranges/provenance、decode error/availability、fields，重新进入 Mixed 不得丢失 provenance。

## 9. Raw / Decode / Mixed

Raw：display=raw，真实磁盘/备份字节，不隐藏 OEM Name 或 ASCII。
Decode：display=decoded；Decode unavailable 时明确显示错误，不能 fallback 后假装就是 Decode。
Mixed：display=decoded（可用部分）；FieldStatus=前景；DecodeStatus=独立 overlay。

## 10. ASCII 与 Hex

ASCII 必须逐 byte 构造 Span，与对应 Hex byte 使用相同 FieldStatus + DecodeStatus；不可打印字符仍显示 `.`；cursor/search 同步。禁止整行统一 muted 导致语义丢失。

## 11. 扇区检查右侧“字节 / 字段”

固定展示：
- Offset / Raw / Decoded。
- Decode 状态：Decoded / Plain / Unavailable。
- Decode Range。
- Field / Status / Type / Range / Value。
- 展开 bits / children / notes。

颜色只用于快速扫描，文字字段才是事实来源。

## 12. J、lazy read、Esc

- J 在 Browser 和扇区检查全局有效，弹全局居中 LBA 输入 modal。
- Browser 中 J 选择对应 Sector node；扇区检查中 J 直接打开对应 Sector。
- 保留 lazy read：非预读 Sector 在选择/Enter/J 时读取。
- Esc：扇区检查→Browser；Browser→正确来源；Jump/Search modal→关闭自身；Loading 不做假取消。

## 13. 文案收口

用户侧统一为：对象摘要、字段 / 证据、扇区检查、字节 / 字段。
删除用户侧 Business、RawFields、Sector Inspector、Typed / Field、字段详情 / Evidence 等上一代术语。内部 enum 可后续清理。

## 14. 测试门禁

### Loading
- Disk/Backup 来源页仍可见。
- modal 全局居中。
- spinner 统一。
- 背景输入冻结。
- 成功才进入 Inspect。
- 失败回来源页。
- 不出现 Browser + None。

### Mixed provenance
| Raw | Decoded | DecodeStatus | 期望 |
|---|---|---|---|
| 00 | 00 | Decoded | Decode overlay |
| A5 | 00 | Decoded | Decode overlay |
| 00 | A5 | Decoded | Decode overlay |
| A5 | A5 | Plain | 无 Decode overlay |

另覆盖 Known/Unknown/Reserved/Preserved/None + Decoded、Decoded+Cursor、Decoded+SearchMatch、Unavailable。硬门禁：Raw==Decoded 不得改变 DecodeStatus。

### Browser / Navigation
- 按节点类型验证摘要与证据职责。
- J Browser / J 扇区检查。
- Enter Browser→Sector。
- Esc Sector→Browser。
- Esc Browser→正确来源。
- Disk/Backup 各跑一套。

## 15. 实施阶段

I0 Loading 生命周期：来源页 modal、共用 spinner、成功才进入 Inspect、失败留来源、删除全屏 Running page。
I1 删除全局三 Tab：Business→对象摘要，RawFields→字段/证据，Hex→下一级扇区检查。
I2 Browser 信息分工：摘要 renderer、字段/证据表、节点内容矩阵、去重复。
I3 Decode provenance：parser/adapter ranges、availability、cache、禁止 equality inference。
I4 Mixed style composition：FieldStatus fg、DecodeStatus overlay、cursor/search overlay、Unknown/Reserved 拆分、Hex/ASCII 同步。
I5 扇区检查详情：Raw/Decoded/Decode range/Field status/type/range/value/Unavailable。
I6 导航与收口：J、lazy read、Esc、error、回归测试、删除旧 renderer/死代码。

## 16. 完成定义

1. 无全屏 Inspect Loading。
2. Disk/Backup 共用居中 Loading modal。
3. 顶部“业务字段 / 原始字段 / Hex”完全消失。
4. Browser 仅结构树 + 对象摘要 + 字段/证据。
5. Hex 只存在于下一级扇区检查。
6. Mixed 不再依赖 raw!=decoded。
7. Known/Unknown/Reserved/Preserved 与 Decode provenance 可同时表达。
8. 00→00 但真实 Decoded 的 byte 仍有 Decode overlay。
9. Hex/ASCII 语义同步。
10. Unavailable 与 Plain 明确区分。
11. Raw 永远忠实展示真实字节。
12. J/Enter/Esc/lazy read/cache 在 Disk/Backup 均通过测试。
