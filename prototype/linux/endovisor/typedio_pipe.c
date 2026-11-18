// SPDX-License-Identifier: GPL-2.0
// Host-owned, zeroed pipe backing and independently retained reservations.
// C supplies the allocation/locking substrate; the same safe Rust metadata
// implementation used by tests decides every reserve/commit/close operation.

#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/err.h>
#include <linux/mm.h>
#include <linux/module.h>
#include <linux/refcount.h>
#include <linux/slab.h>
#include <linux/spinlock.h>
#include <linux/uaccess.h>

#endif
#include "typedio_pipe.h"

#define KLET_TIO_MAX_PIPE_CAPACITY (4ULL << 20)
#define KLET_TIO_PIPE_BYTES_LIMIT (64ULL << 20)
#define KLET_TIO_PIPE_COUNT_LIMIT 1024
static DEFINE_SPINLOCK(klet_tio_pipe_quota_lock);
static u64 klet_tio_pipe_bytes;
static u32 klet_tio_pipe_count;
static u32 klet_tio_pipe_accounts;

struct klet_tio_pipe_account {
	refcount_t refs;
	u64 owner;
	u64 byte_limit;
	u64 bytes;
	u32 pipe_limit;
	u32 pipes;
};

struct klet_tio_pipe {
	refcount_t refs;
	spinlock_t lock;
	struct klet_tio_pipe_account *account;
	struct page *pages;
	unsigned int order;
	u64 charged;
	void *core;
};

extern size_t klet_tio_pipe_core_size(void);
extern void klet_tio_pipe_core_destroy(void *core);
extern int klet_tio_pipe_core_init(void *core, size_t capacity, u64 producer, u64 consumer);
extern int klet_tio_pipe_core_reserve(void *core, u64 owner, u32 role, size_t limit,
				     struct klet_tio_reservation *out);
extern int klet_tio_pipe_core_finalize(void *core, u64 owner, u32 role, u64 token,
				      size_t bytes, u32 operation);
extern int klet_tio_pipe_core_access(void *core, u64 owner, u32 role, u64 token,
                                    struct klet_tio_reservation *out,
                                    size_t bytes, bool ending);
extern bool klet_tio_pipe_core_reclaimable(void *core);
extern void klet_tio_pipe_core_close(void *core, u32 operation);

struct klet_tio_pipe_account *klet_tio_pipe_account_create(u64 owner,
						       u64 byte_limit, u32 pipe_limit)
{
	struct klet_tio_pipe_account *account;
	unsigned long flags;

	if (!owner || byte_limit < sizeof(*account) || !pipe_limit ||
	    byte_limit > KLET_TIO_PIPE_BYTES_LIMIT || pipe_limit > KLET_TIO_PIPE_COUNT_LIMIT)
		return ERR_PTR(-EINVAL);
	account = kzalloc(sizeof(*account), GFP_KERNEL_ACCOUNT);
	if (!account)
		return ERR_PTR(-ENOMEM);
	if (!try_module_get(THIS_MODULE)) {
		kfree(account);
		return ERR_PTR(-ENODEV);
	}
	spin_lock_irqsave(&klet_tio_pipe_quota_lock, flags);
	if (klet_tio_pipe_accounts == KLET_TIO_PIPE_COUNT_LIMIT ||
	    klet_tio_pipe_bytes > KLET_TIO_PIPE_BYTES_LIMIT - sizeof(*account)) {
		spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);
		kfree(account);
		module_put(THIS_MODULE);
		return ERR_PTR(-EDQUOT);
	}
	klet_tio_pipe_accounts++;
	klet_tio_pipe_bytes += sizeof(*account);
	spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);
	account->bytes = sizeof(*account);
	refcount_set(&account->refs, 1);
	account->owner = owner;
	account->byte_limit = byte_limit;
	account->pipe_limit = pipe_limit;
	return account;
}

void klet_tio_pipe_account_put(struct klet_tio_pipe_account *account)
{
	unsigned long flags;

	if (!refcount_dec_and_test(&account->refs))
		return;
	spin_lock_irqsave(&klet_tio_pipe_quota_lock, flags);
	klet_tio_pipe_accounts--;
	klet_tio_pipe_bytes -= sizeof(*account);
	spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);
	kfree(account);
	module_put(THIS_MODULE);
}

static void klet_tio_pipe_uncharge(struct klet_tio_pipe_account *account, u64 bytes)
{
	unsigned long flags;

	spin_lock_irqsave(&klet_tio_pipe_quota_lock, flags);
	account->bytes -= bytes;
	account->pipes--;
	klet_tio_pipe_bytes -= bytes;
	klet_tio_pipe_count--;
	spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);
}

