# 原生逻辑扇区与设备传输统一提交链

## 目标与事实

物理 USB 与 macOS 挂载的虚拟 Disk Image 通过相同的原生设备几何模型、不可变写集、TargetSession 写租约及 WAL 原生块事务执行。逻辑扇区大小是来自设备的独立属性，原生块读取及写入没有针对 512B、4096B 单独分叉。文件系统格式化能力另按格式规范约束。

命令 edpcli list --include-virtual 和 edpcli tui --include-virtual 显示确认属于外部虚拟 Disk Image 的设备，普通启动维持原有列表。设备是否可选、是否能取得写租约仍需在准备写入时根据当前系统事实复核；该参数不会把普通物理硬盘变成虚拟盘。

统一提交函数位于 src/application/provision/native_commit.rs。它获取现有目标会话、核实完整几何、读回来源原生前 13 块、锁定并重新打开同一设备、复核前 13 块及身份后，调用统一 WAL 原生块提交与独立回读机制。写集本身必须由同一原生规划逻辑根据目标模式和分区格式化需求生成。来源用户数据在当前测试中的目标重建属于破坏性制盘，不可称为无损转换。

## macOS 系统真实 4Kn 设备验收

执行 bash scripts/ci/macos-native-4kn-virtual-disk-hil.sh。脚本自行创建 512MiB 稀疏镜像，以 hdiutil -blocksize 4096 挂载；每次写入前都验证 WholeDisk、Internal=false、Virtual、Disk Image、4096B 及实际容量。之后通过同一原生写入函数依次重建 Plain、Mode0、Mode1、Mode2、Mode3、Plain；每次提交都持久化 WAL，卸载后重挂并独立核对原生写集、协议模式、LCE 与可验证文件系统结构。Plain 额外经过 macOS 挂载和实际文件读写。

这一验收证明实际 macOS 虚拟块设备的原生写入和事务链，不证明 USB 控制器行为，也没有证明所有 25 对模式组合、用户数据保留及正式 TUI 全流程已全部实现。原先基于 512B SectorDev 的正式制盘入口仍需迁移到统一原生规划器；不能借虚拟 HIL 测试宣称全功能已完成。

## 2026-10-10 后续实现与验收收口

正式 CLI 的 plan、image、write 与 TUI 制盘应用服务已接入统一的 native_flow 原生规划器。不同设备的只读来源识别使用 provision/native_source，虚拟镜像身份在 platform/system 层正常化。选择虚拟盘仅改变设备发现范围，后续通过共同 TargetSession、NativeRawBlockDevice、原生写集、WAL 和独立回读执行，不以逻辑扇区大小拆成两种提交链。

正式 CLI 实机虚拟块 HIL 入口为 scripts/ci/macos-cli-native-virtual-hil.sh，循环测试 512B 和 4096B 两种系统实际识别的设备，执行六次连续重建和卸载再挂载，并核验 Plain exFAT 卷实际文件写入持久性。更完整的 4Kn 任意来源模式到任意目标模式破坏性测试使用 scripts/ci/macos-native-mode-matrix-hil.sh，每次先实际制成来源模式，重挂并识别，再实际转换成目标模式，重挂并重新独立识别。

重要修复：EDP→Plain 不能只改 MBR，否则 LBA7、LBA12 旧注册内容与新分区表冲突。现在把失效协议块和来源 LCE 纳入同一原生事务清理写集，仍保留制造商 LBA3，并确保 LBA0 最后提交。原生 WAL 是单次事务中修改块的持久原始快照，不等同于可携带的 EDPB 元数据备份。早期模式矩阵采用显式破坏性重建作为验收场景，不能将其视为当前默认行为；统一来源感知规划器默认保留经验证兼容的来源数据和原 `FileKey`，范围或语义不兼容时必须经确认后格式化重建。既有破坏性矩阵不构成无损保留的全范围哈希证据。

已通过的单项事实包括：4Kn Mode3→Plain 后独立重新识别为普通盘；普通盘再进入 Mode1 正式 CLI 只读计划；512B、4Kn 的 12 次连续正式 CLI 模式重建。全量 25 对矩阵和工程门禁以最后一次通过的实际日志为准，不能用计划或脚本存在代替验收证据。

局限：系统磁盘镜像无法代替真实 USB 控制器的固件、VID/PID 及拔插行为。TUI 已完成原生准备、确认与应用提交链路的代码接入和单元回归，交互式完整按键及最终设备显示需分别进行实际操作核对。
