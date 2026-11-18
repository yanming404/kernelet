// SPDX-License-Identifier: GPL-2.0
// Kernel registration and sleepable serialization for the safe Rust backend.
#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/mutex.h>
#include <linux/errno.h>
#endif
#include "typedio_host.h"
#include "typedio_socket.h"
static DEFINE_MUTEX(vsock_policy_lock);
void klet_tio_backend_lock(void) { mutex_lock(&vsock_policy_lock); }
void klet_tio_backend_unlock(void) { mutex_unlock(&vsock_policy_lock); }
extern void klet_tio_vsock_descriptor(struct klet_tio_descriptor *);
extern int klet_tio_vsock_prepare(const struct klet_tio_descriptor *, u64);
extern void klet_tio_vsock_handle(void *);
extern void klet_tio_vsock_remove(unsigned long);
extern void klet_tio_vsock_release(unsigned long);
extern void klet_tio_vsock_poll(void);
static int vsock_prepare(const struct klet_tio_descriptor *descriptor, u64 generation, void **context)
{
    int index = klet_tio_vsock_prepare(descriptor, generation);
    if (index < 0) return index;
    *context = (void *)(unsigned long)index;
    return 0;
}
static void vsock_handle(void *context, struct klet_tio_request *request)
{ (void)context; klet_tio_vsock_handle(request); }
static void vsock_remove(void *context) { klet_tio_vsock_remove((unsigned long)context); }
static void vsock_release(void *context) { klet_tio_vsock_release((unsigned long)context); }
static void vsock_poll(void *context) { (void)context; klet_tio_vsock_poll(); }
static struct klet_tio_backend vsock_backend = {
    .module = THIS_MODULE, .prepare = vsock_prepare, .handle = vsock_handle,
    .remove = vsock_remove, .release = vsock_release, .poll = vsock_poll, .listen = klet_tio_socket_listen,
};
int klet_tio_vsock_register(void)
{
    klet_tio_vsock_descriptor(&vsock_backend.descriptor);
    vsock_backend.descriptor.features = 1;
    return klet_tio_backend_register(&vsock_backend);
}
void klet_tio_vsock_unregister(void) { klet_tio_backend_unregister(&vsock_backend); }