struct klet_tio_pipe *klet_tio_pipe_create(struct klet_tio_pipe_account *account,
					 u64 producer, u64 consumer, u64 capacity)
{
	struct klet_tio_pipe *pipe;
	unsigned int order;
	unsigned long flags;
	u64 charged;

	if (!producer || !consumer || !capacity || capacity > KLET_TIO_MAX_PIPE_CAPACITY)
		return ERR_PTR(-EINVAL);
	order = get_order(capacity);
	charged = ((u64)PAGE_SIZE << order) + sizeof(*pipe) + klet_tio_pipe_core_size();
	spin_lock_irqsave(&klet_tio_pipe_quota_lock, flags);
	if (charged > account->byte_limit || account->bytes > account->byte_limit - charged ||
	    account->pipes == account->pipe_limit || charged > KLET_TIO_PIPE_BYTES_LIMIT ||
	    klet_tio_pipe_bytes > KLET_TIO_PIPE_BYTES_LIMIT - charged ||
	    klet_tio_pipe_count == KLET_TIO_PIPE_COUNT_LIMIT) {
		spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);
		return ERR_PTR(-EDQUOT);
	}
	account->bytes += charged;
	account->pipes++;
	klet_tio_pipe_bytes += charged;
	klet_tio_pipe_count++;
	spin_unlock_irqrestore(&klet_tio_pipe_quota_lock, flags);

	if (!try_module_get(THIS_MODULE))
		goto undo_charge;
	pipe = kzalloc(sizeof(*pipe), GFP_KERNEL_ACCOUNT);
	if (!pipe)
		goto undo_module;
	pipe->core = kzalloc(klet_tio_pipe_core_size(), GFP_KERNEL_ACCOUNT);
	if (!pipe->core)
		goto free_pipe;
	pipe->pages = alloc_pages(GFP_KERNEL_ACCOUNT | __GFP_ZERO | __GFP_NOWARN, order);
	if (!pipe->pages)
		goto free_core;
	if (klet_tio_pipe_core_init(pipe->core, capacity, producer, consumer))
		goto free_pages;
	refcount_set(&pipe->refs, 1);
	spin_lock_init(&pipe->lock);
	refcount_inc(&account->refs);
	pipe->account = account;
	pipe->order = order;
	pipe->charged = charged;
	return pipe;
free_pages:
	__free_pages(pipe->pages, order);
free_core:
	kfree(pipe->core);
free_pipe:
	kfree(pipe);
undo_module:
	module_put(THIS_MODULE);
undo_charge:
	klet_tio_pipe_uncharge(account, charged);
	return ERR_PTR(-ENOMEM);
}

void klet_tio_pipe_get(struct klet_tio_pipe *pipe)
{
	refcount_inc(&pipe->refs);
}

void klet_tio_pipe_put(struct klet_tio_pipe *pipe)
{
	if (!refcount_dec_and_test(&pipe->refs))
		return;
	klet_tio_pipe_core_destroy(pipe->core);
	__free_pages(pipe->pages, pipe->order);
	kfree(pipe->core);
	klet_tio_pipe_uncharge(pipe->account, pipe->charged);
	klet_tio_pipe_account_put(pipe->account);
	kfree(pipe);
	module_put(THIS_MODULE);
}

int klet_tio_pipe_reserve(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			 u64 limit, struct klet_tio_reservation *reservation)
{
	unsigned long flags;
	int result;

	spin_lock_irqsave(&pipe->lock, flags);
	result = klet_tio_pipe_core_reserve(pipe->core, owner, role, limit, reservation);
	if (!result)
		klet_tio_pipe_get(pipe);
	spin_unlock_irqrestore(&pipe->lock, flags);
	return result;
}

static int klet_tio_pipe_finalize(struct klet_tio_pipe *pipe, u64 owner, u32 role,
				  u64 token, u64 bytes, u32 operation)
{
	unsigned long flags;
	int result;

	spin_lock_irqsave(&pipe->lock, flags);
	result = klet_tio_pipe_core_finalize(pipe->core, owner, role, token, bytes, operation);
	spin_unlock_irqrestore(&pipe->lock, flags);
	// Close/reset rejects publication but still finalizes a valid reservation.
	if (operation && (!result || result == -EPIPE || result == -ECONNRESET))
		klet_tio_pipe_put(pipe);
	return result;
}

int klet_tio_pipe_progress(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			  u64 token, u64 bytes)
{
	return klet_tio_pipe_finalize(pipe, owner, role, token, bytes, 0);
}

int klet_tio_pipe_commit(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			u64 token, u64 bytes)
{
	return klet_tio_pipe_finalize(pipe, owner, role, token, bytes, 1);
}

