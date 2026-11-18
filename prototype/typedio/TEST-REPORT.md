# 最终编译与正确性验收报告

验收日期：2026-11-18。固定实现代码为 Kernelet `647d68a00f3541715be4633e908d1ebb66a8762e`、Asterinas `48860174799d0796d4aa446c87274ad08a76dc53`。本目录的报告/验证脚本提交为 Kernelet 实现提交的后继，不改变该代码组合。

## 复现命令

在当前服务器的 Kernelet 根目录：

```bash
source /home/ym/typedio-work/env.sh
LINUX_DIR=/home/ym/typedio-work/linux-6.12 \
KERNEL_RUSTC="$(rustup which --toolchain 1.78.0 rustc)" \
BINDGEN="$(command -v bindgen)" \
ASTERINAS_DIR=/home/ym/typedio-work/asterinas \
JOBS=8 CARGO_BUILD_JOBS=8 prototype/typedio/check.sh
```

最后一次执行包括完整 Asterinas PIE 和 image 合同测试，退出码 **0**；日志 [acceptance-final.log](evidence/acceptance-final.log) 和 [acceptance-final.exit](evidence/acceptance-final.exit)。没有忽略 MODPOST 未解析符号，没有使用 KVM。

| 验证项 | 实际结果 | 证明范围 |
|---|---|---|
| Rust 格式 | 全部变更 Rust 源码检查通过；SDK/关键 ABI 入口另纳入门禁 | 源码格式；不代表 clippy 零告警 |
| TypedIO no_std / vOSTD / 原有原型内核 | 整个 prototype workspace target-none 编译通过 | 实际编译 |
| SDK 功能 | 42 通过、0 失败：31 core + 6 message + 2 concurrency + no-allocation + C/Rust ABI + committed resource | Message、Schema、Queue、Pipe、Flow、生命周期模型及资源提交 |
| Message derive 拒绝 | 9 类通过、0 失败 | Cell、仅 Copy、Drop、函数、未准入枚举字段、MaybeUninit、引用、union、Vec 被真实编译器拒绝 |
| 原生 main | 通过 | actual attachment/discovery/open/dispatch/completion/cancel、活动访问、IRQ/rearm、代际/移除/退出 |
| 原生 resources | 通过 | 真实 C pipe/ledger/grant：双目标配额回滚、死亡、Guest/Host reservation 隔离、lease 延迟释放 |
| 原生 vsock | 通过 | 生产 Guest Queue/Pipe/VmIO → service fixture → Host Framework/backend → peer；双向 echo/peek/EOF |
| 原生 host-file | 通过 | 生产 Linux fops：listen/accept/FD/read/write/poll/nonblocking，partial fault、wait generation、中断、FD 安装失败 Reset |
| 原生 asterinas | 通过 | 实际 Asterinas TypedIO transport：CID 42→Host2、42→7、42→42、多 attachment、多 flow、半关闭 |
| 原生集成 sanitizer | 五个 executable 通过 C 侧 ASan/UBSan/leak 检查 | Rust 静态库未整体 sanitizer 插桩 |
| 平台实际算法测试 | 12 通过、0 失败 | 锁、RCU、动态 CPU-local、frame metadata/RCU identity、VmIO partial fault、DMA separate views、真实 TLS wrapper、实际 Asterinas CpuSync、显式 inventory 注册 |
| CPU-local guard 生命周期 | 1 个编译拒绝测试通过 | 要求真实 E0597，不能因无关错误而算通过 |
| TLS C policy | 29 个 assert 通过 | FS/GS/op、48/57 位规范性、无 FSGSBASE 限制、存活栈内 scalar 范围/对齐/溢出边界 |
| image ABI 8 | 94 项比较全部一致 | 用真实 Linux Kbuild 编译的 abi_layout.o，与直接导入实际 vostd/abi.rs 的 native probe 比较六张 ABI 结构的版本、大小、对齐、全部字段偏移；opaque pointee/未调用入口为 fixture |
| TypedIO C/Rust ABI | SDK 单测通过 | 实际 C 头与实际 Rust Reservation/Descriptor/Record/Call/Resource/Binding |
| 跨编译器 Schema | 96 字节 cmp 一致 | Linux Rust 1.78 与 guest nightly 三个 digest；SHA256 `b0ebba133947c368306fc120ffb5447e1f8911b6b40f86d2f0b93373157e0dfb` |
| Endovisor | CONFIG_RUST=y Linux 6.12 的 C/Rust、MODPOST、.ko 链接通过 | 仅构建，未 insmod |
| Asterinas 回归 | default、typedio、typedio-polling 三配置 aster-core 检查通过 | 仍使用原硬件平台，验证原路径未被平台 feature 改动破坏编译 |
| 完整 Kernelet 平台 | `cargo osdk check -p asterinas --no-default-features --features kernelet` 通过；最终 staticlib/PIE 链接通过 | 整个内核及其组件，不是 Echo 测试内核 |
| 依赖图 | 只有硬件 OSTD 的虚拟 facade → 一份 vostd；无 PCI/VirtIO/NVMe/DRM/framebuffer 驱动 | [实际依赖图](evidence/asterinas-kernelet-dependencies.txt) |
| 完整 PIE | 5645688 字节，虚拟跨度 3364556 字节，三个 RX/R/RW LOAD、15181 RELATIVE relocations | 三个 EntryTable 入口均指向代码；无外部符号、ELF 自动构造、TLS 段；低于 16 MiB |
| image 合同回归 | 10 通过、0 失败 | 接受完整产物，拒绝旧 ABI、共享/非对齐/重复重定位、外部符号重定位、非代码 callback、超限、DT_INIT、解释器 |
| 原型 PIE | piecheck 通过，283 RELATIVE relocations、127768 字节 | 原有构建链的链接回归；不代替完整 Asterinas 产物 |
| 两仓库 diff --check | 通过 | 包括此前未跟踪的新文件在 staged 状态下检查 |

