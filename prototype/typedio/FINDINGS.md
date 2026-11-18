# 最终问题审查与设计依据

## 已澄清：FS Base 已在 Book 中设计

Book Into user mode and back 已明确要求完整 vOSTD 按线程保存并在用户态进入时设置 FS。原型只有 carrier 共用 FS 是实现限制，不能写成设计没考虑。用户批准的 ABI 8 TLS 服务落实它并适配用户 GS；详见 [平台说明](PLATFORM-ABI-EXTENSION.md)。原设计未改。

## 本轮已经修复的实现问题

- 高半区规范 FS/GS 被低半区-only write policy 拒绝：现在仅宿主启用 FSGSBASE 时允许规范高半区，仍拒绝非规范值；保存来源是真实 CPU。
- TLS scalar 仅作 kernel/nofault 检查不足以授权：增加当前实例 host-private live stack ledger 范围验证；不信任共享 stack_limit。
- 完整内核 frame metadata 曾被 prototype 丢弃：现在 factory 逐页保存、末次引用 on_drop/Drop 后才归还 backing，RCU identity 是私有对齐 metadata slot。
- VmIO 缺少当前用户根缺页恢复和 futex 原子操作：使用真实 VMAR handler，在 RCU copy 区域之外、非 atomic 上下文、当前授权 root 才调用；重试有界。
- 完整 Asterinas ThreadLocal/CpuSync 不支持 kernel involuntary preemption：仅 Kernelet 平台禁止该调度入口，保留用户边界与显式调度；Host Linux carrier 调度照常。
- FPU XRSTOR mask 原来按 XSTATE_BV 求交会遗留上一线程 INIT 状态：现在恢复全部许可分量；FXSAVE-only 不读 XCR0，初始化 x87/MXCSR。
- inventory 自动构造与现有 loader 冲突：改为私有 relocation table 的显式注册，原组件 stage/priority 保留。未放开 ELF 自动构造或扩大 image 限制。
- 既有 TypedIO 修正已回归：Grant 范围溢出与 lease、递归 committed 解除发送方析构、active access/quiescence 后发布、同 CID 双 Established 槽、旧 generation/attempt、Host reservation 隔离、同批 IRQ 注销自等待、Host wait generation 与 partial fault/FD 失败清理。

没有发现需要新增 TypedIO 握手消息或改变资源生命周期才能解决的确认矛盾。上述均是实现修复或平台适配，不是修改原协议。

## 版本核对

原 Asterinas API 参考 `ab9a4cfdc726263b3f41ccea0337633e24b443fc` 与集成 `e87d8425f987820431bd35068ef2d4489bdcc735` 的相关差异为七文件 67 增/27 删：transport/connection/utils.rs 的 TimerGuard 类型，port 导入，space 的 BottomHalfDisabled，DMA stream copy/sync/cache 与测试，VmIO 注释/aarch64 PodOnce。当前集成以 e87d 接口为准，未混用旧 Packet 与新 transport。

实际阅读 [Plucky923 7c7ba333](https://github.com/Plucky923/asterinas/tree/7c7ba33375a0186fcd7004f72db53247722e3ad5) 的 context、entry、ABI。其 inactive GS 思路可参考，完整 BootArgs/EntryTable/MM/device 合同不同，不整体复制，不以该分支替代本实现的运行证据。

## 仍需真实运行确认

完整启动代码缺失的旧阻塞已经由 source-backed full PIE 消除，不能继续写“只有 install_host_service 定义”。当前路径直接使用 vOSTD 已初始化的 Services.typedio。真正引导、carrier TLS/FPU/迁移、IRQ/SMP、epoll/信号、跨实例/退出及性能仍未执行；详见交接第八节。console 输入、真实 RTC、硬件驱动和更大 Grant 不是本次承诺已实现的能力。
