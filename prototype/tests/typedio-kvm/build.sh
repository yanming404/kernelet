#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Build real KVM regression programs; all generated files stay under .build.
set -euo pipefail
cd "$(dirname "$0")/../.."
test_dir=tests/typedio-kvm
output="$PWD/.build/typedio-kvm-user"
asterinas_dir="${ASTERINAS_DIR:-$PWD/../../asterinas}"
mkdir -p "$output"
host_flags=(-static -O2 -Wall -Wextra -Werror -I linux/endovisor/uapi -I host/runtime)
gcc "${host_flags[@]}" -pthread "$test_dir/vsock-host.c" -o .build/typedio-vsock-host
gcc "${host_flags[@]}" "$test_dir/vsock-guests.c" -o .build/typedio-vsock-guests
gcc "${host_flags[@]}" "$test_dir/vsock-lifecycle.c" host/runtime/sandbox.c -o .build/typedio-lifecycle
flags=(-O2 -ffreestanding -fno-builtin -fno-stack-protector -Wall -Wextra -Werror)
gcc "${flags[@]}" -mgeneral-regs-only -Dexperiment_main=platform_main -c "$test_dir/platform.c" -o "$output/platform.o"
gcc "${flags[@]}" -mgeneral-regs-only -c "$test_dir/vsock-user.c" -o "$output/vsock.o"
gcc -c "$test_dir/entry.S" -o "$output/entry.o"
gcc -static -nostdlib -no-pie -Wl,--build-id=none "$output/entry.o" "$output/platform.o" "$output/vsock.o" -o "$output/init"
python3 "$asterinas_dir/kernel/kernelet/mkinitramfs.py" "$output/init" "$output/rootfs.cpio"
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked --release --target x86_64-unknown-linux-gnu --manifest-path tools/typedio-device/Cargo.toml
make KLET_KERNEL=asterinas ASTERINAS_DIR="$asterinas_dir" KLET_INITRAMFS="$output/rootfs.cpio" image
python3 tests/test_image_contract.py .build/asterinas.image
