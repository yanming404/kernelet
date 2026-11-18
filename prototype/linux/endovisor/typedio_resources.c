// SPDX-License-Identifier: GPL-2.0
// Authoritative host capability records. No caller-supplied address is accepted.
#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/errno.h>
#include <linux/err.h>
#include <linux/kernel.h>
#include <linux/spinlock.h>
#endif
#include "typedio_resources.h"

#define RESOURCE_SLOTS 512
#define OWNER_ROLES 32
struct tio_resource {
    struct klet_tio_pipe *pipe;
    u64 generation, owner, target, token;
    u32 role;
    bool prepared, revoked, guest, pending;
    struct klet_tio_binding binding;
};
static DEFINE_SPINLOCK(resource_lock);
static struct tio_resource resources[RESOURCE_SLOTS];
struct tio_owner { u64 key; u32 references, poll_cursor; bool alive, released, guest; };
static struct tio_owner owners[RESOURCE_SLOTS];
static struct tio_owner *owner_find(u64 key)
{
    unsigned int i;
    for (i = 0; i < RESOURCE_SLOTS; i++)
        if (owners[i].key == key) return &owners[i];
    return NULL;
}
static void owner_put_locked(u64 key)
{
    struct tio_owner *owner = owner_find(key);
    if (WARN_ON(!owner || !owner->references)) return;
    owner->references--;
    if (!owner->references) {
        WARN_ON(!owner->released);
        owner->key = 0;
    }
}
int klet_tio_resource_owner_register(u64 key)
{
    unsigned long flags;
    unsigned int i;
    int result = -EDQUOT;
    if (!key) return -EINVAL;
    spin_lock_irqsave(&resource_lock, flags);
    if (owner_find(key)) { result = -EEXIST; goto out; }
    for (i = 0; i < RESOURCE_SLOTS; i++)
        if (!owners[i].key) { owners[i] = (struct tio_owner){ .key = key, .references = 1, .alive = true, .guest = true }; result = 0; break; }
out:
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}
void klet_tio_resource_owner_host(u64 key)
{
    unsigned long flags;
    spin_lock_irqsave(&resource_lock, flags);
    if (owner_find(key)) owner_find(key)->guest = false;
    spin_unlock_irqrestore(&resource_lock, flags);
}
void klet_tio_resource_owner_release(u64 key)
{
    unsigned long flags;
    struct tio_owner *owner;
    klet_tio_resource_revoke(key);
    spin_lock_irqsave(&resource_lock, flags);
    owner = owner_find(key);
    if (owner && !owner->released) {
        owner->released = true;
        owner_put_locked(key);
    }
    spin_unlock_irqrestore(&resource_lock, flags);
}

