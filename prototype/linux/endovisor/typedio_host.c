// SPDX-License-Identifier: GPL-2.0
// Host-private attachment, dispatch and retirement records.
// No worker retains the caller's argument address or invokes guest code.

#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/anon_inodes.h>
#include <linux/cgroup.h>
#include <linux/err.h>
#include <linux/file.h>
#include <linux/fs.h>
#include <linux/slab.h>
#include <linux/uaccess.h>
#include <linux/wait.h>
#include "kernelet.h"
#endif
#include "typedio_host.h"
#include "typedio_resources.h"
#include "typedio_socket.h"

#define TIO_MAX_OBJECTS 128
struct tio_device;
struct tio_queue;
struct klet_tio_request {
    struct work_struct work;
    struct tio_queue *queue;
    struct klet_tio_record message, reply;
    u32 accesses;
    bool live, finish_requested, published;
};
struct tio_irq_binding { u32 line, vcpu; u64 generation; };
struct tio_queue {
    refcount_t refs;
    spinlock_t lock;
    struct tio_device *device;
    void *core;
    u64 generation;
    bool closed, irq_pending;
    struct tio_irq_binding binding;
    struct klet_tio_request requests[KLET_TIO_DEPTH];
};
struct tio_device {
    refcount_t refs;
    spinlock_t lock;
    struct klet_tio_backend *backend;
    struct klet_tio_backend backend_copy;
    struct klet_tio_backend *registration;
    struct klet_tio_descriptor descriptor;
    struct klet_inst *instance;
    void *context;
    u64 generation;
    bool bound, removed;
    unsigned int poll_cursor;
    bool poll_resources_next;
    struct tio_queue *queues[KLET_TIO_ATTACHMENTS];
    struct work_struct removal;
    struct work_struct poll_work;
    struct work_struct release_work;
    struct rw_semaphore dispatch_gate;
    wait_queue_head_t host_wait;
    atomic64_t host_wait_generation;
};
static DEFINE_MUTEX(tio_registry_lock);
static LIST_HEAD(tio_backends);
static struct workqueue_struct *tio_workers;
static DEFINE_SPINLOCK(tio_device_table_lock);
static struct tio_device *tio_device_table[TIO_MAX_OBJECTS];
static void tio_device_remove(struct tio_device *device);
static void tio_device_put(struct tio_device *device);
static atomic_t tio_devices = ATOMIC_INIT(0);
static atomic_t tio_queues = ATOMIC_INIT(0);
static atomic64_t tio_generation = ATOMIC64_INIT(0);
extern size_t klet_tio_queue_size(void);
extern int klet_tio_queue_init(void *, u64, size_t);
extern int klet_tio_queue_op(void *, u32, struct klet_tio_record *);
extern void klet_tio_queue_destroy(void *);
extern bool klet_tio_queue_delivery(void *, struct tio_irq_binding *);

