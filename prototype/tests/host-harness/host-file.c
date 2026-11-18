// SPDX-License-Identifier: GPL-2.0
#include "shim.h"
#include "../../linux/endovisor/typedio_host.h"
extern void native_vsock_config(struct klet_tio_descriptor *,u64,u64);
extern void native_vsock_host_roundtrip(int);
#ifdef ASTERINAS_DRIVER_TEST
extern void native_asterinas_roundtrip(int);
#endif
static struct klet_carrier carrier;
static pthread_rwlock_t fixture_gate=PTHREAD_RWLOCK_INITIALIZER;
static bool fixture_stopped;
void native_select_guest(u32 index){assert(index==0);}
s64 native_guest_service(struct klet_tio_call *call){pthread_rwlock_rdlock(&fixture_gate);s64 result=fixture_stopped?-ENODEV:klet_tio_service(&carrier,call);pthread_rwlock_unlock(&fixture_gate);return result;}
int main(void){
 struct cgroup owner={1};native_task.group=&owner;
 struct klet_inst inst={.mutex=PTHREAD_MUTEX_INITIALIZER,.lock=PTHREAD_MUTEX_INITIALIZER,.nr_vcpus=1,.cgrp=&owner};refcount_set(&inst.refs,1);carrier.inst=&inst;inst.vcpus[0]=&carrier;
 assert(!klet_tio_host_init());assert(!klet_tio_vsock_register());
 struct klet_tio_descriptor descriptor;native_vsock_config(&descriptor,42,2);int fd=klet_tio_create_device(&descriptor);assert(fd>=3);struct klet_device attach={.type=KLET_DEVICE_TYPEDIO,.fd=fd};assert(klet_tio_attach(&inst,&attach)==0);native_close(fd);
#ifdef ASTERINAS_DRIVER_TEST
 native_vsock_config(&descriptor,7,42);fd=klet_tio_create_device(&descriptor);assert(fd>=3);attach.fd=fd;assert(klet_tio_attach(&inst,&attach)==1);native_close(fd);
#endif
 native_vsock_config(&descriptor,2,42);int host=klet_tio_create_device(&descriptor);assert(host>=3);
 struct klet_tio_listen_args args={.port=8000,.backlog=2,.flags=O_NONBLOCK|O_CLOEXEC};struct file *provider=fget(host);assert(provider);int listener=provider->f_op->unlocked_ioctl(provider,KLET_TIO_LISTEN,(unsigned long)&args);assert(listener>=3);fput(provider);native_close(host);
#ifdef ASTERINAS_DRIVER_TEST
 native_asterinas_roundtrip(listener);
#else
 native_vsock_host_roundtrip(listener);
#endif
 native_close(listener);
 pthread_rwlock_wrlock(&fixture_gate);fixture_stopped=true;pthread_rwlock_unlock(&fixture_gate);
 inst.dying=true;klet_tio_detach_instance(&inst);klet_tio_resources_evicted(&inst);klet_tio_vsock_unregister();klet_tio_host_exit();assert(atomic_load(&native_module.references)==0);
#ifdef ASTERINAS_DRIVER_TEST
 puts("PASS native mock: actual Asterinas Socket transport -> production guest ABI -> safe Vsock backend -> host Linux File; multiple attachments/connect/accept/echo/peek/half-close/EOF");
#else
 puts("PASS native mock: production Linux File adapter listen/accept/FD/read/write/poll/nonblocking/partial transfer/EOF/retirement with real guest Queue/Pipe and host Vsock backend");
#endif
 return 0;
}