static struct tio_resource *resource_find(struct klet_tio_endpoint key, u64 owner)
{
    struct tio_resource *resource;
    if (key.slot >= RESOURCE_SLOTS)
        return NULL;
    resource = &resources[key.slot];
    if (!resource->pipe || resource->generation != key.generation || resource->owner != owner)
        return NULL;
    return resource;
}
static unsigned int owner_roles(u64 owner)
{
    unsigned int i, count = 0;
    for (i = 0; i < RESOURCE_SLOTS; i++)
        if (resources[i].pipe && (resources[i].owner == owner ||
            (resources[i].prepared && resources[i].target == owner)))
            count++;
    return count;
}
static int resource_create_locked(struct klet_tio_pipe *pipe, u64 owner, u32 role,
                                   struct klet_tio_endpoint *result)
{
    unsigned int i;
    struct tio_owner *identity = owner_find(owner);
    if (!identity || !identity->alive) return -ESTALE;
    if (owner_roles(owner) >= OWNER_ROLES)
        return -EDQUOT;
    for (i = 0; i < RESOURCE_SLOTS; i++)
        if (!resources[i].pipe && resources[i].generation != U64_MAX) {
            resources[i].generation++;
            resources[i].pipe = pipe;
            resources[i].owner = owner;
            identity->references++;
            resources[i].role = role;
            resources[i].token = 0;
            resources[i].prepared = resources[i].revoked = resources[i].guest = resources[i].pending = false;
            resources[i].binding = (struct klet_tio_binding){};
            klet_tio_pipe_get(pipe);
            result->slot = i;
            result->generation = resources[i].generation;
            return 0;
        }
    return -EDQUOT;
}
static void resource_free_locked(struct tio_resource *resource)
{
    struct klet_tio_pipe *pipe = resource->pipe;
    resource->pipe = NULL;
    owner_put_locked(resource->owner);
    klet_tio_pipe_put(pipe);
}
int klet_tio_role_pair(struct klet_tio_pipe_account *account, u64 creator, u64 capacity,
                       struct klet_tio_pipe **control, struct klet_tio_endpoint endpoints[2])
{
    struct klet_tio_pipe *pipe = klet_tio_pipe_create(account, creator, creator, capacity);
    unsigned long flags;
    int result;
    if (IS_ERR(pipe))
        return PTR_ERR(pipe);
    spin_lock_irqsave(&resource_lock, flags);
    result = resource_create_locked(pipe, creator, 0, &endpoints[0]);
    if (!result) {
        result = resource_create_locked(pipe, creator, 1, &endpoints[1]);
        if (result)
            resource_free_locked(&resources[endpoints[0].slot]);
    }
    spin_unlock_irqrestore(&resource_lock, flags);
    if (result) { klet_tio_pipe_put(pipe); return result; }
    *control = pipe;
    return 0;
}
int klet_tio_transfer_prepare(struct klet_tio_transfer *transaction, struct klet_tio_endpoint endpoint,
                              u64 source, u64 target, bool guest)
{
    struct tio_resource *resource;
    unsigned long flags;
    int result = -ESTALE;
    if (transaction->count == KLET_TIO_TRANSFER_MAX || source == target || !target)
        return -EINVAL;
    spin_lock_irqsave(&resource_lock, flags);
    resource = resource_find(endpoint, source);
    if (!owner_find(target) || !owner_find(target)->alive) goto out;
    if (!resource || resource->revoked)
        goto out;
    result = -EBUSY;
    if (resource->prepared || resource->token)
        goto out;
    result = -EDQUOT;
    if (owner_roles(target) >= OWNER_ROLES)
        goto out;
    result = klet_tio_pipe_can_transfer(resource->pipe, source, resource->role);
    if (result)
        goto out;
    resource->prepared = true;
    resource->target = target;
    owner_find(target)->references++;
    transaction->endpoints[transaction->count] = endpoint;
    transaction->source[transaction->count] = source;
    transaction->target[transaction->count] = target;
    transaction->guest[transaction->count++] = guest;
out:
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}
void klet_tio_transfer_rollback(struct klet_tio_transfer *transaction)
{
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&resource_lock, flags);
    for (i = 0; i < transaction->count; i++) {
        struct tio_resource *r = resource_find(transaction->endpoints[i], transaction->source[i]);
        if (r) {
            r->prepared = false;
            owner_put_locked(r->target);
            if (r->revoked && !r->token) resource_free_locked(r);
        }
    }
    transaction->count = 0;
    spin_unlock_irqrestore(&resource_lock, flags);
}
int klet_tio_transfer_commit(struct klet_tio_transfer *transaction)
{
    unsigned long flags;
    unsigned int i;
    int result = 0;
    spin_lock_irqsave(&resource_lock, flags);
    // Preparation reserves destination quotas and prevents new reservations.
    // Validate every entry before changing any role, including revocation races.
    for (i = 0; i < transaction->count; i++) {
        struct tio_resource *r = resource_find(transaction->endpoints[i], transaction->source[i]);
        struct tio_owner *target = owner_find(transaction->target[i]);
        if (!r || !r->prepared || r->revoked || r->target != transaction->target[i] || !target || !target->alive) {
            result = -ESTALE;
            goto out;
        }
    }
    for (i = 0; i < transaction->count; i++) {
        struct tio_resource *r = resource_find(transaction->endpoints[i], transaction->source[i]);
        // No reservation can begin while prepared. This metadata-only move
        // cannot fail after the complete preflight under this same ledger lock.
        result = klet_tio_pipe_transfer(r->pipe, r->owner, r->role, r->target);
        if (WARN_ON(result))
            goto out;
        owner_put_locked(r->owner);
        // The reserved destination reference now owns the transferred role.
        r->owner = r->target;
        r->prepared = false;
        r->guest = transaction->guest[i] && owner_find(r->owner)->guest;
        r->pending = false;
        r->binding = (struct klet_tio_binding){};
    }
    transaction->count = 0;
out:
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}
void klet_tio_endpoint_drop(struct klet_tio_endpoint endpoint, u64 owner)
{
    unsigned long flags;
    struct tio_resource *r;
    spin_lock_irqsave(&resource_lock, flags);
    r = resource_find(endpoint, owner);
    if (r) {
        r->revoked = true;
        klet_tio_pipe_close(r->pipe, r->role);
        if (!r->token && !r->prepared)
            resource_free_locked(r);
    }
    spin_unlock_irqrestore(&resource_lock, flags);
}
void klet_tio_resource_revoke(u64 owner)
{
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&resource_lock, flags);
    if (owner_find(owner)) owner_find(owner)->alive = false;
    for (i = 0; i < RESOURCE_SLOTS; i++) {
        struct tio_resource *r = &resources[i];
        if (!r->pipe || r->owner != owner)
            continue;
        r->revoked = true;
        klet_tio_pipe_close(r->pipe, r->role);
        if (!r->token && !r->prepared)
            resource_free_locked(r);
    }
    spin_unlock_irqrestore(&resource_lock, flags);
}
void klet_tio_resource_invalidate(u64 owner)
{
    unsigned long flags;
    unsigned int i;
    // Stop admission and access immediately. Backend removal must publish its
    // terminal policy before role closure can expose a graceful pipe EOF.
    spin_lock_irqsave(&resource_lock, flags);
    if (owner_find(owner)) owner_find(owner)->alive = false;
    for (i = 0; i < RESOURCE_SLOTS; i++)
        if (resources[i].pipe && resources[i].owner == owner)
            resources[i].revoked = true;
    spin_unlock_irqrestore(&resource_lock, flags);
}
void klet_tio_resource_evicted(u64 owner)
{
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&resource_lock, flags);
    for (i = 0; i < RESOURCE_SLOTS; i++) {
        struct tio_resource *r = &resources[i];
        if (!r->pipe || r->owner != owner || !r->guest || !r->revoked)
            continue;
        // Only after all guest carriers are evicted and tenant mappings gone.
        // Host user-copy reservations are never forcibly finalized here.
        if (r->token) {
            klet_tio_pipe_cancel(r->pipe, owner, r->role, r->token);
            r->token = 0;
        }
        if (!r->prepared)
            resource_free_locked(r);
    }
    spin_unlock_irqrestore(&resource_lock, flags);
}
s64 klet_tio_resource_service(u64 owner, struct klet_tio_call *call)
{
    struct klet_tio_endpoint endpoint = { call->arg0, call->arg1 };
    struct tio_resource *r;
    struct klet_tio_reservation reservation;
    unsigned long flags;
    int result = -ESTALE;
    spin_lock_irqsave(&resource_lock, flags);
    r = resource_find(endpoint, owner);
    if (!r) goto out;
    if (call->operation == 20) {
        r->revoked = true;
        klet_tio_pipe_close(r->pipe, r->role);
        if (!r->token && !r->prepared) resource_free_locked(r);
        result = 0; goto out;
    }
    if (r->prepared) { result = -EBUSY; goto out; }
    if (call->operation >= 21 && call->operation <= 26) {
        struct klet_tio_binding binding = {call->record.sequence, call->record.tag, call->record.generation};
        if (call->operation == 21 && (binding.line >= 64 || !binding.generation)) { result = -EINVAL; goto out; }
        result = klet_tio_pipe_notify(r->pipe, owner, r->role, call->operation, call->record.status, &binding);
        if (call->operation == 21 && !result) {r->binding = binding; r->pending = false;}
        goto out;
    }
    if (call->operation == 16) {
        if (r->revoked) { result = -EPIPE; goto out; }
        if (call->record.status != r->role) { result = -EPERM; goto out; }
        result = klet_tio_pipe_reserve(r->pipe, owner, r->role, call->record.len, &reservation);
        if (!result) {
            r->token = reservation.token;
            call->record.generation = reservation.token;
            call->record.len = reservation.length;
            call->record.sequence = (u64)klet_tio_pipe_address(r->pipe, reservation.offset, reservation.length);
        }
    } else {
        if (!r->token || r->token != call->record.generation) { result = -EINVAL; goto out; }
        if (call->operation == 17)
            result = klet_tio_pipe_progress(r->pipe, owner, r->role, r->token, call->record.len);
        else if (call->operation == 18)
            result = klet_tio_pipe_commit(r->pipe, owner, r->role, r->token, call->record.len);
        else if (call->operation == 19)
            result = klet_tio_pipe_cancel(r->pipe, owner, r->role, r->token);
        else { result = -EINVAL; goto out; }
        if (call->operation != 17 && (!result || result == -EPIPE || result == -ECONNRESET)) {
            r->token = 0;
            if (r->revoked) resource_free_locked(r);
        }
    }
out:
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}

