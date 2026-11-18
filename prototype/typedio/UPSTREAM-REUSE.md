# 上游复用代码与平台适配来源

本表只列本次新增的 vOSTD 文件中，在 Asterinas OSTD 集成基线存在直接对应路径的部分。相同表示逐字节一致；不同只表示有格式/导入/平台适配，不据此声称整个算法原创或语义相同。其余新增 TypedIO/Host/平台文件见 SOURCE-CHANGES。原 vOSTD 的 task/scheduler 等历史复用不列为本次新增。没有整体复制 Plucky923 分支。

| 当前 vOSTD 文件 | Asterinas 基线来源 | 实际比较 |
|---|---|---|
| `prototype/vostd/src/boot/memory_region.rs` | [ostd/src/boot/memory_region.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/boot/memory_region.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/irq/bottom_half.rs` | [ostd/src/irq/bottom_half.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/irq/bottom_half.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/irq/level.rs` | [ostd/src/irq/level.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/irq/level.rs) | 逐字节一致 |
| `prototype/vostd/src/log/bridge.rs` | [ostd/src/log/bridge.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/log/bridge.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/log/level.rs` | [ostd/src/log/level.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/log/level.rs) | 逐字节一致 |
| `prototype/vostd/src/log/logger.rs` | [ostd/src/log/logger.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/log/logger.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/log/macros.rs` | [ostd/src/log/macros.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/log/macros.rs) | 逐字节一致 |
| `prototype/vostd/src/log/mod.rs` | [ostd/src/log/mod.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/log/mod.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/mm/frame/meta.rs` | [ostd/src/mm/frame/meta.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/mm/frame/meta.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/mm/io/util.rs` | [ostd/src/mm/io/util.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/mm/io/util.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/sync/mutex.rs` | [ostd/src/sync/mutex.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/mutex.rs) | 逐字节一致 |
| `prototype/vostd/src/sync/rcu/mod.rs` | [ostd/src/sync/rcu/mod.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rcu/mod.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/sync/rcu/monitor.rs` | [ostd/src/sync/rcu/monitor.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rcu/monitor.rs) | 逐字节一致 |
| `prototype/vostd/src/sync/rcu/non_null/either.rs` | [ostd/src/sync/rcu/non_null/either.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rcu/non_null/either.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/sync/rcu/non_null/mod.rs` | [ostd/src/sync/rcu/non_null/mod.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rcu/non_null/mod.rs) | 逐字节一致 |
| `prototype/vostd/src/sync/rwarc.rs` | [ostd/src/sync/rwarc.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rwarc.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/sync/rwlock.rs` | [ostd/src/sync/rwlock.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rwlock.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/sync/rwmutex.rs` | [ostd/src/sync/rwmutex.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/sync/rwmutex.rs) | 逐字节一致 |
| `prototype/vostd/src/util/either.rs` | [ostd/src/util/either.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/util/either.rs) | 逐字节一致 |
| `prototype/vostd/src/util/id_set.rs` | [ostd/src/util/id_set.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/util/id_set.rs) | 逐字节一致 |
| `prototype/vostd/src/util/macros.rs` | [ostd/src/util/macros.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/util/macros.rs) | 逐字节一致 |
| `prototype/vostd/src/util/mod.rs` | [ostd/src/util/mod.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/util/mod.rs) | 复用/适配；请对照具体 diff |
| `prototype/vostd/src/util/ops.rs` | [ostd/src/util/ops.rs](https://github.com/asterinas/asterinas/blob/e87d8425f987820431bd35068ef2d4489bdcc735/ostd/src/util/ops.rs) | 逐字节一致 |

这些源文件保留 MPL-2.0 等原始 SPDX。上游算法的 native 回归与虚拟平台编译不构成真实硬件运行验证。
