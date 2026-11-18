// SPDX-License-Identifier: GPL-2.0
#include <stddef.h>
#include <stdio.h>
#include "../linux/endovisor/typedio_records.h"
#include "../linux/endovisor/uapi/typedio.h"

int main(void)
{
	printf("%zu,%zu,%zu,%zu,%zu,%zu\n",
	       sizeof(struct klet_tio_reservation),
	       _Alignof(struct klet_tio_reservation),
	       offsetof(struct klet_tio_reservation, token),
	       offsetof(struct klet_tio_reservation, offset),
	       offsetof(struct klet_tio_reservation, length),
	       offsetof(struct klet_tio_reservation, progress));
    printf("%zu,%zu,%zu,%zu,%zu,%zu,%zu,%zu,%zu,%zu,%u\n",
        sizeof(struct klet_tio_descriptor), offsetof(struct klet_tio_descriptor, config),
        sizeof(struct klet_tio_record), offsetof(struct klet_tio_record, resources),
        offsetof(struct klet_tio_record, resource_count), sizeof(struct klet_tio_call),
        offsetof(struct klet_tio_call, descriptor), sizeof(struct klet_tio_resource),
        offsetof(struct klet_tio_resource, kind), sizeof(struct klet_tio_binding), KLET_TIO_ABI);
	return 0;
}