int klet_tio_resource_validate(struct klet_tio_resource resource)
{
    struct klet_tio_endpoint key = {resource.slot, resource.generation};
    struct tio_resource *r;
    unsigned long flags;
    int result = -ESTALE;
    spin_lock_irqsave(&resource_lock, flags);
    r = resource_find(key, resource.owner);
    if (r && !r->revoked && !r->prepared && resource.kind == r->role + 1 && !resource.reserved)
        result = 0;
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}
int klet_tio_record_prepare(struct klet_tio_transfer *transaction, const struct klet_tio_record *record,
                            u64 source, u64 target, bool guest)
{
    unsigned int i, j;
    int result;
    if (record->resource_count > KLET_TIO_TRANSFER_MAX || record->reserved) return -EINVAL;
    for (i = 0; i < record->resource_count; i++) {
        struct klet_tio_resource r = record->resources[i];
        struct klet_tio_endpoint key = {r.slot, r.generation};
        if (r.owner != source || r.kind < 1 || r.kind > 2 || r.reserved) return -EPERM;
        for (j = 0; j < i; j++)
            if (record->resources[j].slot == r.slot && record->resources[j].generation == r.generation)
                return -EINVAL;
        result = klet_tio_resource_validate(r);
        if (result) return result;
        if (source == target) continue;
        result = klet_tio_transfer_prepare(transaction, key, source, target, guest);
        if (result) return result;
    }
    return 0;
}
void klet_tio_record_drop(struct klet_tio_record *record)
{
    unsigned int i;
    for (i = 0; i < min_t(u32, record->resource_count, KLET_TIO_TRANSFER_MAX); i++) {
        struct klet_tio_resource r = record->resources[i];
        struct klet_tio_endpoint key = {r.slot, r.generation};
        klet_tio_endpoint_drop(key, r.owner);
    }
    record->resource_count = 0;
}

