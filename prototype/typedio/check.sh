#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
prototype_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$prototype_dir"

cargo fmt --check -p typedio -p typedio-macros
rustfmt --check --edition 2021 --config skip_children=true linux/endovisor/typedio_core.rs linux/endovisor/typedio_queue.rs linux/endovisor/typedio_host_api.rs linux/endovisor/typedio_vsock.rs vostd/src/typedio.rs libs/typedio/src/frontend.rs libs/typedio/src/frontend/pipe.rs vostd/src/irq/line.rs
cargo check --locked -p typedio -p typedio-macros --target x86_64-unknown-none
make "$prototype_dir/kernels/hello/hello" "$prototype_dir/kernels/probe/probe" "$prototype_dir/kernels/yield/yield" "$prototype_dir/kernels/sched/sched" "$prototype_dir/kernels/mem/mem"
cargo build --locked --workspace --target x86_64-unknown-none
cargo test --locked -p typedio
cargo test --locked --manifest-path tests/platform-harness/Cargo.toml

cargo build --locked --manifest-path tools/typedio-device/Cargo.toml
tools/typedio-device/target/debug/typedio-device --help
./typedio/native-check.sh
mkdir -p .build/typedio-check
gcc -std=c11 -Wall -Wextra -Werror tests/typedio_bounds.c -o .build/typedio-check/bounds
.build/typedio-check/bounds
gcc -std=c11 -Wall -Wextra -Werror tests/platform_tls.c -o .build/typedio-check/platform-tls
.build/typedio-check/platform-tls
gcc -O2 -Wall -Wextra -Werror -I linux/endovisor/uapi host/runtime/holder.c host/runtime/sandbox.c -o .build/typedio-check/holder

if [[ -n "${LINUX_DIR:-}" ]]; then
    : "${KERNEL_RUSTC:?set KERNEL_RUSTC to the Linux 6.12 Rust 1.78 compiler}"
    : "${BINDGEN:?set BINDGEN to bindgen 0.65.1}"
    if ! rg -q '^CONFIG_RUST=y$' "$LINUX_DIR/.config"; then
        echo 'ERROR: the module check requires an actually Rust-enabled Linux tree' >&2
        exit 1
    fi
    make -C "$LINUX_DIR" -j"${JOBS:-8}" \
        RUSTC="$KERNEL_RUSTC" HOSTRUSTC="$KERNEL_RUSTC" BINDGEN="$BINDGEN" \
        M="$prototype_dir/linux/endovisor" modules
    nm linux/endovisor/endovisor.ko | rg ' T klet_tio_pipe_core_reserve$'
    tools/typedio-device/target/debug/typedio-device --schema-bytes > .build/typedio-check/schemas-native.bin
    objcopy --dump-section .rodata.klet_tio_schemas=.build/typedio-check/schemas-linux.bin linux/endovisor/typedio.o
    test -s .build/typedio-check/schemas-linux.bin
    cmp .build/typedio-check/schemas-native.bin .build/typedio-check/schemas-linux.bin
    objcopy --dump-section .rodata.klet_image_abi=.build/typedio-check/image-abi-c.bin linux/endovisor/abi_layout.o
    cargo run --locked --manifest-path tests/platform-harness/Cargo.toml --bin image-abi -- .build/typedio-check/image-abi-c.bin
fi

if [[ -n "${ASTERINAS_DIR:-}" ]]; then
    : "${VDSO_LIBRARY_DIR:?set VDSO_LIBRARY_DIR to the actual linux_vdso checkout}"
    (
        cd "$ASTERINAS_DIR"
        OSDK_LOCAL_DEV=1 cargo osdk check -p aster-core
        OSDK_LOCAL_DEV=1 cargo osdk check -p aster-core --features typedio
        OSDK_LOCAL_DEV=1 cargo osdk check -p aster-core --features typedio-polling
        OSDK_LOCAL_DEV=1 cargo osdk check -p asterinas --no-default-features --features kernelet
    )
    make KLET_KERNEL=asterinas ASTERINAS_DIR="$ASTERINAS_DIR" image
    PYTHONDONTWRITEBYTECODE=1 python3 tests/test_image_contract.py .build/asterinas.image
fi
make KLET_KERNEL=piecheck image
