# Kbuild file for the endovisor.
#
# Built from the parent directory: make module DIR=linux/endovisor
#
# The module carries no kernelet image: an image is a file of its own, which
# the holder registers as a kind (KLET_KIND_REGISTER) and image.c loads and
# relocates. One endovisor.ko serves every kernel proper.
obj-m += endovisor.o

endovisor-y := main.o image.o abi_layout.o grant.o sandbox.o carrier.o space.o evict.o watch.o sched.o switch.o

# objtool does not understand a function that returns on a different stack
# than it was called on.
OBJECT_FILES_NON_STANDARD_switch.o := y

# TypedIO metadata uses Linux Rust support; payload and ownership wrappers stay in C.
endovisor-$(CONFIG_RUST) += typedio_core.o typedio_pipe.o typedio_queue.o typedio_host.o typedio_resources.o typedio_vsock_link.o typedio_socket.o

# The same derive source is compiled by the Linux compiler, not loaded from
# the independently pinned guest nightly. Only proc-macro host dependencies
# differ; generated fields/schema/transfer code come from the same source.
TIO_MACRO_MANIFEST := $(src)/../../libs/typedio-kernel-macros/Cargo.toml
TIO_MACRO_LIBRARY := $(abspath $(src)/../../libs/typedio-kernel-macros/target/release/libtypedio_macros.so)
$(TIO_MACRO_LIBRARY): $(TIO_MACRO_MANIFEST) $(src)/../../libs/typedio-macros/src/lib.rs
	RUSTUP_TOOLCHAIN=1.78.0 cargo build --release --manifest-path $(TIO_MACRO_MANIFEST)
$(obj)/typedio.o: $(TIO_MACRO_LIBRARY)
RUSTFLAGS_typedio.o += --extern typedio_macros=$(TIO_MACRO_LIBRARY)
endovisor-$(CONFIG_RUST) += typedio.o