// Capture routing records under the ledger lock, then raise host vCPU prompts
// outside every pipe/ledger/queue lock. The retained source is authoritative.
void klet_tio_resource_notify_all(void)
{
    unsigned int i;
    unsigned long flags;
    for (i = 0; i < RESOURCE_SLOTS; i++) {
        struct klet_tio_binding binding = {};
        u64 owner = 0;
        spin_lock_irqsave(&resource_lock, flags);
        struct tio_resource *r = &resources[i];
        if (r->pipe && !r->prepared && r->binding.generation &&
            klet_tio_pipe_notify(r->pipe, r->owner, r->role, 25, 0, &binding) == 1) {
            r->pending = true;
            r->binding = binding;
            owner = r->owner;
        }
        spin_unlock_irqrestore(&resource_lock, flags);
        if (owner) klet_tio_signal_owner(owner, binding);
    }
}
int klet_tio_resource_poll(u64 owner, u32 vcpu, struct klet_tio_binding *binding)
{
    unsigned int i;
    unsigned long flags;
    int result = 1;
    spin_lock_irqsave(&resource_lock, flags);
    struct tio_owner *identity = owner_find(owner);
    unsigned int start = identity ? identity->poll_cursor : 0;
    for (i = 0; i < RESOURCE_SLOTS; i++) {
        unsigned int index = (start + i) % RESOURCE_SLOTS;
        struct tio_resource *r = &resources[index];
        if (r->pipe && r->owner == owner && r->pending && r->binding.vcpu == vcpu) {
            r->pending = false;
            if (identity) identity->poll_cursor = (index + 1) % RESOURCE_SLOTS;
            *binding = r->binding;
            result = 0;
            break;
        }
    }
    spin_unlock_irqrestore(&resource_lock, flags);
    return result;
}
void klet_tio_resource_unbind(u64 owner, struct klet_tio_binding binding)
{
    unsigned int i;
    unsigned long flags;
    spin_lock_irqsave(&resource_lock, flags);
    for (i = 0; i < RESOURCE_SLOTS; i++) {
        struct tio_resource *r = &resources[i];
        if (r->pipe && r->owner == owner && r->binding.line == binding.line && r->binding.generation == binding.generation) {
            // Binding an empty tombstone suppresses delivery. The source's
            // terminal causes remain until the role is reclaimed or moved.
            struct klet_tio_binding empty = {};
            klet_tio_pipe_notify(r->pipe, owner, r->role, 21, 0, &empty);
            r->binding = empty;
            r->pending = false;
        }
    }
    spin_unlock_irqrestore(&resource_lock, flags);
}

