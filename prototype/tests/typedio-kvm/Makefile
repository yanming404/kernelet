# SPDX-License-Identifier: MPL-2.0
# Run from prototype with: make -f tests/typedio-kvm/Makefile KLET_KERNEL=asterinas stage-typedio-bootstrap initramfs
include Makefile

TYPEDIO_VCPUS ?= 1
TYPEDIO_HOLD_MS ?= 15000
TYPEDIO_CMDLINE ?= console=hvc0

.PHONY: stage-typedio-bootstrap
stage-typedio-bootstrap: runtime
	@test "$(KLET_KERNEL)" = asterinas
	@test -s $(IMAGE) -a -s $(ENDOVISOR_KO)
	$(call stage_base,)
	$(call stage_cmd,typedio-bootstrap)
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /holder run -k -p -c $(SANDBOX_CG) --klet-kill-after $(TYPEDIO_HOLD_MS) asterinas $(TYPEDIO_VCPUS) $(TYPEDIO_CMDLINE)' \
	  > $(STAGE)/tests/10-typedio-bootstrap.cmd

.PHONY: stage-typedio-guest-pair
stage-typedio-guest-pair: stage-typedio-vsock
	cp $(BUILD)/typedio-vsock-guests $(STAGE)/typedio-vsock-guests
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /typedio-vsock-guests' > $(STAGE)/tests/10-typedio-bootstrap.cmd

.PHONY: stage-typedio-lifecycle
TYPEDIO_LIFECYCLE ?= reset
stage-typedio-lifecycle: stage-typedio-vsock
	cp $(BUILD)/typedio-lifecycle $(STAGE)/typedio-lifecycle
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /typedio-lifecycle $(TYPEDIO_LIFECYCLE)' > $(STAGE)/tests/10-typedio-bootstrap.cmd

.PHONY: stage-typedio-host-lifecycle
stage-typedio-host-lifecycle: stage-typedio-lifecycle
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /typedio-device 2 42 --listen 6000 -- /typedio-lifecycle gh-reset' > $(STAGE)/tests/10-typedio-bootstrap.cmd

.PHONY: stage-typedio-vsock
stage-typedio-vsock: stage-typedio-bootstrap
	cp $(BUILD)/typedio-vsock-host $(STAGE)/typedio-vsock-host
	cp tools/typedio-device/target/x86_64-unknown-linux-gnu/release/typedio-device $(STAGE)/typedio-device
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /typedio-device 2 42 --listen 8000 -- /typedio-vsock-host' \
	  > $(STAGE)/tests/10-typedio-bootstrap.cmd

.PHONY: stage-typedio-migration
stage-typedio-migration: stage-typedio-bootstrap
	$(RT_CC) -I host/runtime -o $(BUILD)/migrate-platform tests/typedio-kvm/migrate-platform.c host/runtime/sandbox.c
	cp $(BUILD)/migrate-platform $(STAGE)/migrate-platform
	printf '%s\n' \
	  'run /holder register asterinas /asterinas.image' \
	  'run /migrate-platform' > $(STAGE)/tests/10-typedio-bootstrap.cmd