12 项平台测试名称与全部编译输出保留在门禁日志。平台 fixture 直接导入生产 CPU-local、锁、RCU、frame、VmIO、DMA、fault、TLS wrapper 与 Asterinas cpu_sync.rs；替换 CPU 身份/IRQ、页表映射、TLS 寄存器等 primitive，未执行真实 trap、carrier 或 FPU 指令。显式注册测试使用实际 inventory 与实际平台 submit 宏，先证明 native ELF 没有自动注册，再执行函数表。

## 本轮测试更新

格式化后 `copy_is_not_admission.stderr` 与 `union.stderr` 的行号/源码展示发生变化。先核对它们仍分别因为缺少 Message trait 和禁止 union 被拒绝，才更新两个快照。没有放宽类型准入。新增 ABI probe 起初用 Rust 的 `protected` 字段名引用 C 结构；C 实际名是 `protected_`，已改正并重新运行，94 项最终一致。

## 尚未执行

没有模块加载、真实 Host ioctl、完整 Asterinas 启动、真实 carrier 迁移/TLS/FPU/IRQ、真实 Linux/Asterinas poll/epoll/信号、真实跨实例或 QEMU/KVM 测试。Native Pollee、poll_wait、Linux 调度/usercopy 和 Host ABI 入口有明确 primitive fixture，不能把它们算作真实运行。504 组、Klet/VirtIO/TypedIO Vsock 性能和任何调参均未执行。

存在 missing_docs、unused/dead_code 等 warning；本报告不声称零警告、完整内核 ktest 通过或真实系统已启动。原型 Heap init 中原有一次性测量代码仅被编译，本次未启动、未执行该测量。

证据中的 `asterinas-integration.patch` 是从实际基线生成的零上下文补丁，避免把合法的空白 context 行当源码尾随空格；如单独应用需使用 `git apply --unidiff-zero`。镜像/Git 静态审计文本仅规整展示尾随空白，原始门禁日志保留原样。
