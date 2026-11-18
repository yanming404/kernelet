// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_RECORDS_H
#define _KLET_TYPEDIO_RECORDS_H

#include <linux/types.h>

// Host-private snapshot mirrored by typedio::abi::ReservationRecord.
// Only the enclosing authenticated resource operation authorizes byte access.
struct klet_tio_reservation {
	__u64 token;
	__u64 offset;
	__u64 length;
	__u64 progress;
};

struct klet_tio_binding { __u32 line, vcpu; __u64 generation; };
#endif
