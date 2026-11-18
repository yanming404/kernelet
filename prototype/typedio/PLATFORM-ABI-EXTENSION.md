# ABI 8：落实 Book 的逐线程 FS/GS Base 契约

状态：用户已批准 TLS Host Service 和 ABI 8；最终实现、实际模块链接、94 项 ABI 布局比较、规范地址策略及实际 Asterinas CpuSync 原生回归通过。真实运行仍待执行。

## 设计依据与问题性质

Book [Into user mode and back](https://github.com/tatetian/kernelet/blob/main/book/src/accepted-design/into-user-mode-and-back.md) 在系统调用返回流程之后，已经说明 prototype 共用 carrier thread-pointer base 的限制，并要求完整 vOSTD 按线程保存并在每次进入用户态时设置。**FS Base 不是原设计遗漏；这里修复的是 prototype 的实现缺口，同时适配 Asterinas 用户 GS。** 原始 Book 和 TypedIO 设计没有修改。

本地原始 Kernelet `a5ec4b875d0a17890321a362fbc3fd16859b7eb4` 的 ABI 为 **6**。先前 TypedIO 服务形成中间 ABI 7，本次批准在 Services 尾部追加 TLS 服务形成 ABI **8**；三者不可混用。

```c
// which: 0=user FS, 1=user GS; operation: 0=read, 1=write.
// value points to an aligned scalar on a live instance-owned kernelet stack.
s64 (*user_tls)(u64 which, u64 operation, u64 *value);
```

Asterinas 的每个 Posix ThreadLocal 已有独立 `CpuSync<FsBase/GsBase>`。PRE_SCHEDULE_HANDLER 在逻辑任务切换前保存 OnCpu 状态；UserModeHooks.pre_user_run 在每次 user_run 前恢复 InMemory 状态。其 OnCpu/Both 优化保留：未切换且用户状态未变更时不必重复写寄存器。本平台禁用 Asterinas 不支持的 kernel involuntary task preemption，仍在用户态边界调度；Linux 调度 carrier 不受此限制。

vOSTD wrapper 调用真实同步服务。Host 在实际 carrier 的 Linux 栈中处理，短暂 preempt_disable 内读取当前 CPU 的真实 FS 或 inactive user GS，并在写入时同步 Linux task 的 selector/base。读取不使用“上次写入缓存”，因为用户态可以 FSGSBASE 改值。Linux 的 carrier context switch 保存/恢复这些实际状态，满足 carrier 被 Linux 调度或迁移时的 bookkeeping。

active kernel GS 始终属于 Linux。实例不执行 SWAPGS；Host 使用 Linux 导出的 load_gs_index 清 selector，再更新 MSR_KERNEL_GS_BASE 的 inactive user GS。Xen PV 显式拒绝。用户 FSGSBASE 是否可执行由 Linux CPU feature/CR4 配置决定；Linux common.c 启用该能力时设置 CR4_FSGSBASE，nofsgsbase 会清除 feature。vOSTD 本身不无条件执行这些指令。

## 最终边界修复

低半区-only 策略不能恢复用户 WRFSBASE/WRGSBASE 设置的高半区规范值，导致下次线程恢复失败。按用户最后验收要求已修复：宿主启用 FSGSBASE 时允许规范高半区；未启用时保持 arch_prctl 用户范围约束；非规范值一律拒绝。测试覆盖 48/57 位；当前 vOSTD page walk **只支持四级**，仍在 BootArgs 校验时拒绝五级 host，57 位 policy 测试不代表启用 LA57。

高半区只是寄存器值，不是内存权限；用户态访问仍受 Linux 页表与特权检查。Linux 带 FSGSBASE 的 NMI entry 用真实 GS 恢复路径处理，不能把旧的 GS 正负判断条件移入实例。

仅做 kernel-address/nofault 检查不足以证明 scalar 所有权。最终服务通过 host-private `kstack_all` 的 live/busy ledger 授权，完整 8 字节须落在当前实例的存活栈内，锁内执行 nofault copy，不保留 pointer。共享 vrec.stack_limit 不能作为授权依据。

源码：`prototype/linux/endovisor/carrier.c::klet_do_user_tls/klet_tls_scalar`、`platform_tls.h`、`prototype/vostd/src/arch/cpu/context/user_tls.rs`；调用者为 Asterinas `process/posix_thread/cpu_sync.rs`、`thread/mod.rs`、`thread/task.rs`。没有改变 TypedIO 消息、握手、Pipe/Grant 所有权、退休或 Quarantine。

参考 [Plucky923 实现](https://github.com/Plucky923/asterinas/blob/7c7ba33375a0186fcd7004f72db53247722e3ad5/ostd/src/arch/x86/cpu/context/mod.rs) 的 inactive GS 处理；该分支的 entry/ABI/mm/device 合同不同，没有整体复制，也不能替代本实现的 Linux current->thread bookkeeping。
