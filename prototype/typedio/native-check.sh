#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Compile exact production sources; mock Linux allocation/scheduling/usercopy
# and OSTD scheduling only. This does not load a module or boot a guest.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .build/typedio-host-harness
# Rust resolves the nested #[path] through this otherwise empty directory.
# Git does not retain empty directories in a fresh checkout.
mkdir -p tests/host-harness/src/mm
cargo build --locked --manifest-path tests/host-harness/Cargo.toml
cargo build --locked --manifest-path tests/asterinas-harness/Cargo.toml
common=(tests/host-harness/shim.c linux/endovisor/typedio_host.c linux/endovisor/typedio_pipe.c linux/endovisor/typedio_resources.c linux/endovisor/typedio_vsock_link.c linux/endovisor/typedio_socket.c)
flags=(-no-pie -D_GNU_SOURCE -DTYPEDIO_NATIVE_TEST -std=gnu11 -Wall -Wextra -Werror -Wno-misleading-indentation -pthread -fsanitize=address,undefined -g)
for fixture in main resources vsock host-file asterinas; do
    source_file="tests/host-harness/$fixture.c"
    library=tests/host-harness/target/debug/libtypedio_host_harness.a
    extra=()
    if [[ $fixture == resources ]]; then extra+=(linux/endovisor/grant.c); fi
    if [[ $fixture == asterinas ]]; then
        source_file=tests/host-harness/host-file.c
        library=tests/asterinas-harness/target/debug/libtypedio_asterinas_harness.a
        extra+=(-DASTERINAS_DRIVER_TEST)
    fi
    output=".build/typedio-host-harness/$fixture-asan"
    gcc "${flags[@]}" "$source_file" "${common[@]}" "${extra[@]}" "$library" -ldl -lm -o "$output"
    ASAN_OPTIONS=detect_leaks=1:symbolize=0 timeout 30 "$output"
done
