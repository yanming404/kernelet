// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_RESOURCES_H
#define _KLET_TYPEDIO_RESOURCES_H
#include "typedio_pipe.h"
#include "typedio_service.h"
#define KLET_TIO_TRANSFER_MAX 4
struct klet_tio_endpoint { u64 slot, generation; };
struct klet_tio_transfer {
    struct klet_tio_endpoint endpoints[KLET_TIO_TRANSFER_MAX];
    u64 source[KLET_TIO_TRANSFER_MAX], target[KLET_TIO_TRANSFER_MAX];
    bool guest[KLET_TIO_TRANSFER_MAX];
    unsigned int count;
};
void klet_tio_resource_notify_all(void);
int klet_tio_resource_poll(u64 owner, u32 vcpu, struct klet_tio_binding *binding);
void klet_tio_resource_unbind(u64 owner, struct klet_tio_binding binding);
void klet_tio_signal_owner(u64 owner, struct klet_tio_binding binding);
long klet_tio_endpoint_copy_user(struct klet_tio_endpoint key, u64 owner, u64 token, void __user *buffer, u64 bytes, int *fault);
int klet_tio_deliver_roles(struct klet_tio_resource *items, const u64 *targets, unsigned int count);
int klet_tio_resource_validate(struct klet_tio_resource resource);
int klet_tio_record_prepare(struct klet_tio_transfer *transaction, const struct klet_tio_record *record, u64 source, u64 target, bool guest);
void klet_tio_record_drop(struct klet_tio_record *record);
int klet_tio_resource_owner_register(u64 key);
void klet_tio_resource_owner_host(u64 key);
void klet_tio_resource_owner_release(u64 key);
int klet_tio_role_pair(struct klet_tio_pipe_account *account, u64 creator, u64 capacity,
                       struct klet_tio_pipe **control, struct klet_tio_endpoint endpoints[2]);
int klet_tio_transfer_prepare(struct klet_tio_transfer *transaction, struct klet_tio_endpoint endpoint,
                              u64 source, u64 target, bool guest);
int klet_tio_transfer_commit(struct klet_tio_transfer *transaction);
void klet_tio_transfer_rollback(struct klet_tio_transfer *transaction);
void klet_tio_endpoint_drop(struct klet_tio_endpoint endpoint, u64 owner);
void klet_tio_resource_revoke(u64 owner);
void klet_tio_resource_evicted(u64 owner);
s64 klet_tio_resource_service(u64 owner, struct klet_tio_call *call);
#endif
