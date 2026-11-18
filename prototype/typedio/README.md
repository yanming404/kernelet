# TypedIO Vsock：实现与验收入口

完整 Asterinas-on-Kernelet 的代码启动链和受限 PIE 已实现，当前环境的编译、链接、静态与原生正确性门禁全部通过。真实启动、真实 Linux/Guest ABI、跨实例通信及性能实验尚未验证，因此不称为“整个系统验证完成”。

- [完整中文项目交接](IMPLEMENTATION-HANDOFF.md)：原项目、前后对比、逐线程 TLS、数据路径、恢复与后续工作。
- [完整源码修改清单](SOURCE-CHANGES.md)：两个仓库全部 225 个实现变更文件，按原始基线比较，含永久 Commit 链接。
- [设计符合性矩阵](COMPLIANCE.md)：实际源码位置、测试方法、结果及限制。
- [最终测试报告](TEST-REPORT.md)、[真实启动路径](BOOTSTRAP-INTEGRATION.md)、[问题与参考核查](FINDINGS.md)。
- [固定代码组合](VERSION-PAIR.json)、[镜像数据](evidence/image-manifest.json)、[完整门禁日志](evidence/acceptance-final.log)。

Kernelet 原始 `design/typed-io` 基线为 `a5ec4b875d0a17890321a362fbc3fd16859b7eb4`；Asterinas 原始 `main`/集成基线为 `e87d8425f987820431bd35068ef2d4489bdcc735`。Book 和原 TypedIO 设计未修改，Host Backend 仍在 Linux 内核中。旧基线 ABI 6 经 TypedIO 服务增加为中间 ABI 7，本次批准的 TLS 服务形成最终 ABI 8。

最终通过：42 项 SDK 功能、9 类 derive 编译拒绝、5 套生产源码原生集成、12 项平台测试、1 项 CPU-local 生命周期编译拒绝、94 项实际 C/Rust image ABI 布局比较、10 项真实完整镜像的合同正反测试。原生 Linux/IRQ/usercopy/Pollee primitive 的替代边界单列在测试报告。

当前工作区为相邻的 `kernelet` 与 `asterinas`，分支分别为 `implement/typedio-vsock`、`integrate/typedio-vsock`。按用户最新选择仅保存本地 Commit 和 Git bundle，未推送、未创建 PR、未合并 main。此前关于“完整 Asterinas 启动入口仍缺失”的报告已由当前真实编译与链接产物取代；这不表示运行时启动已成功。