static u64 tio_new_generation(void)
{
    s64 value = atomic64_read(&tio_generation);
    for (;;) {
        if (value == S64_MAX)
            return 0;
        if (atomic64_try_cmpxchg(&tio_generation, &value, value + 1))
            return value + 1;
    }
}
static bool tio_schemas_equal(const struct klet_tio_descriptor *a,
                              const struct klet_tio_descriptor *b)
{
    return a->abi_version == KLET_TIO_ABI && a->protocol_version == b->protocol_version &&
           !memcmp(a->protocol, b->protocol, sizeof(a->protocol)) &&
           !memcmp(a->schemas, b->schemas, sizeof(a->schemas)) &&
           !(a->features & ~b->features);
}
int klet_tio_backend_register(struct klet_tio_backend *backend)
{
    struct klet_tio_backend *entry;
    int result = 0;
    if (!backend->handle || !backend->prepare || !backend->release ||
        backend->descriptor.abi_version != KLET_TIO_ABI)
        return -EINVAL;
    mutex_lock(&tio_registry_lock);
    list_for_each_entry(entry, &tio_backends, link)
        if (!memcmp(entry->descriptor.protocol, backend->descriptor.protocol, 16)) {
            result = -EEXIST;
            goto out;
        }
    list_add_tail(&backend->link, &tio_backends);
out:
    mutex_unlock(&tio_registry_lock);
    return result;
}
int klet_tio_backend_unregister(struct klet_tio_backend *backend)
{
    struct tio_device *profiles[TIO_MAX_OBJECTS];
    unsigned int i, count = 0;
    unsigned long flags;
    mutex_lock(&tio_registry_lock);
    list_del_init(&backend->link);
    spin_lock_irqsave(&tio_device_table_lock, flags);
    for (i = 0; i < TIO_MAX_OBJECTS; i++)
        if (tio_device_table[i] && tio_device_table[i]->registration == backend &&
            refcount_inc_not_zero(&tio_device_table[i]->refs))
            profiles[count++] = tio_device_table[i];
    spin_unlock_irqrestore(&tio_device_table_lock, flags);
    mutex_unlock(&tio_registry_lock);
    for (i = 0; i < count; i++) {tio_device_remove(profiles[i]); tio_device_put(profiles[i]);}
    return 0;
}
static void tio_device_release_work(struct work_struct *work)
{
    struct tio_device *device = container_of(work, struct tio_device, release_work);
    // Final context destruction may sleep. Never run it in an I/O end callback.
    device->backend->release(device->context);
    klet_tio_resource_owner_release(device->generation | (1ULL << 63));
    klet_tio_resource_owner_release(device->generation);
    module_put(device->backend->module);
    atomic_dec(&tio_devices);
    kfree(device);
    module_put(THIS_MODULE);
}
static void tio_device_put(struct tio_device *device)
{
    if (refcount_dec_and_test(&device->refs)) {
        unsigned long flags;
        unsigned int i;
        spin_lock_irqsave(&tio_device_table_lock, flags);
        for (i = 0; i < TIO_MAX_OBJECTS; i++)
            if (tio_device_table[i] == device) tio_device_table[i] = NULL;
        spin_unlock_irqrestore(&tio_device_table_lock, flags);
        queue_work(tio_workers, &device->release_work);
    }
}
void klet_tio_host_get(void *pointer) { struct tio_device *device = pointer; refcount_inc(&device->refs); }
void klet_tio_host_put(void *pointer) { tio_device_put(pointer); }
struct wait_queue_head *klet_tio_host_wait_queue(void *pointer)
{ struct tio_device *device = pointer; return &device->host_wait; }
u64 klet_tio_host_wait_generation(void *pointer)
{ struct tio_device *device = pointer; return atomic64_read(&device->host_wait_generation); }
static void tio_host_wake_device(struct tio_device *device)
{
    s64 value = atomic64_read(&device->host_wait_generation);
    while (value != S64_MAX)
        if (atomic64_try_cmpxchg(&device->host_wait_generation, &value, value + 1)) break;
    wake_up_interruptible_all(&device->host_wait);
}
void klet_tio_host_wake(u64 owner)
{
    struct tio_device *device = NULL;
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&tio_device_table_lock, flags);
    for (i = 0; i < TIO_MAX_OBJECTS; i++)
        if (tio_device_table[i] && tio_device_table[i]->generation == owner &&
            refcount_inc_not_zero(&tio_device_table[i]->refs)) { device = tio_device_table[i]; break; }
    spin_unlock_irqrestore(&tio_device_table_lock, flags);
    if (device) { tio_host_wake_device(device); tio_device_put(device); }
}
static void tio_queue_put(struct tio_queue *queue)
{
    if (!refcount_dec_and_test(&queue->refs))
        return;
    unsigned int i;
    for (i = 0; i < KLET_TIO_DEPTH; i++) {
        klet_tio_record_drop(&queue->requests[i].message);
        klet_tio_record_drop(&queue->requests[i].reply);
    }
    klet_tio_queue_destroy(queue->core);
    kfree(queue->core);
    tio_device_put(queue->device);
    atomic_dec(&tio_queues);
    kfree(queue);
}
// Publication retains a source prompt independently of the coalesced vCPU bit.
static void tio_capture_irq_locked(struct tio_queue *queue)
{
    struct tio_irq_binding binding;
    if (klet_tio_queue_delivery(queue->core, &binding)) {
        queue->binding = binding;
        queue->irq_pending = true;
    }
}
// Called with no queue/metadata lock held. Device routing is withdrawn before
// instance destruction; a temporary instance reference protects carrier lookup.
static void tio_signal(struct tio_queue *queue)
{
    struct klet_inst *inst;
    struct klet_carrier *carrier;
    unsigned long flags;
    u32 target;
    bool pending;
    spin_lock_irqsave(&queue->lock, flags);
    target = queue->binding.vcpu;
    pending = queue->irq_pending && queue->binding.generation;
    spin_unlock_irqrestore(&queue->lock, flags);
    if (!pending) return;
    spin_lock_irqsave(&queue->device->lock, flags);
    inst = queue->device->instance;
    if (inst) klet_inst_get(inst);
    spin_unlock_irqrestore(&queue->device->lock, flags);
    if (!inst) return;
    spin_lock_irqsave(&inst->lock, flags);
    if (!inst->dying && target < inst->nr_vcpus) {
        carrier = inst->vcpus[target];
        if (carrier) klet_virq_raise(carrier, KLET_VIRQ_LINES);
    }
    spin_unlock_irqrestore(&inst->lock, flags);
    klet_inst_put(inst);
}
static void tio_finalize_resources_locked(struct klet_tio_request *request)
{
    struct tio_queue *queue = request->queue;
    struct klet_tio_record *reply = &request->reply;
    struct klet_tio_transfer transfer = {};
    unsigned int i, j;
    int result = 0;
    if (reply->len > KLET_TIO_METADATA || reply->resource_count > 4 || reply->reserved || reply->status > 6)
        result = -EINVAL;
    if (reply->status) {
        klet_tio_record_drop(reply);
        reply->len = 0;
    } else {
        for (i = 0; !result && i < reply->resource_count; i++) {
            struct klet_tio_record one = { .resource_count = 1 };
            one.resources[0] = reply->resources[i];
            for (j = 0; j < i; j++)
                if (reply->resources[j].slot == one.resources[0].slot &&
                    reply->resources[j].generation == one.resources[0].generation) result = -EINVAL;
            if (!result) result = klet_tio_record_prepare(&transfer, &one, one.resources[0].owner,
                                                        queue->device->generation, true);
        }
        if (!result) result = klet_tio_transfer_commit(&transfer);
        if (!result) for (i = 0; i < reply->resource_count; i++) reply->resources[i].owner = queue->device->generation;
    }
    if (result) {
        klet_tio_transfer_rollback(&transfer);
        klet_tio_record_drop(reply);
        reply->status = result == -ESTALE ? 2 : 4;
        reply->len = 0;
    }
    // Deferred until every accepted host access is quiescent. Sender wrappers
    // can disappear earlier; these host records retain their cleanup authority.
    klet_tio_record_drop(&request->message);
}
static void tio_publish_locked(struct klet_tio_request *request)
{
    struct tio_queue *queue = request->queue;
    struct klet_tio_record discarded = {};
    if (!request->finish_requested || request->accesses || request->published)
        return;
    tio_finalize_resources_locked(request);
    request->published = true;
    WARN_ON(klet_tio_queue_op(queue->core, 101, &request->reply));
    tio_capture_irq_locked(queue);
    // A closed frontend cannot consume results. Keep active records until
    // quiescence, then discard only final records; never free underneath I/O.
    if (queue->closed)
        while (!klet_tio_queue_op(queue->core, KLET_TIO_POP, &discarded)) {
            unsigned int i;
            for (i = 0; i < KLET_TIO_DEPTH; i++)
                if (queue->requests[i].message.sequence == discarded.sequence)
                    { klet_tio_record_drop(&queue->requests[i].reply); queue->requests[i].live = false; }
        }
}
void klet_tio_request_finish(struct klet_tio_request *request,
                             const struct klet_tio_record *reply)
{
    struct tio_queue *queue = request->queue;
    unsigned long flags;
    bool retired;
    spin_lock_irqsave(&queue->lock, flags);
    if (!request->live || request->finish_requested) {
        spin_unlock_irqrestore(&queue->lock, flags);
        return;
    }
    request->reply = *reply;
    request->reply.generation = request->message.generation;
    request->reply.sequence = request->message.sequence;
    request->finish_requested = true;
    tio_publish_locked(request);
    retired = request->published;
    spin_unlock_irqrestore(&queue->lock, flags);
    klet_tio_resource_notify_all();
    if (retired) {
        tio_signal(queue);
        tio_queue_put(queue);
    }
}
int klet_tio_request_begin_access(struct klet_tio_request *request)
{
    unsigned long flags;
    int result = 0;
    spin_lock_irqsave(&request->queue->lock, flags);
    if (!request->live || request->finish_requested || request->accesses == U32_MAX)
        result = -EBUSY;
    else
        request->accesses++;
    spin_unlock_irqrestore(&request->queue->lock, flags);
    return result;
}
void klet_tio_request_end_access(struct klet_tio_request *request)
{
    struct tio_queue *queue = request->queue;
    unsigned long flags;
    bool retired;
    spin_lock_irqsave(&queue->lock, flags);
    if (WARN_ON(!request->accesses)) {
        spin_unlock_irqrestore(&queue->lock, flags);
        return;
    }
    request->accesses--;
    retired = !request->published && request->finish_requested && !request->accesses;
    tio_publish_locked(request);
    spin_unlock_irqrestore(&queue->lock, flags);
    klet_tio_resource_notify_all();
    if (retired) {
        tio_signal(queue);
        tio_queue_put(queue);
    }
}
bool klet_tio_request_cancelled(struct klet_tio_request *request)
{
    unsigned long flags;
    int result;
    struct klet_tio_record id = request->message;
    spin_lock_irqsave(&request->queue->lock, flags);
    result = klet_tio_queue_op(request->queue->core, 106, &id);
    spin_unlock_irqrestore(&request->queue->lock, flags);
    return result == 1 || READ_ONCE(request->queue->closed) || READ_ONCE(request->queue->device->removed);
}
const struct klet_tio_record *klet_tio_request_message(struct klet_tio_request *request)
{
    return &request->message;
}
u64 klet_tio_request_host_owner(struct klet_tio_request *request)
{
    return request->queue->device->generation | (1ULL << 63);
}
u64 klet_tio_request_queue_generation(struct klet_tio_request *request)
{ return request->queue->generation; }
u64 klet_tio_request_caller(struct klet_tio_request *request)
{
    return request->queue->device->generation;
}
static void tio_dispatch(struct work_struct *work)
{
    struct klet_tio_request *request = container_of(work, struct klet_tio_request, work);
    struct tio_device *device = request->queue->device;
    struct klet_tio_record error = { .status = 2 };
    struct tio_queue *queue = request->queue;
    down_read(&device->dispatch_gate);
    if (READ_ONCE(device->removed))
        klet_tio_request_finish(request, &error);
    else
        device->backend->handle(device->context, request);
    up_read(&device->dispatch_gate);
    // The dispatch reference also pins context/module code through callback return.
    tio_queue_put(queue);
}
static void tio_poll_work(struct work_struct *work)
{
    struct tio_device *device = container_of(work, struct tio_device, poll_work);
    down_read(&device->dispatch_gate);
    if (!device->removed && device->backend->poll) device->backend->poll(device->context);
    up_read(&device->dispatch_gate);
    tio_device_put(device);
}
static void tio_schedule_poll(struct tio_device *device)
{
    if (!device->backend->poll) return;
    refcount_inc(&device->refs);
    if (!queue_work(tio_workers, &device->poll_work)) tio_device_put(device);
}
static void tio_remove_work(struct work_struct *work)
{
    struct tio_device *device = container_of(work, struct tio_device, removal);
    down_write(&device->dispatch_gate);
    if (device->backend->remove)
        device->backend->remove(device->context);
    // The backend's Reset is visible before generic role closure can publish
    // EOF. Revoked capabilities and outstanding accesses remain independently
    // retained throughout this ordering.
    klet_tio_resource_revoke(device->generation);
    klet_tio_resource_notify_all();
    up_write(&device->dispatch_gate);
    tio_device_put(device);
}
bool klet_tio_queue_alive(u64 owner, u64 generation)
{
    struct tio_device *device = NULL;
    unsigned long flags;
    unsigned int i;
    bool alive = false;
    spin_lock_irqsave(&tio_device_table_lock, flags);
    for (i = 0; i < TIO_MAX_OBJECTS; i++)
        if (tio_device_table[i] && tio_device_table[i]->generation == owner &&
            refcount_inc_not_zero(&tio_device_table[i]->refs)) { device = tio_device_table[i]; break; }
    spin_unlock_irqrestore(&tio_device_table_lock, flags);
    if (!device) return false;
    spin_lock_irqsave(&device->lock, flags);
    if (!device->removed)
        for (i = 0; i < KLET_TIO_ATTACHMENTS; i++)
            if (device->queues[i] && device->queues[i]->generation == generation) {alive = true; break;}
    spin_unlock_irqrestore(&device->lock, flags);
    tio_device_put(device);
    return alive;
}
void klet_tio_signal_owner(u64 owner, struct klet_tio_binding binding)
{
    struct tio_device *device = NULL;
    struct klet_inst *inst;
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&tio_device_table_lock, flags);
    for (i = 0; i < TIO_MAX_OBJECTS; i++)
        if (tio_device_table[i] && tio_device_table[i]->generation == owner &&
            refcount_inc_not_zero(&tio_device_table[i]->refs)) { device = tio_device_table[i]; break; }
    spin_unlock_irqrestore(&tio_device_table_lock, flags);
    if (!device) return;
    tio_host_wake_device(device);
    spin_lock_irqsave(&device->lock, flags);
    inst = device->instance;
    if (inst) klet_inst_get(inst);
    spin_unlock_irqrestore(&device->lock, flags);
    if (inst) {
        spin_lock_irqsave(&inst->lock, flags);
        if (!inst->dying && binding.generation && binding.vcpu < inst->nr_vcpus && inst->vcpus[binding.vcpu])
            klet_virq_raise(inst->vcpus[binding.vcpu], KLET_VIRQ_LINES);
        spin_unlock_irqrestore(&inst->lock, flags);
        klet_inst_put(inst);
    }
    tio_device_put(device);
}
static void tio_device_remove(struct tio_device *device)
{
    unsigned long flags;
    struct tio_queue *queues[KLET_TIO_ATTACHMENTS];
    struct klet_tio_record empty = {};
    unsigned int i;
    spin_lock_irqsave(&device->lock, flags);
    if (device->removed) {spin_unlock_irqrestore(&device->lock, flags); return;}
    device->removed = true;
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) {
        queues[i] = device->queues[i];
        if (queues[i]) refcount_inc(&queues[i]->refs);
    }
    spin_unlock_irqrestore(&device->lock, flags);
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) {
        if (!queues[i]) continue;
        spin_lock_irqsave(&queues[i]->lock, flags);
        klet_tio_queue_op(queues[i]->core, 102, &empty);
        tio_capture_irq_locked(queues[i]);
        spin_unlock_irqrestore(&queues[i]->lock, flags);
        tio_signal(queues[i]);
        tio_queue_put(queues[i]);
    }
    tio_host_wake_device(device);
    klet_tio_resource_invalidate(device->generation);
    klet_tio_resource_notify_all();
    refcount_inc(&device->refs);
    queue_work(tio_workers, &device->removal);
}
static void tio_device_detach(struct tio_device *device)
{
    struct klet_inst *instance;
    unsigned int i;
    unsigned long flags;
    struct tio_queue *queues[KLET_TIO_ATTACHMENTS];
    struct klet_tio_record empty = {};
    tio_device_remove(device);
    spin_lock_irqsave(&device->lock, flags);
    instance = device->instance;
    device->instance = NULL;
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) {queues[i] = device->queues[i]; device->queues[i] = NULL;}
    spin_unlock_irqrestore(&device->lock, flags);
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) if (queues[i]) {
        spin_lock_irqsave(&queues[i]->lock, flags);
        queues[i]->closed = true;
        while (!klet_tio_queue_op(queues[i]->core, KLET_TIO_POP, &empty)) {
            unsigned int j;
            for (j = 0; j < KLET_TIO_DEPTH; j++)
                if (queues[i]->requests[j].message.sequence == empty.sequence) {
                    klet_tio_record_drop(&queues[i]->requests[j].reply);
                    queues[i]->requests[j].live = false;
                }
        }
        spin_unlock_irqrestore(&queues[i]->lock, flags);
        tio_queue_put(queues[i]);
    }
    if (instance) klet_inst_put(instance);
}
static int tio_device_file_release(struct inode *inode, struct file *file)
{
    (void)inode;
    // Closing the holder's original provider FD does not remove an attachment.
    tio_device_put(file->private_data);
    return 0;
}
static long tio_provider_ioctl(struct file *file, unsigned int command, unsigned long argument)
{
    struct tio_device *device = file->private_data;
    if (command == KLET_TIO_REMOVE_DEVICE) { tio_device_remove(device); return 0; }
    if (command != KLET_TIO_LISTEN || !device->backend->listen || !(device->descriptor.features & 1)) return -ENOTTY;
    down_read(&device->dispatch_gate);
    long result = READ_ONCE(device->removed) ? -ENODEV : device->backend->listen(device, device->context, (void __user *)argument);
    up_read(&device->dispatch_gate);
    return result;
}
static const struct file_operations tio_device_fops = {
    .owner = THIS_MODULE, .release = tio_device_file_release, .unlocked_ioctl = tio_provider_ioctl, .llseek = noop_llseek,
};
long klet_tio_create_device(struct klet_tio_descriptor __user *argument)
{
    struct klet_tio_descriptor descriptor;
    struct klet_tio_backend *backend;
    struct tio_device *device;
    int result = -ENOENT, fd;
    bool host_pinned = false;
    unsigned int table_slot;
    unsigned long table_flags;
    if (copy_from_user(&descriptor, argument, sizeof(descriptor)))
        return -EFAULT;
    if (descriptor.reserved || descriptor.config_len > KLET_TIO_METADATA)
        return -EINVAL;
    if (atomic_inc_return(&tio_devices) > TIO_MAX_OBJECTS) {
        atomic_dec(&tio_devices);
        return -EDQUOT;
    }
    device = kzalloc(sizeof(*device), GFP_KERNEL_ACCOUNT);
    if (!device) { result = -ENOMEM; goto undo; }
    if (!try_module_get(THIS_MODULE)) { result = -ENODEV; goto free_device; }
    host_pinned = true;
    device->generation = tio_new_generation();
    if (!device->generation) { result = -EOVERFLOW; goto free_device; }
    result = klet_tio_resource_owner_register(device->generation);
    if (result) goto free_device;
    result = klet_tio_resource_owner_register(device->generation | (1ULL << 63));
    if (result) { klet_tio_resource_owner_release(device->generation); goto free_device; }
    klet_tio_resource_owner_host(device->generation | (1ULL << 63));
    if (descriptor.features & 1) klet_tio_resource_owner_host(device->generation);
    result = -ENOENT;
    mutex_lock(&tio_registry_lock);
    list_for_each_entry(backend, &tio_backends, link) {
        if (!tio_schemas_equal(&descriptor, &backend->descriptor))
            continue;
        if (!try_module_get(backend->module))
            continue;
        result = backend->prepare(&descriptor, device->generation, &device->context);
        if (result) { module_put(backend->module); break; }
        device->registration = backend;
        device->backend_copy = *backend;
        device->backend = &device->backend_copy;
        break;
    }
    if (result) {
        mutex_unlock(&tio_registry_lock);
        klet_tio_resource_owner_release(device->generation | (1ULL << 63));
        klet_tio_resource_owner_release(device->generation);
        goto free_device;
    }
    refcount_set(&device->refs, 1);
    spin_lock_init(&device->lock);
    init_rwsem(&device->dispatch_gate);
    init_waitqueue_head(&device->host_wait);
    atomic64_set(&device->host_wait_generation, 1);
    INIT_WORK(&device->removal, tio_remove_work);
    INIT_WORK(&device->poll_work, tio_poll_work);
    INIT_WORK(&device->release_work, tio_device_release_work);
    device->descriptor = descriptor;
    spin_lock_irqsave(&tio_device_table_lock, table_flags);
    for (table_slot = 0; table_slot < TIO_MAX_OBJECTS; table_slot++) if (!tio_device_table[table_slot]) break;
    if (table_slot < TIO_MAX_OBJECTS) tio_device_table[table_slot] = device;
    spin_unlock_irqrestore(&tio_device_table_lock, table_flags);
    mutex_unlock(&tio_registry_lock);
    if (table_slot == TIO_MAX_OBJECTS) {tio_device_put(device); return -EDQUOT;}
    fd = anon_inode_getfd("typedio-device", &tio_device_fops, device, O_CLOEXEC | O_RDWR);
    if (fd < 0) { tio_device_put(device); return fd; }
    return fd;
free_device:
    kfree(device);
    if (host_pinned) module_put(THIS_MODULE);
undo:
    atomic_dec(&tio_devices);
    return result;
}
long klet_tio_attach(struct klet_inst *inst, struct klet_device __user *argument)
{
    struct klet_device input;
    struct file *file;
    struct tio_device *device;
    struct cgroup *group;
    unsigned int slot;
    unsigned long flags;
    int result = -EINVAL;
    if (copy_from_user(&input, argument, sizeof(input)))
        return -EFAULT;
    if (input.type == KLET_DEVICE_BLOCK || input.type == KLET_DEVICE_NET)
        return -EOPNOTSUPP;
    if (input.type != KLET_DEVICE_TYPEDIO || input.flags || input.pad)
        return -EINVAL;
    file = fget(input.fd);
    if (!file)
        return -EBADF;
    if (file->f_op != &tio_device_fops)
        goto out;
    device = file->private_data;
    if (device->descriptor.features & 1) { result = -EPERM; goto out; }
    rcu_read_lock();
    group = task_dfl_cgroup(current);
    result = group == inst->cgrp ? 0 : -EPERM;
    rcu_read_unlock();
    if (result)
        goto out;
    mutex_lock(&inst->mutex);
    spin_lock_irqsave(&inst->lock, flags);
    if (inst->started || inst->dying) { result = -EBUSY; goto unlock_instance; }
    for (slot = 0; slot < KLET_TIO_ATTACHMENTS; slot++)
        if (!inst->typedio_devices[slot])
            break;
    if (slot == KLET_TIO_ATTACHMENTS) { result = -ENOSPC; goto unlock_instance; }
    spin_lock(&device->lock);
    if (device->bound || device->removed)
        result = -EBUSY;
    else {
        device->bound = true;
        device->instance = inst;
        klet_inst_get(inst);
        refcount_inc(&device->refs);
        inst->typedio_devices[slot] = device;
        result = slot;
    }
    spin_unlock(&device->lock);
unlock_instance:
    spin_unlock_irqrestore(&inst->lock, flags);
    mutex_unlock(&inst->mutex);
out:
    fput(file);
    return result;
}
void klet_tio_detach_instance(struct klet_inst *inst)
{
    struct tio_device *devices[KLET_TIO_ATTACHMENTS];
    unsigned long flags;
    unsigned int i;
    spin_lock_irqsave(&inst->lock, flags);
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) {
        devices[i] = inst->typedio_devices[i];
        if (devices[i]) inst->typedio_retired[i] = devices[i]->generation;
        inst->typedio_devices[i] = NULL;
    }
    spin_unlock_irqrestore(&inst->lock, flags);
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++)
        if (devices[i]) { tio_device_detach(devices[i]); tio_device_put(devices[i]); }
}
void klet_tio_resources_evicted(struct klet_inst *inst)
{
    unsigned int i;
    for (i = 0; i < KLET_TIO_ATTACHMENTS; i++)
        if (inst->typedio_retired[i]) klet_tio_resource_evicted(inst->typedio_retired[i]);
}
static struct tio_queue *tio_open(struct tio_device *device, unsigned int depth)
{
    struct tio_queue *queue;
    unsigned int i;
    if (!depth || depth > KLET_TIO_DEPTH)
        return ERR_PTR(-EINVAL);
    if (atomic_inc_return(&tio_queues) > TIO_MAX_OBJECTS) {
        atomic_dec(&tio_queues); return ERR_PTR(-EDQUOT);
    }
    queue = kzalloc(sizeof(*queue), GFP_KERNEL_ACCOUNT);
    if (!queue) goto undo;
    queue->core = kzalloc(klet_tio_queue_size(), GFP_KERNEL_ACCOUNT);
    if (!queue->core) goto free_queue;
    queue->generation = tio_new_generation();
    if (!queue->generation || klet_tio_queue_init(queue->core, queue->generation, depth))
        goto free_core;
    refcount_set(&queue->refs, 1);
    spin_lock_init(&queue->lock);
    queue->device = device;
    refcount_inc(&device->refs);
    for (i = 0; i < KLET_TIO_DEPTH; i++) {
        queue->requests[i].queue = queue;
        INIT_WORK(&queue->requests[i].work, tio_dispatch);
    }
    return queue;
free_core:
    kfree(queue->core);
free_queue:
    kfree(queue);
undo:
    atomic_dec(&tio_queues);
    return ERR_PTR(-ENOMEM);
}
s64 klet_tio_service(struct klet_carrier *carrier, struct klet_tio_call *argument)
{
    struct klet_tio_call call;
    struct tio_device *device;
    struct tio_queue *queue = NULL;
    struct klet_tio_request *request = NULL;
    struct klet_tio_record empty = {};
    unsigned long flags;
    unsigned int i, slot;
    s64 result = -EINVAL;
    // Only trusted vOSTD calls this with a live, stable metadata record.
    // Capture completes before dispatch and no caller pointer is retained.
    memcpy(&call, argument, sizeof(call));
    if (call.flags || call.handle >= KLET_TIO_ATTACHMENTS)
        return -EINVAL;
    spin_lock_irqsave(&carrier->inst->lock, flags);
    device = carrier->inst->typedio_devices[call.handle];
    if (carrier->inst->dying || !device) {
        spin_unlock_irqrestore(&carrier->inst->lock, flags);
        return -ENODEV;
    }
    refcount_inc(&device->refs);
    spin_unlock_irqrestore(&carrier->inst->lock, flags);
    if (call.operation == KLET_TIO_IRQ_POLL || call.operation == KLET_TIO_UNBIND_LINE) {
        result = 1;
        spin_lock_irqsave(&device->lock, flags);
        bool prefer_resources = device->poll_resources_next;
        spin_unlock_irqrestore(&device->lock, flags);
        if (call.operation == KLET_TIO_IRQ_POLL && prefer_resources) {
            struct klet_tio_binding binding;
            if (!klet_tio_resource_poll(device->generation, carrier->vcpu, &binding)) {
                call.record.generation = binding.generation;call.record.sequence = binding.line;call.record.tag = binding.vcpu;
                spin_lock_irqsave(&device->lock, flags);device->poll_resources_next = false;spin_unlock_irqrestore(&device->lock, flags);
                result = 0;goto copy_out;
            }
        }
        spin_lock_irqsave(&device->lock, flags);
        unsigned int start = device->poll_cursor;
        for (i = 0; i < KLET_TIO_ATTACHMENTS; i++) {
            unsigned int index = (start + i) % KLET_TIO_ATTACHMENTS;
            struct tio_queue *source = device->queues[index];
            if (!source) continue;
            spin_lock(&source->lock);
            if (call.operation == KLET_TIO_UNBIND_LINE) {
                if (source->binding.line == call.record.sequence &&
                    source->binding.generation == call.record.generation) {
                    klet_tio_queue_op(source->core, 105, &empty);
                    source->irq_pending = false;
                }
            } else if (source->irq_pending && source->binding.vcpu == carrier->vcpu) {
                source->irq_pending = false;
                device->poll_cursor = (index + 1) % KLET_TIO_ATTACHMENTS;
                device->poll_resources_next = true;
                call.record.generation = source->binding.generation;
                call.record.sequence = source->binding.line;
                call.record.tag = source->binding.vcpu;
                result = 0;
                spin_unlock(&source->lock);
                break;
            }
            spin_unlock(&source->lock);
        }
        spin_unlock_irqrestore(&device->lock, flags);
        if (call.operation == KLET_TIO_IRQ_POLL && result) {
            struct klet_tio_binding binding;
            result = klet_tio_resource_poll(device->generation, carrier->vcpu, &binding);
            if (!result) {call.record.generation = binding.generation; call.record.sequence = binding.line; call.record.tag = binding.vcpu;}
        } else if (call.operation == KLET_TIO_UNBIND_LINE) {
            struct klet_tio_binding binding = {call.record.sequence, 0, call.record.generation};
            klet_tio_resource_unbind(device->generation, binding);
        }
        goto copy_out;
    }
    if (call.operation == KLET_TIO_DISCOVER) {
        if (READ_ONCE(device->removed)) { result = -ENODEV; goto out; }
        call.descriptor = device->descriptor;
        call.generation = device->generation;
        result = 0;
        goto copy_out;
    }
    if (call.generation != device->generation) { result = -ESTALE; goto out; }
    if (call.operation == KLET_TIO_OPEN) {
        if (carrier->excess) { result = -EWOULDBLOCK; goto out; }
        if (!tio_schemas_equal(&call.descriptor, &device->descriptor)) { result = -EPROTO; goto out; }
        queue = tio_open(device, call.arg0);
        if (IS_ERR(queue)) { result = PTR_ERR(queue); queue = NULL; goto out; }
        spin_lock_irqsave(&device->lock, flags);
        for (slot = 0; slot < KLET_TIO_ATTACHMENTS; slot++)
            if (!device->queues[slot]) break;
        if (device->removed || slot == KLET_TIO_ATTACHMENTS) {
            spin_unlock_irqrestore(&device->lock, flags); result = -ENOSPC; goto out;
        }
        device->queues[slot] = queue;
        refcount_inc(&queue->refs);
        spin_unlock_irqrestore(&device->lock, flags);
        call.arg0 = slot;
        call.arg1 = queue->generation;
        result = 0;
        goto copy_out;
    }
    if (call.operation >= 16 && call.operation <= 24) {
        if (call.operation == 21 && call.record.tag >= carrier->inst->nr_vcpus) { result = -EINVAL; goto out; }
        result = klet_tio_resource_service(device->generation, &call);
        klet_tio_resource_notify_all();
        if (call.operation == 18 || call.operation == 19 || call.operation == 20) tio_schedule_poll(device);
        goto copy_out;
    }
    slot = call.arg0;
    if (slot >= KLET_TIO_ATTACHMENTS) goto out;
    spin_lock_irqsave(&device->lock, flags);
    queue = device->queues[slot];
    if (queue) refcount_inc(&queue->refs);
    spin_unlock_irqrestore(&device->lock, flags);
    if (!queue || queue->generation != call.arg1) { result = -ESTALE; goto out; }
    spin_lock_irqsave(&device->lock, flags);
    spin_lock(&queue->lock);
    if (call.operation == KLET_TIO_SUBMIT) {
        if (READ_ONCE(device->removed) || queue->closed) { result = -ENODEV; goto unlock_queue; }
        for (i = 0; i < KLET_TIO_DEPTH; i++)
            if (!queue->requests[i].live) { request = &queue->requests[i]; break; }
        if (!request) { result = -EAGAIN; goto unlock_queue; }
        struct klet_tio_transfer transfer = {};
        u64 host_owner = device->generation | (1ULL << 63);
        result = klet_tio_record_prepare(&transfer, &call.record, device->generation, host_owner, false);
        if (result) { klet_tio_transfer_rollback(&transfer); request = NULL; goto unlock_queue; }
        result = klet_tio_queue_op(queue->core, KLET_TIO_SUBMIT, &call.record);
        if (result) { klet_tio_transfer_rollback(&transfer); request = NULL; goto unlock_queue; }
        result = klet_tio_transfer_commit(&transfer);
        // Defensive rollback also covers an unexpected late revocation. No
        // worker or frontend has observed this provisional queue admission.
        if (result) {
            klet_tio_transfer_rollback(&transfer);
            WARN_ON(klet_tio_queue_op(queue->core, 107, &call.record));
            request = NULL;
            goto unlock_queue;
        }
        for (i = 0; i < call.record.resource_count; i++) call.record.resources[i].owner = host_owner;
        request->message = call.record;
        klet_tio_queue_op(queue->core, 100, &request->message);
        for (i = 0; i < request->message.resource_count; i++) request->message.resources[i].owner = host_owner;
        request->reply = empty;
        request->accesses = 0;
        request->finish_requested = false;
        request->published = false;
        request->live = true;
        refcount_add(2, &queue->refs); // backend authority and worker execution
    } else if (call.operation == KLET_TIO_BIND) {
        if (call.record.tag >= carrier->inst->nr_vcpus || !call.record.generation || call.record.sequence >= 64)
            result = -EINVAL;
        else {
            result = klet_tio_queue_op(queue->core, KLET_TIO_BIND, &call.record);
            queue->binding.line = call.record.sequence;
            queue->binding.vcpu = call.record.tag;
            queue->binding.generation = call.record.generation;
            queue->irq_pending = false;
        }
    } else if (call.operation == KLET_TIO_CLOSE) {
        queue->closed = true;
        result = klet_tio_queue_op(queue->core, 102, &empty);
        while (!klet_tio_queue_op(queue->core, KLET_TIO_POP, &empty)) {
            for (i = 0; i < KLET_TIO_DEPTH; i++)
                if (queue->requests[i].message.sequence == empty.sequence)
                    { klet_tio_record_drop(&queue->requests[i].reply); queue->requests[i].live = false; }
        }
        device->queues[slot] = NULL;
    } else if (call.operation == KLET_TIO_POP || call.operation == KLET_TIO_CANCEL ||
               call.operation == KLET_TIO_EVENTS || call.operation == KLET_TIO_REARM) {
        result = klet_tio_queue_op(queue->core, call.operation, &call.record);
        if (!result && call.operation == KLET_TIO_POP)
            for (i = 0; i < KLET_TIO_DEPTH; i++)
                if (queue->requests[i].message.sequence == call.record.sequence)
                    { queue->requests[i].reply.resource_count = 0; queue->requests[i].live = false; }
    } else result = -EINVAL;
unlock_queue:
    tio_capture_irq_locked(queue);
    spin_unlock(&queue->lock);
    spin_unlock_irqrestore(&device->lock, flags);
    if (call.operation == KLET_TIO_CLOSE && !result)
        tio_queue_put(queue); // frontend table reference
    if (!result && call.operation != KLET_TIO_CLOSE)
        tio_signal(queue);
    if (request && !result)
        queue_work(tio_workers, &request->work);
copy_out:
    if (call.operation == KLET_TIO_CANCEL || call.operation == KLET_TIO_CLOSE) tio_schedule_poll(device);
    memcpy(argument, &call, sizeof(call));
out:
    if (queue) tio_queue_put(queue);
    tio_device_put(device);
    return result;
}
int klet_tio_host_init(void)
{
    tio_workers = alloc_workqueue("typedio", WQ_UNBOUND | WQ_MEM_RECLAIM, 8);
    return tio_workers ? 0 : -ENOMEM;
}
void klet_tio_host_exit(void)
{
    destroy_workqueue(tio_workers);
}