int klet_tio_pipe_cancel(struct klet_tio_pipe *pipe, u64 owner, u32 role, u64 token)
{
	return klet_tio_pipe_finalize(pipe, owner, role, token, 0, 2);
}

void klet_tio_pipe_close(struct klet_tio_pipe *pipe, u32 operation)
{
	unsigned long flags;

	spin_lock_irqsave(&pipe->lock, flags);
	klet_tio_pipe_core_close(pipe->core, operation);
	spin_unlock_irqrestore(&pipe->lock, flags);
}

// No pipe metadata lock spans a user fault. A separate operation reference and
// Rust's active-access state prevent concurrent cleanup from freeing backing.
long klet_tio_pipe_copy_user(struct klet_tio_pipe *pipe, u64 owner, u32 role,
                            u64 token, void __user *buffer, u64 count, int *fault)
{
	struct klet_tio_reservation reservation;
	unsigned long flags, not_copied;
	u64 bytes, copied;
	void *address;
	int result;

	*fault = 0;
	spin_lock_irqsave(&pipe->lock, flags);
	result = klet_tio_pipe_core_access(pipe->core, owner, role, token,
                                      &reservation, 0, false);
	if (!result)
		klet_tio_pipe_get(pipe);
	spin_unlock_irqrestore(&pipe->lock, flags);
	if (result)
		return result;

	bytes = min(count, reservation.length - reservation.progress);
	address = page_address(pipe->pages) + reservation.offset + reservation.progress;
	if (!role)
		not_copied = copy_from_user(address, buffer, bytes);
	else
		not_copied = copy_to_user(buffer, address, bytes);
	copied = bytes - not_copied;
	if (not_copied)
		*fault = -EFAULT;

	spin_lock_irqsave(&pipe->lock, flags);
	result = klet_tio_pipe_core_access(pipe->core, owner, role, token,
                                      NULL, copied, true);
	spin_unlock_irqrestore(&pipe->lock, flags);
	klet_tio_pipe_put(pipe);
	if (result)
		return result;
	return copied;
}

extern int klet_tio_pipe_core_transfer(void *core, u64 owner, u32 role, u64 target, bool commit);
void *klet_tio_pipe_address(struct klet_tio_pipe *pipe, u64 offset, u64 length)
{
    u64 extent = (u64)PAGE_SIZE << pipe->order;
    if (length > extent || offset > extent - length)
        return NULL;
    return page_address(pipe->pages) + offset;
}
static int tio_pipe_transfer(struct klet_tio_pipe *pipe, u64 owner, u32 role, u64 target, bool commit)
{
    unsigned long flags;
    int result;
    spin_lock_irqsave(&pipe->lock, flags);
    result = klet_tio_pipe_core_transfer(pipe->core, owner, role, target, commit);
    spin_unlock_irqrestore(&pipe->lock, flags);
    return result;
}
int klet_tio_pipe_can_transfer(struct klet_tio_pipe *pipe, u64 owner, u32 role)
{
    return tio_pipe_transfer(pipe, owner, role, owner, false);
}
int klet_tio_pipe_transfer(struct klet_tio_pipe *pipe, u64 owner, u32 role, u64 target)
{
    return tio_pipe_transfer(pipe, owner, role, target, true);
}

extern int klet_tio_pipe_core_notify(void *, u64, u32, u32, u32, struct klet_tio_binding *);
int klet_tio_pipe_notify(struct klet_tio_pipe *pipe, u64 owner, u32 role, u32 operation,
                         u32 value, struct klet_tio_binding *binding)
{
    unsigned long flags;
    int result;
    spin_lock_irqsave(&pipe->lock, flags);
    result = klet_tio_pipe_core_notify(pipe->core, owner, role, operation, value, binding);
    spin_unlock_irqrestore(&pipe->lock, flags);
    return result;
}

u64 klet_tio_pipe_account_budget(u64 capacity, u32 pipes)
{
    if (!capacity || capacity > KLET_TIO_MAX_PIPE_CAPACITY || pipes > KLET_TIO_PIPE_COUNT_LIMIT) return 0;
    return sizeof(struct klet_tio_pipe_account) + pipes *
        (((u64)PAGE_SIZE << get_order(capacity)) + sizeof(struct klet_tio_pipe) + klet_tio_pipe_core_size());
}

bool klet_tio_pipe_quiescent(struct klet_tio_pipe *pipe)
{
    unsigned long flags;
    bool result;
    spin_lock_irqsave(&pipe->lock, flags);
    result = klet_tio_pipe_core_reclaimable(pipe->core);
    spin_unlock_irqrestore(&pipe->lock, flags);
    return result;
}
