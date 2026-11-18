// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_SERVICE_H
#define _KLET_TYPEDIO_SERVICE_H
#include <linux/types.h>
#include <linux/ioctl.h>

#define KLET_TIO_ABI 2
#define KLET_TIO_METADATA 128
#define KLET_TIO_DEPTH 8
#define KLET_TIO_ATTACHMENTS 8
#define KLET_TIO_DISCOVER 1
#define KLET_TIO_OPEN 2
#define KLET_TIO_SUBMIT 3
#define KLET_TIO_POP 4
#define KLET_TIO_CANCEL 5
#define KLET_TIO_EVENTS 6
#define KLET_TIO_REARM 7
#define KLET_TIO_CLOSE 8
#define KLET_TIO_BIND 9
#define KLET_TIO_IRQ_POLL 10
#define KLET_TIO_UNBIND_LINE 11

struct klet_tio_descriptor {
    __u32 abi_version, protocol_version;
    __u64 features;
    __u8 protocol[16], schemas[3][32];
    __u32 config_len, reserved;
    __u8 config[KLET_TIO_METADATA];
};
struct klet_tio_resource {
    __u64 slot, generation, owner;
    __u32 kind, reserved;
};
struct klet_tio_record {
    __u64 generation, sequence, tag;
    __u32 len, status;
    __u8 data[KLET_TIO_METADATA];
    struct klet_tio_resource resources[4];
    __u32 resource_count, reserved;
};
struct klet_tio_call {
    __u32 operation, flags;
    __u64 handle, generation, arg0, arg1;
    struct klet_tio_record record;
    struct klet_tio_descriptor descriptor;
};
struct klet_tio_listen_args { __u32 port, backlog, flags, reserved; };
#define KLET_TIO_LISTEN _IOW('T', 1, struct klet_tio_listen_args)
#define KLET_TIO_ACCEPT _IO('T', 2)
// Provider authority may revoke its attachment without destroying its sandbox.
// Existing requests finish only after their independently retained accesses end.
#define KLET_TIO_REMOVE_DEVICE _IO('T', 3)
#endif
