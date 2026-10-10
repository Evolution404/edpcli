# 原生逻辑扇区与设备传输统一提交链

## 目标与事实

物理 USB 与 macOS 挂载的虚拟 Disk Image 通过相同的原生设备几何模型、不可变写集、TargetSession 写租约及 WAL 原生块事务执行。逻辑扇区大小是来自设备的独立属性，原生块读取及写入没有针对 512B、4096B 单独分叉。文件系统格式化能力另按格式规范约束。

命令 edpcli list --include-virtual 和 edpcli tui --include-virtual 显示确认属于外部虚拟 Disk Image 的设备，普通启动维持原有列表。设备是否可选、是否能取得写租约仍需在准备写入时根据当前系统事实复核；该参数不会把普通物理硬盘变成虚拟盘。

统一提交函数位于 src/application/provision/native_commit.rs。它获取现有目标会话、核实完整几何、读回来源原生前 13 块、锁定并重新打开同一设备、复核前 13 块及身份后，调用统一 WAL 原生块提交与独立回读机制。写集本身必须由同一原生规划逻辑根据目标模式和分区格式化需求生成。来源用户数据在当前测试中的目标重建属于破坏性制盘，不可称为无损转换。

## macOS 系统真实 4Kn 设备验收

执行 bash scripts/ci/macos-native-4kn-virtual-disk-hil.sh。脚本自行创建 512MiB 稀疏镜像，以 hdiutil -blocksize 4096 挂载；每次写入前都验证 WholeDisk、Internal=false、Virtual、Disk Image、4096B 及实际容量。之后通过同一原生写入函数依次重建 Plain、Mode0、Mode1、Mode2、Mode3、Plain；每次提交都持久化 WAL，卸载后重挂并独立核对原生写集、协议模式、LCE 与可验证文件系统结构。Plain 额外经过 macOS 挂载和实际文件读写。

这一验收证明实际 macOS 虚拟块设备的原生写入和事务链，不证明 USB 控制器行为，也没有证明所有 25 对模式组合、用户数据保留及正式 TUI 全流程已全部实现。原先基于 512B SectorDev 的正式制盘入口仍需迁移到统一原生规划器；不能借虚拟 HIL 测试宣称全功能已完成。
