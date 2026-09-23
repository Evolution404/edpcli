# 安全 U 盘制盘工具本地变体审计

状态：**已验证的本地完整性边界**（2026-09-21）。

当前 `~/Desktop/u_disk/VRV/cems/ydcc/` 下的三个可执行文件并不是三个历史产品代际。它们具有相同的 PE 布局、编译时间戳和版本资源：

- PE 编译时间戳：2024-01-07 03:22:07
- 文件/产品版本：`8198.2104.17.2157`
- 大小：1,258,312 字节
- `cemssafeudisklabeltool_orig.exe`，SHA-256 `b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11`
- `cemssafeudisklabeltool_2ndbackup.exe`，SHA-256 `8ce3f107e13df8dac1a752d2875d1f6089e586c90309aca3b6c2ce02d6ec6415`
- `cemssafeudisklabeltool.exe`，SHA-256 `1b1ddfb92298f2860dfa82952557139d57e87daa6e387507f4bbd9ab18c27427`

## 补丁谱系

三个文件中只有 `_orig.exe` 是未修改基线。

`_2ndbackup.exe` 与 `_orig.exe` 恰好只有一个文件字节不同：

- 文件偏移 `0x48F76`，VA `0x00449B76`
- 原始 `75 76` = `jne 0x00449BEE`
- 修改后 `EB 76` = 无条件 `jmp 0x00449BEE`

被跳过的分支显示字符串 `"No terminal tool policy, label tool prohibited from starting"`。因此这是本地策略绕过补丁，不是协议生成变化。

当前 `cemssafeudisklabeltool.exe` 包含同一个绕过，并有额外补丁。相对 `_orig.exe` 共改变 83 字节，重要的可执行变化为：

1. `0x0042DDC0` 把正常策略读取函数尾部改成跳转到 `0x004289FB` 的代码洞。
2. 代码洞向返回策略对象的 `+0x20/+0x3C/+0x4C/+0x50/+0x7C/+0x148` 写固定值后返回。`writeLabel.cpp` 会消费同一对象；可执行文件自身的 `PrintPolicy` 格式把该策略家族命名为 `normalDetail`。
3. `0x00471C84` 把全 `"0000"` 分支从 `xor al,al`（false）改为 `mov al,1`（true）。
4. 其余变化是注入代码对应的 PE 头/重定位维护字节。

这些变化有助于重建本地测试环境，但**绝不能作为任何 LBA 字段的官方写入端证据**。

## 原始二进制中的官方前端边界

未修改的 `_orig.exe` 在 `fcn.0042FDB0` 动态加载 `/usbtoolBusManage.dll`，解析 `CreateBusManageImp`，创建 BusManage 对象，并通过虚接口初始化回调。

当前配套 `usbtoolBusManage.dll` 的 `BusManageImp` 虚表基址为 `0x100E14FC`。虚表字节偏移 `+0x20` 的槽是 `BusManageImp::WriteLabel@0x100A28E0`。进入函数后，它把调用方请求完整复制为 `0x265` 个 DWORD 加一个字（`0x996 = 2454` 字节）到本地请求对象，再调用内部 WriteLabel 实现。

这就是 2024/当前代已经验证的“前端 -> 业务层”边界。它不能识别缺失的 CEMS2.0/join59 写入端，也不能把历史 Netac 格式化器与旧版 EDP 配置类型选择器连接起来。
