# 深度文件系统分析 v1

深度是元数据的只读超集，不读取普通文件的数据负载簇。原始元数据证据保持为 `evidence_only`；摘要、文件列表和解码扇区等派生结果标记为 `derived_only`。默认密码的 LBA12 v0x0206 模式2 条目只有在文件密钥通过 `FileKeyCRC` 校验后才允许解码。其他加密配置类型保持锁定状态。

## 分析约定

`edpcli.deep.filesystem.v1` 的状态包括 `parsed`、`locked`、`unsupported`、`parse_failed` 和 `not_captured`。只有 `parsed` 状态可以提供目录清单以及空间/数量统计；其他状态必须使用 JSON null，禁止伪造空卷。分区几何中的 `partition_bytes` 与文件系统的 `total_bytes` 是两个不同概念。仅有一段元数据前缀不能证明已经获得完整目录清单。

FAT16 和 FAT32 使用 BPB 簇数量、分配表和有界目录遍历进行只读解析。路径以 `/` 开头；目录以 `/` 结尾。根目录会列出，但不计入 `directory_count`。文件数量按路径计数，不包含已删除条目或卷标。FAT 空闲空间按 FAT 中值为零的条目计算；已用字节包含已分配簇、保留结构以及尾部不完整簇。文件的已分配大小按完整 FAT 链计算。时间使用文件系统本地 ISO 8601 墙上时间，不臆造 UTC 偏移；`ctime` 表示创建时间。无效或不可用的时间戳使用 null。长文件名要求 UTF-16、序号和短文件名校验和全部有效。由于无法安全推断 OEM 代码页，目前对非 ASCII 短文件名采用无法确认即拒绝继续的策略。

解析器会拒绝环、交叉链接、非法名称、非法大小、镜像 FAT 不一致、短扇区以及越过分区边界的读取。工作上限为：4,194,304 个数据簇、100,000 个条目、4096 字节路径、每分区 32 MiB 目录元数据和 64 MiB 原始读取证据。超过限制属于解析失败，不能把不完整结果冒充完整清单。普通文件数据始终不读取。

exFAT 采用主引导区校验和、活动 FAT、分配位图和有界目录遍历进行只读解析。文件条目集必须具有有效的条目集校验和和 UTF-16 文件名序列。通过 FAT 链分配以及连续 `NoFatChain` 分配都会校验分区/簇边界和分配位图。允许读取目录与文件系统元数据簇；禁止读取普通文件负载簇。NTFS 当前仍不支持，是后续文件系统解析阶段。没有已验证默认模式2 密钥的加密 type2/type4 分区必须报告 `locked`，即使密文碰巧包含文件系统签名也不能改变状态。`PartitionReader` 只暴露相对扇区读取。解码读取器会把每个解码后的扇区保存为派生产物，并引用对应原始扇区证据。有效的文件密钥 CRC 只能证明解包后的密钥正确，不能单独证明文件系统目录已经完整获取。

## 参考资料

FAT 布局、簇分类、目录项和长文件名规则：

[Microsoft FAT 文件系统规范 v1.03](https://www.cs.fsu.edu/~cop4610t/assignments/project3/spec/fatspec.pdf)。

exFAT 引导区、分配位图、FAT 链和目录条目集：

[Microsoft exFAT 文件系统规范](https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification)。

现有 EDP 分区/密钥证据见 [EDP 协议逆向总文档](../protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md) 第 6.1 节及后续内容。只有当 `UserKeyCRC` 识别为 `0000aaaa` 时才使用 v0x0206 默认替代规则；禁止猜测密码或伪造密钥材料。

## 采集与重放

`edpcli backup create --disk N --deep` 使用与元数据相同的 `O_RDONLY` 设备打开路径。不使用 `--deep` 时，手工备份和自动备份仍保持元数据级别。深度首先获取元数据（包括 LCE），然后把额外原始读取范围以及 `derived.partition.<index>.filesystem_summary`、`derived.partition.<index>.file_list` 存入同一个 EDPB。使用数字索引是为了在分区表存在多个同类型条目时避免冲突。每份 JSON 同时记录 `partition_type`。分析未完成时，文件列表中对应项必须为 null。不会创建旁挂文件。解析失败不能阻止已经成功读取的原始证据或既有元数据产物被保存。

原始读取会缓存并合并为连续证据范围。已经采集的元数据字节优先于后续对同一扇区的读取。这不是文件系统冻结：如果源设备正在并发变化，可能得到失败或不一致快照，因此禁止描述为原子文件系统快照。深度不会卸载或改变源卷状态。

只读评估既有元数据 EDPB：

```sh
cargo run --example deep_assess_metadata -- /path/to/backup.edpb
```

该命令会验证源容器，只报告前缀评估结果。对于已验证的默认模式2 密钥，它可以识别解码前缀中的文件系统签名，但不会报告卷统计或文件列表；也不会把源容器重新标记为深度，更不会伪造元数据中不存在的扇区。

2026-09-22 的 Lexar LCE 采集容器 SHA-256 为 `0a52a9cb52e47ca5e11d3a74d8c6f9f07937b0dc3d06f4d0802f3fb5917e254f`。在加入默认模式2 解码前曾进行离线重放：type2 分区为 118477684736 字节，type4 分区为 6234963968 字节，当时两个分区都报告 `locked`，并未建立文件系统目录清单。重放过程没有打开源设备。

之后又对现有 Lexar 元数据 EDPB（容器 SHA-256=`38575406e72006f003deabdf485a33df14de979d48c9a99150691aea2e926f7d`）执行离线重放，使用 `deep_default_probe` 检查两个加密分区前缀。两个解码前缀都包含 `EXFAT   ` 引导签名，并具有有效的 12 扇区 exFAT 引导校验和。该历史重放早于 exFAT 目录清单解析器，因此 `deep_assess_metadata` 当时报告 `unsupported` 且 `filesystem=exfat`，数量、大小和条目均保持 null。当前解析器只有在完整深度读取器能提供所需 FAT、分配位图和目录元数据时，才可以生成完整目录清单；仅有元数据前缀仍然不够。该重放读取的是备份文件，而不是源设备。
