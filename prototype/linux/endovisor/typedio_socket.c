// SPDX-License-Identifier: GPL-2.0
// Linux File adapter. Routing, handshake, backlog and pipe allocation remain
// in the shared safe Rust Vsock backend. No user copy holds its policy lock.
#ifdef TYPEDIO_NATIVE_TEST
#include "../../tests/host-harness/shim.h"
#else
#include <linux/anon_inodes.h>
#include <linux/capability.h>
#include <linux/err.h>
#include <linux/file.h>
#include <linux/fs.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/poll.h>
#include <linux/slab.h>
#include <linux/uaccess.h>
#include <linux/wait.h>
#endif
#include "typedio_host.h"
#include "typedio_resources.h"
#include "typedio_socket.h"

struct tio_socket {
    void *device;
    u64 listener;
    struct klet_tio_record connection;
    struct mutex read_lock, write_lock;
};
static atomic_t socket_count = ATOMIC_INIT(0);
extern s64 klet_tio_vsock_host_listen(unsigned long, u32, u32);
extern int klet_tio_vsock_host_ready(u64);
extern int klet_tio_vsock_host_accept(u64, struct klet_tio_record *);
extern void klet_tio_vsock_host_close(u64);
extern void klet_tio_vsock_host_end(const struct klet_tio_record *, bool);