// Commit every event destination before either event becomes observable.
int klet_tio_deliver_roles(struct klet_tio_resource *items, const u64 *targets, unsigned int count)
{
    struct klet_tio_transfer transaction = {};
    unsigned int i;
    int result = -EINVAL;
    if (count > KLET_TIO_TRANSFER_MAX) return result;
    for (i = 0; i < count; i++) {
        struct klet_tio_record record = { .resource_count = 1 };
        record.resources[0] = items[i];
        result = klet_tio_record_prepare(&transaction, &record, items[i].owner, targets[i], true);
        if (result) goto rollback;
    }
    result = klet_tio_transfer_commit(&transaction);
    if (result) goto rollback;
    for (i = 0; i < count; i++) items[i].owner = targets[i];
    return 0;
rollback:
    klet_tio_transfer_rollback(&transaction);
    return result;
}

// User copy pins the pipe separately before dropping the capability ledger
// lock. No user fault can hold a global metadata or backend policy lock.
long klet_tio_endpoint_copy_user(struct klet_tio_endpoint key, u64 owner,
                                u64 token, void __user *buffer, u64 bytes, int *fault)
{
    struct klet_tio_pipe *pipe = NULL;
    struct tio_resource *r;
    unsigned long flags;
    unsigned int role = 0;
    long result;
    spin_lock_irqsave(&resource_lock, flags);
    r = resource_find(key, owner);
    if (r && r->token == token) { pipe = r->pipe; role = r->role; klet_tio_pipe_get(pipe); }
    spin_unlock_irqrestore(&resource_lock, flags);
    if (!pipe) return -ESTALE;
    result = klet_tio_pipe_copy_user(pipe, owner, role, token, buffer, bytes, fault);
    klet_tio_pipe_put(pipe);
    return result;
}
