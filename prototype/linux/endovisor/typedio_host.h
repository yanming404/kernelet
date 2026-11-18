// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_HOST_H
#define _KLET_TYPEDIO_HOST_H
#ifndef TYPEDIO_NATIVE_TEST
#include <linux/list.h>
#include <linux/module.h>
#endif
#include "typedio_service.h"
struct klet_inst;
struct klet_device;
void klet_tio_backend_lock(void);
void klet_tio_backend_unlock(void);
struct klet_carrier;
struct klet_tio_request;
struct klet_tio_backend {
    struct list_head link;
    struct module *module;
    struct klet_tio_descriptor descriptor;
    int (*prepare)(const struct klet_tio_descriptor *, u64, void **);
    void (*handle)(void *, struct klet_tio_request *);
    void (*remove)(void *);
    void (*poll)(void *);
    long (*listen)(void *, void *, void __user *);
    void (*release)(void *);
};
struct wait_queue_head;
void klet_tio_host_get(void *);
void klet_tio_host_put(void *);
struct wait_queue_head *klet_tio_host_wait_queue(void *);
u64 klet_tio_host_wait_generation(void *);
void klet_tio_host_wake(u64 owner);
int klet_tio_backend_register(struct klet_tio_backend *backend);
int klet_tio_backend_unregister(struct klet_tio_backend *backend);
long klet_tio_create_device(struct klet_tio_descriptor __user *argument);
long klet_tio_attach(struct klet_inst *inst, struct klet_device __user *argument);
void klet_tio_resources_evicted(struct klet_inst *inst);
void klet_tio_detach_instance(struct klet_inst *inst);
s64 klet_tio_service(struct klet_carrier *carrier, struct klet_tio_call *argument);
int klet_tio_vsock_register(void);
void klet_tio_vsock_unregister(void);
int klet_tio_host_init(void);
void klet_tio_host_exit(void);
bool klet_tio_request_cancelled(struct klet_tio_request *request);
const struct klet_tio_record *klet_tio_request_message(struct klet_tio_request *request);
bool klet_tio_queue_alive(u64 owner, u64 generation);
u64 klet_tio_request_queue_generation(struct klet_tio_request *request);
u64 klet_tio_request_host_owner(struct klet_tio_request *request);
u64 klet_tio_request_caller(struct klet_tio_request *request);
void klet_tio_request_finish(struct klet_tio_request *request, const struct klet_tio_record *reply);
int klet_tio_request_begin_access(struct klet_tio_request *request);
void klet_tio_request_end_access(struct klet_tio_request *request);
#endif