static struct tio_socket *socket_alloc(void *device)
{
    struct tio_socket *socket;
    if (atomic_inc_return(&socket_count) > 128) {atomic_dec(&socket_count); return NULL;}
    socket = kzalloc(sizeof(*socket), GFP_KERNEL_ACCOUNT);
    if (!socket) {atomic_dec(&socket_count); return NULL;}
    klet_tio_host_get(device);
    socket->device = device;
    mutex_init(&socket->read_lock);
    mutex_init(&socket->write_lock);
    return socket;
}
static void socket_free(struct tio_socket *socket, bool abort)
{
    if (socket->listener) klet_tio_vsock_host_close(socket->listener);
    else if (socket->connection.resource_count) {
        klet_tio_vsock_host_end(&socket->connection, abort);
        klet_tio_record_drop(&socket->connection);
        klet_tio_resource_notify_all();
    }
    klet_tio_host_put(socket->device);
    kfree(socket);
    atomic_dec(&socket_count);
}
static int socket_release(struct inode *inode, struct file *file)
{
    (void)inode;
    socket_free(file->private_data, false);
    return 0;
}
static int endpoint_op(struct tio_socket *socket, unsigned int role, u32 operation,
                        u64 token, u32 bytes, struct klet_tio_call *output)
{
    struct klet_tio_resource endpoint = socket->connection.resources[role];
    struct klet_tio_call call = { .operation = operation, .arg0 = endpoint.slot, .arg1 = endpoint.generation };
    int result;
    call.record.generation = token;
    call.record.len = bytes;
    call.record.status = role;
    if (operation == 21) {call.record.generation = endpoint.owner; call.record.sequence = 0;}
    if (operation == 22) call.record.status = 1;
    result = klet_tio_resource_service(endpoint.owner, &call);
    klet_tio_resource_notify_all();
    if (output) *output = call;
    return result;
}
static void endpoint_rearm(struct tio_socket *socket, unsigned int role)
{
    endpoint_op(socket, role, 23, 0, 0, NULL);
    endpoint_op(socket, role, 24, 0, 0, NULL);
}
static ssize_t socket_copy(struct file *file, char __user *buffer, size_t count, unsigned int role)
{
    struct tio_socket *socket = file->private_data;
    struct mutex *lock = role ? &socket->read_lock : &socket->write_lock;
    struct klet_tio_call reservation;
    struct klet_tio_resource endpoint;
    struct klet_tio_endpoint key;
    u64 generation;
    long copied, result;
    int fault = 0;
    if (socket->listener) return -ENOTCONN;
    if (!count) return 0;
    if (mutex_lock_interruptible(lock)) return -ERESTARTSYS;
    for (;;) {
        generation = klet_tio_host_wait_generation(socket->device);
        if (generation == S64_MAX) {result = -EOVERFLOW; goto unlock;}
        endpoint_op(socket, role, 23, 0, 0, NULL);
        result = endpoint_op(socket, role, 16, 0, min_t(size_t, count, U32_MAX), &reservation);
        if (result != -EAGAIN) break;
        if (file->f_flags & O_NONBLOCK) goto unlock;
        if (endpoint_op(socket, role, 24, 0, 0, NULL) == 0) continue;
        result = wait_event_interruptible(*klet_tio_host_wait_queue(socket->device),
                    klet_tio_host_wait_generation(socket->device) != generation);
        if (result) goto unlock;
    }
    if (result == 1) {result = 0; goto unlock;}
    if (result) goto unlock;
    endpoint = socket->connection.resources[role];
    key = (struct klet_tio_endpoint){endpoint.slot, endpoint.generation};
    copied = klet_tio_endpoint_copy_user(key, endpoint.owner, reservation.record.generation,
                                         buffer, reservation.record.len, &fault);
    if (copied < 0) {
        endpoint_op(socket, role, 19, reservation.record.generation, 0, NULL);
        result = copied; goto unlock;
    }
    result = endpoint_op(socket, role, 18, reservation.record.generation, copied, NULL);
    if (result) {
        endpoint_op(socket, role, 19, reservation.record.generation, 0, NULL);
        if (role && copied) result = copied;
    } else result = copied ? copied : fault;
    endpoint_rearm(socket, role);
unlock:
    mutex_unlock(lock);
    return result;
}
static ssize_t socket_read(struct file *file, char __user *buffer, size_t count, loff_t *position)
{ (void)position; return socket_copy(file, buffer, count, 1); }
static ssize_t socket_write(struct file *file, const char __user *buffer, size_t count, loff_t *position)
{ (void)position; return socket_copy(file, (char __user *)buffer, count, 0); }
static __poll_t socket_poll(struct file *file, poll_table *wait)
{
    struct tio_socket *socket = file->private_data;
    int read, write;
    __poll_t events = 0;
    poll_wait(file, klet_tio_host_wait_queue(socket->device), wait);
    if (klet_tio_host_wait_generation(socket->device) == S64_MAX) return EPOLLERR;
    if (socket->listener) {
        int ready = klet_tio_vsock_host_ready(socket->listener);
        return ready < 0 ? EPOLLERR | EPOLLHUP : ready ? EPOLLIN | EPOLLRDNORM : 0;
    }
retry:
    endpoint_op(socket, 0, 23, 0, 0, NULL);
    endpoint_op(socket, 1, 23, 0, 0, NULL);
    read = endpoint_op(socket, 1, 26, 0, 0, NULL);
    write = endpoint_op(socket, 0, 26, 0, 0, NULL);
    if (read < 0 || write < 0 || (read | write) & 4) return EPOLLERR | EPOLLHUP;
    if (read & 1) events |= EPOLLIN | EPOLLRDNORM;
    if (read & 2) events |= EPOLLIN | EPOLLRDNORM | EPOLLRDHUP;
    if (write & 1) events |= EPOLLOUT | EPOLLWRNORM;
    if (write & 2) events |= EPOLLERR | EPOLLHUP;
    int producer_armed = endpoint_op(socket, 0, 24, 0, 0, NULL);
    int consumer_armed = endpoint_op(socket, 1, 24, 0, 0, NULL);
    if (!producer_armed || !consumer_armed) goto retry;
    return events;
}
static const struct file_operations socket_ops;
static long socket_ioctl(struct file *file, unsigned int command, unsigned long argument)
{
    struct tio_socket *listener = file->private_data, *socket;
    u64 generation;
    int result, fd;
    if (command != KLET_TIO_ACCEPT || !listener->listener) return -ENOTTY;
    if (argument & ~(O_CLOEXEC | O_NONBLOCK)) return -EINVAL;
    socket = socket_alloc(listener->device);
    if (!socket) return -ENOMEM;
    for (;;) {
        generation = klet_tio_host_wait_generation(listener->device);
        if (generation == S64_MAX) {result = -EOVERFLOW; break;}
        result = klet_tio_vsock_host_accept(listener->listener, &socket->connection);
        if (result != -EAGAIN) break;
        if (file->f_flags & O_NONBLOCK) break;
        result = wait_event_interruptible(*klet_tio_host_wait_queue(listener->device),
                    klet_tio_host_wait_generation(listener->device) != generation);
        if (result) break;
    }
    if (result) {socket_free(socket, false); return result;}
    // Install generic endpoint interest before exposing the descriptor. No
    // IRQ callback or user virtual address is kept in these records.
    for (unsigned int role = 0; role < 2; role++) {
        result = endpoint_op(socket, role, 21, 0, 0, NULL);
        if (!result) result = endpoint_op(socket, role, 22, 0, 0, NULL);
        if (result) {socket_free(socket, true); return result;}
        endpoint_rearm(socket, role);
    }
    fd = anon_inode_getfd("typedio-vsock", &socket_ops, socket, O_RDWR | argument);
    if (fd < 0) socket_free(socket, true);
    return fd;
}
static const struct file_operations socket_ops = {
    .owner = THIS_MODULE, .release = socket_release, .read = socket_read,
    .write = socket_write, .poll = socket_poll, .unlocked_ioctl = socket_ioctl,

};
long klet_tio_socket_listen(void *device, void *context, void __user *argument)
{
    struct klet_tio_listen_args args;
    struct tio_socket *listener;
    s64 id;
    int fd;
    if (copy_from_user(&args, argument, sizeof(args))) return -EFAULT;
    if (args.reserved || args.flags & ~(O_CLOEXEC | O_NONBLOCK)) return -EINVAL;
    if (args.port < 1024 && !capable(CAP_NET_BIND_SERVICE)) return -EACCES;
    listener = socket_alloc(device);
    if (!listener) return -ENOMEM;
    id = klet_tio_vsock_host_listen((unsigned long)context, args.port, args.backlog);
    if (id < 0) {socket_free(listener, false); return id;}
    listener->listener = id;
    fd = anon_inode_getfd("typedio-vsock-listener", &socket_ops, listener, O_RDWR | args.flags);
    if (fd < 0) socket_free(listener, true);
    return fd;
}
