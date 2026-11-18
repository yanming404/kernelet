# 完整源码变更索引

对比固定 clone 基线与冻结实现提交；没有拿最新 main 作为基准。下列每个文件的用途归入交接文档第三节的模块机制；A=新增、M=修改、D=删除。链接是个人仓库将来上传后的目标永久 URL，本次没有 Push；当前请结合本地路径与 bundle 使用。

## kernelet

基线 `a5ec4b875d0a17890321a362fbc3fd16859b7eb4` → 实现 `647d68a00f3541715be4633e908d1ebb66a8762e`。统计：{'M': 40, 'A': 130}。

| 类型 | 文件（固定实现 Commit） | 增加 | 删除 |
|---|---|---|---|
| M | [prototype/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/Cargo.lock) | 336 | 2 |
| M | [prototype/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/Cargo.toml) | 1 | 1 |
| M | [prototype/Makefile](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/Makefile) | 26 | 3 |
| M | [prototype/host/runtime/holder.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/host/runtime/holder.c) | 24 | 1 |
| M | [prototype/host/runtime/sandbox.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/host/runtime/sandbox.c) | 7 | 1 |
| M | [prototype/host/runtime/sandbox.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/host/runtime/sandbox.h) | 2 | 0 |
| A | [prototype/libs/guest-registry/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/guest-registry/Cargo.toml) | 13 | 0 |
| A | [prototype/libs/guest-registry/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/guest-registry/src/lib.rs) | 31 | 0 |
| A | [prototype/libs/typedio-kernel-macros/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio-kernel-macros/Cargo.lock) | 47 | 0 |
| A | [prototype/libs/typedio-kernel-macros/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio-kernel-macros/Cargo.toml) | 14 | 0 |
| A | [prototype/libs/typedio-macros/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio-macros/Cargo.toml) | 17 | 0 |
| A | [prototype/libs/typedio-macros/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio-macros/src/lib.rs) | 232 | 0 |
| A | [prototype/libs/typedio/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/Cargo.toml) | 15 | 0 |
| A | [prototype/libs/typedio/src/abi.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/abi.rs) | 18 | 0 |
| A | [prototype/libs/typedio/src/api.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/api.rs) | 18 | 0 |
| A | [prototype/libs/typedio/src/flow.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/flow.rs) | 286 | 0 |
| A | [prototype/libs/typedio/src/frontend.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/frontend.rs) | 347 | 0 |
| A | [prototype/libs/typedio/src/frontend/pipe.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/frontend/pipe.rs) | 286 | 0 |
| A | [prototype/libs/typedio/src/irq.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/irq.rs) | 248 | 0 |
| A | [prototype/libs/typedio/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/lib.rs) | 14 | 0 |
| A | [prototype/libs/typedio/src/lifetime.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/lifetime.rs) | 257 | 0 |
| A | [prototype/libs/typedio/src/message.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/message.rs) | 547 | 0 |
| A | [prototype/libs/typedio/src/notification.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/notification.rs) | 170 | 0 |
| A | [prototype/libs/typedio/src/pipe.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/pipe.rs) | 590 | 0 |
| A | [prototype/libs/typedio/src/queue.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/queue.rs) | 345 | 0 |
| A | [prototype/libs/typedio/src/resource.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/resource.rs) | 320 | 0 |
| A | [prototype/libs/typedio/src/schema.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/schema.rs) | 186 | 0 |
| A | [prototype/libs/typedio/src/service.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/service.rs) | 103 | 0 |
| A | [prototype/libs/typedio/src/vsock.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/src/vsock.rs) | 92 | 0 |
| A | [prototype/libs/typedio/tests/abi.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/abi.rs) | 77 | 0 |
| A | [prototype/libs/typedio/tests/commit_resources.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/commit_resources.rs) | 52 | 0 |
| A | [prototype/libs/typedio/tests/concurrency.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/concurrency.rs) | 88 | 0 |
| A | [prototype/libs/typedio/tests/derive.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/derive.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/messages.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/messages.rs) | 137 | 0 |
| A | [prototype/libs/typedio/tests/no_allocation.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/no_allocation.rs) | 46 | 0 |
| A | [prototype/libs/typedio/tests/ui/cell.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/cell.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/ui/cell.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/cell.stderr) | 21 | 0 |
| A | [prototype/libs/typedio/tests/ui/copy_is_not_admission.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/copy_is_not_admission.rs) | 8 | 0 |
| A | [prototype/libs/typedio/tests/ui/copy_is_not_admission.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/copy_is_not_admission.stderr) | 26 | 0 |
| A | [prototype/libs/typedio/tests/ui/custom_drop.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/custom_drop.rs) | 9 | 0 |
| A | [prototype/libs/typedio/tests/ui/custom_drop.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/custom_drop.stderr) | 10 | 0 |
| A | [prototype/libs/typedio/tests/ui/function.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/function.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/ui/function.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/function.stderr) | 21 | 0 |
| A | [prototype/libs/typedio/tests/ui/inactive_variant.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/inactive_variant.rs) | 7 | 0 |
| A | [prototype/libs/typedio/tests/ui/inactive_variant.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/inactive_variant.stderr) | 21 | 0 |
| A | [prototype/libs/typedio/tests/ui/maybe_uninit.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/maybe_uninit.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/ui/maybe_uninit.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/maybe_uninit.stderr) | 21 | 0 |
| A | [prototype/libs/typedio/tests/ui/reference.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/reference.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/ui/reference.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/reference.stderr) | 19 | 0 |
| A | [prototype/libs/typedio/tests/ui/union.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/union.rs) | 7 | 0 |
| A | [prototype/libs/typedio/tests/ui/union.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/union.stderr) | 5 | 0 |
| A | [prototype/libs/typedio/tests/ui/vector.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/vector.rs) | 6 | 0 |
| A | [prototype/libs/typedio/tests/ui/vector.stderr](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/libs/typedio/tests/ui/vector.stderr) | 21 | 0 |
| M | [prototype/linux/endovisor/Makefile](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/Makefile) | 15 | 1 |
| A | [prototype/linux/endovisor/abi_layout.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/abi_layout.c) | 93 | 0 |
| M | [prototype/linux/endovisor/carrier.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/carrier.c) | 116 | 1 |
| A | [prototype/linux/endovisor/grant.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/grant.c) | 101 | 0 |
| M | [prototype/linux/endovisor/kernelet.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/kernelet.h) | 24 | 3 |
| M | [prototype/linux/endovisor/kernelet.lds](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/kernelet.lds) | 7 | 1 |
| M | [prototype/linux/endovisor/main.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/main.c) | 11 | 0 |
| A | [prototype/linux/endovisor/platform_tls.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/platform_tls.h) | 37 | 0 |
| M | [prototype/linux/endovisor/sandbox.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/sandbox.c) | 20 | 7 |
| A | [prototype/linux/endovisor/typedio.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio.rs) | 14 | 0 |
| A | [prototype/linux/endovisor/typedio_bounds.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_bounds.h) | 16 | 0 |
| A | [prototype/linux/endovisor/typedio_core.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_core.rs) | 279 | 0 |
| A | [prototype/linux/endovisor/typedio_host.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_host.c) | 911 | 0 |
| A | [prototype/linux/endovisor/typedio_host.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_host.h) | 52 | 0 |
| A | [prototype/linux/endovisor/typedio_host_api.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_host_api.rs) | 347 | 0 |
| A | [prototype/linux/endovisor/typedio_host_socket.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_host_socket.rs) | 105 | 0 |
| A | [prototype/linux/endovisor/typedio_pipe.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_pipe.c) | 347 | 0 |
| A | [prototype/linux/endovisor/typedio_pipe.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_pipe.h) | 45 | 0 |
| A | [prototype/linux/endovisor/typedio_queue.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_queue.rs) | 171 | 0 |
| A | [prototype/linux/endovisor/typedio_records.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_records.h) | 17 | 0 |
| A | [prototype/linux/endovisor/typedio_resources.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_resources.c) | 485 | 0 |
| A | [prototype/linux/endovisor/typedio_resources.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_resources.h) | 36 | 0 |
| A | [prototype/linux/endovisor/typedio_service.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_service.h) | 2 | 0 |
| A | [prototype/linux/endovisor/typedio_socket.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_socket.c) | 217 | 0 |
| A | [prototype/linux/endovisor/typedio_socket.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_socket.h) | 5 | 0 |
| A | [prototype/linux/endovisor/typedio_vsock.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_vsock.rs) | 843 | 0 |
| A | [prototype/linux/endovisor/typedio_vsock_link.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/typedio_vsock_link.c) | 42 | 0 |
| M | [prototype/linux/endovisor/uapi/endovisor.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/uapi/endovisor.h) | 13 | 3 |
| A | [prototype/linux/endovisor/uapi/typedio.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/linux/endovisor/uapi/typedio.h) | 53 | 0 |
| A | [prototype/tests/asterinas-harness/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/asterinas-harness/Cargo.lock) | 121 | 0 |
| A | [prototype/tests/asterinas-harness/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/asterinas-harness/Cargo.toml) | 15 | 0 |
| A | [prototype/tests/asterinas-harness/build.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/asterinas-harness/build.rs) | 14 | 0 |
| A | [prototype/tests/asterinas-harness/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/asterinas-harness/src/lib.rs) | 258 | 0 |
| A | [prototype/tests/asterinas-harness/src/net/socket/vsock/socket_transport/checks.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/asterinas-harness/src/net/socket/vsock/socket_transport/checks.rs) | 220 | 0 |
| A | [prototype/tests/host-harness/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/Cargo.lock) | 82 | 0 |
| A | [prototype/tests/host-harness/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/Cargo.toml) | 14 | 0 |
| A | [prototype/tests/host-harness/host-file.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/host-file.c) | 38 | 0 |
| A | [prototype/tests/host-harness/main.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/main.c) | 52 | 0 |
| A | [prototype/tests/host-harness/resources.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/resources.c) | 69 | 0 |
| A | [prototype/tests/host-harness/shim.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/shim.c) | 97 | 0 |
| A | [prototype/tests/host-harness/shim.h](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/shim.h) | 161 | 0 |
| A | [prototype/tests/host-harness/src/guest.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/src/guest.rs) | 405 | 0 |
| A | [prototype/tests/host-harness/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/src/lib.rs) | 159 | 0 |
| A | [prototype/tests/host-harness/vsock.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/host-harness/vsock.c) | 29 | 0 |
| A | [prototype/tests/platform-harness/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/Cargo.lock) | 185 | 0 |
| A | [prototype/tests/platform-harness/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/Cargo.toml) | 18 | 0 |
| A | [prototype/tests/platform-harness/src/bin/image-abi.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/bin/image-abi.rs) | 235 | 0 |
| A | [prototype/tests/platform-harness/src/cpu/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/cpu/mod.rs) | 45 | 0 |
| A | [prototype/tests/platform-harness/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/lib.rs) | 175 | 0 |
| A | [prototype/tests/platform-harness/src/mm/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/mm/mod.rs) | 84 | 0 |
| A | [prototype/tests/platform-harness/src/sync/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/sync/mod.rs) | 107 | 0 |
| A | [prototype/tests/platform-harness/src/tests.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/src/tests.rs) | 514 | 0 |
| A | [prototype/tests/platform-harness/tests/guard_lifetime.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform-harness/tests/guard_lifetime.rs) | 54 | 0 |
| A | [prototype/tests/platform_tls.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/platform_tls.c) | 34 | 0 |
| A | [prototype/tests/test_image_contract.py](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/test_image_contract.py) | 97 | 0 |
| A | [prototype/tests/typedio_abi.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/typedio_abi.c) | 23 | 0 |
| A | [prototype/tests/typedio_bounds.c](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tests/typedio_bounds.c) | 18 | 0 |
| A | [prototype/tools/typedio-device/Cargo.lock](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tools/typedio-device/Cargo.lock) | 61 | 0 |
| A | [prototype/tools/typedio-device/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tools/typedio-device/Cargo.toml) | 8 | 0 |
| A | [prototype/tools/typedio-device/src/main.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/tools/typedio-device/src/main.rs) | 159 | 0 |
| M | [prototype/vostd/Cargo.toml](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/Cargo.toml) | 15 | 0 |
| M | [prototype/vostd/src/abi.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/abi.rs) | 17 | 2 |
| M | [prototype/vostd/src/arch/cpu/context.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/cpu/context.rs) | 38 | 2 |
| A | [prototype/vostd/src/arch/cpu/context/user_tls.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/cpu/context/user_tls.rs) | 47 | 0 |
| A | [prototype/vostd/src/arch/cpu/cpuid.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/cpu/cpuid.rs) | 17 | 0 |
| M | [prototype/vostd/src/arch/cpu/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/cpu/mod.rs) | 1 | 0 |
| M | [prototype/vostd/src/arch/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/mod.rs) | 36 | 0 |
| M | [prototype/vostd/src/arch/trap.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/arch/trap.rs) | 71 | 0 |
| A | [prototype/vostd/src/boot/memory_region.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/boot/memory_region.rs) | 56 | 0 |
| M | [prototype/vostd/src/boot/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/boot/mod.rs) | 78 | 16 |
| M | [prototype/vostd/src/console.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/console.rs) | 10 | 0 |
| M | [prototype/vostd/src/cpu/local.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/cpu/local.rs) | 89 | 9 |
| A | [prototype/vostd/src/cpu/local/dynamic.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/cpu/local/dynamic.rs) | 110 | 0 |
| M | [prototype/vostd/src/cpu/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/cpu/mod.rs) | 12 | 1 |
| A | [prototype/vostd/src/irq/bottom_half.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/irq/bottom_half.rs) | 80 | 0 |
| A | [prototype/vostd/src/irq/level.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/irq/level.rs) | 120 | 0 |
| A | [prototype/vostd/src/irq/line.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/irq/line.rs) | 206 | 0 |
| M | [prototype/vostd/src/irq/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/irq/mod.rs) | 20 | 21 |
| M | [prototype/vostd/src/lib.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/lib.rs) | 9 | 6 |
| A | [prototype/vostd/src/log/bridge.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/log/bridge.rs) | 112 | 0 |
| A | [prototype/vostd/src/log/level.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/log/level.rs) | 211 | 0 |
| A | [prototype/vostd/src/log/logger.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/log/logger.rs) | 193 | 0 |
| A | [prototype/vostd/src/log/macros.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/log/macros.rs) | 247 | 0 |
| A | [prototype/vostd/src/log/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/log/mod.rs) | 132 | 0 |
| A | [prototype/vostd/src/mm/dma.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/dma.rs) | 224 | 0 |
| A | [prototype/vostd/src/mm/fault.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/fault.rs) | 30 | 0 |
| M | [prototype/vostd/src/mm/frame.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/frame.rs) | 220 | 94 |
| A | [prototype/vostd/src/mm/frame/meta.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/frame/meta.rs) | 133 | 0 |
| M | [prototype/vostd/src/mm/heap.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/heap.rs) | 13 | 2 |
| M | [prototype/vostd/src/mm/io.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/io.rs) | 402 | 3 |
| A | [prototype/vostd/src/mm/io/util.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/io/util.rs) | 115 | 0 |
| M | [prototype/vostd/src/mm/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/mod.rs) | 23 | 1 |
| M | [prototype/vostd/src/mm/tlb.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/tlb.rs) | 15 | 1 |
| M | [prototype/vostd/src/mm/vm_space.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/mm/vm_space.rs) | 51 | 13 |
| M | [prototype/vostd/src/panic.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/panic.rs) | 19 | 0 |
| M | [prototype/vostd/src/power.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/power.rs) | 22 | 1 |
| M | [prototype/vostd/src/sync/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/mod.rs) | 21 | 5 |
| A | [prototype/vostd/src/sync/mutex.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/mutex.rs) | 168 | 0 |
| A | [prototype/vostd/src/sync/rcu/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rcu/mod.rs) | 545 | 0 |
| A | [prototype/vostd/src/sync/rcu/monitor.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rcu/monitor.rs) | 143 | 0 |
| A | [prototype/vostd/src/sync/rcu/non_null/either.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rcu/non_null/either.rs) | 162 | 0 |
| A | [prototype/vostd/src/sync/rcu/non_null/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rcu/non_null/mod.rs) | 203 | 0 |
| A | [prototype/vostd/src/sync/rwarc.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rwarc.rs) | 140 | 0 |
| A | [prototype/vostd/src/sync/rwlock.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rwlock.rs) | 424 | 0 |
| A | [prototype/vostd/src/sync/rwmutex.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/sync/rwmutex.rs) | 402 | 0 |
| M | [prototype/vostd/src/task/preempt/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/task/preempt/mod.rs) | 1 | 1 |
| M | [prototype/vostd/src/task/processor.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/task/processor.rs) | 4 | 3 |
| M | [prototype/vostd/src/task/scheduler/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/task/scheduler/mod.rs) | 6 | 0 |
| M | [prototype/vostd/src/timer/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/timer/mod.rs) | 1 | 1 |
| A | [prototype/vostd/src/typedio.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/typedio.rs) | 19 | 0 |
| M | [prototype/vostd/src/user.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/user.rs) | 3 | 0 |
| A | [prototype/vostd/src/util/either.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/util/either.rs) | 46 | 0 |
| A | [prototype/vostd/src/util/id_set.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/util/id_set.rs) | 938 | 0 |
| A | [prototype/vostd/src/util/macros.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/util/macros.rs) | 48 | 0 |
| A | [prototype/vostd/src/util/mod.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/util/mod.rs) | 10 | 0 |
| A | [prototype/vostd/src/util/ops.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/util/ops.rs) | 113 | 0 |
| M | [prototype/vostd/src/vcpu.rs](https://github.com/yanming404/kernelet/blob/647d68a00f3541715be4633e908d1ebb66a8762e/prototype/vostd/src/vcpu.rs) | 100 | 18 |
## asterinas

基线 `e87d8425f987820431bd35068ef2d4489bdcc735` → 实现 `48860174799d0796d4aa446c87274ad08a76dc53`。统计：{'M': 43, 'A': 12}。

| 类型 | 文件（固定实现 Commit） | 增加 | 删除 |
|---|---|---|---|
| M | [Cargo.lock](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/Cargo.lock) | 90 | 23 |
| M | [Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/Cargo.toml) | 8 | 1 |
| M | [kernel/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/Cargo.toml) | 7 | 5 |
| M | [kernel/core/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/Cargo.toml) | 9 | 4 |
| M | [kernel/core/comps/cmdline/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/cmdline/Cargo.toml) | 4 | 0 |
| M | [kernel/core/comps/cmdline/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/cmdline/src/lib.rs) | 3 | 0 |
| M | [kernel/core/comps/console/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/console/Cargo.toml) | 3 | 0 |
| M | [kernel/core/comps/console/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/console/src/lib.rs) | 30 | 0 |
| M | [kernel/core/comps/time/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/time/Cargo.toml) | 3 | 0 |
| M | [kernel/core/comps/time/src/rtc/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/comps/time/src/rtc/mod.rs) | 4 | 4 |
| M | [kernel/core/src/arch/x86/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/arch/x86/mod.rs) | 1 | 0 |
| A | [kernel/core/src/arch/x86/power_kernelet.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/arch/x86/power_kernelet.rs) | 13 | 0 |
| M | [kernel/core/src/device/misc/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/misc/mod.rs) | 2 | 0 |
| M | [kernel/core/src/device/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/mod.rs) | 2 | 0 |
| M | [kernel/core/src/device/registry/block.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/registry/block.rs) | 3 | 0 |
| M | [kernel/core/src/device/tty/device.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/tty/device.rs) | 18 | 4 |
| M | [kernel/core/src/device/tty/hvc.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/tty/hvc.rs) | 6 | 1 |
| M | [kernel/core/src/device/tty/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/device/tty/mod.rs) | 2 | 0 |
| M | [kernel/core/src/fs/fs_impls/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/fs/fs_impls/mod.rs) | 2 | 0 |
| M | [kernel/core/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/lib.rs) | 7 | 0 |
| M | [kernel/core/src/net/iface/init.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/iface/init.rs) | 5 | 0 |
| M | [kernel/core/src/net/socket/vsock/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/mod.rs) | 4 | 1 |
| A | [kernel/core/src/net/socket/vsock/socket_transport.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/socket_transport.rs) | 183 | 0 |
| A | [kernel/core/src/net/socket/vsock/socket_transport_kernelet.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/socket_transport_kernelet.rs) | 6 | 0 |
| M | [kernel/core/src/net/socket/vsock/stream/connected.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/stream/connected.rs) | 1 | 1 |
| M | [kernel/core/src/net/socket/vsock/stream/connecting.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/stream/connecting.rs) | 1 | 1 |
| M | [kernel/core/src/net/socket/vsock/stream/init.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/stream/init.rs) | 8 | 8 |
| M | [kernel/core/src/net/socket/vsock/stream/listen.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/stream/listen.rs) | 1 | 1 |
| A | [kernel/core/src/net/socket/vsock/typedio.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/net/socket/vsock/typedio.rs) | 1149 | 0 |
| M | [kernel/core/src/power.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/power.rs) | 3 | 0 |
| M | [kernel/core/src/thread/kernel_thread.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/thread/kernel_thread.rs) | 7 | 1 |
| M | [kernel/core/src/thread/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/thread/mod.rs) | 3 | 0 |
| M | [kernel/core/src/thread/task.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/thread/task.rs) | 8 | 1 |
| M | [kernel/core/src/vm/mod.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/vm/mod.rs) | 5 | 0 |
| M | [kernel/core/src/vm/vmar/vm_mapping.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/vm/vmar/vm_mapping.rs) | 4 | 1 |
| M | [kernel/core/src/vm/vmar/vmar_impls/access_alien.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/vm/vmar/vmar_impls/access_alien.rs) | 1 | 0 |
| M | [kernel/core/src/vm/vmar/vmar_impls/fork.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/vm/vmar/vmar_impls/fork.rs) | 1 | 0 |
| M | [kernel/core/src/vm/vmar/vmar_impls/remap.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/core/src/vm/vmar/vmar_impls/remap.rs) | 1 | 0 |
| A | [kernel/kernelet/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/Cargo.toml) | 17 | 0 |
| A | [kernel/kernelet/README.md](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/README.md) | 11 | 0 |
| A | [kernel/kernelet/build.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/build.rs) | 20 | 0 |
| A | [kernel/kernelet/init.S](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/init.S) | 19 | 0 |
| A | [kernel/kernelet/mkinitramfs.py](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/mkinitramfs.py) | 33 | 0 |
| A | [kernel/kernelet/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/kernelet/src/lib.rs) | 12 | 0 |
| M | [kernel/libs/comp-sys/component/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/libs/comp-sys/component/Cargo.toml) | 4 | 0 |
| M | [kernel/libs/comp-sys/component/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/libs/comp-sys/component/src/lib.rs) | 3 | 0 |
| M | [kernel/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/kernel/src/lib.rs) | 4 | 0 |
| M | [osdk/deps/frame-allocator/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/osdk/deps/frame-allocator/Cargo.toml) | 3 | 0 |
| M | [osdk/deps/frame-allocator/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/osdk/deps/frame-allocator/src/lib.rs) | 18 | 0 |
| M | [osdk/deps/heap-allocator/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/osdk/deps/heap-allocator/Cargo.toml) | 3 | 0 |
| A | [osdk/deps/heap-allocator/src/kernelet.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/osdk/deps/heap-allocator/src/kernelet.rs) | 23 | 0 |
| M | [osdk/deps/heap-allocator/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/osdk/deps/heap-allocator/src/lib.rs) | 18 | 0 |
| M | [ostd/Cargo.toml](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/ostd/Cargo.toml) | 4 | 0 |
| M | [ostd/src/lib.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/ostd/src/lib.rs) | 42 | 0 |
| A | [ostd/src/typedio.rs](https://github.com/yanming404/asterinas/blob/48860174799d0796d4aa446c87274ad08a76dc53/ostd/src/typedio.rs) | 41 | 0 |

## 复用代码、平台适配与删除的含义

Kernelet 原来的 task/scheduler/Host 服务/loader 已有，本次是扩展或修正，不是全部新建。vOSTD 新增 sync/rcu、mutex/rwlock/rwmutex/rwarc、log、util 主要复用 Asterinas OSTD 算法并适配其虚拟平台依赖；详见对应原 SPDX 和文件注释。TypedIO SDK/framework/backend/ledger、TLS Host Service、frame metadata、dynamic CPU-local 和 image assembler 为本次新增或扩展。

Asterinas 原来的 Socket stream、等待/信号、CpuSync、组件 stage/priority 大部分保留，新增 transport/platform selection，不重新实现另一套 syscall/socket 内核。cfg 删除硬件路径只影响 kernelet feature；默认硬件路径仍通过编译回归。两仓库没有 D 类型的整文件删除；删行是路径替换和条件编译等，不能表述成删除了整个子系统。

## 本地交接/验证文件

以下位于 prototype/typedio，为实现提交之后新增的验证脚本、报告或生成证据，不改变生产协议。日志/patch/hash/config 是生成资料，绝非真实性能数据。旧 baseline/bootstrap 日志保留历史含义，当前结论只采用 acceptance-final 与 image-contract。

- `prototype/typedio/BOOTSTRAP-INTEGRATION.md`
- `prototype/typedio/COMPLIANCE.md`
- `prototype/typedio/FINDINGS.md`
- `prototype/typedio/IMPLEMENTATION-HANDOFF.md`
- `prototype/typedio/PLATFORM-ABI-EXTENSION.md`
- `prototype/typedio/PR-DRAFT.md`
- `prototype/typedio/README.md`
- `prototype/typedio/SOURCE-CHANGES.md`
- `prototype/typedio/TEST-REPORT.md`
- `prototype/typedio/UPSTREAM-REUSE.md`
- `prototype/typedio/VERSION-PAIR.json`
- `prototype/typedio/check.sh`
- `prototype/typedio/evidence/acceptance-final.exit`
- `prototype/typedio/evidence/acceptance-final.log`
- `prototype/typedio/evidence/asterinas-baseline.log`
- `prototype/typedio/evidence/asterinas-git-audit.txt`
- `prototype/typedio/evidence/asterinas-integration.patch`
- `prototype/typedio/evidence/asterinas-kernelet-dependencies.txt`
- `prototype/typedio/evidence/bootstrap-audit.txt`
- `prototype/typedio/evidence/bootstrap-final-audit.txt`
- `prototype/typedio/evidence/bootstrap-regression.exit`
- `prototype/typedio/evidence/bootstrap-regression.log`
- `prototype/typedio/evidence/check.log`
- `prototype/typedio/evidence/image-contract.log`
- `prototype/typedio/evidence/image-link.log`
- `prototype/typedio/evidence/image-manifest.json`
- `prototype/typedio/evidence/image-static-audit.txt`
- `prototype/typedio/evidence/kernelet-git-audit.txt`
- `prototype/typedio/evidence/linux-config-used.config`
- `prototype/typedio/evidence/rustavailable.log`
- `prototype/typedio/evidence/source-sha256.txt`
- `prototype/typedio/evidence/vostd-baseline.log`
- `prototype/typedio/native-check.sh`

`.build/`、各 Cargo target、.image/.ko/.o/CPIO 为构建产物，不提交源码仓库。handoff-20261118 下的 bundle、source archive、artifact 与 hash 是本地生成的交付包。原 Book、原 TypedIO 设计与原仓库历史实验文件未修改。
