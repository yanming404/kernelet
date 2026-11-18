// SPDX-License-Identifier: GPL-2.0
// Retain a whole grant independently of the instance and its private heap.
// A host lease contains only this host-owned record, never a guest Arc.

#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/mm.h>
#include <linux/module.h>
#include <linux/slab.h>

#include "kernelet.h"
#endif

// Bound allocations even when repeated instance deaths leave leases alive.
// Account the full contiguous allocation, not a request's selected subrange.
#define KLET_GRANT_BYTES_LIMIT (256ULL << 20)
static atomic64_t klet_grant_bytes = ATOMIC64_INIT(0);

struct klet_grant_backing {
	refcount_t refs;
	struct page *pages;
	unsigned int order;
	u64 bytes;
};

struct klet_grant_backing *klet_grant_alloc(unsigned int order)
{
	struct klet_grant_backing *backing;
	u64 bytes;
	s64 used;

	if (order >= MAX_PAGE_ORDER + 1)
		return NULL;
	bytes = (u64)PAGE_SIZE << order;
	used = atomic64_read(&klet_grant_bytes);
	for (;;) {
		if (bytes > KLET_GRANT_BYTES_LIMIT ||
		    (u64)used > KLET_GRANT_BYTES_LIMIT - bytes)
			return NULL;
		if (atomic64_try_cmpxchg(&klet_grant_bytes, &used, used + bytes))
			break;
	}
	backing = kzalloc(sizeof(*backing), GFP_KERNEL_ACCOUNT);
	if (!backing)
		goto undo_charge;
	backing->pages = alloc_pages(GFP_KERNEL_ACCOUNT | __GFP_ZERO |
				    __GFP_NOWARN, order);
	if (!backing->pages) {
		kfree(backing);
		goto undo_charge;
	}
	backing->order = order;
	backing->bytes = bytes;
	refcount_set(&backing->refs, 1);
	return backing;
undo_charge:
	atomic64_sub(bytes, &klet_grant_bytes);
	return NULL;
}

struct page *klet_grant_pages(struct klet_grant_backing *backing)
{
	return backing->pages;
}

// Caller holds inst->lock, shared with the instance-death admission decision.
// The returned record must stay owned until all registered accesses end.
struct klet_grant_backing *klet_grant_lease_get(struct klet_inst *inst,
					     u64 paddr, u64 len)
{
	struct klet_grant_backing *backing = inst->grant_backing;

	lockdep_assert_held(&inst->lock);
	if (inst->dying || !backing || !len || !klet_in_grant(inst, paddr, len))
		return NULL;
	if (!try_module_get(THIS_MODULE))
		return NULL;
	refcount_inc(&backing->refs);
	return backing;
}

// This put must run only after every byte access and asynchronous DMA has ended.
// It is also used to release the instance's initial reference after eviction
// and tenant mapping teardown; that reference does not carry a module pin.
void klet_grant_put(struct klet_grant_backing *backing)
{
	if (backing && refcount_dec_and_test(&backing->refs)) {
		__free_pages(backing->pages, backing->order);
		atomic64_sub(backing->bytes, &klet_grant_bytes);
		kfree(backing);
	}
}

void klet_grant_lease_put(struct klet_grant_backing *backing)
{
	if (!backing)
		return;
	klet_grant_put(backing);
	module_put(THIS_MODULE);
}
